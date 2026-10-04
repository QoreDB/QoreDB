// SPDX-License-Identifier: Apache-2.0

use qore_core::{DataEngine, EngineError, RowData, SessionId};
use qore_drivers::session_manager::SessionManager;
use qore_sql::generator::{SandboxChangeDto, SandboxChangeType};
use serde::Serialize;
use std::{sync::Arc, time::Instant};

use crate::{
    cache::QueryCache,
    interceptor::{InterceptorPipeline, QueryExecutionResult},
};

#[derive(Debug, Serialize)]
pub struct FailedChange {
    pub index: usize,
    pub error: String,
}

#[derive(Debug, Default, Serialize)]
pub struct ApplyBatchResult {
    pub success: bool,
    /// Confirmed changes only: a rolled-back or uncertain transaction contributes none.
    pub applied_count: usize,
    pub applied_indices: Vec<usize>,
    pub outcome_unknown: bool,
    pub error: Option<String>,
    pub failed_changes: Vec<FailedChange>,
    #[serde(skip)]
    execution_times_ms: Vec<f64>,
}

impl ApplyBatchResult {
    pub fn rejected(error: String) -> Self {
        Self {
            error: Some(error),
            ..Self::default()
        }
    }
}

/// Validate the entire batch before its first write, including ordinary mutation guards.
pub async fn apply_batch(
    manager: &SessionManager,
    interceptor: &InterceptorPipeline,
    cache: &QueryCache,
    session: SessionId,
    changes: &[SandboxChangeDto],
    use_transaction: bool,
    acknowledged: bool,
) -> ApplyBatchResult {
    let mut preflights = Vec::with_capacity(changes.len());
    for (index, change) in changes.iter().enumerate() {
        let check = async {
            validate_change(change)?;
            let empty = RowData::new();
            let data = RowData {
                columns: change.new_values.clone().unwrap_or_default(),
            };
            super::check_update_masking(
                manager,
                session,
                &change.table_name,
                change.primary_key.as_ref().unwrap_or(&empty),
                &data,
            )
            .await?;
            // Values stay out of the audit preview, including masked data and secrets.
            let quote = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));
            let table = change
                .namespace
                .schema
                .as_ref()
                .map(|schema| format!("{}.{}", quote(schema), quote(&change.table_name)))
                .unwrap_or_else(|| quote(&change.table_name));
            let table = format!("{}.{}", quote(&change.namespace.database), table);
            let preview = match change.change_type {
                SandboxChangeType::Insert => format!("INSERT INTO {table} VALUES (...)"),
                SandboxChangeType::Update => format!("UPDATE {table} SET ... WHERE ..."),
                SandboxChangeType::Delete => format!("DELETE FROM {table} WHERE ..."),
            };
            super::preflight(
                manager,
                interceptor,
                session,
                &session.0.to_string(),
                &preview,
                &change.namespace.database,
                acknowledged,
            )
            .await
        }
        .await;
        match check {
            Ok(preflight) => preflights.push(preflight),
            Err(error) => {
                return ApplyBatchResult {
                    failed_changes: vec![FailedChange {
                        index,
                        error: error.clone(),
                    }],
                    ..ApplyBatchResult::rejected(error)
                };
            }
        }
    }
    let Some(first) = preflights.first() else {
        return ApplyBatchResult {
            success: true,
            ..Default::default()
        };
    };
    let result = execute_batch(&first.driver, session, changes, use_transaction).await;
    if !result.execution_times_ms.is_empty() {
        if let Some(key) = manager.connection_key(session).await {
            cache.invalidate_connection(&key);
        }
    }
    for (index, preflight) in preflights
        .iter()
        .take(result.execution_times_ms.len())
        .enumerate()
    {
        let confirmed = result.applied_indices.contains(&index);
        interceptor.post_execute(
            &preflight.context,
            &QueryExecutionResult {
                success: confirmed,
                error: if confirmed {
                    None
                } else {
                    result.error.clone()
                },
                execution_time_ms: result.execution_times_ms[index],
                row_count: None,
            },
            false,
            preflight.safety_warning.as_deref(),
        );
    }
    result
}

fn validate_change(change: &SandboxChangeDto) -> Result<(), String> {
    if change.table_name.is_empty() {
        return Err("Missing table name".into());
    }
    if !matches!(change.change_type, SandboxChangeType::Insert)
        && change
            .primary_key
            .as_ref()
            .is_none_or(|pk| pk.columns.is_empty())
    {
        return Err("UPDATE/DELETE requires a non-empty primary key".into());
    }
    if !matches!(change.change_type, SandboxChangeType::Delete)
        && change
            .new_values
            .as_ref()
            .is_none_or(|values| values.is_empty())
    {
        return Err("INSERT/UPDATE requires non-empty new_values".into());
    }
    Ok(())
}

async fn execute_batch(
    driver: &Arc<dyn DataEngine>,
    session: SessionId,
    changes: &[SandboxChangeDto],
    use_transaction: bool,
) -> ApplyBatchResult {
    let mut result = ApplyBatchResult::default();
    if use_transaction {
        if !driver.supports_transactions_for_session(session).await {
            return ApplyBatchResult::rejected(
                "Transactions are not supported by this session".into(),
            );
        }
        if let Err(error) = driver.begin_transaction(session).await {
            // A pre-existing transaction belongs to its caller; never roll it back here.
            return ApplyBatchResult::rejected(format!(
                "Failed to begin transaction: {}",
                error.sanitized_message()
            ));
        }
    }

    for (index, change) in changes.iter().enumerate() {
        let start = Instant::now();
        let applied = apply_single_change(driver, session, change).await;
        result
            .execution_times_ms
            .push(start.elapsed().as_secs_f64() * 1000.0);
        match applied {
            Ok(()) => result.applied_indices.push(index),
            Err(error) => {
                let message = error.sanitized_message();
                result.failed_changes.push(FailedChange {
                    index,
                    error: message.clone(),
                });
                result.error = Some(format!("Change {} failed: {message}", index + 1));
                if use_transaction {
                    result.applied_indices.clear();
                    match driver.rollback(session).await {
                        Ok(()) => result
                            .error
                            .as_mut()
                            .unwrap()
                            .push_str(". Transaction rolled back."),
                        Err(error) => {
                            result.outcome_unknown = true;
                            result.error.as_mut().unwrap().push_str(&format!(
                                ". Rollback also failed: {}",
                                error.sanitized_message()
                            ));
                        }
                    }
                } else {
                    // A driver error may arrive after the server applied the write.
                    result.outcome_unknown = !matches!(error, EngineError::ValidationError { .. });
                }
                result.applied_count = result.applied_indices.len();
                return result;
            }
        }
    }

    if use_transaction {
        if let Err(error) = driver.commit(session).await {
            result.applied_indices.clear();
            result.outcome_unknown = true;
            result.error = Some(format!(
                "Failed to commit transaction: {}",
                error.sanitized_message()
            ));
            // The commit may already have reached the server. Cleanup cannot establish its outcome.
            if let Err(error) = driver.rollback(session).await {
                result.error.as_mut().unwrap().push_str(&format!(
                    ". Cleanup rollback failed: {}",
                    error.sanitized_message()
                ));
            }
            return result;
        }
    }
    result.success = true;
    result.applied_count = result.applied_indices.len();
    result
}

async fn apply_single_change(
    driver: &Arc<dyn DataEngine>,
    session: SessionId,
    change: &SandboxChangeDto,
) -> Result<(), EngineError> {
    let result = match change.change_type {
        SandboxChangeType::Insert => {
            let values = change
                .new_values
                .as_ref()
                .ok_or_else(|| EngineError::validation("INSERT missing new_values"))?;
            driver
                .insert_row(
                    session,
                    &change.namespace,
                    &change.table_name,
                    &RowData {
                        columns: values.clone(),
                    },
                )
                .await?
        }
        SandboxChangeType::Update => {
            let pk = change
                .primary_key
                .as_ref()
                .ok_or_else(|| EngineError::validation("UPDATE missing primary_key"))?;
            let values = change
                .new_values
                .as_ref()
                .ok_or_else(|| EngineError::validation("UPDATE missing new_values"))?;
            driver
                .update_row(
                    session,
                    &change.namespace,
                    &change.table_name,
                    pk,
                    &RowData {
                        columns: values.clone(),
                    },
                )
                .await?
        }
        SandboxChangeType::Delete => {
            let pk = change
                .primary_key
                .as_ref()
                .ok_or_else(|| EngineError::validation("DELETE missing primary_key"))?;
            driver
                .delete_row(session, &change.namespace, &change.table_name, pk)
                .await?
        }
    };
    if matches!(result.affected_rows, Some(0)) {
        return Err(EngineError::validation(
            "Change affected 0 rows (possible conflict)",
        ));
    }
    Ok(())
}

#[cfg(all(test, feature = "driver-sqlite"))]
#[path = "batch_tests.rs"]
mod tests;

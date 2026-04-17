// SPDX-License-Identifier: Apache-2.0

//! Mutation Tauri Commands
//!
//! Commands for executing insert, update, and delete operations.

use serde::Serialize;
use std::sync::Arc;
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::engine::types::{Namespace, QueryResult, RowData, SessionId};
use crate::interceptor::{Environment, QueryExecutionResult, SafetyAction};
use crate::time_travel::capture::{
    build_changelog_entry, fetch_row_by_pk, merge_before_with_data, rowdata_to_json_map,
};
use crate::time_travel::ChangeOperation;

const READ_ONLY_BLOCKED: &str = "Operation blocked: read-only mode";
const MUTATIONS_NOT_SUPPORTED: &str = "Mutations are not supported by this driver";
const DANGEROUS_BLOCKED: &str = "Dangerous query blocked: confirmation required";
const SAFETY_RULE_BLOCKED: &str = "Query blocked by safety rule";

fn map_environment(env: &str) -> Environment {
    match env {
        "production" => Environment::Production,
        "staging" => Environment::Staging,
        _ => Environment::Development,
    }
}

fn format_table_ref(database: &str, schema: &Option<String>, table: &str) -> String {
    if let Some(schema) = schema {
        format!("{}.{}.{}", database, schema, table)
    } else {
        format!("{}.{}", database, table)
    }
}

/// Response wrapper for mutation results
#[derive(Debug, Serialize)]
pub struct MutationResponse {
    pub success: bool,
    pub result: Option<QueryResult>,
    pub error: Option<String>,
}

/// Parses a session ID string into SessionId
fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

/// Inserts a row into a table
#[tauri::command]
#[instrument(
    skip(state, data),
    fields(session_id = %session_id, database = %database, schema = ?schema, table = %table)
)]
pub async fn insert_row(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    table: String,
    data: RowData,
    acknowledged_dangerous: Option<bool>,
) -> Result<MutationResponse, String> {
    let state_guard = state.lock().await;
    let session_manager = Arc::clone(&state_guard.session_manager);
    let interceptor = Arc::clone(&state_guard.interceptor);
    let changelog_store = Arc::clone(&state_guard.changelog_store);
    drop(state_guard);

    let session = parse_session_id(&session_id)?;

    let read_only = session_manager
        .is_read_only(session)
        .await
        .map_err(|e| e.to_string())?;
    if read_only {
        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
        });
    }

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.capabilities().mutations {
        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(MUTATIONS_NOT_SUPPORTED.to_string()),
        });
    }

    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let interceptor_env = map_environment(&environment);
    let query_preview = format!(
        "INSERT INTO {} VALUES (...)",
        format_table_ref(&database, &schema, &table)
    );

    let acknowledged = acknowledged_dangerous.unwrap_or(false);
    let interceptor_context = interceptor.build_context(
        &session_id,
        &query_preview,
        driver.driver_id(),
        interceptor_env,
        read_only,
        acknowledged,
        Some(&database),
        None,
        true,
    );

    let safety_result = interceptor.pre_execute(&interceptor_context);
    if !safety_result.allowed {
        interceptor.post_execute(
            &interceptor_context,
            &QueryExecutionResult {
                success: false,
                error: safety_result.message.clone(),
                execution_time_ms: 0.0,
                row_count: None,
            },
            true,
            safety_result.triggered_rule.as_deref(),
        );

        let error_msg = match safety_result.action {
            SafetyAction::Block => {
                format!(
                    "{}: {}",
                    SAFETY_RULE_BLOCKED,
                    safety_result.message.unwrap_or_default()
                )
            }
            SafetyAction::RequireConfirmation => {
                format!(
                    "{}: {}",
                    DANGEROUS_BLOCKED,
                    safety_result.message.unwrap_or_default()
                )
            }
            SafetyAction::Warn => "Warning triggered".to_string(),
        };

        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(error_msg),
        });
    }

    let safety_warning = if matches!(safety_result.action, SafetyAction::Warn) {
        safety_result.triggered_rule.clone()
    } else {
        None
    };

    let namespace = Namespace { database, schema };

    let start_time = std::time::Instant::now();
    match driver.insert_row(session, &namespace, &table, &data).await {
        Ok(mut result) => {
            result.execution_time_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: true,
                    error: None,
                    execution_time_ms: result.execution_time_ms,
                    row_count: result.affected_rows.map(|a| a as i64),
                },
                false,
                safety_warning.as_deref(),
            );

            // Time-Travel: record the INSERT (after-image = data params)
            if changelog_store.should_capture(&table, &environment) {
                let after_image = rowdata_to_json_map(&data);
                let entry = build_changelog_entry(
                    &session_id,
                    driver.driver_id(),
                    &namespace,
                    &table,
                    ChangeOperation::Insert,
                    &data, // PK = the inserted data
                    None,
                    Some(after_image),
                    None,
                    &environment,
                );
                changelog_store.record(entry);
            }

            Ok(MutationResponse {
                success: true,
                result: Some(result),
                error: None,
            })
        }
        Err(e) => {
            let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: false,
                    error: Some(e.to_string()),
                    execution_time_ms: duration_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(MutationResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// Updates a row in a table
#[tauri::command]
#[instrument(
    skip(state, primary_key, data),
    fields(session_id = %session_id, database = %database, schema = ?schema, table = %table)
)]
pub async fn update_row(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    table: String,
    primary_key: RowData,
    data: RowData,
    acknowledged_dangerous: Option<bool>,
) -> Result<MutationResponse, String> {
    let state_guard = state.lock().await;
    let session_manager = Arc::clone(&state_guard.session_manager);
    let interceptor = Arc::clone(&state_guard.interceptor);
    let changelog_store = Arc::clone(&state_guard.changelog_store);
    drop(state_guard);
    let session = parse_session_id(&session_id)?;

    let read_only = session_manager
        .is_read_only(session)
        .await
        .map_err(|e| e.to_string())?;
    if read_only {
        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
        });
    }

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.capabilities().mutations {
        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(MUTATIONS_NOT_SUPPORTED.to_string()),
        });
    }

    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let interceptor_env = map_environment(&environment);
    let query_preview = format!(
        "UPDATE {} SET ... WHERE ...",
        format_table_ref(&database, &schema, &table)
    );

    let acknowledged = acknowledged_dangerous.unwrap_or(false);
    let interceptor_context = interceptor.build_context(
        &session_id,
        &query_preview,
        driver.driver_id(),
        interceptor_env,
        read_only,
        acknowledged,
        Some(&database),
        None,
        true,
    );

    let safety_result = interceptor.pre_execute(&interceptor_context);
    if !safety_result.allowed {
        interceptor.post_execute(
            &interceptor_context,
            &QueryExecutionResult {
                success: false,
                error: safety_result.message.clone(),
                execution_time_ms: 0.0,
                row_count: None,
            },
            true,
            safety_result.triggered_rule.as_deref(),
        );

        let error_msg = match safety_result.action {
            SafetyAction::Block => {
                format!(
                    "{}: {}",
                    SAFETY_RULE_BLOCKED,
                    safety_result.message.unwrap_or_default()
                )
            }
            SafetyAction::RequireConfirmation => {
                format!(
                    "{}: {}",
                    DANGEROUS_BLOCKED,
                    safety_result.message.unwrap_or_default()
                )
            }
            SafetyAction::Warn => "Warning triggered".to_string(),
        };

        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(error_msg),
        });
    }

    let safety_warning = if matches!(safety_result.action, SafetyAction::Warn) {
        safety_result.triggered_rule.clone()
    } else {
        None
    };

    let namespace = Namespace { database, schema };

    // Time-Travel: fetch before-image BEFORE the mutation
    let before_image = if changelog_store.should_capture(&table, &environment) {
        fetch_row_by_pk(&driver, session, &namespace, &table, &primary_key).await
    } else {
        None
    };

    let start_time = std::time::Instant::now();
    match driver
        .update_row(session, &namespace, &table, &primary_key, &data)
        .await
    {
        Ok(mut result) => {
            result.execution_time_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: true,
                    error: None,
                    execution_time_ms: result.execution_time_ms,
                    row_count: result.affected_rows.map(|a| a as i64),
                },
                false,
                safety_warning.as_deref(),
            );

            // Time-Travel: record the UPDATE with before + after images
            if changelog_store.should_capture(&table, &environment) {
                let after_image = before_image
                    .as_ref()
                    .map(|before| merge_before_with_data(before, &data));
                let entry = build_changelog_entry(
                    &session_id,
                    driver.driver_id(),
                    &namespace,
                    &table,
                    ChangeOperation::Update,
                    &primary_key,
                    before_image,
                    after_image,
                    None,
                    &environment,
                );
                changelog_store.record(entry);
            }

            Ok(MutationResponse {
                success: true,
                result: Some(result),
                error: None,
            })
        }
        Err(e) => {
            let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: false,
                    error: Some(e.to_string()),
                    execution_time_ms: duration_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(MutationResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// Deletes a row from a table
#[tauri::command]
#[instrument(
    skip(state, primary_key),
    fields(session_id = %session_id, database = %database, schema = ?schema, table = %table)
)]
pub async fn delete_row(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    table: String,
    primary_key: RowData,
    acknowledged_dangerous: Option<bool>,
) -> Result<MutationResponse, String> {
    let state_guard = state.lock().await;
    let session_manager = Arc::clone(&state_guard.session_manager);
    let interceptor = Arc::clone(&state_guard.interceptor);
    let changelog_store = Arc::clone(&state_guard.changelog_store);
    drop(state_guard);
    let session = parse_session_id(&session_id)?;

    let read_only = session_manager
        .is_read_only(session)
        .await
        .map_err(|e| e.to_string())?;
    if read_only {
        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
        });
    }

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.capabilities().mutations {
        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(MUTATIONS_NOT_SUPPORTED.to_string()),
        });
    }

    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let interceptor_env = map_environment(&environment);
    let query_preview = format!(
        "DELETE FROM {} WHERE ...",
        format_table_ref(&database, &schema, &table)
    );

    let acknowledged = acknowledged_dangerous.unwrap_or(false);
    let interceptor_context = interceptor.build_context(
        &session_id,
        &query_preview,
        driver.driver_id(),
        interceptor_env,
        read_only,
        acknowledged,
        Some(&database),
        None,
        true,
    );

    let safety_result = interceptor.pre_execute(&interceptor_context);
    if !safety_result.allowed {
        interceptor.post_execute(
            &interceptor_context,
            &QueryExecutionResult {
                success: false,
                error: safety_result.message.clone(),
                execution_time_ms: 0.0,
                row_count: None,
            },
            true,
            safety_result.triggered_rule.as_deref(),
        );

        let error_msg = match safety_result.action {
            SafetyAction::Block => {
                format!(
                    "{}: {}",
                    SAFETY_RULE_BLOCKED,
                    safety_result.message.unwrap_or_default()
                )
            }
            SafetyAction::RequireConfirmation => {
                format!(
                    "{}: {}",
                    DANGEROUS_BLOCKED,
                    safety_result.message.unwrap_or_default()
                )
            }
            SafetyAction::Warn => "Warning triggered".to_string(),
        };

        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(error_msg),
        });
    }

    let safety_warning = if matches!(safety_result.action, SafetyAction::Warn) {
        safety_result.triggered_rule.clone()
    } else {
        None
    };

    let namespace = Namespace { database, schema };

    // Time-Travel: fetch before-image BEFORE the deletion
    let before_image = if changelog_store.should_capture(&table, &environment) {
        fetch_row_by_pk(&driver, session, &namespace, &table, &primary_key).await
    } else {
        None
    };

    let start_time = std::time::Instant::now();
    match driver
        .delete_row(session, &namespace, &table, &primary_key)
        .await
    {
        Ok(mut result) => {
            result.execution_time_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: true,
                    error: None,
                    execution_time_ms: result.execution_time_ms,
                    row_count: result.affected_rows.map(|a| a as i64),
                },
                false,
                safety_warning.as_deref(),
            );

            // Time-Travel: record the DELETE with before-image
            if changelog_store.should_capture(&table, &environment) {
                let entry = build_changelog_entry(
                    &session_id,
                    driver.driver_id(),
                    &namespace,
                    &table,
                    ChangeOperation::Delete,
                    &primary_key,
                    before_image,
                    None,
                    None,
                    &environment,
                );
                changelog_store.record(entry);
            }

            Ok(MutationResponse {
                success: true,
                result: Some(result),
                error: None,
            })
        }
        Err(e) => {
            let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: false,
                    error: Some(e.to_string()),
                    execution_time_ms: duration_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(MutationResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// Checks if the driver supports mutations
#[tauri::command]
pub async fn supports_mutations(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<bool, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    Ok(driver.capabilities().mutations)
}

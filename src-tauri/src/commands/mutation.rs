// SPDX-License-Identifier: Apache-2.0

//! Commands for executing insert, update, and delete operations.

use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tracing::instrument;

use super::{SharedStateExt, parse_session_id};
use crate::engine::types::{Namespace, QueryResult, RowData};
use crate::interceptor::QueryExecutionResult;
use crate::time_travel::ChangeOperation;
use crate::time_travel::capture::record_capture;
use qore_service::mutation::capture::{finish_capture, prepare_capture};
use qore_sql::generator::SandboxChangeType;

fn format_table_ref(database: &str, schema: &Option<String>, table: &str) -> String {
    if let Some(schema) = schema {
        format!("{}.{}.{}", database, schema, table)
    } else {
        format!("{}.{}", database, table)
    }
}

#[derive(Debug, Serialize)]
pub struct MutationResponse {
    pub success: bool,
    pub result: Option<QueryResult>,
    pub error: Option<String>,
}

#[tauri::command]
#[instrument(
    skip(state, data),
    fields(session_id = %session_id, database = %database, schema = ?schema, table = %table)
)]
pub async fn insert_row(
    app: AppHandle,
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
    let query_cache = Arc::clone(&state_guard.query_cache);
    drop(state_guard);

    let session = parse_session_id(&session_id)?;
    let connection_identity = session_manager.get_saved_connection_identity(session).await;
    let workspace_id = session_manager.workspace_id(session).await;

    let query_preview = format!(
        "INSERT INTO {} VALUES (...)",
        format_table_ref(&database, &schema, &table)
    );

    let preflight = match qore_service::mutation::preflight(
        &session_manager,
        &interceptor,
        session,
        &session_id,
        &query_preview,
        &database,
        acknowledged_dangerous.unwrap_or(false),
    )
    .await
    {
        Ok(pf) => pf,
        Err(msg) => {
            return Ok(MutationResponse {
                success: false,
                result: None,
                error: Some(msg),
            });
        }
    };
    let qore_service::mutation::MutationPreflight {
        driver,
        context: interceptor_context,
        environment,
        safety_warning,
    } = preflight;

    let namespace = Namespace { database, schema };
    let mut prepared = if changelog_store.should_capture(&table, &environment) {
        Some(
            prepare_capture(
                driver.as_ref(),
                session,
                &namespace,
                &table,
                SandboxChangeType::Insert,
                &data,
            )
            .await,
        )
    } else {
        None
    };

    let start_time = std::time::Instant::now();
    match driver
        .insert_row_returning(
            session,
            &namespace,
            &table,
            &data,
            prepared
                .as_ref()
                .map(|capture| capture.returning_columns())
                .unwrap_or(&[]),
        )
        .await
    {
        Ok(outcome) => {
            let mut result = outcome.result;
            if let Some(prepared) = prepared.as_mut() {
                prepared.use_inserted_values(outcome.returned_values);
            }
            result.execution_time_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            qore_service::query::apply_masking(
                &session_manager,
                session,
                Some(&table),
                &mut result,
            )
            .await;
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

            if let Some(prepared) = prepared {
                if let Some(capture) = finish_capture(
                    driver.as_ref(),
                    session,
                    &namespace,
                    &table,
                    SandboxChangeType::Insert,
                    &data,
                    prepared,
                    &result,
                )
                .await
                {
                    record_capture(
                        &changelog_store,
                        &session_id,
                        driver.driver_id(),
                        workspace_id.as_deref(),
                        connection_identity.as_ref().map(|(id, _)| id.as_str()),
                        connection_identity.as_ref().map(|(_, name)| name.as_str()),
                        &environment,
                        &namespace,
                        &table,
                        ChangeOperation::Insert,
                        &capture,
                        session_manager
                            .masking(session)
                            .await
                            .as_ref()
                            .map(|masking| &masking.config),
                    );
                }
            }

            #[cfg(feature = "pro")]
            crate::contracts::alert::schedule_post_mutation_check(
                app.clone(),
                session,
                namespace.schema.clone(),
                table.clone(),
            );

            if let Some(key) = session_manager.connection_key(session).await {
                query_cache.invalidate_connection(&key);
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
                    error: Some(e.sanitized_message()),
                    execution_time_ms: duration_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(MutationResponse {
                success: false,
                result: None,
                error: Some(e.sanitized_message()),
            })
        }
    }
}

#[tauri::command]
#[instrument(
    skip(state, primary_key, data),
    fields(session_id = %session_id, database = %database, schema = ?schema, table = %table)
)]
pub async fn update_row(
    app: AppHandle,
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
    let query_cache = Arc::clone(&state_guard.query_cache);
    drop(state_guard);
    let session = parse_session_id(&session_id)?;
    let connection_identity = session_manager.get_saved_connection_identity(session).await;
    let workspace_id = session_manager.workspace_id(session).await;

    let query_preview = format!(
        "UPDATE {} SET ... WHERE ...",
        format_table_ref(&database, &schema, &table)
    );

    if let Err(error) = qore_service::mutation::check_update_masking(
        &session_manager,
        session,
        &table,
        &primary_key,
        &data,
    )
    .await
    {
        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(error),
        });
    }

    let preflight = match qore_service::mutation::preflight(
        &session_manager,
        &interceptor,
        session,
        &session_id,
        &query_preview,
        &database,
        acknowledged_dangerous.unwrap_or(false),
    )
    .await
    {
        Ok(pf) => pf,
        Err(msg) => {
            return Ok(MutationResponse {
                success: false,
                result: None,
                error: Some(msg),
            });
        }
    };
    let qore_service::mutation::MutationPreflight {
        driver,
        context: interceptor_context,
        environment,
        safety_warning,
    } = preflight;

    let namespace = Namespace { database, schema };

    let prepared = if changelog_store.should_capture(&table, &environment) {
        Some(
            prepare_capture(
                driver.as_ref(),
                session,
                &namespace,
                &table,
                SandboxChangeType::Update,
                &primary_key,
            )
            .await,
        )
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
            qore_service::query::apply_masking(
                &session_manager,
                session,
                Some(&table),
                &mut result,
            )
            .await;
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

            if let Some(prepared) = prepared {
                if let Some(capture) = finish_capture(
                    driver.as_ref(),
                    session,
                    &namespace,
                    &table,
                    SandboxChangeType::Update,
                    &data,
                    prepared,
                    &result,
                )
                .await
                {
                    record_capture(
                        &changelog_store,
                        &session_id,
                        driver.driver_id(),
                        workspace_id.as_deref(),
                        connection_identity.as_ref().map(|(id, _)| id.as_str()),
                        connection_identity.as_ref().map(|(_, name)| name.as_str()),
                        &environment,
                        &namespace,
                        &table,
                        ChangeOperation::Update,
                        &capture,
                        session_manager
                            .masking(session)
                            .await
                            .as_ref()
                            .map(|masking| &masking.config),
                    );
                }
            }

            #[cfg(feature = "pro")]
            crate::contracts::alert::schedule_post_mutation_check(
                app.clone(),
                session,
                namespace.schema.clone(),
                table.clone(),
            );

            if let Some(key) = session_manager.connection_key(session).await {
                query_cache.invalidate_connection(&key);
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
                    error: Some(e.sanitized_message()),
                    execution_time_ms: duration_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(MutationResponse {
                success: false,
                result: None,
                error: Some(e.sanitized_message()),
            })
        }
    }
}

#[tauri::command]
#[instrument(
    skip(state, primary_key),
    fields(session_id = %session_id, database = %database, schema = ?schema, table = %table)
)]
pub async fn delete_row(
    app: AppHandle,
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
    let query_cache = Arc::clone(&state_guard.query_cache);
    drop(state_guard);
    let session = parse_session_id(&session_id)?;
    let connection_identity = session_manager.get_saved_connection_identity(session).await;
    let workspace_id = session_manager.workspace_id(session).await;

    let query_preview = format!(
        "DELETE FROM {} WHERE ...",
        format_table_ref(&database, &schema, &table)
    );

    let preflight = match qore_service::mutation::preflight(
        &session_manager,
        &interceptor,
        session,
        &session_id,
        &query_preview,
        &database,
        acknowledged_dangerous.unwrap_or(false),
    )
    .await
    {
        Ok(pf) => pf,
        Err(msg) => {
            return Ok(MutationResponse {
                success: false,
                result: None,
                error: Some(msg),
            });
        }
    };
    let qore_service::mutation::MutationPreflight {
        driver,
        context: interceptor_context,
        environment,
        safety_warning,
    } = preflight;

    let namespace = Namespace { database, schema };

    let prepared = if changelog_store.should_capture(&table, &environment) {
        Some(
            prepare_capture(
                driver.as_ref(),
                session,
                &namespace,
                &table,
                SandboxChangeType::Delete,
                &primary_key,
            )
            .await,
        )
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
            qore_service::query::apply_masking(
                &session_manager,
                session,
                Some(&table),
                &mut result,
            )
            .await;
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

            if let Some(prepared) = prepared {
                if let Some(capture) = finish_capture(
                    driver.as_ref(),
                    session,
                    &namespace,
                    &table,
                    SandboxChangeType::Delete,
                    &RowData::new(),
                    prepared,
                    &result,
                )
                .await
                {
                    record_capture(
                        &changelog_store,
                        &session_id,
                        driver.driver_id(),
                        workspace_id.as_deref(),
                        connection_identity.as_ref().map(|(id, _)| id.as_str()),
                        connection_identity.as_ref().map(|(_, name)| name.as_str()),
                        &environment,
                        &namespace,
                        &table,
                        ChangeOperation::Delete,
                        &capture,
                        session_manager
                            .masking(session)
                            .await
                            .as_ref()
                            .map(|masking| &masking.config),
                    );
                }
            }

            #[cfg(feature = "pro")]
            crate::contracts::alert::schedule_post_mutation_check(
                app.clone(),
                session,
                namespace.schema.clone(),
                table.clone(),
            );

            if let Some(key) = session_manager.connection_key(session).await {
                query_cache.invalidate_connection(&key);
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
                    error: Some(e.sanitized_message()),
                    execution_time_ms: duration_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(MutationResponse {
                success: false,
                result: None,
                error: Some(e.sanitized_message()),
            })
        }
    }
}

#[tauri::command]
pub async fn supports_mutations(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<bool, String> {
    let session_manager = state.session_manager().await;
    let session = parse_session_id(&session_id)?;

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;

    Ok(driver.capabilities().mutations)
}

// SPDX-License-Identifier: Apache-2.0

//! Commands for executing insert, update, and delete operations.

use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tracing::instrument;
use uuid::Uuid;

use crate::engine::types::{Namespace, QueryResult, RowData, SessionId};
use crate::interceptor::QueryExecutionResult;
use crate::time_travel::capture::{
    build_changelog_entry, fetch_row_by_pk, merge_before_with_data, rowdata_to_json_map,
};
use crate::time_travel::ChangeOperation;

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

fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
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

            // Time-Travel: after-image equals the inserted data; PK is also the data row.
            if changelog_store.should_capture(&table, &environment) {
                let after_image = rowdata_to_json_map(&data);
                let entry = build_changelog_entry(
                    &session_id,
                    driver.driver_id(),
                    &namespace,
                    &table,
                    ChangeOperation::Insert,
                    &data,
                    None,
                    Some(after_image),
                    None,
                    &environment,
                );
                changelog_store.record(entry);
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

    let query_preview = format!(
        "UPDATE {} SET ... WHERE ...",
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

    // Time-Travel: fetch before-image prior to the mutation.
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

    // Time-Travel: fetch before-image prior to the deletion.
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
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;

    Ok(driver.capabilities().mutations)
}

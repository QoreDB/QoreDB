//! Mutation Tauri Commands
//!
//! Commands for executing insert, update, and delete operations.

use serde::Serialize;
use tauri::State;
use uuid::Uuid;
use std::sync::Arc;

use crate::engine::{types::{Namespace, QueryResult, RowData, SessionId}};

const READ_ONLY_BLOCKED: &str = "Operation blocked: read-only mode";

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
pub async fn insert_row(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    table: String,
    data: RowData,
) -> Result<MutationResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    if session_manager
        .is_read_only(session)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
        });
    }

    let driver = session_manager.get_driver(session).await
        .map_err(|e| e.to_string())?;

    let namespace = Namespace {
        database,
        schema,
    };

    let start_time = std::time::Instant::now();
    match driver.insert_row(session, &namespace, &table, &data).await {
        Ok(mut result) => {
            result.execution_time_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            Ok(MutationResponse {
                success: true,
                result: Some(result),
                error: None,
            })
        },
        Err(e) => Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Updates a row in a table
#[tauri::command]
pub async fn update_row(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    table: String,
    primary_key: RowData,
    data: RowData,
) -> Result<MutationResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    if session_manager
        .is_read_only(session)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
        });
    }

    let driver = session_manager.get_driver(session).await
        .map_err(|e| e.to_string())?;

    let namespace = Namespace {
        database,
        schema,
    };

    let start_time = std::time::Instant::now();
    match driver.update_row(session, &namespace, &table, &primary_key, &data).await {
        Ok(mut result) => {
            result.execution_time_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            Ok(MutationResponse {
                success: true,
                result: Some(result),
                error: None,
            })
        },
        Err(e) => Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Deletes a row from a table
#[tauri::command]
pub async fn delete_row(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    table: String,
    primary_key: RowData,
) -> Result<MutationResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    if session_manager
        .is_read_only(session)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
        });
    }

    let driver = session_manager.get_driver(session).await
        .map_err(|e| e.to_string())?;

    let namespace = Namespace {
        database,
        schema,
    };

    let start_time = std::time::Instant::now();
    match driver.delete_row(session, &namespace, &table, &primary_key).await {
        Ok(mut result) => {
            result.execution_time_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            Ok(MutationResponse {
                success: true,
                result: Some(result),
                error: None,
            })
        },
        Err(e) => Ok(MutationResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
        }),
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

    let driver = session_manager.get_driver(session).await
        .map_err(|e| e.to_string())?;

    Ok(driver.supports_mutations())
}

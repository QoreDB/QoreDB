//! Connection Tauri Commands
//!
//! Commands for managing database connections.

use serde::Serialize;
use tauri::State;
use std::sync::Arc;
use uuid::Uuid;

use crate::engine::types::ConnectionConfig;

/// Response for connection operations
#[derive(Debug, Serialize)]
pub struct ConnectionResponse {
    pub success: bool,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

/// Session info for list response
#[derive(Debug, Serialize)]
pub struct SessionListItem {
    pub id: String,
    pub display_name: String,
}

/// Tests a database connection without persisting it
#[tauri::command]
pub async fn test_connection(
    state: State<'_, crate::SharedState>,
    config: ConnectionConfig,
) -> Result<ConnectionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };

    match session_manager.test_connection(&config).await {
        Ok(()) => Ok(ConnectionResponse {
            success: true,
            session_id: None,
            error: None,
        }),
        Err(e) => Ok(ConnectionResponse {
            success: false,
            session_id: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Establishes a new database connection
#[tauri::command]
pub async fn connect(
    state: State<'_, crate::SharedState>,
    config: ConnectionConfig,
) -> Result<ConnectionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };

    match session_manager.connect(config).await {
        Ok(session_id) => Ok(ConnectionResponse {
            success: true,
            session_id: Some(session_id.0.to_string()),
            error: None,
        }),
        Err(e) => Ok(ConnectionResponse {
            success: false,
            session_id: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Disconnects an active session
#[tauri::command]
pub async fn disconnect(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<ConnectionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };

    let uuid = Uuid::parse_str(&session_id)
        .map_err(|e| format!("Invalid session ID: {}", e))?;

    match session_manager
        .disconnect(crate::engine::types::SessionId(uuid))
        .await {
        Ok(()) => Ok(ConnectionResponse {
            success: true,
            session_id: None,
            error: None,
        }),
        Err(e) => Ok(ConnectionResponse {
            success: false,
            session_id: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Lists all active sessions
#[tauri::command]
pub async fn list_sessions(
    state: State<'_, crate::SharedState>,
) -> Result<Vec<SessionListItem>, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };

    let sessions = session_manager.list_sessions().await;

    Ok(sessions
        .into_iter()
        .map(|(id, name)| SessionListItem {
            id: id.0.to_string(),
            display_name: name,
        })
        .collect())
}

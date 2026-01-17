//! Connection Tauri Commands
//!
//! Commands for managing database connections.

use serde::Serialize;
use tauri::State;
use std::sync::Arc;
use uuid::Uuid;

use crate::engine::types::ConnectionConfig;
use crate::vault::VaultStorage;

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

fn load_saved_connection_config(
    project_id: &str,
    connection_id: &str,
) -> Result<ConnectionConfig, String> {
    let storage = VaultStorage::new(project_id);
    let saved = storage
        .get_connection(connection_id)
        .map_err(|e| e.to_string())?;

    if saved.project_id != project_id {
        return Err("Connection project mismatch".to_string());
    }

    let creds = storage
        .get_credentials(connection_id)
        .map_err(|e| e.to_string())?;

    saved.to_connection_config(&creds).map_err(|e| e.to_string())
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

/// Tests a saved connection using vault metadata + credentials
#[tauri::command]
pub async fn test_saved_connection(
    state: State<'_, crate::SharedState>,
    project_id: String,
    connection_id: String,
) -> Result<ConnectionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        if state.vault_lock.is_locked() {
            return Ok(ConnectionResponse {
                success: false,
                session_id: None,
                error: Some("Vault is locked".to_string()),
            });
        }
        Arc::clone(&state.session_manager)
    };

    let config = match load_saved_connection_config(&project_id, &connection_id) {
        Ok(cfg) => cfg,
        Err(e) => {
            return Ok(ConnectionResponse {
                success: false,
                session_id: None,
                error: Some(e),
            });
        }
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

/// Establishes a new database connection from a saved connection
#[tauri::command]
pub async fn connect_saved_connection(
    state: State<'_, crate::SharedState>,
    project_id: String,
    connection_id: String,
) -> Result<ConnectionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        if state.vault_lock.is_locked() {
            return Ok(ConnectionResponse {
                success: false,
                session_id: None,
                error: Some("Vault is locked".to_string()),
            });
        }
        Arc::clone(&state.session_manager)
    };

    let config = match load_saved_connection_config(&project_id, &connection_id) {
        Ok(cfg) => cfg,
        Err(e) => {
            return Ok(ConnectionResponse {
                success: false,
                session_id: None,
                error: Some(e),
            });
        }
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

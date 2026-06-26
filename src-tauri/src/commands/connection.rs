// SPDX-License-Identifier: Apache-2.0

//! Commands for managing database connections.

use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use tracing::instrument;
use uuid::Uuid;

use crate::engine::types::ConnectionConfig;
use crate::vault::backend::KeyringProvider;
use crate::vault::VaultStorage;

#[derive(Debug, Serialize)]
pub struct ConnectionResponse {
    pub success: bool,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionListItem {
    pub id: String,
    pub display_name: String,
}

fn load_saved_connection_config(
    project_id: &str,
    connection_id: &str,
    storage_dir: PathBuf,
) -> Result<ConnectionConfig, String> {
    let storage = VaultStorage::new(project_id, storage_dir, Box::new(KeyringProvider::new()));
    let saved = storage
        .get_connection(connection_id)
        .map_err(|e| e.sanitized_message())?;

    if saved.project_id != project_id {
        return Err("Connection project mismatch".to_string());
    }

    let creds = storage
        .get_credentials(connection_id)
        .map_err(|e| e.sanitized_message())?;

    saved
        .to_connection_config(&creds)
        .map_err(|e| e.sanitized_message())
}

/// Like `load_saved_connection_config` but also returns the saved connection name.
fn load_saved_connection_config_with_name(
    project_id: &str,
    connection_id: &str,
    storage_dir: PathBuf,
) -> Result<(ConnectionConfig, String), String> {
    let storage = VaultStorage::new(project_id, storage_dir, Box::new(KeyringProvider::new()));
    let saved = storage
        .get_connection(connection_id)
        .map_err(|e| e.sanitized_message())?;

    if saved.project_id != project_id {
        return Err("Connection project mismatch".to_string());
    }

    let name = saved.name.clone();
    let creds = storage
        .get_credentials(connection_id)
        .map_err(|e| e.sanitized_message())?;

    let config = saved
        .to_connection_config(&creds)
        .map_err(|e| e.sanitized_message())?;
    Ok((config, name))
}

#[tauri::command]
#[instrument(
    skip(state, config),
    fields(
        driver = %config.driver,
        host = %config.host,
        port = config.port,
        database = ?config.database,
        ssh = config.ssh_tunnel.is_some()
    )
)]
pub async fn test_connection(
    state: State<'_, crate::SharedState>,
    config: ConnectionConfig,
) -> Result<ConnectionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };

    match qore_service::connection::test_connection(&session_manager, config).await {
        Ok(()) => Ok(ConnectionResponse {
            success: true,
            session_id: None,
            error: None,
        }),
        Err(e) => Ok(ConnectionResponse {
            success: false,
            session_id: None,
            error: Some(e.sanitized()),
        }),
    }
}

#[tauri::command]
#[instrument(skip(app, state), fields(project_id = %project_id, connection_id = %connection_id))]
pub async fn test_saved_connection(
    app: AppHandle,
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

    let storage_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;

    let config = match load_saved_connection_config(&project_id, &connection_id, storage_dir) {
        Ok(cfg) => cfg,
        Err(e) => {
            return Ok(ConnectionResponse {
                success: false,
                session_id: None,
                error: Some(e),
            });
        }
    };

    match qore_service::connection::test_connection(&session_manager, config).await {
        Ok(()) => Ok(ConnectionResponse {
            success: true,
            session_id: None,
            error: None,
        }),
        Err(e) => Ok(ConnectionResponse {
            success: false,
            session_id: None,
            error: Some(e.sanitized()),
        }),
    }
}

#[tauri::command]
#[instrument(
    skip(state, config),
    fields(
        driver = %config.driver,
        host = %config.host,
        port = config.port,
        database = ?config.database,
        ssh = config.ssh_tunnel.is_some()
    )
)]
pub async fn connect(
    state: State<'_, crate::SharedState>,
    config: ConnectionConfig,
) -> Result<ConnectionResponse, String> {
    if !cfg!(debug_assertions) {
        return Ok(ConnectionResponse {
            success: false,
            session_id: None,
            error: Some("Direct connect is disabled in release builds. Save the connection and use connect_saved_connection.".to_string()),
        });
    }

    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };

    match qore_service::connection::connect(&session_manager, config).await {
        Ok(session_id) => Ok(ConnectionResponse {
            success: true,
            session_id: Some(session_id.0.to_string()),
            error: None,
        }),
        Err(e) => Ok(ConnectionResponse {
            success: false,
            session_id: None,
            error: Some(e.sanitized()),
        }),
    }
}

#[tauri::command]
#[instrument(skip(app, state), fields(project_id = %project_id, connection_id = %connection_id))]
pub async fn connect_saved_connection(
    app: AppHandle,
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

    let storage_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;

    let (config, connection_name) =
        match load_saved_connection_config_with_name(&project_id, &connection_id, storage_dir) {
            Ok(pair) => pair,
            Err(e) => {
                return Ok(ConnectionResponse {
                    success: false,
                    session_id: None,
                    error: Some(e),
                });
            }
        };

    match qore_service::connection::connect(&session_manager, config).await {
        Ok(session_id) => {
            session_manager
                .set_display_name(session_id, connection_name)
                .await;
            Ok(ConnectionResponse {
                success: true,
                session_id: Some(session_id.0.to_string()),
                error: None,
            })
        }
        Err(e) => Ok(ConnectionResponse {
            success: false,
            session_id: None,
            error: Some(e.sanitized()),
        }),
    }
}

#[tauri::command]
#[instrument(skip(state), fields(session_id = %session_id))]
pub async fn disconnect(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<ConnectionResponse, String> {
    let (session_manager, query_rate_limiter) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.query_rate_limiter),
        )
    };

    let uuid = Uuid::parse_str(&session_id).map_err(|e| format!("Invalid session ID: {}", e))?;

    match qore_service::connection::disconnect(
        &session_manager,
        &query_rate_limiter,
        crate::engine::types::SessionId(uuid),
    )
    .await
    {
        Ok(()) => Ok(ConnectionResponse {
            success: true,
            session_id: None,
            error: None,
        }),
        Err(e) => Ok(ConnectionResponse {
            success: false,
            session_id: None,
            error: Some(e.sanitized()),
        }),
    }
}

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

/// Checks the health of an active connection (on-demand ping).
#[tauri::command]
pub async fn check_connection_health(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<String, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };

    let uuid = Uuid::parse_str(&session_id).map_err(|e| format!("Invalid session ID: {}", e))?;
    let sid = crate::engine::types::SessionId(uuid);

    match session_manager.ping(sid).await {
        Ok(()) => Ok("healthy".to_string()),
        Err(e) => Ok(format!("unhealthy: {}", e)),
    }
}

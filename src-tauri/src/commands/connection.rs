// SPDX-License-Identifier: Apache-2.0

//! Commands for managing database connections.

use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use tracing::instrument;
use uuid::Uuid;

use super::SharedStateExt;
use crate::commands::vault::get_workspace_store;
use crate::commands::workspace::SharedWorkspaceManager;
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

/// Resolves a saved connection to a ready-to-use config plus its display name.
///
/// File-based workspaces keep connections in their own `.qoredb/connections/`
/// directory, so isolation is by directory and the flat-vault `project_id` guard
/// does not apply there. The default workspace shares a single `connections.json`
/// across projects, so that branch still enforces the guard.
async fn resolve_saved_connection(
    app: &AppHandle,
    ws_manager: &State<'_, SharedWorkspaceManager>,
    project_id: &str,
    connection_id: &str,
) -> Result<(ConnectionConfig, String), String> {
    if let Some(ws_store) = get_workspace_store(ws_manager).await {
        let saved = ws_store
            .get_connection(connection_id)
            .map_err(|e| e.sanitized_message())?;
        let creds = ws_store
            .get_credentials(connection_id)
            .map_err(|e| e.sanitized_message())?;
        let name = saved.name.clone();
        let config = saved
            .to_connection_config(&creds)
            .map_err(|e| e.sanitized_message())?;
        return Ok((config, name));
    }

    let storage_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
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
    let session_manager = state.session_manager().await;

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
#[instrument(skip(app, state, ws_manager), fields(project_id = %project_id, connection_id = %connection_id))]
pub async fn test_saved_connection(
    app: AppHandle,
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
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

    let config =
        match resolve_saved_connection(&app, &ws_manager, &project_id, &connection_id).await {
            Ok((cfg, _name)) => cfg,
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

    let session_manager = state.session_manager().await;

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
#[instrument(skip(app, state, ws_manager), fields(project_id = %project_id, connection_id = %connection_id))]
pub async fn connect_saved_connection(
    app: AppHandle,
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
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

    let (config, connection_name) =
        match resolve_saved_connection(&app, &ws_manager, &project_id, &connection_id).await {
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
                .set_saved_connection_identity(session_id, connection_id.clone(), connection_name)
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
    let session_manager = state.session_manager().await;

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
    let session_manager = state.session_manager().await;

    let uuid = Uuid::parse_str(&session_id).map_err(|e| format!("Invalid session ID: {}", e))?;
    let sid = crate::engine::types::SessionId(uuid);

    match session_manager.ping(sid).await {
        Ok(()) => Ok("healthy".to_string()),
        Err(e) => Ok(format!("unhealthy: {}", e)),
    }
}

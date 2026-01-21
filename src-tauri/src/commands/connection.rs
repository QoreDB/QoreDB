//! Connection Tauri Commands
//!
//! Commands for managing database connections.

use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use std::sync::Arc;
use uuid::Uuid;
use tracing::instrument;

use crate::engine::types::{ConnectionConfig, SshAuth};
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
    storage_dir: PathBuf,
) -> Result<ConnectionConfig, String> {
    let storage = VaultStorage::new(project_id, storage_dir);
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


fn normalize_environment(env: &str) -> Result<String, String> {
    let normalized = env.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok("development".to_string());
    }

    match normalized.as_str() {
        "development" | "staging" | "production" => Ok(normalized),
        _ => Err(format!("Invalid environment: {}", env)),
    }
}

fn normalize_config(mut config: ConnectionConfig) -> Result<ConnectionConfig, String> {
    let driver = config.driver.trim();
    if driver.is_empty() {
        return Err("Driver is required".to_string());
    }
    config.driver = driver.to_string();

    let host = config.host.trim();
    if host.is_empty() {
        return Err("Host is required".to_string());
    }
    config.host = host.to_string();

    let username = config.username.trim();
    if username.is_empty() {
        return Err("Username is required".to_string());
    }
    config.username = username.to_string();

    if config.port == 0 {
        return Err("Port must be greater than 0".to_string());
    }

    if let Some(database) = config.database.take() {
        let trimmed = database.trim();
        if !trimmed.is_empty() {
            config.database = Some(trimmed.to_string());
        }
    }

    config.environment = normalize_environment(&config.environment)?;

    if let Some(ref mut ssh) = config.ssh_tunnel {
        let host = ssh.host.trim();
        if host.is_empty() {
            return Err("SSH host is required".to_string());
        }
        ssh.host = host.to_string();

        let username = ssh.username.trim();
        if username.is_empty() {
            return Err("SSH username is required".to_string());
        }
        ssh.username = username.to_string();

        if ssh.port == 0 {
            return Err("SSH port must be greater than 0".to_string());
        }

        match &mut ssh.auth {
            SshAuth::Password { password } => {
                if password.trim().is_empty() {
                    return Err("SSH password is required".to_string());
                }
            }
            SshAuth::Key {
                private_key_path, ..
            } => {
                if private_key_path.trim().is_empty() {
                    return Err("SSH key path is required".to_string());
                }
            }
        }
    }

    let max_connections = config.pool_max_connections.unwrap_or(5);
    if max_connections == 0 {
        return Err("Pool max connections must be greater than 0".to_string());
    }
    let min_connections = config.pool_min_connections.unwrap_or(0);
    if min_connections > max_connections {
        return Err("Pool min connections must be <= max connections".to_string());
    }
    let acquire_timeout = config.pool_acquire_timeout_secs.unwrap_or(30);
    if acquire_timeout < 5 {
        return Err("Pool acquire timeout must be at least 5 seconds".to_string());
    }

    config.pool_max_connections = Some(max_connections);
    config.pool_min_connections = Some(min_connections);
    config.pool_acquire_timeout_secs = Some(acquire_timeout);

    Ok(config)
}


/// Tests a database connection without persisting it
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

    let config = match normalize_config(config) {
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

/// Tests a saved connection using vault metadata + credentials
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

    let config = match load_saved_connection_config(&project_id, &connection_id, storage_dir)
        .and_then(normalize_config)
    {
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

    let config = match normalize_config(config) {
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

/// Establishes a new database connection from a saved connection
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

    let config = match load_saved_connection_config(&project_id, &connection_id, storage_dir)
        .and_then(normalize_config)
    {
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
#[instrument(skip(state), fields(session_id = %session_id))]
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

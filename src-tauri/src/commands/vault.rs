// SPDX-License-Identifier: Apache-2.0

//! Vault Tauri Commands
//!
//! Commands for managing saved connections and vault lock.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::commands::workspace::SharedWorkspaceManager;
use crate::observability::Sensitive;
use crate::vault::backend::KeyringProvider;
use crate::engine::types::MssqlAuthMode;
use crate::vault::credentials::{
    Environment, ProxyInfo, SavedConnection, SshTunnelInfo, StoredCredentials,
};
use crate::vault::storage::VaultStorage;
use crate::workspace::connection_store::WorkspaceConnectionStore;
use crate::workspace::types::WorkspaceSource;
use crate::SharedState;

/// Determines if the active workspace is file-based and returns its connection store.
/// Returns None if the default workspace is active (use VaultStorage instead).
async fn get_workspace_store(
    ws_manager: &State<'_, SharedWorkspaceManager>,
) -> Option<WorkspaceConnectionStore> {
    let mgr = ws_manager.lock().await;
    let ws = mgr.active();
    if ws.source == WorkspaceSource::Default {
        return None;
    }
    Some(WorkspaceConnectionStore::new(
        ws.path.join("connections"),
        format!("qoredb_{}", mgr.project_id()),
        Box::new(KeyringProvider::new()),
    ))
}

/// Response for vault operations
#[derive(Debug, Serialize)]
pub struct VaultResponse {
    pub success: bool,
    pub error: Option<String>,
}

/// Response for duplicating a saved connection
#[derive(Debug, Serialize)]
pub struct DuplicateConnectionResponse {
    pub success: bool,
    pub connection: Option<SavedConnection>,
    pub error: Option<String>,
}

/// Response for checking vault status
#[derive(Debug, Serialize)]
pub struct VaultStatusResponse {
    pub is_locked: bool,
    pub has_master_password: bool,
}

/// Input for saving a connection
#[derive(Debug, Deserialize)]
pub struct SaveConnectionInput {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub environment: Environment,
    pub read_only: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: Option<String>,
    pub ssl: bool,
    #[serde(default)]
    pub ssl_mode: Option<String>,
    pub pool_max_connections: Option<u32>,
    pub pool_min_connections: Option<u32>,
    pub pool_acquire_timeout_secs: Option<u32>,
    pub project_id: String,
    pub ssh_tunnel: Option<SshTunnelInput>,
    pub proxy: Option<ProxyInput>,
    #[serde(default)]
    pub mssql_auth: Option<MssqlAuthMode>,
}

#[derive(Debug, Deserialize)]
pub struct SshTunnelInput {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
    pub key_passphrase: Option<String>,

    pub host_key_policy: String,

    pub proxy_jump: Option<String>,

    pub connect_timeout_secs: u32,
    pub keepalive_interval_secs: u32,
    pub keepalive_count_max: u32,
}

#[derive(Debug, Deserialize)]
pub struct ProxyInput {
    pub proxy_type: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub connect_timeout_secs: u32,
}

/// Checks the vault lock status
#[tauri::command]
pub async fn get_vault_status(
    state: State<'_, SharedState>,
) -> Result<VaultStatusResponse, String> {
    let state = state.lock().await;

    let has_master_password = state
        .vault_lock
        .has_master_password()
        .map_err(|e| e.to_string())?;

    Ok(VaultStatusResponse {
        is_locked: state.vault_lock.is_locked(),
        has_master_password,
    })
}

/// Sets up a master password for the vault
#[tauri::command]
pub async fn setup_master_password(
    state: State<'_, SharedState>,
    password: String,
) -> Result<VaultResponse, String> {
    let mut state = state.lock().await;

    match state.vault_lock.setup_master_password(&password) {
        Ok(()) => Ok(VaultResponse {
            success: true,
            error: None,
        }),
        Err(e) => Ok(VaultResponse {
            success: false,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Unlocks the vault with the master password
#[tauri::command]
pub async fn unlock_vault(
    state: State<'_, SharedState>,
    password: String,
) -> Result<VaultResponse, String> {
    let mut state = state.lock().await;

    match state.vault_lock.unlock(&password) {
        Ok(true) => Ok(VaultResponse {
            success: true,
            error: None,
        }),
        Ok(false) => Ok(VaultResponse {
            success: false,
            error: Some("Invalid password".to_string()),
        }),
        Err(e) => Ok(VaultResponse {
            success: false,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Locks the vault
#[tauri::command]
pub async fn lock_vault(state: State<'_, SharedState>) -> Result<VaultResponse, String> {
    let mut state = state.lock().await;
    state.vault_lock.lock();

    Ok(VaultResponse {
        success: true,
        error: None,
    })
}

/// Saves a connection to the vault
#[tauri::command]
pub async fn save_connection(
    app: AppHandle,
    state: State<'_, SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    input: SaveConnectionInput,
) -> Result<VaultResponse, String> {
    let app_state = state.lock().await;

    if app_state.vault_lock.is_locked() {
        return Ok(VaultResponse {
            success: false,
            error: Some("Vault is locked".to_string()),
        });
    }
    drop(app_state);

    let input_project_id = input.project_id.clone();
    let ssh_tunnel = input.ssh_tunnel.as_ref().map(|ssh| SshTunnelInfo {
        host: ssh.host.clone(),
        port: ssh.port,
        username: ssh.username.clone(),
        auth_type: ssh.auth_type.clone(),
        key_path: ssh.key_path.clone(),
        host_key_policy: ssh.host_key_policy.clone(),
        proxy_jump: ssh.proxy_jump.clone(),
        connect_timeout_secs: ssh.connect_timeout_secs,
        keepalive_interval_secs: ssh.keepalive_interval_secs,
        keepalive_count_max: ssh.keepalive_count_max,
    });

    let proxy = input.proxy.as_ref().map(|p| ProxyInfo {
        proxy_type: p.proxy_type.clone(),
        host: p.host.clone(),
        port: p.port,
        username: p.username.clone(),
        connect_timeout_secs: p.connect_timeout_secs,
    });

    let connection = SavedConnection {
        id: input.id.clone(),
        name: input.name,
        driver: input.driver,
        environment: input.environment,
        read_only: input.read_only,
        host: input.host,
        port: input.port,
        username: input.username,
        database: input.database,
        ssl: input.ssl,
        ssl_mode: input.ssl_mode,
        pool_max_connections: input.pool_max_connections,
        pool_min_connections: input.pool_min_connections,
        pool_acquire_timeout_secs: input.pool_acquire_timeout_secs,
        ssh_tunnel,
        proxy,
        mssql_auth: input.mssql_auth,
        project_id: input.project_id,
    };

    let credentials = StoredCredentials {
        db_password: Sensitive::new(input.password),
        ssh_password: input
            .ssh_tunnel
            .as_ref()
            .and_then(|s| s.password.clone().map(Sensitive::new)),
        ssh_key_passphrase: input
            .ssh_tunnel
            .as_ref()
            .and_then(|s| s.key_passphrase.clone().map(Sensitive::new)),
        proxy_password: input
            .proxy
            .as_ref()
            .and_then(|p| p.password.clone().map(Sensitive::new)),
    };

    // Route to workspace connection store if a file-based workspace is active
    let result = if let Some(ws_store) = get_workspace_store(&ws_manager).await {
        ws_store.save_connection(&connection, &credentials)
    } else {
        let storage_dir = app.path().app_config_dir().map_err(|e: tauri::Error| e.to_string())?;
        let storage = VaultStorage::new(
            &input_project_id,
            storage_dir,
            Box::new(KeyringProvider::new()),
        );
        storage.save_connection(&connection, &credentials)
    };

    match result {
        Ok(()) => Ok(VaultResponse {
            success: true,
            error: None,
        }),
        Err(e) => Ok(VaultResponse {
            success: false,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Lists all saved connections (metadata only, no passwords)
#[tauri::command]
pub async fn list_saved_connections(
    app: AppHandle,
    state: State<'_, SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    project_id: String,
) -> Result<Vec<SavedConnection>, String> {
    let state = state.lock().await;

    if state.vault_lock.is_locked() {
        return Err("Vault is locked".to_string());
    }
    drop(state);

    // Route to workspace connection store if a file-based workspace is active
    if let Some(ws_store) = get_workspace_store(&ws_manager).await {
        return ws_store.list_connections().map_err(|e| e.to_string());
    }

    let storage_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let storage = VaultStorage::new(&project_id, storage_dir, Box::new(KeyringProvider::new()));

    storage.list_connections_full().map_err(|e| e.to_string())
}

/// Deletes a saved connection
#[tauri::command]
pub async fn delete_saved_connection(
    app: AppHandle,
    state: State<'_, SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    project_id: String,
    connection_id: String,
) -> Result<VaultResponse, String> {
    let app_state = state.lock().await;

    if app_state.vault_lock.is_locked() {
        return Ok(VaultResponse {
            success: false,
            error: Some("Vault is locked".to_string()),
        });
    }
    drop(app_state);

    let result = if let Some(ws_store) = get_workspace_store(&ws_manager).await {
        ws_store.delete_connection(&connection_id)
    } else {
        let storage_dir = app.path().app_config_dir().map_err(|e: tauri::Error| e.to_string())?;
        let storage = VaultStorage::new(&project_id, storage_dir, Box::new(KeyringProvider::new()));
        storage.delete_connection(&connection_id)
    };

    match result {
        Ok(()) => Ok(VaultResponse {
            success: true,
            error: None,
        }),
        Err(e) => Ok(VaultResponse {
            success: false,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Duplicates a saved connection (metadata + secrets) entirely within the vault.
#[tauri::command]
pub async fn duplicate_saved_connection(
    app: AppHandle,
    state: State<'_, SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    project_id: String,
    connection_id: String,
) -> Result<DuplicateConnectionResponse, String> {
    let app_state = state.lock().await;

    if app_state.vault_lock.is_locked() {
        return Ok(DuplicateConnectionResponse {
            success: false,
            connection: None,
            error: Some("Vault is locked".to_string()),
        });
    }
    drop(app_state);

    let result = if let Some(ws_store) = get_workspace_store(&ws_manager).await {
        ws_store.duplicate_connection(&connection_id)
    } else {
        let storage_dir = app.path().app_config_dir().map_err(|e: tauri::Error| e.to_string())?;
        let storage = VaultStorage::new(&project_id, storage_dir, Box::new(KeyringProvider::new()));
        storage.duplicate_connection(&connection_id)
    };

    match result {
        Ok(connection) => Ok(DuplicateConnectionResponse {
            success: true,
            connection: Some(connection),
            error: None,
        }),
        Err(e) => Ok(DuplicateConnectionResponse {
            success: false,
            connection: None,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Response for getting credentials
#[derive(Debug, Serialize)]
pub struct CredentialsResponse {
    pub success: bool,
    pub password: Option<String>,
    pub error: Option<String>,
}

/// Gets the password for a saved connection
#[tauri::command]
pub async fn get_connection_credentials(
    app: AppHandle,
    state: State<'_, SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    project_id: String,
    connection_id: String,
) -> Result<CredentialsResponse, String> {
    let app_state = state.lock().await;

    if app_state.vault_lock.is_locked() {
        return Ok(CredentialsResponse {
            success: false,
            password: None,
            error: Some("Vault is locked".to_string()),
        });
    }
    drop(app_state);

    let result = if let Some(ws_store) = get_workspace_store(&ws_manager).await {
        ws_store.get_credentials(&connection_id)
    } else {
        let storage_dir = app.path().app_config_dir().map_err(|e: tauri::Error| e.to_string())?;
        let storage = VaultStorage::new(&project_id, storage_dir, Box::new(KeyringProvider::new()));
        storage.get_credentials(&connection_id)
    };

    match result {
        Ok(creds) => Ok(CredentialsResponse {
            success: true,
            password: Some(creds.db_password.expose().clone()),
            error: None,
        }),
        Err(e) => Ok(CredentialsResponse {
            success: false,
            password: None,
            error: Some(e.sanitized_message()),
        }),
    }
}

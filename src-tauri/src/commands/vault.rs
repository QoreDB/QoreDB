//! Vault Tauri Commands
//!
//! Commands for managing saved connections and vault lock.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::vault::credentials::{Environment, SavedConnection, SshTunnelInfo, StoredCredentials};
use crate::vault::storage::VaultStorage;
use crate::SharedState;

/// Response for vault operations
#[derive(Debug, Serialize)]
pub struct VaultResponse {
    pub success: bool,
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
    pub pool_max_connections: Option<u32>,
    pub pool_min_connections: Option<u32>,
    pub pool_acquire_timeout_secs: Option<u32>,
    pub project_id: String,
    pub ssh_tunnel: Option<SshTunnelInput>,
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

/// Checks the vault lock status
#[tauri::command]
pub async fn get_vault_status(
    state: State<'_, SharedState>,
) -> Result<VaultStatusResponse, String> {
    let state = state.lock().await;

    let has_master_password = crate::vault::VaultLock::has_master_password()
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
            error: Some(e.to_string()),
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
            error: Some(e.to_string()),
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
    state: State<'_, SharedState>,
    input: SaveConnectionInput,
) -> Result<VaultResponse, String> {
    let state = state.lock().await;

    if state.vault_lock.is_locked() {
        return Ok(VaultResponse {
            success: false,
            error: Some("Vault is locked".to_string()),
        });
    }

    let storage = VaultStorage::new(&input.project_id);

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
        pool_max_connections: input.pool_max_connections,
        pool_min_connections: input.pool_min_connections,
        pool_acquire_timeout_secs: input.pool_acquire_timeout_secs,
        ssh_tunnel,
        project_id: input.project_id,
    };

    let credentials = StoredCredentials {
        db_password: input.password,
        ssh_password: input.ssh_tunnel.as_ref().and_then(|s| s.password.clone()),
        ssh_key_passphrase: input.ssh_tunnel.as_ref().and_then(|s| s.key_passphrase.clone()),
    };

    match storage.save_connection(&connection, &credentials) {
        Ok(()) => Ok(VaultResponse {
            success: true,
            error: None,
        }),
        Err(e) => Ok(VaultResponse {
            success: false,
            error: Some(e.to_string()),
        }),
    }
}

/// Lists all saved connections (metadata only, no passwords)
#[tauri::command]
pub async fn list_saved_connections(
    state: State<'_, SharedState>,
    project_id: String,
) -> Result<Vec<SavedConnection>, String> {
    let state = state.lock().await;

    if state.vault_lock.is_locked() {
        return Err("Vault is locked".to_string());
    }

    let storage = VaultStorage::new(&project_id);

    storage
        .list_connections_full()
        .map_err(|e| e.to_string())
}

/// Deletes a saved connection
#[tauri::command]
pub async fn delete_saved_connection(
    state: State<'_, SharedState>,
    project_id: String,
    connection_id: String,
) -> Result<VaultResponse, String> {
    let state = state.lock().await;

    if state.vault_lock.is_locked() {
        return Ok(VaultResponse {
            success: false,
            error: Some("Vault is locked".to_string()),
        });
    }

    let storage = VaultStorage::new(&project_id);

    match storage.delete_connection(&connection_id) {
        Ok(()) => Ok(VaultResponse {
            success: true,
            error: None,
        }),
        Err(e) => Ok(VaultResponse {
            success: false,
            error: Some(e.to_string()),
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
    state: State<'_, SharedState>,
    project_id: String,
    connection_id: String,
) -> Result<CredentialsResponse, String> {
    let state = state.lock().await;

    if state.vault_lock.is_locked() {
        return Ok(CredentialsResponse {
            success: false,
            password: None,
            error: Some("Vault is locked".to_string()),
        });
    }

    let storage = VaultStorage::new(&project_id);

    match storage.get_credentials(&connection_id) {
        Ok(creds) => Ok(CredentialsResponse {
            success: true,
            password: Some(creds.db_password),
            error: None,
        }),
        Err(e) => Ok(CredentialsResponse {
            success: false,
            password: None,
            error: Some(e.to_string()),
        }),
    }
}

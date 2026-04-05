// SPDX-License-Identifier: Apache-2.0

//! Workspace Tauri Commands
//!
//! Commands for managing workspace lifecycle: detection, creation, switching.

use serde::Serialize;
use std::path::PathBuf;
use tauri::{Manager, State};

use crate::workspace::types::{RecentWorkspace, WorkspaceInfo, WorkspaceSource};
use crate::workspace::WorkspaceManager;

pub type SharedWorkspaceManager = std::sync::Arc<tokio::sync::Mutex<WorkspaceManager>>;
pub type WatcherPathSender =
    std::sync::Arc<tokio::sync::watch::Sender<Option<std::path::PathBuf>>>;

#[derive(Debug, Serialize)]
pub struct WorkspaceResponse {
    pub success: bool,
    pub workspace: Option<WorkspaceInfo>,
    pub error: Option<String>,
}

/// Detects a workspace from the current working directory.
#[tauri::command]
pub async fn detect_workspace(
    ws_manager: State<'_, SharedWorkspaceManager>,
    ws_path_tx: State<'_, WatcherPathSender>,
) -> Result<Option<WorkspaceInfo>, String> {
    let mut mgr = ws_manager.lock().await;
    let result = mgr.detect_and_activate();
    if let Some(ref info) = result {
        let _ = ws_path_tx.send(Some(info.path.clone()));
    }
    Ok(result)
}

/// Returns the currently active workspace.
#[tauri::command]
pub async fn get_active_workspace(
    ws_manager: State<'_, SharedWorkspaceManager>,
) -> Result<WorkspaceInfo, String> {
    let mgr = ws_manager.lock().await;
    Ok(mgr.active().clone())
}

/// Returns the project ID of the active workspace.
#[tauri::command]
pub async fn get_workspace_project_id(
    ws_manager: State<'_, SharedWorkspaceManager>,
) -> Result<String, String> {
    let mgr = ws_manager.lock().await;
    Ok(mgr.project_id())
}

/// Creates a new workspace at the given project directory.
#[tauri::command]
pub async fn create_workspace(
    ws_manager: State<'_, SharedWorkspaceManager>,
    ws_path_tx: State<'_, WatcherPathSender>,
    project_dir: String,
    name: String,
) -> Result<WorkspaceResponse, String> {
    let mut mgr = ws_manager.lock().await;
    match mgr.create_workspace(&PathBuf::from(&project_dir), &name) {
        Ok(info) => {
            let _ = ws_path_tx.send(Some(info.path.clone()));
            Ok(WorkspaceResponse {
                success: true,
                workspace: Some(info),
                error: None,
            })
        }
        Err(e) => Ok(WorkspaceResponse {
            success: false,
            workspace: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Opens an existing workspace at the given `.qoredb/` path.
#[tauri::command]
pub async fn open_workspace(
    ws_manager: State<'_, SharedWorkspaceManager>,
    ws_path_tx: State<'_, WatcherPathSender>,
    qoredb_path: String,
) -> Result<WorkspaceResponse, String> {
    let mut mgr = ws_manager.lock().await;
    match mgr.switch_to(&PathBuf::from(&qoredb_path), WorkspaceSource::Manual) {
        Ok(info) => {
            let _ = ws_path_tx.send(Some(info.path.clone()));
            Ok(WorkspaceResponse {
                success: true,
                workspace: Some(info),
                error: None,
            })
        }
        Err(e) => Ok(WorkspaceResponse {
            success: false,
            workspace: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Switches to an existing workspace.
#[tauri::command]
pub async fn switch_workspace(
    ws_manager: State<'_, SharedWorkspaceManager>,
    ws_path_tx: State<'_, WatcherPathSender>,
    qoredb_path: String,
) -> Result<WorkspaceResponse, String> {
    let mut mgr = ws_manager.lock().await;
    match mgr.switch_to(&PathBuf::from(&qoredb_path), WorkspaceSource::Manual) {
        Ok(info) => {
            let _ = ws_path_tx.send(Some(info.path.clone()));
            Ok(WorkspaceResponse {
                success: true,
                workspace: Some(info),
                error: None,
            })
        }
        Err(e) => Ok(WorkspaceResponse {
            success: false,
            workspace: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Renames the active workspace.
#[tauri::command]
pub async fn rename_workspace(
    ws_manager: State<'_, SharedWorkspaceManager>,
    new_name: String,
) -> Result<WorkspaceResponse, String> {
    let mut mgr = ws_manager.lock().await;
    match mgr.rename_workspace(&new_name) {
        Ok(info) => Ok(WorkspaceResponse {
            success: true,
            workspace: Some(info),
            error: None,
        }),
        Err(e) => Ok(WorkspaceResponse {
            success: false,
            workspace: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Switches back to the default workspace.
#[tauri::command]
pub async fn switch_to_default_workspace(
    ws_manager: State<'_, SharedWorkspaceManager>,
    ws_path_tx: State<'_, WatcherPathSender>,
) -> Result<WorkspaceInfo, String> {
    let mut mgr = ws_manager.lock().await;
    let _ = ws_path_tx.send(None);
    Ok(mgr.switch_to_default())
}

/// Lists recently opened workspaces.
#[tauri::command]
pub async fn list_recent_workspaces(
    ws_manager: State<'_, SharedWorkspaceManager>,
) -> Result<Vec<RecentWorkspace>, String> {
    let mgr = ws_manager.lock().await;
    Ok(mgr.list_recent())
}

/// Imports connections from the default vault into the active file-based workspace.
/// Copies metadata files into `.qoredb/connections/` and credentials into the workspace keyring.
/// Returns the number of connections imported.
#[tauri::command]
pub async fn import_default_connections(
    app: tauri::AppHandle,
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
) -> Result<u32, String> {
    use crate::vault::backend::KeyringProvider;
    use crate::vault::storage::VaultStorage;
    use crate::workspace::connection_store::WorkspaceConnectionStore;

    let app_state = state.lock().await;
    if app_state.vault_lock.is_locked() {
        return Err("Vault is locked".to_string());
    }
    drop(app_state);

    let mgr = ws_manager.lock().await;
    let ws = mgr.active();
    if ws.source == WorkspaceSource::Default {
        return Err("Cannot import into the default workspace".to_string());
    }

    let storage_dir = app
        .path()
        .app_config_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    let default_vault = VaultStorage::new("default", storage_dir, Box::new(KeyringProvider::new()));

    let connections = default_vault
        .list_connections_full()
        .map_err(|e| e.to_string())?;

    let ws_store = WorkspaceConnectionStore::new(
        ws.path.join("connections"),
        format!("qoredb_{}", mgr.project_id()),
        Box::new(KeyringProvider::new()),
    );

    let mut imported = 0u32;
    for conn in &connections {
        // Skip if credentials are missing (deleted/corrupted)
        let creds = match default_vault.get_credentials(&conn.id) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if ws_store.save_connection(conn, &creds).is_ok() {
            imported += 1;
        }
    }

    Ok(imported)
}

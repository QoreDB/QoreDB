// SPDX-License-Identifier: Apache-2.0

//! Workspace Tauri Commands
//!
//! Commands for managing workspace lifecycle: detection, creation, switching.

use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

use crate::workspace::types::{RecentWorkspace, WorkspaceInfo, WorkspaceSource};
use crate::workspace::WorkspaceManager;

pub type SharedWorkspaceManager = std::sync::Arc<tokio::sync::Mutex<WorkspaceManager>>;

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
) -> Result<Option<WorkspaceInfo>, String> {
    let mut mgr = ws_manager.lock().await;
    Ok(mgr.detect_and_activate())
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
    project_dir: String,
    name: String,
) -> Result<WorkspaceResponse, String> {
    let mut mgr = ws_manager.lock().await;
    match mgr.create_workspace(&PathBuf::from(&project_dir), &name) {
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

/// Opens an existing workspace at the given `.qoredb/` path.
#[tauri::command]
pub async fn open_workspace(
    ws_manager: State<'_, SharedWorkspaceManager>,
    qoredb_path: String,
) -> Result<WorkspaceResponse, String> {
    let mut mgr = ws_manager.lock().await;
    match mgr.switch_to(&PathBuf::from(&qoredb_path), WorkspaceSource::Manual) {
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

/// Switches to an existing workspace.
#[tauri::command]
pub async fn switch_workspace(
    ws_manager: State<'_, SharedWorkspaceManager>,
    qoredb_path: String,
) -> Result<WorkspaceResponse, String> {
    let mut mgr = ws_manager.lock().await;
    match mgr.switch_to(&PathBuf::from(&qoredb_path), WorkspaceSource::Manual) {
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
) -> Result<WorkspaceInfo, String> {
    let mut mgr = ws_manager.lock().await;
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

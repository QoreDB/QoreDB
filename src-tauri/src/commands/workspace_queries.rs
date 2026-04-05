// SPDX-License-Identifier: Apache-2.0

//! Workspace Query Library Commands
//!
//! Read/write the query library stored in `.qoredb/queries/library.json`.

use serde::{Deserialize, Serialize};
use std::fs;
use tauri::State;

use crate::commands::workspace::SharedWorkspaceManager;
use crate::engine::error::EngineError;
use crate::workspace::types::WorkspaceSource;
use crate::workspace::write_registry::WriteRegistry;

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceQueryLibrary {
    pub version: u32,
    pub folders: Vec<serde_json::Value>,
    pub items: Vec<serde_json::Value>,
}

/// Gets the query library from the active workspace.
/// Returns None if the workspace is the default (uses localStorage instead).
#[tauri::command]
pub async fn ws_get_query_library(
    ws_manager: State<'_, SharedWorkspaceManager>,
) -> Result<Option<WorkspaceQueryLibrary>, String> {
    let mgr = ws_manager.lock().await;
    let ws = mgr.active();

    if ws.source == WorkspaceSource::Default {
        return Ok(None);
    }

    let library_path = ws.path.join("queries").join("library.json");
    if !library_path.exists() {
        return Ok(Some(WorkspaceQueryLibrary {
            version: 1,
            folders: Vec::new(),
            items: Vec::new(),
        }));
    }

    let content = fs::read_to_string(&library_path)
        .map_err(|e| EngineError::internal(format!("Failed to read library: {}", e)).to_string())?;

    let library: WorkspaceQueryLibrary = serde_json::from_str(&content)
        .map_err(|e| EngineError::internal(format!("Invalid library format: {}", e)).to_string())?;

    Ok(Some(library))
}

/// Saves the query library to the active workspace.
/// Does nothing if the workspace is the default.
#[tauri::command]
pub async fn ws_save_query_library(
    ws_manager: State<'_, SharedWorkspaceManager>,
    write_registry: State<'_, WriteRegistry>,
    library: WorkspaceQueryLibrary,
) -> Result<bool, String> {
    let mgr = ws_manager.lock().await;
    let ws = mgr.active();

    if ws.source == WorkspaceSource::Default {
        return Ok(false);
    }

    let queries_dir = ws.path.join("queries");
    fs::create_dir_all(&queries_dir)
        .map_err(|e| EngineError::internal(format!("Failed to create queries dir: {}", e)).to_string())?;

    let content = serde_json::to_string_pretty(&library)
        .map_err(|e| EngineError::internal(format!("Serialization error: {}", e)).to_string())?;

    let library_path = queries_dir.join("library.json");
    write_registry.register_with_auto_unregister(library_path.clone());
    fs::write(&library_path, content)
        .map_err(|e| EngineError::internal(format!("Failed to write library: {}", e)).to_string())?;

    Ok(true)
}

// SPDX-License-Identifier: Apache-2.0

//! Read/write the query library stored in `.qoredb/queries/library.json`.

use std::fs;
use tauri::State;

use qore_service::workspace::query_library::{self, WorkspaceQueryLibrary};

use crate::commands::workspace::SharedWorkspaceManager;
use crate::engine::error::EngineError;
use crate::workspace::types::WorkspaceSource;
use crate::workspace::write_registry::WriteRegistry;

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

    query_library::read(&ws.path)
        .map(Some)
        .map_err(|e| EngineError::internal(e).to_string())
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
    fs::create_dir_all(&queries_dir).map_err(|e| {
        EngineError::internal(format!("Failed to create queries dir: {}", e)).to_string()
    })?;

    let content = serde_json::to_string_pretty(&library)
        .map_err(|e| EngineError::internal(format!("Serialization error: {}", e)).to_string())?;

    let library_path = queries_dir.join("library.json");
    write_registry.register_with_auto_unregister(library_path.clone());
    fs::write(&library_path, content).map_err(|e| {
        EngineError::internal(format!("Failed to write library: {}", e)).to_string()
    })?;

    Ok(true)
}

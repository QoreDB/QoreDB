// SPDX-License-Identifier: Apache-2.0

//! Read/write the query library stored in `.qoredb/queries/library.json`.

use tauri::State;

use qore_service::workspace::query_library::{self, WorkspaceQueryLibrary};

use crate::commands::workspace::SharedWorkspaceManager;
use crate::engine::error::EngineError;
use crate::workspace::WorkspaceManager;
use crate::workspace::types::WorkspaceSource;
use crate::workspace::write_registry::WriteRegistry;

/// Gets the query library from the active workspace.
/// Returns None if the workspace is the default (uses localStorage instead).
#[tauri::command]
pub async fn ws_get_query_library(
    ws_manager: State<'_, SharedWorkspaceManager>,
    project_id: String,
) -> Result<Option<WorkspaceQueryLibrary>, String> {
    let mgr = ws_manager.lock().await;
    read_library(&mgr, &project_id)
}

fn read_library(
    mgr: &WorkspaceManager,
    project_id: &str,
) -> Result<Option<WorkspaceQueryLibrary>, String> {
    require_project(mgr, project_id)?;
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
    project_id: String,
) -> Result<bool, String> {
    let mgr = ws_manager.lock().await;
    save_library(&mgr, &write_registry, &library, &project_id)
}

fn require_project(mgr: &WorkspaceManager, project_id: &str) -> Result<(), String> {
    if mgr.project_id() != project_id {
        return Err("The query library workspace is no longer active".to_string());
    }
    Ok(())
}

fn save_library(
    mgr: &WorkspaceManager,
    write_registry: &WriteRegistry,
    library: &WorkspaceQueryLibrary,
    project_id: &str,
) -> Result<bool, String> {
    // The manager remains locked from identity validation through the file IO.
    require_project(mgr, project_id)?;
    let ws = mgr.active();

    if ws.source == WorkspaceSource::Default {
        return Ok(false);
    }

    write_registry.register_with_auto_unregister(ws.path.join("queries/library.json"));
    query_library::write(&ws.path, library).map_err(|e| EngineError::internal(e).to_string())?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn stale_library_requests_cannot_read_or_overwrite_another_workspace() {
        let config = TempDir::new().unwrap();
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        let mut mgr = WorkspaceManager::new(config.path().to_path_buf());
        let registry = WriteRegistry::new();
        mgr.create_workspace(a.path(), "A").unwrap();
        let project_a = mgr.project_id();
        let library = WorkspaceQueryLibrary {
            version: 1,
            folders: vec![json!({"id":"folder", "name":"Archives"})],
            items: vec![
                json!({"id":"query", "query":"SELECT 9007199254740993", "tags":["legacy"]}),
            ],
        };
        assert!(save_library(&mgr, &registry, &library, &project_a).unwrap());
        let path_a = mgr.active().path.join("queries/library.json");
        let saved_a = fs::read(&path_a).unwrap();
        mgr.create_workspace(b.path(), "B").unwrap();
        let path_b = mgr.active().path.join("queries/library.json");
        let saved_b = fs::read(&path_b).unwrap();

        assert!(read_library(&mgr, &project_a).is_err());
        assert!(save_library(&mgr, &registry, &library, &project_a).is_err());
        assert_eq!(fs::read(&path_a).unwrap(), saved_a);
        assert_eq!(fs::read(&path_b).unwrap(), saved_b);
        assert!(
            read_library(&mgr, &mgr.project_id())
                .unwrap()
                .unwrap()
                .items
                .is_empty()
        );

        mgr.switch_to(&a.path().join(".qoredb"), WorkspaceSource::Manual)
            .unwrap();
        let reopened = read_library(&mgr, &project_a).unwrap().unwrap();
        assert_eq!(reopened.items, library.items);
        assert_eq!(reopened.folders, library.folders);
        mgr.switch_to_default();
        assert!(save_library(&mgr, &registry, &library, &project_a).is_err());
        assert!(read_library(&mgr, &project_a).is_err());
        assert!(!save_library(&mgr, &registry, &library, "default").unwrap());
        assert!(read_library(&mgr, "default").unwrap().is_none());
    }
}

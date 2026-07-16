// SPDX-License-Identifier: BUSL-1.1

//! Read/write schema baselines stored in `.qoredb/baselines/`.
//!
//! A baseline is a structured snapshot of a connection's schema, used as the
//! reference for drift detection and migration generation. One JSON file per
//! connection (`<connection_id>.json`), holding a map of database -> snapshot.
//! Baselines are a local drift reference (git-ignored by default), not applied
//! state — applied migrations live in the target database.

use std::fs;
use std::path::Path;
use tauri::State;

use crate::commands::workspace::SharedWorkspaceManager;
use crate::engine::error::EngineError;
use crate::workspace::types::WorkspaceSource;
use crate::workspace::write_registry::WriteRegistry;

const BASELINES_DIR: &str = "baselines";
/// Guards against absurdly large payloads landing on disk (structural metadata
/// only; a few thousand tables stay well under this).
const MAX_BASELINE_BYTES: usize = 32 * 1024 * 1024;

/// Validates that a connection id is safe to use as a baseline filename stem.
/// Rejects path traversal and enforces `[A-Za-z0-9_-]`.
pub(crate) fn validate_baseline_id(connection_id: &str) -> Result<(), String> {
    if connection_id.is_empty() {
        return Err(EngineError::internal("Connection id cannot be empty").to_string());
    }
    if connection_id.contains("..")
        || connection_id.contains('/')
        || connection_id.contains('\\')
        || connection_id.contains('\0')
    {
        return Err(EngineError::internal("Connection id contains invalid characters").to_string());
    }
    if !connection_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(EngineError::internal(
            "Connection id must only contain alphanumeric characters, underscores, or hyphens",
        )
        .to_string());
    }
    Ok(())
}

fn baseline_path(ws_path: &Path, connection_id: &str) -> Result<std::path::PathBuf, String> {
    validate_baseline_id(connection_id)?;
    Ok(ws_path
        .join(BASELINES_DIR)
        .join(format!("{}.json", connection_id)))
}

/// Reads the raw baseline file for a connection.
/// Returns None if the workspace is the default or no baseline exists yet.
#[tauri::command]
pub async fn ws_read_baseline(
    ws_manager: State<'_, SharedWorkspaceManager>,
    connection_id: String,
) -> Result<Option<String>, String> {
    let mgr = ws_manager.lock().await;
    let ws = mgr.active();

    if ws.source == WorkspaceSource::Default {
        return Ok(None);
    }

    let path = baseline_path(&ws.path, &connection_id)?;
    match fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(EngineError::internal(format!("Failed to read baseline: {}", e)).to_string()),
    }
}

/// Writes (creates or overwrites) the baseline file for a connection.
/// Does nothing if the workspace is the default.
#[tauri::command]
pub async fn ws_write_baseline(
    ws_manager: State<'_, SharedWorkspaceManager>,
    write_registry: State<'_, WriteRegistry>,
    connection_id: String,
    content: String,
) -> Result<bool, String> {
    let mgr = ws_manager.lock().await;
    let ws = mgr.active();

    if ws.source == WorkspaceSource::Default {
        return Ok(false);
    }

    if content.len() > MAX_BASELINE_BYTES {
        return Err(EngineError::internal("Baseline payload is too large").to_string());
    }

    let path = baseline_path(&ws.path, &connection_id)?;
    let dir = ws.path.join(BASELINES_DIR);
    fs::create_dir_all(&dir).map_err(|e| {
        EngineError::internal(format!("Failed to create baselines dir: {}", e)).to_string()
    })?;

    write_registry.register_with_auto_unregister(path.clone());
    fs::write(&path, content).map_err(|e| {
        EngineError::internal(format!("Failed to write baseline: {}", e)).to_string()
    })?;

    Ok(true)
}

/// Deletes the baseline file for a connection.
/// Does nothing if the workspace is the default or the file is absent.
#[tauri::command]
pub async fn ws_delete_baseline(
    ws_manager: State<'_, SharedWorkspaceManager>,
    write_registry: State<'_, WriteRegistry>,
    connection_id: String,
) -> Result<bool, String> {
    let mgr = ws_manager.lock().await;
    let ws = mgr.active();

    if ws.source == WorkspaceSource::Default {
        return Ok(false);
    }

    let path = baseline_path(&ws.path, &connection_id)?;
    if !path.exists() {
        return Ok(false);
    }

    write_registry.register_with_auto_unregister(path.clone());
    fs::remove_file(&path).map_err(|e| {
        EngineError::internal(format!("Failed to delete baseline: {}", e)).to_string()
    })?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_ids() {
        assert!(validate_baseline_id("conn_abc123").is_ok());
        assert!(validate_baseline_id("a-b-c").is_ok());
        assert!(validate_baseline_id("").is_err());
        assert!(validate_baseline_id("../escape").is_err());
        assert!(validate_baseline_id("a/b").is_err());
        assert!(validate_baseline_id("a.b").is_err());
        assert!(validate_baseline_id("a b").is_err());
    }

    #[test]
    fn builds_path_under_baselines_dir() {
        let p = baseline_path(Path::new("/ws"), "conn1").unwrap();
        assert!(p.ends_with("baselines/conn1.json"));
    }
}

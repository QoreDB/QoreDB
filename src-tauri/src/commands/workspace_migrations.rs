// SPDX-License-Identifier: Apache-2.0

//! Read/write schema migration files stored in `.qoredb/migrations/`.
//!
//! Each migration is a single portable `.sql` file (`<version>_<slug>.sql`)
//! holding `-- migrate:up` / `-- migrate:down` sections, diffable in Git and
//! readable by ecosystem tools. Applied state lives in the target database,
//! not here.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tauri::State;

use crate::commands::workspace::SharedWorkspaceManager;
use crate::engine::error::EngineError;
use crate::workspace::types::WorkspaceSource;
use crate::workspace::write_registry::WriteRegistry;

#[derive(Debug, Serialize, Deserialize)]
pub struct MigrationSummary {
    pub version: String,
    pub name: String,
    pub filename: String,
}

const UP_MARKER: &str = "-- migrate:up";
const DOWN_MARKER: &str = "-- migrate:down";

/// Validates that a filename is a safe migration file name.
/// Rejects path traversal and enforces the `<stem>.sql` shape.
pub(crate) fn validate_migration_filename(filename: &str) -> Result<(), String> {
    if filename.is_empty() {
        return Err(EngineError::internal("Migration filename cannot be empty").to_string());
    }
    if filename.contains("..")
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains('\0')
    {
        return Err(
            EngineError::internal("Migration filename contains invalid characters").to_string(),
        );
    }
    let stem = filename.strip_suffix(".sql").ok_or_else(|| {
        EngineError::internal("Migration filename must end with .sql").to_string()
    })?;
    if stem.is_empty()
        || !stem
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(EngineError::internal(
            "Migration name must only contain alphanumeric characters, underscores, or hyphens",
        )
        .to_string());
    }
    Ok(())
}

/// Splits `<version>_<slug>.sql` into its version prefix and human name.
pub(crate) fn summarize(filename: &str) -> MigrationSummary {
    let stem = filename.strip_suffix(".sql").unwrap_or(filename);
    let (version, name) = match stem.split_once('_') {
        Some((v, rest)) => (v.to_string(), rest.to_string()),
        None => (stem.to_string(), stem.to_string()),
    };
    MigrationSummary {
        version,
        name,
        filename: filename.to_string(),
    }
}

/// Reads a migration file's raw content from a workspace `migrations/` dir.
pub(crate) fn read_migration_file(ws_path: &Path, filename: &str) -> Result<String, String> {
    validate_migration_filename(filename)?;
    let path = ws_path.join("migrations").join(filename);
    fs::read_to_string(&path)
        .map_err(|e| EngineError::internal(format!("Failed to read migration: {}", e)).to_string())
}

/// Splits a migration file body into its `up` and `down` scripts.
/// Mirrors the frontend `parseMigration` (dbmate `-- migrate:up/down` format).
pub(crate) fn split_up_down(content: &str) -> (String, String) {
    let up_idx = content.find(UP_MARKER);
    let down_idx = content.find(DOWN_MARKER);
    if up_idx.is_none() && down_idx.is_none() {
        return (content.trim().to_string(), String::new());
    }
    let up = match up_idx {
        None => String::new(),
        Some(u) => {
            let start = u + UP_MARKER.len();
            let end = match down_idx {
                Some(d) if d >= start => d,
                _ => content.len(),
            };
            content[start..end].trim().to_string()
        }
    };
    let down = match down_idx {
        None => String::new(),
        Some(d) => content[d + DOWN_MARKER.len()..].trim().to_string(),
    };
    (up, down)
}

/// Lists the migrations of the active workspace, sorted by version.
/// Returns None if the workspace is the default (migrations are file-based only).
#[tauri::command]
pub async fn ws_list_migrations(
    ws_manager: State<'_, SharedWorkspaceManager>,
) -> Result<Option<Vec<MigrationSummary>>, String> {
    let mgr = ws_manager.lock().await;
    let ws = mgr.active();

    if ws.source == WorkspaceSource::Default {
        return Ok(None);
    }

    let dir = ws.path.join("migrations");
    if !dir.exists() {
        return Ok(Some(Vec::new()));
    }

    let mut filenames: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| {
            EngineError::internal(format!("Failed to read migrations: {}", e)).to_string()
        })?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| Path::new(name).extension().and_then(|e| e.to_str()) == Some("sql"))
        .collect();
    filenames.sort();

    Ok(Some(filenames.iter().map(|f| summarize(f)).collect()))
}

/// Reads the raw content of a migration file.
/// Returns None if the workspace is the default.
#[tauri::command]
pub async fn ws_read_migration(
    ws_manager: State<'_, SharedWorkspaceManager>,
    filename: String,
) -> Result<Option<String>, String> {
    let mgr = ws_manager.lock().await;
    let ws = mgr.active();

    if ws.source == WorkspaceSource::Default {
        return Ok(None);
    }

    let content = read_migration_file(&ws.path, &filename)?;
    Ok(Some(content))
}

/// Writes (creates or overwrites) a migration file.
/// Does nothing if the workspace is the default.
#[tauri::command]
pub async fn ws_write_migration(
    ws_manager: State<'_, SharedWorkspaceManager>,
    write_registry: State<'_, WriteRegistry>,
    filename: String,
    content: String,
) -> Result<bool, String> {
    let mgr = ws_manager.lock().await;
    let ws = mgr.active();

    if ws.source == WorkspaceSource::Default {
        return Ok(false);
    }

    validate_migration_filename(&filename)?;
    let dir = ws.path.join("migrations");
    fs::create_dir_all(&dir).map_err(|e| {
        EngineError::internal(format!("Failed to create migrations dir: {}", e)).to_string()
    })?;

    let path = dir.join(&filename);
    write_registry.register_with_auto_unregister(path.clone());
    fs::write(&path, content).map_err(|e| {
        EngineError::internal(format!("Failed to write migration: {}", e)).to_string()
    })?;

    Ok(true)
}

/// Deletes a migration file.
/// Does nothing if the workspace is the default.
#[tauri::command]
pub async fn ws_delete_migration(
    ws_manager: State<'_, SharedWorkspaceManager>,
    write_registry: State<'_, WriteRegistry>,
    filename: String,
) -> Result<bool, String> {
    let mgr = ws_manager.lock().await;
    let ws = mgr.active();

    if ws.source == WorkspaceSource::Default {
        return Ok(false);
    }

    validate_migration_filename(&filename)?;
    let path = ws.path.join("migrations").join(&filename);
    if !path.exists() {
        return Ok(false);
    }

    write_registry.register_with_auto_unregister(path.clone());
    fs::remove_file(&path).map_err(|e| {
        EngineError::internal(format!("Failed to delete migration: {}", e)).to_string()
    })?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_good_and_bad_names() {
        assert!(validate_migration_filename("0001_create_users.sql").is_ok());
        assert!(validate_migration_filename("0002_add-index.sql").is_ok());
        assert!(validate_migration_filename("evil.json").is_err());
        assert!(validate_migration_filename("../escape.sql").is_err());
        assert!(validate_migration_filename("a/b.sql").is_err());
        assert!(validate_migration_filename(".sql").is_err());
        assert!(validate_migration_filename("").is_err());
    }

    #[test]
    fn summarizes_filename() {
        let s = summarize("0001_create_users.sql");
        assert_eq!(s.version, "0001");
        assert_eq!(s.name, "create_users");
        assert_eq!(s.filename, "0001_create_users.sql");
    }

    #[test]
    fn splits_up_and_down() {
        let content = "-- migrate:up\nCREATE TABLE t (id int);\n\n-- migrate:down\nDROP TABLE t;\n";
        let (up, down) = split_up_down(content);
        assert_eq!(up, "CREATE TABLE t (id int);");
        assert_eq!(down, "DROP TABLE t;");

        let (up_only, down_empty) = split_up_down("SELECT 1;");
        assert_eq!(up_only, "SELECT 1;");
        assert!(down_empty.is_empty());
    }
}

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

/// Upper bound on the version prefix: enough for a `YYYYMMDDHHMMSS` stamp.
const MAX_VERSION_DIGITS: usize = 14;

/// Validates a filename for creation.
pub(crate) fn validate_new_migration_filename(filename: &str) -> Result<(), String> {
    validate_migration_filename(filename)?;
    parse_version(filename)?;
    let stem = filename.strip_suffix(".sql").unwrap_or(filename);
    let slug = stem.split_once('_').map(|(_, rest)| rest).unwrap_or("");
    if slug.is_empty() {
        return Err(EngineError::internal(
            "Migration name must be `<version>_<name>.sql`, e.g. 0001_create_users.sql",
        )
        .to_string());
    }
    Ok(())
}

/// Parses the numeric version prefix. This is the history table's primary key,
/// so it must exist and parse.
pub(crate) fn parse_version(filename: &str) -> Result<u64, String> {
    let stem = filename.strip_suffix(".sql").unwrap_or(filename);
    let digits = stem.split('_').next().unwrap_or("");
    let invalid = || {
        EngineError::internal(format!(
            "Migration `{filename}` must start with a numeric version, e.g. 0001_create_users.sql"
        ))
        .to_string()
    };
    if digits.is_empty()
        || digits.len() > MAX_VERSION_DIGITS
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid());
    }
    digits.parse::<u64>().map_err(|_| invalid())
}

/// A problem found across a set of migration filenames.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationLintIssue {
    /// Two files claim one version — they would share a single history row,
    /// so neither one's state can be tracked.
    DuplicateVersion {
        version: u64,
        files: Vec<String>,
    },
    MalformedVersion {
        file: String,
        reason: String,
    },
    /// Informational: a merged branch can legitimately land out of order.
    NonMonotonic {
        file: String,
        previous: String,
    },
}

impl MigrationLintIssue {
    pub(crate) fn affects_duplicate(&self, filename: &str) -> bool {
        matches!(self, Self::DuplicateVersion { files, .. } if files.iter().any(|f| f == filename))
    }

    pub(crate) fn affects_malformed(&self, filename: &str) -> bool {
        matches!(self, Self::MalformedVersion { file, .. } if file == filename)
    }
}

/// Lints a set of migration filenames. Pure.
pub(crate) fn lint_migrations(filenames: &[String]) -> Vec<MigrationLintIssue> {
    let mut issues = Vec::new();
    let mut by_version: std::collections::BTreeMap<u64, Vec<String>> = Default::default();

    for filename in filenames {
        match parse_version(filename) {
            Ok(v) => by_version.entry(v).or_default().push(filename.clone()),
            Err(reason) => issues.push(MigrationLintIssue::MalformedVersion {
                file: filename.clone(),
                reason,
            }),
        }
    }

    for (version, files) in &by_version {
        if files.len() > 1 {
            issues.push(MigrationLintIssue::DuplicateVersion {
                version: *version,
                files: files.clone(),
            });
        }
    }

    // Sorting is lexicographic on disk, so `10_x` precedes `9_x`. Report when the
    // on-disk order disagrees with the numeric one.
    let mut previous: Option<(u64, &String)> = None;
    for filename in filenames {
        let Ok(v) = parse_version(filename) else {
            continue;
        };
        if let Some((pv, pf)) = previous {
            if v < pv {
                issues.push(MigrationLintIssue::NonMonotonic {
                    file: filename.clone(),
                    previous: pf.clone(),
                });
            }
        }
        previous = Some((v, filename));
    }

    issues
}

/// Lists the `.sql` filenames of a workspace's migrations directory, sorted.
pub(crate) fn list_migration_filenames(ws_path: &Path) -> Result<Vec<String>, String> {
    let dir = ws_path.join("migrations");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| {
            EngineError::internal(format!("Failed to read migrations: {}", e)).to_string()
        })?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| Path::new(name).extension().and_then(|e| e.to_str()) == Some("sql"))
        .collect();
    names.sort();
    Ok(names)
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

    let filenames = list_migration_filenames(&ws.path)?;
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

    validate_new_migration_filename(&filename)?;

    // The version prefix is the history table's primary key: two files sharing
    // one would share a single row. Overwriting the same file stays allowed.
    let version = parse_version(&filename)?;
    if let Some(other) = list_migration_filenames(&ws.path)?
        .into_iter()
        .find(|f| f != &filename && parse_version(f).is_ok_and(|v| v == version))
    {
        return Err(
            EngineError::internal(format!("Version {version} is already used by {other}"))
                .to_string(),
        );
    }

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

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_padded_and_timestamp_versions() {
        assert_eq!(parse_version("0001_create_users.sql").unwrap(), 1);
        assert_eq!(
            parse_version("20260716120000_x.sql").unwrap(),
            20260716120000
        );
    }

    #[test]
    fn rejects_missing_or_non_numeric_version() {
        assert!(parse_version("create_users.sql").is_err());
        assert!(parse_version("v1_x.sql").is_err());
        assert!(parse_version("_x.sql").is_err());
        // Beyond MAX_VERSION_DIGITS.
        assert!(parse_version("123456789012345_x.sql").is_err());
    }

    #[test]
    fn new_filenames_must_carry_version_and_slug() {
        assert!(validate_new_migration_filename("0001_create_users.sql").is_ok());
        assert!(validate_new_migration_filename("create_users.sql").is_err());
        assert!(validate_new_migration_filename("0001_.sql").is_err());
        assert!(validate_new_migration_filename("0001.sql").is_err());
    }

    #[test]
    fn legacy_filenames_stay_readable_and_deletable() {
        // The permissive validator still gates read/delete, so files already on
        // disk never become unreachable.
        assert!(validate_migration_filename("legacy_no_version.sql").is_ok());
        assert!(validate_new_migration_filename("legacy_no_version.sql").is_err());
    }

    #[test]
    fn lint_detects_duplicate_version_across_padding() {
        let issues = lint_migrations(&files(&["0001_a.sql", "1_b.sql"]));
        let dup = issues
            .iter()
            .find(|i| matches!(i, MigrationLintIssue::DuplicateVersion { .. }))
            .expect("duplicate reported");
        assert!(dup.affects_duplicate("0001_a.sql"));
        assert!(dup.affects_duplicate("1_b.sql"));
    }

    #[test]
    fn lint_flags_malformed_version() {
        let issues = lint_migrations(&files(&["oops.sql"]));
        assert!(issues.iter().any(|i| i.affects_malformed("oops.sql")));
    }

    #[test]
    fn lint_flags_non_monotonic_as_warning_not_error() {
        // Lexicographic order puts 10 before 9; that is reported, not refused.
        let issues = lint_migrations(&files(&["0010_b.sql", "0009_a.sql"]));
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, MigrationLintIssue::NonMonotonic { .. }))
        );
        assert!(
            !issues
                .iter()
                .any(|i| matches!(i, MigrationLintIssue::DuplicateVersion { .. }))
        );
    }

    #[test]
    fn lint_clean_set_has_no_issues() {
        assert!(lint_migrations(&files(&["0001_a.sql", "0002_b.sql"])).is_empty());
    }
}

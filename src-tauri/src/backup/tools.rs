// SPDX-License-Identifier: Apache-2.0

//! Detection and persistence of backup-tool binary paths.
//!
//! Each `BackupTool` value corresponds to one upstream binary. We try `$PATH`
//! first (cross-platform, respects user shell setup) and fall back to a
//! per-tool override the user can save in Settings.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Every binary the backup runner knows how to spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupTool {
    PgDump,
    PgRestore,
    Psql,
    MysqlDump,
    MariaDbDump,
    Mysql,
    MongoDump,
    MongoRestore,
    Sqlite3,
}

impl BackupTool {
    /// The name used to look up the binary in `$PATH`. On Windows the `.exe`
    /// suffix is added by `which` automatically.
    pub fn binary_name(self) -> &'static str {
        match self {
            Self::PgDump => "pg_dump",
            Self::PgRestore => "pg_restore",
            Self::Psql => "psql",
            Self::MysqlDump => "mysqldump",
            Self::MariaDbDump => "mariadb-dump",
            Self::Mysql => "mysql",
            Self::MongoDump => "mongodump",
            Self::MongoRestore => "mongorestore",
            Self::Sqlite3 => "sqlite3",
        }
    }

    pub fn all() -> [Self; 9] {
        [
            Self::PgDump,
            Self::PgRestore,
            Self::Psql,
            Self::MysqlDump,
            Self::MariaDbDump,
            Self::Mysql,
            Self::MongoDump,
            Self::MongoRestore,
            Self::Sqlite3,
        ]
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupToolInfo {
    pub tool: BackupTool,
    pub binary_name: String,
    /// Absolute path resolved from the override or `$PATH`. `None` means
    /// nothing was found.
    pub path: Option<PathBuf>,
    /// `true` when a manual override is in effect (vs. PATH discovery).
    pub overridden: bool,
}

/// Holds user-supplied overrides for individual binaries.
#[derive(Debug, Default)]
pub struct BackupToolPaths {
    overrides: RwLock<HashMap<BackupTool, PathBuf>>,
}

impl BackupToolPaths {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, tool: BackupTool, path: PathBuf) {
        self.overrides.write().insert(tool, path);
    }

    pub fn clear(&self, tool: BackupTool) {
        self.overrides.write().remove(&tool);
    }

    pub fn get(&self, tool: BackupTool) -> Option<PathBuf> {
        self.overrides.read().get(&tool).cloned()
    }
}

/// Resolve a tool's path: override first, otherwise `$PATH`. Returns `None`
/// when neither yields a valid binary.
pub fn detect_tool(tool: BackupTool, overrides: &BackupToolPaths) -> BackupToolInfo {
    if let Some(custom) = overrides.get(tool) {
        if is_executable(&custom) {
            return BackupToolInfo {
                tool,
                binary_name: tool.binary_name().to_string(),
                path: Some(custom),
                overridden: true,
            };
        }
    }

    let path = which::which(tool.binary_name()).ok();

    BackupToolInfo {
        tool,
        binary_name: tool.binary_name().to_string(),
        path,
        overridden: false,
    }
}

fn is_executable(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    // On Unix we could check the executable bit; trusting the user's pick
    // here (the picker dialog already restricts to files) is acceptable for
    // a desktop app that asks confirmation before running.
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_names_are_stable() {
        assert_eq!(BackupTool::PgDump.binary_name(), "pg_dump");
        assert_eq!(BackupTool::MariaDbDump.binary_name(), "mariadb-dump");
        assert_eq!(BackupTool::Sqlite3.binary_name(), "sqlite3");
    }

    #[test]
    fn all_returns_every_variant() {
        let all = BackupTool::all();
        assert_eq!(all.len(), 9);
    }

    #[test]
    fn override_round_trip() {
        let paths = BackupToolPaths::new();
        let custom = PathBuf::from("/opt/qore/pg_dump");
        paths.set(BackupTool::PgDump, custom.clone());
        assert_eq!(paths.get(BackupTool::PgDump), Some(custom));
        paths.clear(BackupTool::PgDump);
        assert_eq!(paths.get(BackupTool::PgDump), None);
    }

    #[test]
    fn detect_unknown_binary_returns_none_path() {
        let paths = BackupToolPaths::new();
        let custom = PathBuf::from("/nonexistent/qoredb-backup-fake");
        paths.set(BackupTool::PgDump, custom);
        let info = detect_tool(BackupTool::PgDump, &paths);
        // Override is invalid → falls back to PATH (which may also be None on the test host).
        assert!(!info.overridden);
    }
}

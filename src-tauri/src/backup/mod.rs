// SPDX-License-Identifier: Apache-2.0

//! Backup / Restore Helpers
//!
//! Wrappers around the official database CLI tools (`pg_dump`, `pg_restore`,
//! `mysqldump`, `mariadb-dump`, `mongodump`, `mongorestore`, `sqlite3`).
//! QoreDB does not reimplement the dump format — it orchestrates the
//! upstream binaries, exposes their progress, and surfaces a coherent UI
//! across drivers.
//!
//! # Threat model
//!
//! Arguments are passed via `Command::arg` (no shell interpolation), and
//! identifier-style fields (database, table names) are validated against a
//! conservative regex before reaching the child. Output paths come from a
//! file picker, so they are inherently trusted as user intent.

pub mod args;
pub mod duckdb_native;
pub mod runner;
pub mod tools;

pub use args::{BackupFormat, BackupMode, BackupOptions, RestoreOptions};
pub use duckdb_native::{run_duckdb_backup, run_duckdb_restore};
pub use runner::{run_backup, run_restore, BackupEvent, BackupJob, BackupJobOutcome};
pub use tools::{detect_tool, BackupTool, BackupToolInfo, BackupToolPaths};

/// Render a path as a UTF-8 `String`, erroring on non-UTF-8 (shared by the
/// arg builders and the native DuckDB runner — cf. dédup D29).
pub(crate) fn path_to_string(path: &std::path::Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| format!("Path {:?} is not valid UTF-8", path))
}

// SPDX-License-Identifier: Apache-2.0

//! Tauri commands for the backup / restore subsystem.
//!
//! Frontend usage:
//! ```ts
//! const tools = await invoke('detect_backup_tools');
//! await invoke('set_backup_tool_path', { tool: 'pg_dump', path: '/usr/local/bin/pg_dump' });
//! const outcome = await invoke('start_backup', { options });
//! // Listen for live progress: window.listen('backup-progress', cb);
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::backup::runner::{ActiveBackups, EventSink};
use crate::backup::{
    detect_tool, run_backup, run_duckdb_backup, run_duckdb_restore, run_restore, BackupEvent,
    BackupFormat, BackupJobOutcome, BackupOptions, BackupTool, BackupToolInfo, RestoreOptions,
};
use crate::vault::backend::KeyringProvider;
use crate::vault::VaultStorage;

/// Event topic emitted on every line of stdout/stderr and on completion.
const BACKUP_EVENT: &str = "backup-progress";

#[derive(Debug, serde::Serialize)]
pub struct DetectBackupToolsResponse {
    pub tools: Vec<BackupToolInfo>,
}

/// Resolve every known backup binary against `$PATH` and any saved override.
#[tauri::command]
pub async fn detect_backup_tools(
    state: State<'_, crate::SharedState>,
) -> Result<DetectBackupToolsResponse, String> {
    let overrides = {
        let state = state.lock().await;
        Arc::clone(&state.backup_tool_paths)
    };

    let tools = BackupTool::all()
        .iter()
        .map(|tool| detect_tool(*tool, &overrides))
        .collect();

    Ok(DetectBackupToolsResponse { tools })
}

/// Register a custom path for a tool. Pass an empty string to clear it.
///
/// The path is validated before being stored: it must point at an existing,
/// non-symlink file whose basename matches the expected binary name for the
/// requested tool (e.g. only `pg_dump` / `pg_dump.exe` for `BackupTool::PgDump`).
/// Without this check, an attacker who could invoke this IPC command from the
/// webview could redirect `pg_dump` to `/bin/sh` or any other binary, which
/// would then run with the arguments the backup runner produces (cf. audit
/// B6-C3).
#[tauri::command]
pub async fn set_backup_tool_path(
    state: State<'_, crate::SharedState>,
    tool: BackupTool,
    path: String,
) -> Result<(), String> {
    let overrides = {
        let state = state.lock().await;
        Arc::clone(&state.backup_tool_paths)
    };

    let trimmed = path.trim();
    if trimmed.is_empty() {
        overrides.clear(tool);
        return Ok(());
    }

    let validated = validate_backup_tool_path(tool, trimmed)?;
    overrides.set(tool, validated);
    Ok(())
}

/// Resolves `path` to a real, existing file and verifies that its basename
/// matches the expected binary name (or `<name>.exe` on any platform — Wine
/// setups under Linux ship `.exe` artefacts).
fn validate_backup_tool_path(tool: BackupTool, path: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(path);

    // Reject relative paths — an override is meant to point at a specific
    // installation, not a `$PATH` lookup.
    if !candidate.is_absolute() {
        return Err(format!("Backup tool path must be absolute, got `{}`", path));
    }

    // Resolve symlinks. This both catches `pg_dump -> /bin/sh` (the symlink
    // attack) and surfaces dangling-symlink misconfiguration up-front.
    let canonical = std::fs::canonicalize(&candidate)
        .map_err(|e| format!("Backup tool path `{}` is not accessible: {}", path, e))?;

    let metadata = std::fs::metadata(&canonical)
        .map_err(|e| format!("Backup tool path `{}` stat failed: {}", path, e))?;
    if !metadata.is_file() {
        return Err(format!("Backup tool path `{}` is not a regular file", path));
    }

    let basename = canonical
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Backup tool path `{}` has no filename", path))?;
    let expected = tool.binary_name();
    let basename_lower = basename.to_ascii_lowercase();
    let matches = basename_lower == expected || basename_lower == format!("{}.exe", expected);
    if !matches {
        return Err(format!(
            "Backup tool path `{}` does not match expected binary `{}` (got `{}`)",
            path, expected, basename
        ));
    }

    Ok(canonical)
}

#[derive(Debug, Deserialize)]
pub struct StartBackupArgs {
    pub options: BackupOptions,
}

/// Resolves the stored database password from the vault for a saved connection,
/// mirroring how `connect_saved_connection` obtains credentials. The plaintext
/// stays in the backend and is handed straight to the CLI subprocess. Best-effort:
/// returns `None` when the vault is locked or the credential can't be read, so a
/// manually-entered password can still take over.
fn resolve_saved_password(
    app: &AppHandle,
    project_id: &str,
    connection_id: &str,
) -> Option<String> {
    let storage_dir = app.path().app_config_dir().ok()?;
    let storage = VaultStorage::new(project_id, storage_dir, Box::new(KeyringProvider::new()));
    let creds = storage.get_credentials(connection_id).ok()?;
    Some(creds.db_password.expose().clone())
}

/// Fills `options.password` from the vault when the caller left it empty and
/// supplied the saved-connection coordinates.
fn fill_password_from_vault(
    app: &AppHandle,
    password: &mut Option<String>,
    connection_id: Option<String>,
    project_id: Option<String>,
) {
    if password.as_deref().unwrap_or("").is_empty() {
        if let (Some(cid), Some(pid)) = (connection_id, project_id) {
            *password = resolve_saved_password(app, &pid, &cid);
        }
    }
}

/// Spawn the right binary for the given driver, stream events, return the
/// final outcome. DuckDB short-circuits to the in-process runner since it
/// ships `EXPORT DATABASE` natively and needs no external binary.
#[tauri::command]
pub async fn start_backup(
    app: AppHandle,
    state: State<'_, crate::SharedState>,
    mut options: BackupOptions,
    connection_id: Option<String>,
    project_id: Option<String>,
) -> Result<BackupJobOutcome, String> {
    fill_password_from_vault(&app, &mut options.password, connection_id, project_id);

    let (overrides, active) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.backup_tool_paths),
            Arc::clone(&state.active_backups),
        )
    };

    if options.driver.eq_ignore_ascii_case("duckdb") {
        let sink: Arc<dyn EventSink> = Arc::new(AppHandleSink { app });
        return run_duckdb_backup(options, sink, active).await;
    }

    let tool = backup_tool_for_driver(&options.driver, options.format)?;
    let info = detect_tool(tool, &overrides);
    let binary = info
        .path
        .ok_or_else(|| format!("Binary {} not found in PATH", tool.binary_name()))?;

    let redirect = match tool {
        BackupTool::Sqlite3 => Some(options.output_path.clone()),
        _ => None,
    };

    let sink: Arc<dyn EventSink> = Arc::new(AppHandleSink { app });
    run_backup(binary, tool, options, redirect, sink, active).await
}

#[tauri::command]
pub async fn start_restore(
    app: AppHandle,
    state: State<'_, crate::SharedState>,
    mut options: RestoreOptions,
    connection_id: Option<String>,
    project_id: Option<String>,
) -> Result<BackupJobOutcome, String> {
    fill_password_from_vault(&app, &mut options.password, connection_id, project_id);

    let (overrides, active) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.backup_tool_paths),
            Arc::clone(&state.active_backups),
        )
    };

    if options.driver.eq_ignore_ascii_case("duckdb") {
        let sink: Arc<dyn EventSink> = Arc::new(AppHandleSink { app });
        return run_duckdb_restore(options, sink, active).await;
    }

    let tool = restore_tool_for_driver(&options.driver, options.format)?;
    let info = detect_tool(tool, &overrides);
    let binary = info
        .path
        .ok_or_else(|| format!("Binary {} not found in PATH", tool.binary_name()))?;

    let sink: Arc<dyn EventSink> = Arc::new(AppHandleSink { app });
    run_restore(binary, tool, options, sink, active).await
}

/// Cancel a running backup or restore job. Returns `true` if the job was found
/// and signalled.
#[tauri::command]
pub async fn cancel_backup(
    state: State<'_, crate::SharedState>,
    job_id: String,
) -> Result<bool, String> {
    let active: Arc<ActiveBackups> = {
        let state = state.lock().await;
        Arc::clone(&state.active_backups)
    };
    Ok(active.cancel(&job_id))
}

fn backup_tool_for_driver(driver: &str, format: BackupFormat) -> Result<BackupTool, String> {
    let driver_lower = driver.to_ascii_lowercase();
    match (driver_lower.as_str(), format) {
        // PostgreSQL family — supabase / neon / timescale all reuse pg_dump.
        ("postgres" | "postgresql" | "supabase" | "neon" | "timescaledb" | "cockroachdb", _) => {
            Ok(BackupTool::PgDump)
        }
        ("mysql", _) => Ok(BackupTool::MysqlDump),
        ("mariadb", _) => Ok(BackupTool::MariaDbDump),
        ("mongodb", _) => Ok(BackupTool::MongoDump),
        ("sqlite", _) => Ok(BackupTool::Sqlite3),
        (other, _) => Err(format!("Backup not supported for driver '{}'", other)),
    }
}

fn restore_tool_for_driver(driver: &str, format: BackupFormat) -> Result<BackupTool, String> {
    let driver_lower = driver.to_ascii_lowercase();
    match (driver_lower.as_str(), format) {
        (
            "postgres" | "postgresql" | "supabase" | "neon" | "timescaledb" | "cockroachdb",
            BackupFormat::PostgresCustom,
        ) => Ok(BackupTool::PgRestore),
        ("postgres" | "postgresql" | "supabase" | "neon" | "timescaledb" | "cockroachdb", _) => {
            Ok(BackupTool::Psql)
        }
        ("mysql" | "mariadb", _) => Ok(BackupTool::Mysql),
        ("mongodb", _) => Ok(BackupTool::MongoRestore),
        ("sqlite", _) => Ok(BackupTool::Sqlite3),
        (other, _) => Err(format!("Restore not supported for driver '{}'", other)),
    }
}

struct AppHandleSink {
    app: AppHandle,
}

impl EventSink for AppHandleSink {
    fn emit(&self, job_id: &str, event: BackupEvent) {
        // Tag every event with its job_id so multiple concurrent jobs can
        // share the same window listener.
        let payload = serde_json::json!({
            "job_id": job_id,
            "event": event,
        });
        let _ = self.app.emit(BACKUP_EVENT, payload);
    }
}

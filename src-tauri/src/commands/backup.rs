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
use tauri::{AppHandle, Emitter, State};

use crate::backup::{
    detect_tool, run_backup, run_duckdb_backup, run_duckdb_restore, run_restore, BackupEvent,
    BackupFormat, BackupJobOutcome, BackupOptions, BackupTool, BackupToolInfo, RestoreOptions,
};
use crate::backup::runner::{ActiveBackups, EventSink};

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
    } else {
        overrides.set(tool, PathBuf::from(trimmed));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct StartBackupArgs {
    pub options: BackupOptions,
}

/// Spawn the right binary for the given driver, stream events, return the
/// final outcome. DuckDB short-circuits to the in-process runner since it
/// ships `EXPORT DATABASE` natively and needs no external binary.
#[tauri::command]
pub async fn start_backup(
    app: AppHandle,
    state: State<'_, crate::SharedState>,
    options: BackupOptions,
) -> Result<BackupJobOutcome, String> {
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
    options: RestoreOptions,
) -> Result<BackupJobOutcome, String> {
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
        ("postgres" | "postgresql" | "supabase" | "neon" | "timescaledb" | "cockroachdb", BackupFormat::PostgresCustom) => {
            Ok(BackupTool::PgRestore)
        }
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

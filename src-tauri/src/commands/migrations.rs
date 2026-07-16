// SPDX-License-Identifier: Apache-2.0

//! Schema migration runner: applies/rolls back versioned migration files against
//! a live connection and tracks applied state in a `qoredb_migrations` table
//! inside the target database.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;

use crate::commands::parse_session_id;
use crate::commands::workspace::SharedWorkspaceManager;
use crate::commands::workspace_migrations::{read_migration_file, split_up_down, summarize};
use crate::engine::types::QueryId;
use crate::workspace::types::WorkspaceSource;

const HISTORY_TABLE: &str = "qoredb_migrations";

#[derive(Debug, Serialize)]
pub struct ApplyMigrationResponse {
    pub success: bool,
    pub execution_ms: u64,
    pub error: Option<String>,
    /// Index of the statement that failed, when the failure was in the script.
    pub failed_statement: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct MigrationStatusEntry {
    pub version: String,
    pub name: String,
    pub filename: String,
    /// "applied" | "pending" | "rolled_back"
    pub status: String,
    pub applied_at: Option<String>,
    /// True when an applied file was edited after being applied (checksum drift).
    pub checksum_mismatch: bool,
}

fn fail(msg: String) -> ApplyMigrationResponse {
    ApplyMigrationResponse {
        success: false,
        execution_ms: 0,
        error: Some(msg),
        failed_statement: None,
    }
}

fn checksum(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Escapes a value as a single-quoted SQL string literal.
fn sql_str(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn is_sqlserver(driver_id: &str) -> bool {
    matches!(driver_id, "sqlserver" | "mssql")
}

/// DDL to create the history table if absent. Portable across the SQL drivers
/// except SQL Server, which lacks `CREATE TABLE IF NOT EXISTS`.
fn history_table_ddl(driver_id: &str) -> String {
    if is_sqlserver(driver_id) {
        format!(
            "IF NOT EXISTS (SELECT * FROM sys.tables WHERE name = '{HISTORY_TABLE}') \
             CREATE TABLE {HISTORY_TABLE} (version NVARCHAR(255) PRIMARY KEY, name NVARCHAR(MAX) NOT NULL, \
             checksum NVARCHAR(255) NOT NULL, applied_at NVARCHAR(64) NOT NULL, applied_by NVARCHAR(255), \
             execution_ms BIGINT, rolled_back_at NVARCHAR(64))"
        )
    } else {
        format!(
            "CREATE TABLE IF NOT EXISTS {HISTORY_TABLE} (version VARCHAR(255) PRIMARY KEY, name TEXT NOT NULL, \
             checksum TEXT NOT NULL, applied_at VARCHAR(64) NOT NULL, applied_by TEXT, \
             execution_ms BIGINT, rolled_back_at VARCHAR(64))"
        )
    }
}

/// Applies (`up`) or rolls back (`down`) a migration against the session's
/// database, recording the result in the history table — all transactionally
/// when the driver supports it.
#[tauri::command]
pub async fn apply_migration(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    session_id: String,
    filename: String,
    direction: String,
    database: String,
    acknowledged: Option<bool>,
) -> Result<ApplyMigrationResponse, String> {
    let content = {
        let mgr = ws_manager.lock().await;
        let ws = mgr.active();
        if ws.source == WorkspaceSource::Default {
            return Ok(fail(
                "Migrations require a file-based workspace".to_string(),
            ));
        }
        read_migration_file(&ws.path, &filename)?
    };

    let is_up = direction != "down";
    let (up, down) = split_up_down(&content);
    let script = if is_up { up } else { down };
    if script.trim().is_empty() {
        return Ok(fail(format!("Migration has no {} script", direction)));
    }

    let summary = summarize(&filename);
    let file_checksum = checksum(&content);

    let (session_manager, interceptor) = {
        let guard = state.lock().await;
        (
            Arc::clone(&guard.session_manager),
            Arc::clone(&guard.interceptor),
        )
    };
    let session = parse_session_id(&session_id)?;

    // Read-only, capabilities and interceptor safety, plus the gated driver.
    let preflight = match qore_service::mutation::preflight(
        &session_manager,
        &interceptor,
        session,
        &session_id,
        &script,
        &database,
        acknowledged.unwrap_or(false),
    )
    .await
    {
        Ok(pf) => pf,
        Err(msg) => return Ok(fail(msg)),
    };
    let driver = preflight.driver;
    let driver_id = driver.driver_id().to_string();

    let statements = match crate::engine::sql_safety::split_sql_statements(&driver_id, &script) {
        Ok(stmts) => stmts,
        Err(e) => return Ok(fail(format!("Failed to parse migration SQL: {}", e))),
    };

    let start = std::time::Instant::now();

    // History table is created outside the migration transaction so it persists
    // even if the migration is rolled back.
    if let Err(e) = driver
        .execute(session, &history_table_ddl(&driver_id), QueryId::new())
        .await
    {
        return Ok(fail(format!(
            "Failed to prepare migration history: {}",
            e.sanitized_message()
        )));
    }

    let supports_tx = driver.supports_transactions_for_session(session).await;
    if supports_tx {
        if let Err(e) = driver.begin_transaction(session).await {
            return Ok(fail(format!(
                "Failed to begin transaction: {}",
                e.sanitized_message()
            )));
        }
    }

    for (idx, stmt) in statements.iter().enumerate() {
        if stmt.trim().is_empty() {
            continue;
        }
        if let Err(e) = driver.execute(session, stmt, QueryId::new()).await {
            if supports_tx {
                let _ = driver.rollback(session).await;
            }
            return Ok(ApplyMigrationResponse {
                success: false,
                execution_ms: start.elapsed().as_millis() as u64,
                error: Some(e.sanitized_message()),
                failed_statement: Some(idx),
            });
        }
    }

    // Record applied state. Up replaces any prior row; down marks the row rolled
    // back. DELETE+INSERT avoids dialect-specific upsert syntax.
    let now = chrono::Utc::now().to_rfc3339();
    let history_sql: Vec<String> = if is_up {
        vec![
            format!(
                "DELETE FROM {HISTORY_TABLE} WHERE version = {}",
                sql_str(&summary.version)
            ),
            format!(
                "INSERT INTO {HISTORY_TABLE} (version, name, checksum, applied_at, applied_by, execution_ms, rolled_back_at) \
                 VALUES ({}, {}, {}, {}, {}, {}, NULL)",
                sql_str(&summary.version),
                sql_str(&summary.name),
                sql_str(&file_checksum),
                sql_str(&now),
                sql_str(&session_id),
                start.elapsed().as_millis()
            ),
        ]
    } else {
        vec![format!(
            "UPDATE {HISTORY_TABLE} SET rolled_back_at = {} WHERE version = {}",
            sql_str(&now),
            sql_str(&summary.version)
        )]
    };

    for sql in &history_sql {
        if let Err(e) = driver.execute(session, sql, QueryId::new()).await {
            if supports_tx {
                let _ = driver.rollback(session).await;
            }
            return Ok(fail(format!(
                "Failed to record migration history: {}",
                e.sanitized_message()
            )));
        }
    }

    if supports_tx {
        if let Err(e) = driver.commit(session).await {
            return Ok(fail(format!("Failed to commit: {}", e.sanitized_message())));
        }
    }

    // Schema likely changed — drop cached previews for this connection.
    if let Some(key) = session_manager.connection_key(session).await {
        let query_cache = {
            let guard = state.lock().await;
            Arc::clone(&guard.query_cache)
        };
        query_cache.invalidate_connection(&key);
    }

    Ok(ApplyMigrationResponse {
        success: true,
        execution_ms: start.elapsed().as_millis() as u64,
        error: None,
        failed_statement: None,
    })
}

/// Returns the applied/pending status of the active workspace's migrations for
/// the given connection. None if the workspace is the default.
#[tauri::command]
pub async fn get_migration_status(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    session_id: String,
) -> Result<Option<Vec<MigrationStatusEntry>>, String> {
    let files: Vec<(String, String)> = {
        let mgr = ws_manager.lock().await;
        let ws = mgr.active();
        if ws.source == WorkspaceSource::Default {
            return Ok(None);
        }
        let dir = ws.path.join("migrations");
        if !dir.exists() {
            return Ok(Some(Vec::new()));
        }
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .map_err(|e| format!("Failed to read migrations: {}", e))?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.ends_with(".sql"))
            .collect();
        names.sort();
        names
            .into_iter()
            .map(|name| {
                let content = std::fs::read_to_string(dir.join(&name)).unwrap_or_default();
                (name, content)
            })
            .collect()
    };

    let session_manager = {
        let guard = state.lock().await;
        Arc::clone(&guard.session_manager)
    };
    let session = parse_session_id(&session_id)?;
    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;

    // version -> (checksum, applied_at, rolled_back_at). Absent table => empty.
    let history: HashMap<String, (String, Option<String>, Option<String>)> = {
        let query =
            format!("SELECT version, checksum, applied_at, rolled_back_at FROM {HISTORY_TABLE}");
        match driver.execute(session, &query, QueryId::new()).await {
            Ok(result) => result
                .rows
                .iter()
                .filter_map(|row| {
                    let cell = |i: usize| row.values.get(i).and_then(|v| v.as_text());
                    let version = cell(0)?.to_string();
                    let cs = cell(1).unwrap_or("").to_string();
                    let applied_at = cell(2).map(|s| s.to_string());
                    let rolled_back = cell(3).map(|s| s.to_string());
                    Some((version, (cs, applied_at, rolled_back)))
                })
                .collect(),
            Err(_) => HashMap::new(),
        }
    };

    let entries = files
        .iter()
        .map(|(filename, content)| {
            let summary = summarize(filename);
            match history.get(&summary.version) {
                Some((stored_checksum, applied_at, rolled_back)) => {
                    let applied = rolled_back.is_none();
                    MigrationStatusEntry {
                        version: summary.version,
                        name: summary.name,
                        filename: filename.clone(),
                        status: if applied { "applied" } else { "rolled_back" }.to_string(),
                        applied_at: applied_at.clone(),
                        checksum_mismatch: applied && *stored_checksum != checksum(content),
                    }
                }
                None => MigrationStatusEntry {
                    version: summary.version,
                    name: summary.name,
                    filename: filename.clone(),
                    status: "pending".to_string(),
                    applied_at: None,
                    checksum_mismatch: false,
                },
            }
        })
        .collect();

    Ok(Some(entries))
}

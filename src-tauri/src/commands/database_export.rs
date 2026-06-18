// SPDX-License-Identifier: Apache-2.0

//! Full Database Export Tauri Command
//!
//! Exports a whole database (schema DDL + table data) in a single operation.
//! Two output formats are supported:
//!   * a single replayable `.sql` file (DDL + INSERTs ordered by FK dependency),
//!   * a `.zip` archive (`schema.sql` + one CSV file per table).
//!
//! The job runs asynchronously and reports progress through the
//! `db_export_progress:<export_id>` window event, mirroring the streaming
//! export pipeline.

use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::sync::Arc;
use std::time::Instant;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::RwLock;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::commands::schema_export::{
    build_schema_ddl, list_and_describe_tables, resolve_export_path, DescribedTable,
    SchemaExportOptions,
};
use crate::engine::sql_generator::SqlDialect;
use crate::engine::traits::{DataEngine, StreamEvent};
use crate::engine::types::{ColumnInfo, Namespace, QueryId, Row, SessionId, Value};
use crate::export::types::ExportState;

fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseExportFormat {
    /// Single replayable `.sql` file.
    Sql,
    /// `.zip` archive: `schema.sql` + one CSV per table.
    Zip,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseExportOptions {
    /// Emit schema DDL (defaults to true).
    pub include_schema: Option<bool>,
    /// Emit table data (defaults to true).
    pub include_data: Option<bool>,
    /// Which schema objects to include (tables/routines/triggers/...).
    pub schema: Option<SchemaExportOptions>,
    /// Restrict the export to these tables (None = every table).
    pub tables: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseExportProgress {
    pub export_id: String,
    pub state: ExportState,
    pub current_table: Option<String>,
    pub tables_done: u32,
    pub tables_total: u32,
    pub rows_exported: u64,
    pub bytes_written: u64,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DatabaseExportStartResponse {
    pub export_id: String,
}

#[derive(Debug, Serialize)]
pub struct DatabaseExportCancelResponse {
    pub success: bool,
    pub export_id: String,
    pub error: Option<String>,
}

/// Tracks running full-database export jobs so they can be cancelled.
pub struct DatabaseExportManager {
    jobs: RwLock<HashMap<String, CancellationToken>>,
}

impl DatabaseExportManager {
    pub fn new() -> Self {
        Self {
            jobs: RwLock::new(HashMap::new()),
        }
    }

    async fn register(&self, export_id: &str) -> Result<CancellationToken, String> {
        let mut jobs = self.jobs.write().await;
        if jobs.contains_key(export_id) {
            return Err("Database export already in progress".to_string());
        }
        let token = CancellationToken::new();
        jobs.insert(export_id.to_string(), token.clone());
        Ok(token)
    }

    async fn finish(&self, export_id: &str) {
        self.jobs.write().await.remove(export_id);
    }

    pub async fn cancel(&self, export_id: &str) -> Result<(), String> {
        let jobs = self.jobs.read().await;
        let token = jobs
            .get(export_id)
            .ok_or_else(|| "Database export not found".to_string())?;
        token.cancel();
        Ok(())
    }
}

impl Default for DatabaseExportManager {
    fn default() -> Self {
        Self::new()
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn export_database_full(
    state: State<'_, crate::SharedState>,
    window: tauri::Window,
    session_id: String,
    database: String,
    schema: Option<String>,
    file_path: String,
    format: DatabaseExportFormat,
    options: DatabaseExportOptions,
    export_id: Option<String>,
) -> Result<DatabaseExportStartResponse, String> {
    let (session_manager, manager) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.database_export_manager),
        )
    };

    let session = parse_session_id(&session_id)?;
    let export_id = match export_id {
        Some(id) => {
            Uuid::parse_str(&id).map_err(|e| format!("Invalid export ID: {}", e))?;
            id
        }
        None => Uuid::new_v4().to_string(),
    };

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    let driver_id = driver.driver_id().to_string();
    let dialect = SqlDialect::from_driver_id(&driver_id)
        .ok_or_else(|| "Full database export is not supported for this driver".to_string())?;

    let include_data = options.include_data.unwrap_or(true);
    if include_data && !driver.capabilities().streaming {
        return Err("Streaming is not supported by this driver".to_string());
    }

    // Validate destination before spawning: `file_path` is frontend input and
    // the writers below bypass the Tauri `fs:scope` plugin.
    let resolved = resolve_export_path(&file_path)?;

    let cancel = manager.register(&export_id).await?;

    let namespace = Namespace {
        database,
        schema,
    };

    let manager_for_task = Arc::clone(&manager);
    let export_id_task = export_id.clone();
    tokio::spawn(async move {
        let result = run_database_export(
            driver,
            driver_id,
            session,
            namespace,
            dialect,
            options,
            format,
            resolved.to_string_lossy().to_string(),
            export_id_task.clone(),
            cancel,
            window,
        )
        .await;

        if let Err(err) = result {
            tracing::error!("Database export {} failed: {}", export_id_task, err);
        }
        manager_for_task.finish(&export_id_task).await;
    });

    Ok(DatabaseExportStartResponse { export_id })
}

#[tauri::command]
pub async fn cancel_database_export(
    state: State<'_, crate::SharedState>,
    export_id: String,
) -> Result<DatabaseExportCancelResponse, String> {
    let manager = {
        let state = state.lock().await;
        Arc::clone(&state.database_export_manager)
    };

    match manager.cancel(&export_id).await {
        Ok(()) => Ok(DatabaseExportCancelResponse {
            success: true,
            export_id,
            error: None,
        }),
        Err(err) => Ok(DatabaseExportCancelResponse {
            success: false,
            export_id,
            error: Some(err),
        }),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_database_export(
    driver: Arc<dyn DataEngine>,
    driver_id: String,
    session: SessionId,
    namespace: Namespace,
    dialect: SqlDialect,
    options: DatabaseExportOptions,
    format: DatabaseExportFormat,
    output_path: String,
    export_id: String,
    cancel: CancellationToken,
    window: tauri::Window,
) -> Result<(), String> {
    let start = Instant::now();
    let schema_options = options.schema.unwrap_or(SchemaExportOptions {
        include_tables: Some(true),
        include_routines: Some(true),
        include_triggers: Some(true),
        include_events: Some(true),
        include_sequences: Some(true),
    });
    let include_schema = options.include_schema.unwrap_or(true);
    let include_data = options.include_data.unwrap_or(true);

    emit(
        &window,
        DatabaseExportProgress {
            export_id: export_id.clone(),
            state: ExportState::Pending,
            current_table: None,
            tables_done: 0,
            tables_total: 0,
            rows_exported: 0,
            bytes_written: 0,
            elapsed_ms: start.elapsed().as_millis() as u64,
            error: None,
        },
    );

    // Introspect once, then order by FK dependency so the resulting dump
    // replays cleanly (referenced tables created/filled first).
    let described = list_and_describe_tables(
        driver.as_ref(),
        session,
        &namespace,
        options.tables.as_deref(),
    )
    .await;

    let described = match described {
        Ok(tables) => topo_sort(tables),
        Err(err) => {
            emit_terminal(&window, &export_id, ExportState::Failed, 0, 0, 0, 0, start, Some(err.clone()));
            return Err(err);
        }
    };

    let data_table_count = described.iter().filter(|t| t.is_base_table()).count() as u32;
    let tables_total = if include_data { data_table_count } else { 0 };

    // Build schema DDL sections upfront (small, kept in memory like the
    // schema-only export).
    let (schema_sections, _counts) = if include_schema {
        match build_schema_ddl(
            driver.as_ref(),
            session,
            &namespace,
            dialect,
            &schema_options,
            &described,
        )
        .await
        {
            Ok(res) => res,
            Err(err) => {
                emit_terminal(&window, &export_id, ExportState::Failed, 0, 0, 0, 0, start, Some(err.clone()));
                return Err(err);
            }
        }
    } else {
        (String::new(), Default::default())
    };

    let header = file_header(&namespace, &driver_id);

    let outcome = match format {
        DatabaseExportFormat::Sql => {
            export_as_sql(
                &driver,
                session,
                &namespace,
                dialect,
                &output_path,
                &header,
                include_schema,
                &schema_sections,
                include_data,
                &described,
                tables_total,
                &cancel,
                &window,
                &export_id,
                start,
            )
            .await
        }
        DatabaseExportFormat::Zip => {
            export_as_zip(
                &driver,
                session,
                &namespace,
                dialect,
                &output_path,
                &header,
                include_schema,
                &schema_sections,
                include_data,
                &described,
                tables_total,
                &cancel,
                &window,
                &export_id,
                start,
            )
            .await
        }
    };

    match outcome {
        Ok((state, rows, bytes)) => {
            emit_terminal(&window, &export_id, state, tables_total, tables_total, rows, bytes, start, None);
            Ok(())
        }
        Err(err) => {
            emit_terminal(&window, &export_id, ExportState::Failed, 0, tables_total, 0, 0, start, Some(err.clone()));
            Err(err)
        }
    }
}

/// Result of a successful export run: terminal state, total rows, bytes written.
type RunOutcome = (ExportState, u64, u64);

#[allow(clippy::too_many_arguments)]
async fn export_as_sql(
    driver: &Arc<dyn DataEngine>,
    session: SessionId,
    namespace: &Namespace,
    dialect: SqlDialect,
    output_path: &str,
    header: &str,
    include_schema: bool,
    schema_sections: &str,
    include_data: bool,
    tables: &[DescribedTable],
    tables_total: u32,
    cancel: &CancellationToken,
    window: &tauri::Window,
    export_id: &str,
    start: Instant,
) -> Result<RunOutcome, String> {
    let file = tokio::fs::File::create(output_path)
        .await
        .map_err(|e| format!("Failed to create export file: {}", e))?;
    let mut writer = BufWriter::new(file);
    let mut bytes: u64 = 0;

    write_async(&mut writer, &mut bytes, header).await?;

    if include_schema {
        write_async(&mut writer, &mut bytes, schema_sections).await?;
    }

    let mut total_rows: u64 = 0;
    let mut state = ExportState::Completed;

    if include_data {
        let (pre, post) = fk_toggle(dialect);
        write_async(&mut writer, &mut bytes, "-- ================================================\n").await?;
        write_async(&mut writer, &mut bytes, "-- DATA\n").await?;
        write_async(&mut writer, &mut bytes, "-- ================================================\n\n").await?;
        if !pre.is_empty() {
            write_async(&mut writer, &mut bytes, pre).await?;
        }

        let mut tables_done: u32 = 0;
        let mut last_emit = Instant::now();
        for table in tables.iter().filter(|t| t.is_base_table()) {
            if cancel.is_cancelled() {
                state = ExportState::Cancelled;
                break;
            }

            let qualified = dialect.qualified_table(namespace, &table.name);
            write_async(
                &mut writer,
                &mut bytes,
                &format!("-- Data for table: {}\n", qualified),
            )
            .await?;

            let query = driver
                .build_export_select(session, namespace, &table.name, &qualified)
                .await
                .unwrap_or_else(|_| format!("SELECT * FROM {}", qualified));
            let mut columns_sql: Option<String> = None;
            let mut columns: Vec<ColumnInfo> = Vec::new();

            let (mut receiver, task, query_id) =
                spawn_table_stream(Arc::clone(driver), session, namespace.clone(), query);
            let result = loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        let _ = driver.cancel(session, Some(query_id)).await;
                        break StreamLoop::Cancelled;
                    }
                    event = receiver.recv() => {
                        match event {
                            Some(StreamEvent::Columns(cols)) => {
                                columns_sql = Some(columns_sql_str(dialect, &cols));
                                columns = cols;
                            }
                            Some(StreamEvent::Row(row)) => {
                                let cols_sql = columns_sql.clone().unwrap_or_default();
                                let line = insert_stmt(dialect, &qualified, &cols_sql, &columns, &row);
                                if let Err(e) = write_async(&mut writer, &mut bytes, &line).await {
                                    break StreamLoop::Failed(e);
                                }
                                if let Err(e) = write_async(&mut writer, &mut bytes, "\n").await {
                                    break StreamLoop::Failed(e);
                                }
                                total_rows += 1;
                                maybe_emit_progress(window, export_id, &table.name, tables_done, tables_total, total_rows, bytes, start, &mut last_emit);
                            }
                            Some(StreamEvent::RowBatch(batch)) => {
                                let cols_sql = columns_sql.clone().unwrap_or_default();
                                let mut batch_err: Option<String> = None;
                                for row in batch {
                                    let line = insert_stmt(dialect, &qualified, &cols_sql, &columns, &row);
                                    if let Err(e) = write_async(&mut writer, &mut bytes, &line).await {
                                        batch_err = Some(e);
                                        break;
                                    }
                                    if let Err(e) = write_async(&mut writer, &mut bytes, "\n").await {
                                        batch_err = Some(e);
                                        break;
                                    }
                                    total_rows += 1;
                                }
                                if let Some(e) = batch_err {
                                    break StreamLoop::Failed(e);
                                }
                                maybe_emit_progress(window, export_id, &table.name, tables_done, tables_total, total_rows, bytes, start, &mut last_emit);
                            }
                            Some(StreamEvent::Error(e)) => break StreamLoop::Failed(e),
                            Some(StreamEvent::Done(_)) => break StreamLoop::Done,
                            None => break StreamLoop::Done,
                        }
                    }
                }
            };
            join_stream(task).await;

            match result {
                StreamLoop::Done => {}
                StreamLoop::Cancelled => {
                    state = ExportState::Cancelled;
                    break;
                }
                StreamLoop::Failed(e) => {
                    writer.flush().await.ok();
                    return Err(e);
                }
            }

            write_async(&mut writer, &mut bytes, "\n").await?;
            tables_done += 1;
        }

        if matches!(state, ExportState::Completed) && !post.is_empty() {
            write_async(&mut writer, &mut bytes, post).await?;
        }
    }

    writer
        .flush()
        .await
        .map_err(|e| format!("Failed to flush export file: {}", e))?;
    writer
        .shutdown()
        .await
        .map_err(|e| format!("Failed to finalize export file: {}", e))?;

    Ok((state, total_rows, bytes))
}

#[allow(clippy::too_many_arguments)]
async fn export_as_zip(
    driver: &Arc<dyn DataEngine>,
    session: SessionId,
    namespace: &Namespace,
    dialect: SqlDialect,
    output_path: &str,
    header: &str,
    include_schema: bool,
    schema_sections: &str,
    include_data: bool,
    tables: &[DescribedTable],
    tables_total: u32,
    cancel: &CancellationToken,
    window: &tauri::Window,
    export_id: &str,
    start: Instant,
) -> Result<RunOutcome, String> {
    let file = std::fs::File::create(output_path)
        .map_err(|e| format!("Failed to create export archive: {}", e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut bytes: u64 = 0;

    if include_schema {
        zip.start_file("schema.sql", options)
            .map_err(|e| format!("Failed to write schema entry: {}", e))?;
        let payload = format!("{}{}", header, schema_sections);
        zip.write_all(payload.as_bytes())
            .map_err(|e| format!("Failed to write schema entry: {}", e))?;
        bytes += payload.len() as u64;
    }

    let mut total_rows: u64 = 0;
    let mut state = ExportState::Completed;

    if include_data {
        let mut tables_done: u32 = 0;
        let mut last_emit = Instant::now();
        for table in tables.iter().filter(|t| t.is_base_table()) {
            if cancel.is_cancelled() {
                state = ExportState::Cancelled;
                break;
            }

            zip.start_file(format!("data/{}.csv", table.name), options)
                .map_err(|e| format!("Failed to write data entry: {}", e))?;

            let qualified = dialect.qualified_table(namespace, &table.name);
            let query = driver
                .build_export_select(session, namespace, &table.name, &qualified)
                .await
                .unwrap_or_else(|_| format!("SELECT * FROM {}", qualified));
            let mut header_written = false;
            let mut columns: Vec<ColumnInfo> = Vec::new();

            let (mut receiver, task, query_id) =
                spawn_table_stream(Arc::clone(driver), session, namespace.clone(), query);
            let result = loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        let _ = driver.cancel(session, Some(query_id)).await;
                        break StreamLoop::Cancelled;
                    }
                    event = receiver.recv() => {
                        match event {
                            Some(StreamEvent::Columns(cols)) => {
                                columns = cols;
                                let line = csv_header_line(&columns);
                                if let Err(e) = zip_write(&mut zip, &mut bytes, &line) {
                                    break StreamLoop::Failed(e);
                                }
                                header_written = true;
                            }
                            Some(StreamEvent::Row(row)) => {
                                if !header_written {
                                    let line = csv_header_line(&columns);
                                    let _ = zip_write(&mut zip, &mut bytes, &line);
                                    header_written = true;
                                }
                                let line = csv_row_line(&columns, &row);
                                if let Err(e) = zip_write(&mut zip, &mut bytes, &line) {
                                    break StreamLoop::Failed(e);
                                }
                                total_rows += 1;
                                maybe_emit_progress(window, export_id, &table.name, tables_done, tables_total, total_rows, bytes, start, &mut last_emit);
                            }
                            Some(StreamEvent::RowBatch(batch)) => {
                                if !header_written {
                                    let line = csv_header_line(&columns);
                                    let _ = zip_write(&mut zip, &mut bytes, &line);
                                    header_written = true;
                                }
                                let mut batch_err: Option<String> = None;
                                for row in batch {
                                    let line = csv_row_line(&columns, &row);
                                    if let Err(e) = zip_write(&mut zip, &mut bytes, &line) {
                                        batch_err = Some(e);
                                        break;
                                    }
                                    total_rows += 1;
                                }
                                if let Some(e) = batch_err {
                                    break StreamLoop::Failed(e);
                                }
                                maybe_emit_progress(window, export_id, &table.name, tables_done, tables_total, total_rows, bytes, start, &mut last_emit);
                            }
                            Some(StreamEvent::Error(e)) => break StreamLoop::Failed(e),
                            Some(StreamEvent::Done(_)) => break StreamLoop::Done,
                            None => break StreamLoop::Done,
                        }
                    }
                }
            };
            join_stream(task).await;

            match result {
                StreamLoop::Done => {}
                StreamLoop::Cancelled => {
                    state = ExportState::Cancelled;
                    break;
                }
                StreamLoop::Failed(e) => {
                    zip.finish().ok();
                    return Err(e);
                }
            }
            tables_done += 1;
        }
    }

    zip.finish()
        .map_err(|e| format!("Failed to finalize export archive: {}", e))?;

    Ok((state, total_rows, bytes))
}

enum StreamLoop {
    Done,
    Cancelled,
    Failed(String),
}

type StreamTask = tokio::task::JoinHandle<Result<(), crate::engine::error::EngineError>>;

/// Spawn a `SELECT * FROM table` stream in a background task. Returns the event
/// receiver, the task handle, and the query id (used to cancel the query).
fn spawn_table_stream(
    driver: Arc<dyn DataEngine>,
    session: SessionId,
    namespace: Namespace,
    query: String,
) -> (
    tokio::sync::mpsc::Receiver<StreamEvent>,
    StreamTask,
    QueryId,
) {
    let (sender, receiver) = tokio::sync::mpsc::channel(100);
    let query_id = QueryId::new();
    let task = tokio::spawn(async move {
        driver
            .execute_stream_in_namespace(session, Some(namespace), &query, query_id, sender)
            .await
    });
    (receiver, task, query_id)
}

/// Wait briefly for the driver task to wind down (it may still be cancelling).
async fn join_stream(task: StreamTask) {
    let _ = tokio::time::timeout(Duration::from_secs(2), task).await;
}

fn topo_sort(tables: Vec<DescribedTable>) -> Vec<DescribedTable> {
    let names: HashSet<String> = tables.iter().map(|t| t.name.clone()).collect();
    let mut remaining = tables;
    let mut emitted: HashSet<String> = HashSet::new();
    let mut result: Vec<DescribedTable> = Vec::with_capacity(remaining.len());

    while !remaining.is_empty() {
        let pos = remaining
            .iter()
            .position(|t| deps(t, &names).iter().all(|d| emitted.contains(d)));

        match pos {
            Some(i) => {
                let table = remaining.remove(i);
                emitted.insert(table.name.clone());
                result.push(table);
            }
            None => {
                // Cyclic FK dependency: emit the rest in their current order.
                result.append(&mut remaining);
            }
        }
    }

    result
}

fn deps(table: &DescribedTable, names: &HashSet<String>) -> HashSet<String> {
    let mut deps = HashSet::new();
    if let Some(schema) = &table.schema {
        for fk in &schema.foreign_keys {
            if fk.is_virtual || fk.referenced_table == table.name {
                continue;
            }
            if names.contains(&fk.referenced_table) {
                deps.insert(fk.referenced_table.clone());
            }
        }
    }
    deps
}

fn fk_toggle(dialect: SqlDialect) -> (&'static str, &'static str) {
    match dialect {
        SqlDialect::MySql => ("SET FOREIGN_KEY_CHECKS=0;\n\n", "\nSET FOREIGN_KEY_CHECKS=1;\n"),
        SqlDialect::Sqlite => ("PRAGMA foreign_keys=OFF;\n\n", "\nPRAGMA foreign_keys=ON;\n"),
        SqlDialect::Postgres | SqlDialect::SqlServer => ("", ""),
    }
}

fn file_header(namespace: &Namespace, driver_id: &str) -> String {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let mut header = String::new();
    header.push_str("-- ================================================\n");
    header.push_str("-- QoreDB Database Export\n");
    header.push_str(&format!("-- Database: {}\n", namespace.database));
    if let Some(ref s) = namespace.schema {
        header.push_str(&format!("-- Schema: {}\n", s));
    }
    header.push_str(&format!("-- Driver: {}\n", driver_id));
    header.push_str(&format!("-- Date: {}\n", now));
    header.push_str("-- ================================================\n\n");
    header
}

fn columns_sql_str(dialect: SqlDialect, columns: &[ColumnInfo]) -> String {
    columns
        .iter()
        .map(|col| dialect.quote_ident(&col.name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn insert_stmt(
    dialect: SqlDialect,
    qualified: &str,
    columns_sql: &str,
    columns: &[ColumnInfo],
    row: &Row,
) -> String {
    let values: Vec<String> = (0..columns.len())
        .map(|idx| dialect.format_value(row.values.get(idx).unwrap_or(&Value::Null)))
        .collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        qualified,
        columns_sql,
        values.join(", ")
    )
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn csv_format_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Text(s) => s.clone(),
        Value::Bytes(b) => STANDARD.encode(b),
        Value::Json(j) => j.to_string(),
        Value::Array(arr) => serde_json::to_string(arr).unwrap_or_else(|_| "[]".to_string()),
    }
}

fn csv_header_line(columns: &[ColumnInfo]) -> String {
    let header = columns
        .iter()
        .map(|col| csv_escape(&col.name))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}\n", header)
}

fn csv_row_line(columns: &[ColumnInfo], row: &Row) -> String {
    let fields = (0..columns.len())
        .map(|idx| csv_escape(&csv_format_value(row.values.get(idx).unwrap_or(&Value::Null))))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}\n", fields)
}

async fn write_async(
    writer: &mut BufWriter<tokio::fs::File>,
    bytes: &mut u64,
    text: &str,
) -> Result<(), String> {
    writer
        .write_all(text.as_bytes())
        .await
        .map_err(|e| format!("Failed to write export file: {}", e))?;
    *bytes += text.len() as u64;
    Ok(())
}

fn zip_write(
    zip: &mut zip::ZipWriter<std::fs::File>,
    bytes: &mut u64,
    text: &str,
) -> Result<(), String> {
    zip.write_all(text.as_bytes())
        .map_err(|e| format!("Failed to write archive entry: {}", e))?;
    *bytes += text.len() as u64;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn maybe_emit_progress(
    window: &tauri::Window,
    export_id: &str,
    current_table: &str,
    tables_done: u32,
    tables_total: u32,
    rows: u64,
    bytes: u64,
    start: Instant,
    last_emit: &mut Instant,
) {
    if last_emit.elapsed() < Duration::from_millis(250) {
        return;
    }
    emit(
        window,
        DatabaseExportProgress {
            export_id: export_id.to_string(),
            state: ExportState::Running,
            current_table: Some(current_table.to_string()),
            tables_done,
            tables_total,
            rows_exported: rows,
            bytes_written: bytes,
            elapsed_ms: start.elapsed().as_millis() as u64,
            error: None,
        },
    );
    *last_emit = Instant::now();
}

#[allow(clippy::too_many_arguments)]
fn emit_terminal(
    window: &tauri::Window,
    export_id: &str,
    state: ExportState,
    tables_done: u32,
    tables_total: u32,
    rows: u64,
    bytes: u64,
    start: Instant,
    error: Option<String>,
) {
    emit(
        window,
        DatabaseExportProgress {
            export_id: export_id.to_string(),
            state,
            current_table: None,
            tables_done,
            tables_total,
            rows_exported: rows,
            bytes_written: bytes,
            elapsed_ms: start.elapsed().as_millis() as u64,
            error,
        },
    );
}

fn emit(window: &tauri::Window, progress: DatabaseExportProgress) {
    let _ = window.emit(&format!("db_export_progress:{}", progress.export_id), progress);
}

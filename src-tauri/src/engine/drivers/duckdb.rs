// SPDX-License-Identifier: Apache-2.0

//! DuckDB Driver
//!
//! Implements the DataEngine trait for DuckDB using the native `duckdb` crate.
//!
//! ## DuckDB Specifics
//!
//! - DuckDB is a file-based embedded OLAP database
//! - `host` in ConnectionConfig contains the file path
//! - Supports `:memory:` for in-memory databases
//! - Supports multiple schemas within a single database file
//! - Uses `information_schema` for metadata (not PRAGMA like SQLite)
//!
//! ## Concurrency Model
//!
//! The `duckdb` crate provides a synchronous API. All operations are wrapped
//! in `tokio::task::spawn_blocking`. The `Connection` is `Send` but `!Sync`,
//! so it is protected by a `std::sync::Mutex`.
//!
//! ## Transaction Handling
//!
//! Uses a simple `transaction_active` flag. Since all operations go through
//! the same connection (via Mutex), transactions are serialized naturally.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use ::duckdb::{params_from_iter, types::Value as DuckValue, Connection};
use tokio::sync::RwLock;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::sql_safety;
use crate::engine::traits::{DataEngine, StreamEvent, StreamSender};
use crate::engine::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType, ColumnInfo,
    ConnectionConfig, FilterOperator, ForeignKey, Namespace, PaginatedQueryResult, QueryId,
    QueryResult, Row as QRow, RowData, SessionId, SortDirection, TableColumn, TableIndex,
    TableQueryOptions, TableSchema, Value,
};

// ==================== Session & Driver ====================

/// Holds the connection state for a DuckDB session.
pub struct DuckDbSession {
    /// The DuckDB connection, protected by a std Mutex (Connection is !Sync).
    conn: std::sync::Mutex<Connection>,
    /// Whether a transaction is currently active.
    transaction_active: AtomicBool,
    /// The file path to the database (or ":memory:").
    pub db_path: String,
}

/// DuckDB driver implementation.
pub struct DuckDbDriver {
    sessions: Arc<RwLock<HashMap<SessionId, Arc<DuckDbSession>>>>,
}

impl DuckDbDriver {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn get_session(&self, session: SessionId) -> EngineResult<Arc<DuckDbSession>> {
        let sessions = self.sessions.read().await;
        sessions
            .get(&session)
            .cloned()
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))
    }

    fn quote_ident(name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    /// Opens a DuckDB connection from a config.
    fn open_connection(config: &ConnectionConfig) -> EngineResult<Connection> {
        let path = config.host.trim();

        if path == ":memory:" || path == "duckdb::memory:" {
            Connection::open_in_memory()
                .map_err(|e| EngineError::connection_failed(format!("Failed to open DuckDB in-memory: {e}")))
        } else {
            Connection::open(path)
                .map_err(|e| EngineError::connection_failed(format!("Failed to open DuckDB file '{}': {e}", path)))
        }
    }

    /// Validates the DuckDB file path.
    fn validate_path(path: &str) -> EngineResult<()> {
        let path = path.trim();

        if path == ":memory:" || path == "duckdb::memory:" {
            return Ok(());
        }

        if path.eq_ignore_ascii_case("localhost") {
            return Err(EngineError::connection_failed(
                "Invalid DuckDB path: 'localhost'. Please select a valid file path.".to_string(),
            ));
        }

        if path.is_empty() {
            return Err(EngineError::connection_failed(
                "DuckDB path cannot be empty.".to_string(),
            ));
        }

        let path_lower = path.to_lowercase();
        let valid_extensions = [".duckdb", ".db"];
        let has_extension = valid_extensions.iter().any(|ext| path_lower.ends_with(ext));

        if !has_extension && path.contains("://") {
            return Err(EngineError::connection_failed(format!(
                "Invalid DuckDB path format: {}",
                path
            )));
        }

        Ok(())
    }

    /// Runs a synchronous closure on the session's connection inside spawn_blocking.
    async fn with_conn<F, R>(session: &Arc<DuckDbSession>, f: F) -> EngineResult<R>
    where
        F: FnOnce(&Connection) -> EngineResult<R> + Send + 'static,
        R: Send + 'static,
    {
        let session = Arc::clone(session);
        tokio::task::spawn_blocking(move || {
            let conn = session.conn.lock().map_err(|e| {
                EngineError::internal(format!("Failed to lock DuckDB connection: {e}"))
            })?;
            f(&conn)
        })
        .await
        .map_err(|e| EngineError::internal(format!("DuckDB task panicked: {e}")))?
    }
}

impl Default for DuckDbDriver {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Type Conversion ====================

/// Converts a QoreDB Value to a DuckDB Value for parameter binding.
fn value_to_duckdb(value: &Value) -> DuckValue {
    match value {
        Value::Null => DuckValue::Null,
        Value::Bool(b) => DuckValue::Boolean(*b),
        Value::Int(i) => DuckValue::BigInt(*i),
        Value::Float(f) => DuckValue::Double(*f),
        Value::Text(s) => DuckValue::Text(s.clone()),
        Value::Bytes(b) => DuckValue::Blob(b.clone()),
        Value::Json(j) => DuckValue::Text(j.to_string()),
        Value::Array(arr) => DuckValue::Text(serde_json::to_string(arr).unwrap_or_default()),
    }
}

/// Extracts a value from a DuckDB row and converts it to a QoreDB Value.
fn duckdb_value_to_qoredb(row: &::duckdb::Row<'_>, idx: usize) -> Value {
    // Try types in order of likelihood
    if let Ok(v) = row.get::<_, Option<i64>>(idx) {
        return match v {
            Some(i) => Value::Int(i),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.get::<_, Option<i32>>(idx) {
        return match v {
            Some(i) => Value::Int(i as i64),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.get::<_, Option<f64>>(idx) {
        return match v {
            Some(f) => Value::Float(f),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.get::<_, Option<bool>>(idx) {
        return match v {
            Some(b) => Value::Bool(b),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.get::<_, Option<String>>(idx) {
        return match v {
            Some(s) => Value::Text(s),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.get::<_, Option<Vec<u8>>>(idx) {
        return match v {
            Some(b) => Value::Bytes(b),
            None => Value::Null,
        };
    }
    Value::Null
}

/// Executes a SELECT-style query and returns a QueryResult.
///
/// NOTE: DuckDB crate requires that `column_name()` is called AFTER the statement
/// has been executed (i.e., after iterating rows). We collect rows first, then
/// extract column names.
fn execute_select(conn: &Connection, sql: &str, start: Instant) -> EngineResult<QueryResult> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| classify_error(e.to_string()))?;

    // DuckDB crate: column_count/column_name panic before execution.
    // query_map executes internally, so we get column_count from the Row.
    let rows_iter = stmt
        .query_map([], |row| {
            let col_count = row.as_ref().column_count();
            let values: Vec<Value> = (0..col_count)
                .map(|i| duckdb_value_to_qoredb(row, i))
                .collect();
            Ok(QRow { values })
        })
        .map_err(|e| classify_error(e.to_string()))?;

    let mut rows = Vec::new();
    for row_result in rows_iter {
        let row = row_result.map_err(|e| EngineError::execution_error(e.to_string()))?;
        rows.push(row);
    }

    // After iteration, statement has been executed — column_count/column_name work
    let column_count = stmt.column_count();
    let columns: Vec<ColumnInfo> = (0..column_count)
        .map(|i| ColumnInfo {
            name: stmt
                .column_name(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("col_{}", i)),
            data_type: "VARCHAR".to_string(),
            nullable: true,
        })
        .collect();

    let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

    Ok(QueryResult {
        columns,
        rows,
        affected_rows: None,
        execution_time_ms,
    })
}

/// Executes a DML-style statement and returns affected rows.
fn execute_dml(conn: &Connection, sql: &str, start: Instant) -> EngineResult<QueryResult> {
    let affected = conn
        .execute(sql, [])
        .map_err(|e| classify_error(e.to_string()))?;

    let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

    Ok(QueryResult::with_affected_rows(
        affected as u64,
        execution_time_ms,
    ))
}

/// Classifies a DuckDB error message into syntax or execution error.
fn classify_error(msg: String) -> EngineError {
    let lower = msg.to_lowercase();
    if lower.contains("syntax") || lower.contains("parser") {
        EngineError::syntax_error(msg)
    } else {
        EngineError::execution_error(msg)
    }
}

// ==================== DataEngine Implementation ====================

#[async_trait]
impl DataEngine for DuckDbDriver {
    fn driver_id(&self) -> &'static str {
        "duckdb"
    }

    fn driver_name(&self) -> &'static str {
        "DuckDB"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        Self::validate_path(&config.host)?;

        let config = config.clone();
        tokio::task::spawn_blocking(move || {
            let conn = Self::open_connection(&config)?;
            conn.execute("SELECT 1", [])
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| EngineError::internal(format!("DuckDB task panicked: {e}")))?
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        Self::validate_path(&config.host)?;

        let db_path = config.host.clone();
        let config = config.clone();
        let conn = tokio::task::spawn_blocking(move || Self::open_connection(&config))
            .await
            .map_err(|e| EngineError::internal(format!("DuckDB task panicked: {e}")))??;

        let session_id = SessionId::new();
        let session = Arc::new(DuckDbSession {
            conn: std::sync::Mutex::new(conn),
            transaction_active: AtomicBool::new(false),
            db_path,
        });

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id, session);

        Ok(session_id)
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        let mut sessions = self.sessions.write().await;
        sessions
            .remove(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;
        // Connection is dropped when session Arc refcount reaches 0
        Ok(())
    }

    // ==================== Schema Browsing ====================

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let duck_session = self.get_session(session).await?;

        Self::with_conn(&duck_session, |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT schema_name FROM information_schema.schemata \
                     WHERE catalog_name = current_database() \
                     AND schema_name NOT IN ('information_schema', 'pg_catalog') \
                     ORDER BY schema_name",
                )
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let rows = stmt
                .query_map([], |row| {
                    let name: String = row.get(0)?;
                    Ok(name)
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut namespaces = Vec::new();
            for row in rows {
                let schema_name =
                    row.map_err(|e| EngineError::execution_error(e.to_string()))?;
                namespaces.push(Namespace::new(schema_name));
            }

            Ok(namespaces)
        })
        .await
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let duck_session = self.get_session(session).await?;
        let namespace = namespace.clone();
        let schema_name = namespace.schema.clone().unwrap_or_else(|| namespace.database.clone());

        Self::with_conn(&duck_session, move |conn| {
            let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

            // Count query — unified params to avoid closure type mismatch
            let mut count_sql = String::from(
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = ?1",
            );
            let mut count_params: Vec<String> = vec![schema_name.clone()];
            if let Some(ref pattern) = search_pattern {
                count_sql.push_str(" AND table_name LIKE ?2");
                count_params.push(pattern.clone());
            }

            let total_count: i64 = conn
                .query_row(&count_sql, params_from_iter(count_params.iter()), |row| {
                    row.get(0)
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            // Data query with pagination — always use ?1 for schema,
            // optionally ?2 for search pattern via a unified param list.
            let mut data_sql = String::from(
                "SELECT table_name, table_type FROM information_schema.tables \
                 WHERE table_schema = ?1",
            );

            let mut params: Vec<String> = vec![schema_name.clone()];
            if let Some(ref pattern) = search_pattern {
                data_sql.push_str(" AND table_name LIKE ?2");
                params.push(pattern.clone());
            }
            data_sql.push_str(" ORDER BY table_name");

            if let Some(limit) = options.page_size {
                data_sql.push_str(&format!(" LIMIT {}", limit));
                if let Some(page) = options.page {
                    let offset = (page.max(1) - 1) * limit;
                    data_sql.push_str(&format!(" OFFSET {}", offset));
                }
            }

            let mut stmt = conn
                .prepare(&data_sql)
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let rows = stmt
                .query_map(params_from_iter(params.iter()), |row| {
                    let name: String = row.get(0)?;
                    let table_type: String = row.get(1)?;
                    Ok((name, table_type))
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut collections = Vec::new();
            for row in rows {
                let (name, table_type) =
                    row.map_err(|e| EngineError::execution_error(e.to_string()))?;
                let collection_type = if table_type.contains("VIEW") {
                    CollectionType::View
                } else {
                    CollectionType::Table
                };
                collections.push(Collection {
                    namespace: namespace.clone(),
                    name,
                    collection_type,
                });
            }

            Ok(CollectionList {
                collections,
                total_count: total_count as u32,
            })
        })
        .await
    }

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        let duck_session = self.get_session(session).await?;
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());
        let table = table.to_string();

        Self::with_conn(&duck_session, move |conn| {
            // 1. Get columns from information_schema
            let mut col_stmt = conn
                .prepare(
                    "SELECT column_name, data_type, is_nullable, column_default \
                     FROM information_schema.columns \
                     WHERE table_schema = ?1 AND table_name = ?2 \
                     ORDER BY ordinal_position",
                )
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let col_rows = col_stmt
                .query_map([&schema_name, &table], |row| {
                    let name: String = row.get(0)?;
                    let data_type: String = row.get(1)?;
                    let is_nullable: String = row.get(2)?;
                    let default_value: Option<String> = row.get(3)?;
                    Ok((name, data_type, is_nullable, default_value))
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut columns = Vec::new();
            for row in col_rows {
                let (name, data_type, is_nullable, default_value) =
                    row.map_err(|e| EngineError::execution_error(e.to_string()))?;
                columns.push(TableColumn {
                    name,
                    data_type,
                    nullable: is_nullable == "YES",
                    default_value,
                    is_primary_key: false, // Set below from constraints
                });
            }

            // 2. Get primary key constraints
            let mut pk_columns: Vec<String> = Vec::new();
            if let Ok(mut pk_stmt) = conn.prepare(
                "SELECT unnest(constraint_column_names) as col_name \
                 FROM duckdb_constraints() \
                 WHERE schema_name = ?1 AND table_name = ?2 \
                 AND constraint_type = 'PRIMARY KEY'",
            ) {
                if let Ok(pk_rows) = pk_stmt.query_map([&schema_name, &table], |row| {
                    let col: String = row.get(0)?;
                    Ok(col)
                }) {
                    for row in pk_rows {
                        if let Ok(col) = row {
                            pk_columns.push(col);
                        }
                    }
                }
            }

            // Mark PK columns
            for col in &mut columns {
                if pk_columns.contains(&col.name) {
                    col.is_primary_key = true;
                }
            }

            // 3. Get foreign keys
            let mut foreign_keys: Vec<ForeignKey> = Vec::new();
            if let Ok(mut fk_stmt) = conn.prepare(
                "SELECT \
                     unnest(constraint_column_names) as from_col, \
                     unnest(constraint_column_names) as ref_col \
                 FROM duckdb_constraints() \
                 WHERE schema_name = ?1 AND table_name = ?2 \
                 AND constraint_type = 'FOREIGN KEY'",
            ) {
                if let Ok(fk_rows) = fk_stmt.query_map([&schema_name, &table], |row| {
                    let from_col: String = row.get(0)?;
                    let ref_col: String = row.get(1)?;
                    Ok((from_col, ref_col))
                }) {
                    for row in fk_rows {
                        if let Ok((from_col, ref_col)) = row {
                            foreign_keys.push(ForeignKey {
                                column: from_col,
                                referenced_table: String::new(),
                                referenced_column: ref_col,
                                referenced_schema: Some(schema_name.clone()),
                                referenced_database: None,
                                constraint_name: None,
                                is_virtual: false,
                            });
                        }
                    }
                }
            }

            // 4. Get indexes
            let mut indexes: Vec<TableIndex> = Vec::new();
            if let Ok(mut idx_stmt) = conn.prepare(
                "SELECT index_name, is_unique, sql \
                 FROM duckdb_indexes() \
                 WHERE schema_name = ?1 AND table_name = ?2",
            ) {
                if let Ok(idx_rows) = idx_stmt.query_map([&schema_name, &table], |row| {
                    let name: String = row.get(0)?;
                    let is_unique: bool = row.get(1)?;
                    let sql: Option<String> = row.get(2)?;
                    Ok((name, is_unique, sql))
                }) {
                    for row in idx_rows {
                        if let Ok((name, is_unique, sql)) = row {
                            // Extract column names from CREATE INDEX SQL
                            let idx_columns = extract_index_columns(sql.as_deref());
                            indexes.push(TableIndex {
                                name,
                                columns: idx_columns,
                                is_unique,
                                is_primary: false,
                            });
                        }
                    }
                }
            }

            // 5. Get row count estimate (fast via DuckDB statistics)
            let row_count_estimate = conn
                .query_row(
                    &format!(
                        "SELECT estimated_size FROM duckdb_tables() \
                         WHERE schema_name = '{}' AND table_name = '{}'",
                        schema_name.replace('\'', "''"),
                        table.replace('\'', "''")
                    ),
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .ok()
                .map(|c| c as u64);

            Ok(TableSchema {
                columns,
                primary_key: if pk_columns.is_empty() {
                    None
                } else {
                    Some(pk_columns)
                },
                foreign_keys,
                row_count_estimate,
                indexes,
            })
        })
        .await
    }

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());
        let query = format!(
            "SELECT * FROM {}.{} LIMIT {}",
            Self::quote_ident(&schema_name),
            Self::quote_ident(table),
            limit
        );
        self.execute(session, &query, QueryId::new()).await
    }

    // ==================== Query Execution ====================

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        self.execute_in_namespace(session, None, query, query_id)
            .await
    }

    async fn execute_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        _query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let duck_session = self.get_session(session).await?;
        let query = query.to_string();
        let returns_rows = sql_safety::returns_rows("duckdb", &query)
            .unwrap_or_else(|_| sql_safety::is_select_prefix(&query));

        Self::with_conn(&duck_session, move |conn| {
            // Set schema if namespace provided
            if let Some(ns) = &namespace {
                let schema = ns.schema.as_deref().unwrap_or(&ns.database);
                conn.execute(&format!("SET schema = '{}'", schema.replace('\'', "''")), [])
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;
            }

            let start = Instant::now();
            if returns_rows {
                execute_select(conn, &query, start)
            } else {
                execute_dml(conn, &query, start)
            }
        })
        .await
    }

    async fn execute_stream(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        self.execute_stream_in_namespace(session, None, query, query_id, sender)
            .await
    }

    async fn execute_stream_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        _query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        let duck_session = self.get_session(session).await?;
        let query = query.to_string();

        let returns_rows = sql_safety::returns_rows("duckdb", &query)
            .unwrap_or_else(|_| sql_safety::is_select_prefix(&query));

        if !returns_rows {
            let result = self
                .execute_in_namespace(session, namespace, &query, QueryId::new())
                .await?;
            let _ = sender
                .send(StreamEvent::Done(result.affected_rows.unwrap_or(0)))
                .await;
            return Ok(());
        }

        // Collect results in spawn_blocking, then stream them out
        let (columns, rows) = Self::with_conn(&duck_session, move |conn| {
            // Set schema if namespace provided
            if let Some(ns) = &namespace {
                let schema = ns.schema.as_deref().unwrap_or(&ns.database);
                conn.execute(&format!("SET schema = '{}'", schema.replace('\'', "''")), [])
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;
            }

            let mut stmt = conn
                .prepare(&query)
                .map_err(|e| classify_error(e.to_string()))?;

            // DuckDB crate: column_count/column_name panic before execution.
            let rows_iter = stmt
                .query_map([], |row| {
                    let col_count = row.as_ref().column_count();
                    let values: Vec<Value> = (0..col_count)
                        .map(|i| duckdb_value_to_qoredb(row, i))
                        .collect();
                    Ok(QRow { values })
                })
                .map_err(|e| classify_error(e.to_string()))?;

            let mut rows = Vec::new();
            for row_result in rows_iter {
                let row = row_result.map_err(|e| EngineError::execution_error(e.to_string()))?;
                rows.push(row);
            }

            // After iteration, statement has been executed
            let column_count = stmt.column_count();
            let columns: Vec<ColumnInfo> = (0..column_count)
                .map(|i| ColumnInfo {
                    name: stmt
                        .column_name(i)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|_| format!("col_{}", i)),
                    data_type: "VARCHAR".to_string(),
                    nullable: true,
                })
                .collect();

            Ok((columns, rows))
        })
        .await?;

        // Stream results to the channel
        if sender.send(StreamEvent::Columns(columns)).await.is_err() {
            return Ok(());
        }

        let row_count = rows.len() as u64;
        for row in rows {
            if sender.send(StreamEvent::Row(row)).await.is_err() {
                return Ok(());
            }
        }

        let _ = sender.send(StreamEvent::Done(row_count)).await;
        Ok(())
    }

    // ==================== Table Querying ====================

    async fn query_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        let duck_session = self.get_session(session).await?;
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());
        let table = table.to_string();

        let page = options.effective_page();
        let page_size = options.effective_page_size();
        let offset = options.offset();

        Self::with_conn(&duck_session, move |conn| {
            let start = Instant::now();
            let table_ref = format!(
                "{}.{}",
                Self::quote_ident(&schema_name),
                Self::quote_ident(&table)
            );

            // Build WHERE clause
            let mut where_clauses: Vec<String> = Vec::new();
            let mut bind_values: Vec<DuckValue> = Vec::new();

            if let Some(filters) = &options.filters {
                for filter in filters {
                    let col_ident = Self::quote_ident(&filter.column);
                    let clause = match filter.operator {
                        FilterOperator::Eq => {
                            bind_values.push(value_to_duckdb(&filter.value));
                            format!("{} = ?", col_ident)
                        }
                        FilterOperator::Neq => {
                            bind_values.push(value_to_duckdb(&filter.value));
                            format!("{} != ?", col_ident)
                        }
                        FilterOperator::Gt => {
                            bind_values.push(value_to_duckdb(&filter.value));
                            format!("{} > ?", col_ident)
                        }
                        FilterOperator::Gte => {
                            bind_values.push(value_to_duckdb(&filter.value));
                            format!("{} >= ?", col_ident)
                        }
                        FilterOperator::Lt => {
                            bind_values.push(value_to_duckdb(&filter.value));
                            format!("{} < ?", col_ident)
                        }
                        FilterOperator::Lte => {
                            bind_values.push(value_to_duckdb(&filter.value));
                            format!("{} <= ?", col_ident)
                        }
                        FilterOperator::Like => {
                            bind_values.push(value_to_duckdb(&filter.value));
                            format!("{} ILIKE ?", col_ident)
                        }
                        FilterOperator::IsNull => format!("{} IS NULL", col_ident),
                        FilterOperator::IsNotNull => format!("{} IS NOT NULL", col_ident),
                    };
                    where_clauses.push(clause);
                }
            }

            // Search across text columns
            if let Some(ref search_term) = options.search {
                if !search_term.trim().is_empty() {
                    // Get column info for text columns
                    if let Ok(mut col_stmt) = conn.prepare(
                        "SELECT column_name, data_type FROM information_schema.columns \
                         WHERE table_schema = ?1 AND table_name = ?2",
                    ) {
                        if let Ok(col_rows) = col_stmt.query_map([&schema_name, &table], |row| {
                            let name: String = row.get(0)?;
                            let dtype: String = row.get(1)?;
                            Ok((name, dtype))
                        }) {
                            let mut search_clauses: Vec<String> = Vec::new();
                            for row in col_rows {
                                if let Ok((col_name, dtype)) = row {
                                    let upper = dtype.to_uppercase();
                                    // Skip binary/unsearchable types
                                    if upper.contains("BLOB") {
                                        continue;
                                    }

                                    let col_ident = Self::quote_ident(&col_name);
                                    bind_values.push(DuckValue::Text(format!(
                                        "%{}%",
                                        search_term
                                    )));

                                    // Text columns can use ILIKE directly, others need CAST
                                    if upper.contains("VARCHAR")
                                        || upper.contains("TEXT")
                                        || upper.contains("CHAR")
                                    {
                                        search_clauses.push(format!("{} ILIKE ?", col_ident));
                                    } else {
                                        search_clauses.push(format!("CAST({} AS VARCHAR) ILIKE ?", col_ident));
                                    }
                                }
                            }
                            if !search_clauses.is_empty() {
                                where_clauses
                                    .push(format!("({})", search_clauses.join(" OR ")));
                            }
                        }
                    }
                }
            }

            let where_sql = if where_clauses.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", where_clauses.join(" AND "))
            };

            let order_sql = if let Some(sort_col) = &options.sort_column {
                let sort_ident = Self::quote_ident(sort_col);
                let direction = match options.sort_direction.unwrap_or_default() {
                    SortDirection::Asc => "ASC",
                    SortDirection::Desc => "DESC",
                };
                format!(" ORDER BY {} {}", sort_ident, direction)
            } else {
                String::new()
            };

            // COUNT query
            let count_sql = format!("SELECT COUNT(*) AS cnt FROM {}{}", table_ref, where_sql);
            let total_rows: i64 = conn
                .query_row(
                    &count_sql,
                    params_from_iter(bind_values.iter()),
                    |row| row.get(0),
                )
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            let total_rows = total_rows.max(0) as u64;

            // Data query
            let data_sql = format!(
                "SELECT * FROM {}{}{} LIMIT {} OFFSET {}",
                table_ref, where_sql, order_sql, page_size, offset
            );

            let mut stmt = conn
                .prepare(&data_sql)
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            // DuckDB crate: column_count/column_name panic before execution.
            let rows_iter = stmt
                .query_map(params_from_iter(bind_values.iter()), |row| {
                    let col_count = row.as_ref().column_count();
                    let values: Vec<Value> = (0..col_count)
                        .map(|i| duckdb_value_to_qoredb(row, i))
                        .collect();
                    Ok(QRow { values })
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut rows = Vec::new();
            for row_result in rows_iter {
                let row = row_result.map_err(|e| EngineError::execution_error(e.to_string()))?;
                rows.push(row);
            }

            // After iteration, statement has been executed
            let column_count = stmt.column_count();
            let columns: Vec<ColumnInfo> = (0..column_count)
                .map(|i| ColumnInfo {
                    name: stmt
                        .column_name(i)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|_| format!("col_{}", i)),
                    data_type: "VARCHAR".to_string(),
                    nullable: true,
                })
                .collect();

            let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

            let result = QueryResult {
                columns,
                rows,
                affected_rows: None,
                execution_time_ms,
            };

            Ok(PaginatedQueryResult::new(result, total_rows, page, page_size))
        })
        .await
    }

    async fn peek_foreign_key(
        &self,
        session: SessionId,
        namespace: &Namespace,
        foreign_key: &ForeignKey,
        value: &Value,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let duck_session = self.get_session(session).await?;
        let limit = limit.max(1).min(50);
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());
        let ref_table = foreign_key.referenced_table.clone();
        let ref_column = foreign_key.referenced_column.clone();
        let duck_value = value_to_duckdb(value);

        Self::with_conn(&duck_session, move |conn| {
            let sql = format!(
                "SELECT * FROM {}.{} WHERE {} = ?1 LIMIT {}",
                Self::quote_ident(&schema_name),
                Self::quote_ident(&ref_table),
                Self::quote_ident(&ref_column),
                limit
            );

            let start = Instant::now();
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            // DuckDB crate: column_count/column_name panic before execution.
            let rows_iter = stmt
                .query_map([&duck_value], |row| {
                    let col_count = row.as_ref().column_count();
                    let values: Vec<Value> = (0..col_count)
                        .map(|i| duckdb_value_to_qoredb(row, i))
                        .collect();
                    Ok(QRow { values })
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut rows = Vec::new();
            for row_result in rows_iter {
                let row = row_result.map_err(|e| EngineError::execution_error(e.to_string()))?;
                rows.push(row);
            }

            // After iteration, statement has been executed
            let column_count = stmt.column_count();
            let columns: Vec<ColumnInfo> = (0..column_count)
                .map(|i| ColumnInfo {
                    name: stmt
                        .column_name(i)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|_| format!("col_{}", i)),
                    data_type: "VARCHAR".to_string(),
                    nullable: true,
                })
                .collect();

            let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

            Ok(QueryResult {
                columns,
                rows,
                affected_rows: None,
                execution_time_ms,
            })
        })
        .await
    }

    // ==================== Schema Management ====================

    async fn create_database(
        &self,
        session: SessionId,
        name: &str,
        _options: Option<Value>,
    ) -> EngineResult<()> {
        let duck_session = self.get_session(session).await?;
        let name = name.to_string();

        Self::with_conn(&duck_session, move |conn| {
            let sql = format!("CREATE SCHEMA {}", Self::quote_ident(&name));
            conn.execute(&sql, [])
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            Ok(())
        })
        .await
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        let duck_session = self.get_session(session).await?;
        let name = name.to_string();

        Self::with_conn(&duck_session, move |conn| {
            let sql = format!("DROP SCHEMA {} CASCADE", Self::quote_ident(&name));
            conn.execute(&sql, [])
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            Ok(())
        })
        .await
    }

    // ==================== Transaction Methods ====================

    async fn begin_transaction(&self, session: SessionId) -> EngineResult<()> {
        let duck_session = self.get_session(session).await?;

        if duck_session.transaction_active.load(Ordering::Acquire) {
            return Err(EngineError::transaction_error(
                "A transaction is already active on this session",
            ));
        }

        Self::with_conn(&duck_session, |conn| {
            conn.execute("BEGIN TRANSACTION", [])
                .map_err(|e| EngineError::execution_error(format!("Failed to begin transaction: {e}")))?;
            Ok(())
        })
        .await?;

        duck_session.transaction_active.store(true, Ordering::Release);
        Ok(())
    }

    async fn commit(&self, session: SessionId) -> EngineResult<()> {
        let duck_session = self.get_session(session).await?;

        if !duck_session.transaction_active.load(Ordering::Acquire) {
            return Err(EngineError::transaction_error(
                "No active transaction to commit",
            ));
        }

        Self::with_conn(&duck_session, |conn| {
            conn.execute("COMMIT", [])
                .map_err(|e| EngineError::execution_error(format!("Failed to commit transaction: {e}")))?;
            Ok(())
        })
        .await?;

        duck_session.transaction_active.store(false, Ordering::Release);
        Ok(())
    }

    async fn rollback(&self, session: SessionId) -> EngineResult<()> {
        let duck_session = self.get_session(session).await?;

        if !duck_session.transaction_active.load(Ordering::Acquire) {
            return Err(EngineError::transaction_error(
                "No active transaction to rollback",
            ));
        }

        Self::with_conn(&duck_session, |conn| {
            conn.execute("ROLLBACK", [])
                .map_err(|e| EngineError::execution_error(format!("Failed to rollback transaction: {e}")))?;
            Ok(())
        })
        .await?;

        duck_session.transaction_active.store(false, Ordering::Release);
        Ok(())
    }

    fn supports_transactions(&self) -> bool {
        true
    }

    // ==================== Mutation Methods ====================

    async fn insert_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let duck_session = self.get_session(session).await?;
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());
        let table = table.to_string();
        let data = data.clone();

        Self::with_conn(&duck_session, move |conn| {
            let start = Instant::now();
            let table_ref = format!(
                "{}.{}",
                Self::quote_ident(&schema_name),
                Self::quote_ident(&table)
            );

            let mut keys: Vec<&String> = data.columns.keys().collect();
            keys.sort();

            let sql = if keys.is_empty() {
                format!("INSERT INTO {} DEFAULT VALUES", table_ref)
            } else {
                let cols_str = keys
                    .iter()
                    .map(|k| Self::quote_ident(k))
                    .collect::<Vec<_>>()
                    .join(", ");
                let params_str: Vec<String> = (1..=keys.len()).map(|i| format!("?{}", i)).collect();
                format!(
                    "INSERT INTO {} ({}) VALUES ({})",
                    table_ref,
                    cols_str,
                    params_str.join(", ")
                )
            };

            let duck_values: Vec<DuckValue> = keys
                .iter()
                .map(|k| value_to_duckdb(data.columns.get(*k).unwrap()))
                .collect();

            let affected = conn
                .execute(&sql, params_from_iter(duck_values.iter()))
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            Ok(QueryResult::with_affected_rows(
                affected as u64,
                start.elapsed().as_micros() as f64 / 1000.0,
            ))
        })
        .await
    }

    async fn update_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let duck_session = self.get_session(session).await?;
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());
        let table = table.to_string();
        let primary_key = primary_key.clone();
        let data = data.clone();

        if primary_key.columns.is_empty() {
            return Err(EngineError::execution_error(
                "Primary key required for update operations".to_string(),
            ));
        }

        if data.columns.is_empty() {
            return Ok(QueryResult::with_affected_rows(0, 0.0));
        }

        Self::with_conn(&duck_session, move |conn| {
            let start = Instant::now();
            let table_ref = format!(
                "{}.{}",
                Self::quote_ident(&schema_name),
                Self::quote_ident(&table)
            );

            let mut data_keys: Vec<&String> = data.columns.keys().collect();
            data_keys.sort();
            let mut pk_keys: Vec<&String> = primary_key.columns.keys().collect();
            pk_keys.sort();

            let set_clauses: Vec<String> = data_keys
                .iter()
                .enumerate()
                .map(|(i, k)| format!("{}=?{}", Self::quote_ident(k), i + 1))
                .collect();

            let where_clauses: Vec<String> = pk_keys
                .iter()
                .enumerate()
                .map(|(i, k)| format!("{}=?{}", Self::quote_ident(k), data_keys.len() + i + 1))
                .collect();

            let sql = format!(
                "UPDATE {} SET {} WHERE {}",
                table_ref,
                set_clauses.join(", "),
                where_clauses.join(" AND ")
            );

            let mut duck_values: Vec<DuckValue> = Vec::new();
            for k in &data_keys {
                duck_values.push(value_to_duckdb(data.columns.get(*k).unwrap()));
            }
            for k in &pk_keys {
                duck_values.push(value_to_duckdb(primary_key.columns.get(*k).unwrap()));
            }

            let affected = conn
                .execute(&sql, params_from_iter(duck_values.iter()))
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            Ok(QueryResult::with_affected_rows(
                affected as u64,
                start.elapsed().as_micros() as f64 / 1000.0,
            ))
        })
        .await
    }

    async fn delete_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
    ) -> EngineResult<QueryResult> {
        let duck_session = self.get_session(session).await?;
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());
        let table = table.to_string();
        let primary_key = primary_key.clone();

        if primary_key.columns.is_empty() {
            return Err(EngineError::execution_error(
                "Primary key required for delete operations".to_string(),
            ));
        }

        Self::with_conn(&duck_session, move |conn| {
            let start = Instant::now();
            let table_ref = format!(
                "{}.{}",
                Self::quote_ident(&schema_name),
                Self::quote_ident(&table)
            );

            let mut pk_keys: Vec<&String> = primary_key.columns.keys().collect();
            pk_keys.sort();

            let where_clauses: Vec<String> = pk_keys
                .iter()
                .enumerate()
                .map(|(i, k)| format!("{}=?{}", Self::quote_ident(k), i + 1))
                .collect();

            let sql = format!(
                "DELETE FROM {} WHERE {}",
                table_ref,
                where_clauses.join(" AND ")
            );

            let duck_values: Vec<DuckValue> = pk_keys
                .iter()
                .map(|k| value_to_duckdb(primary_key.columns.get(*k).unwrap()))
                .collect();

            let affected = conn
                .execute(&sql, params_from_iter(duck_values.iter()))
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            Ok(QueryResult::with_affected_rows(
                affected as u64,
                start.elapsed().as_micros() as f64 / 1000.0,
            ))
        })
        .await
    }

    fn supports_mutations(&self) -> bool {
        true
    }

    // ==================== Capability Flags ====================

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_explain(&self) -> bool {
        true
    }

    async fn cancel(&self, _session: SessionId, _query_id: Option<QueryId>) -> EngineResult<()> {
        Err(EngineError::not_supported(
            "DuckDB does not support query cancellation",
        ))
    }

    fn cancel_support(&self) -> CancelSupport {
        CancelSupport::None
    }
}

// ==================== Helpers ====================

/// Extracts column names from a CREATE INDEX SQL statement.
fn extract_index_columns(sql: Option<&str>) -> Vec<String> {
    let Some(sql) = sql else {
        return Vec::new();
    };

    // Parse "... ON table (col1, col2, ...)" pattern
    if let Some(start) = sql.rfind('(') {
        if let Some(end) = sql.rfind(')') {
            if start < end {
                return sql[start + 1..end]
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
    }
    Vec::new()
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connect_memory() {
        let driver = DuckDbDriver::new();

        let config = ConnectionConfig {
            driver: "duckdb".to_string(),
            host: ":memory:".to_string(),
            port: 0,
            username: String::new(),
            password: String::new(),
            database: None,
            ssl: false,
            environment: "development".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
        };

        let session_id = driver.connect(&config).await.unwrap();
        driver.disconnect(session_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_create_and_query() {
        let driver = DuckDbDriver::new();

        let config = ConnectionConfig {
            driver: "duckdb".to_string(),
            host: ":memory:".to_string(),
            port: 0,
            username: String::new(),
            password: String::new(),
            database: None,
            ssl: false,
            environment: "development".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
        };

        let session_id = driver.connect(&config).await.unwrap();

        // Create table
        let result = driver
            .execute(
                session_id,
                "CREATE TABLE test (id INTEGER PRIMARY KEY, name VARCHAR, value DOUBLE)",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert!(result.affected_rows.is_some());

        // Insert data
        let result = driver
            .execute(
                session_id,
                "INSERT INTO test VALUES (1, 'hello', 3.14)",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert_eq!(result.affected_rows, Some(1));

        // Query data
        let result = driver
            .execute(session_id, "SELECT * FROM test", QueryId::new())
            .await
            .unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.columns.len(), 3);

        driver.disconnect(session_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_list_namespaces() {
        let driver = DuckDbDriver::new();

        let config = ConnectionConfig {
            driver: "duckdb".to_string(),
            host: ":memory:".to_string(),
            port: 0,
            username: String::new(),
            password: String::new(),
            database: None,
            ssl: false,
            environment: "development".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
        };

        let session_id = driver.connect(&config).await.unwrap();

        let namespaces = driver.list_namespaces(session_id).await.unwrap();
        // DuckDB always has at least the 'main' schema
        assert!(namespaces.iter().any(|n| n.database == "main"));

        driver.disconnect(session_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_transactions() {
        let driver = DuckDbDriver::new();

        let config = ConnectionConfig {
            driver: "duckdb".to_string(),
            host: ":memory:".to_string(),
            port: 0,
            username: String::new(),
            password: String::new(),
            database: None,
            ssl: false,
            environment: "development".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
        };

        let session_id = driver.connect(&config).await.unwrap();

        // Create table
        driver
            .execute(
                session_id,
                "CREATE TABLE test (id INTEGER PRIMARY KEY, name VARCHAR)",
                QueryId::new(),
            )
            .await
            .unwrap();

        // Begin transaction
        driver.begin_transaction(session_id).await.unwrap();

        // Insert within transaction
        driver
            .execute(
                session_id,
                "INSERT INTO test VALUES (1, 'tx_test')",
                QueryId::new(),
            )
            .await
            .unwrap();

        // Rollback
        driver.rollback(session_id).await.unwrap();

        // Verify rollback
        let result = driver
            .execute(session_id, "SELECT * FROM test", QueryId::new())
            .await
            .unwrap();
        assert_eq!(result.rows.len(), 0);

        driver.disconnect(session_id).await.unwrap();
    }

    #[test]
    fn test_validate_path() {
        assert!(DuckDbDriver::validate_path(":memory:").is_ok());
        assert!(DuckDbDriver::validate_path("/tmp/test.duckdb").is_ok());
        assert!(DuckDbDriver::validate_path("/tmp/test.db").is_ok());
        assert!(DuckDbDriver::validate_path("localhost").is_err());
        assert!(DuckDbDriver::validate_path("").is_err());
    }

    #[test]
    fn test_extract_index_columns() {
        assert_eq!(
            extract_index_columns(Some("CREATE INDEX idx ON t (\"a\", \"b\")")),
            vec!["a", "b"]
        );
        assert_eq!(
            extract_index_columns(Some("CREATE INDEX idx ON t (col1)")),
            vec!["col1"]
        );
        assert!(extract_index_columns(None).is_empty());
    }
}

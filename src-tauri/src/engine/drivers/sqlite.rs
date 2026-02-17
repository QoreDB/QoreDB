// SPDX-License-Identifier: Apache-2.0

//! SQLite Driver
//!
//! Implements the DataEngine trait for SQLite databases using SQLx.
//!
//! ## SQLite Specifics
//!
//! - SQLite is a file-based database, so `host` in ConnectionConfig contains the file path
//! - Supports `:memory:` for in-memory databases
//! - Uses WAL mode for better concurrency
//! - Single namespace per file (no schema switching)
//!
//! ## Transaction Handling
//!
//! Same architecture as PostgreSQL/MySQL: dedicated connection acquired from pool
//! on BEGIN and released on COMMIT/ROLLBACK.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use sqlx::pool::PoolConnection;
use sqlx::sqlite::{Sqlite, SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::{Column, Row, TypeInfo};
use tokio::sync::{Mutex, RwLock};
use futures::StreamExt;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::sql_safety;
use crate::engine::traits::{DataEngine, StreamEvent, StreamSender};
use crate::engine::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType, ColumnInfo,
    ConnectionConfig, FilterOperator, ForeignKey, Namespace, PaginatedQueryResult, QueryId,
    QueryResult, Row as QRow, RowData, SessionId, SortDirection, TableColumn, TableIndex,
    TableQueryOptions, TableSchema, Value,
    Trigger, TriggerList, TriggerListOptions, TriggerTiming, TriggerEvent,
};

/// Holds the connection state for a SQLite session.
pub struct SqliteSession {
    pub pool: SqlitePool,
    pub transaction_conn: Mutex<Option<PoolConnection<Sqlite>>>,
    pub db_path: String,
}

impl SqliteSession {
    pub fn new(pool: SqlitePool, db_path: String) -> Self {
        Self {
            pool,
            transaction_conn: Mutex::new(None),
            db_path,
        }
    }
}

/// SQLite driver implementation
pub struct SqliteDriver {
    sessions: Arc<RwLock<HashMap<SessionId, Arc<SqliteSession>>>>,
}

impl SqliteDriver {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn create_pool(
        config: &ConnectionConfig,
        max_connections: u32,
        min_connections: u32,
        acquire_timeout_secs: u64,
        run_test_query: bool,
    ) -> EngineResult<SqlitePool> {
        let opts = Self::build_connect_options(config);

        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .acquire_timeout(std::time::Duration::from_secs(acquire_timeout_secs))
            .connect_with(opts)
            .await
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        if run_test_query {
            sqlx::query("SELECT 1")
                .execute(&pool)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
        }

        Ok(pool)
    }

    async fn get_session(&self, session: SessionId) -> EngineResult<Arc<SqliteSession>> {
        let sessions = self.sessions.read().await;
        sessions
            .get(&session)
            .cloned()
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))
    }

    fn quote_ident(name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    fn build_connect_options(config: &ConnectionConfig) -> SqliteConnectOptions {
        use std::str::FromStr;

        let path = &config.host;

        // Build connection string
        let conn_str = if path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite:{}", path)
        };

        SqliteConnectOptions::from_str(&conn_str)
            .unwrap_or_else(|_| SqliteConnectOptions::new().filename(path))
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(30))
    }

    /// Helper to bind a Value to a SQLite query
    fn bind_param<'q>(
        query: sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
        value: &'q Value,
    ) -> sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
        match value {
            Value::Null => query.bind(Option::<String>::None),
            Value::Bool(b) => query.bind(b),
            Value::Int(i) => query.bind(i),
            Value::Float(f) => query.bind(f),
            Value::Text(s) => query.bind(s.as_str()),
            Value::Bytes(b) => query.bind(b.as_slice()),
            Value::Json(j) => query.bind(j.to_string()),
            Value::Array(_) => query.bind(Option::<String>::None),
        }
    }

    /// Converts a SQLx row to our universal Row type
    fn convert_row(sqlite_row: &SqliteRow) -> QRow {
        let values: Vec<Value> = sqlite_row
            .columns()
            .iter()
            .map(|col| Self::extract_value(sqlite_row, col.ordinal()))
            .collect();

        QRow { values }
    }

    /// Extracts a value from a SqliteRow at the given index
    ///
    /// SQLite has dynamic typing, so we try multiple types in order of likelihood
    fn extract_value(row: &SqliteRow, idx: usize) -> Value {
        // Try integer first (most common)
        if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
            return v.map(Value::Int).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<i32>, _>(idx) {
            return v.map(|i| Value::Int(i as i64)).unwrap_or(Value::Null);
        }
        // Try float
        if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
            return v.map(Value::Float).unwrap_or(Value::Null);
        }
        // Try bool (SQLite stores as 0/1)
        if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
            return v.map(Value::Bool).unwrap_or(Value::Null);
        }
        // Try string
        if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
            return v.map(Value::Text).unwrap_or(Value::Null);
        }
        // Try bytes (BLOB)
        if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
            return v.map(Value::Bytes).unwrap_or(Value::Null);
        }

        Value::Null
    }

    /// Gets column info from a SqliteRow
    fn get_column_info(row: &SqliteRow) -> Vec<ColumnInfo> {
        row.columns()
            .iter()
            .map(|col| ColumnInfo {
                name: col.name().to_string(),
                data_type: col.type_info().name().to_string(),
                nullable: true, // SQLite doesn't easily expose nullability from row metadata
            })
            .collect()
    }
    /// Validates the SQLite connection path
    fn validate_path(path: &str) -> EngineResult<()> {
        let path = path.trim();
        
        if path == ":memory:" || path == "sqlite::memory:" {
            return Ok(());
        }

        if path.eq_ignore_ascii_case("localhost") {
            return Err(EngineError::connection_failed(
                "Invalid SQLite path: 'localhost'. Please select a valid file path.".to_string(),
            ));
        }

        if path.is_empty() {
             return Err(EngineError::connection_failed(
                "SQLite path cannot be empty.".to_string(),
            ));
        }
        
        let path_lower = path.to_lowercase();
        let valid_extensions = [".db", ".sqlite", ".sqlite3", ".db3", ".s3db", ".sl3"];
        let has_extension = valid_extensions.iter().any(|ext| path_lower.ends_with(ext));
        
        if !has_extension && path.contains("://") {
             return Err(EngineError::connection_failed(
                format!("Invalid SQLite path format: {}", path)
            ));
        }

        Ok(())
    }
}

impl Default for SqliteDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for SqliteDriver {
    fn driver_id(&self) -> &'static str {
        "sqlite"
    }

    fn driver_name(&self) -> &'static str {
        "SQLite"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        Self::validate_path(&config.host)?;
        let pool = Self::create_pool(config, 1, 0, 10, true).await?;
        pool.close().await;
        Ok(())
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        Self::validate_path(&config.host)?;

        let max_connections = config.pool_max_connections.unwrap_or(5);
        let min_connections = config.pool_min_connections.unwrap_or(0);
        let acquire_timeout = config.pool_acquire_timeout_secs.unwrap_or(30);

        let pool = Self::create_pool(
            config,
            max_connections,
            min_connections,
            acquire_timeout as u64,
            false,
        )
        .await?;

        let session_id = SessionId::new();
        let session = Arc::new(SqliteSession::new(pool, config.host.clone()));

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id, session);

        Ok(session_id)
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        let session = {
            let mut sessions = self.sessions.write().await;
            sessions
                .remove(&session)
                .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?
        };

        {
            let mut tx = session.transaction_conn.lock().await;
            tx.take();
        }

        session.pool.close().await;
        Ok(())
    }

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let sqlite_session = self.get_session(session).await?;

        // SQLite has only one "database" per file
        // Use the filename as the namespace name
        let db_name = if sqlite_session.db_path == ":memory:" {
            "memory".to_string()
        } else {
            std::path::Path::new(&sqlite_session.db_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("main")
                .to_string()
        };

        Ok(vec![Namespace::new(db_name)])
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let sqlite_session = self.get_session(session).await?;
        let pool = &sqlite_session.pool;

        let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

        // Get total count
        let count_query = r#"
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type = 'table'
            AND name NOT LIKE 'sqlite_%'
            AND ($1 IS NULL OR name LIKE $2)
        "#;

        let count_row: (i64,) = sqlx::query_as(count_query)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_one(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let total_count = count_row.0;

        // Get paginated results
        let mut query_str = r#"
            SELECT name, type
            FROM sqlite_master
            WHERE type IN ('table', 'view')
            AND name NOT LIKE 'sqlite_%'
            AND ($1 IS NULL OR name LIKE $2)
            ORDER BY name
        "#
        .to_string();

        if let Some(limit) = options.page_size {
            query_str.push_str(&format!(" LIMIT {}", limit));
            if let Some(page) = options.page {
                let offset = (page.max(1) - 1) * limit;
                query_str.push_str(&format!(" OFFSET {}", offset));
            }
        }

        let rows: Vec<(String, String)> = sqlx::query_as(&query_str)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_all(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let collections = rows
            .into_iter()
            .map(|(name, obj_type)| {
                let collection_type = match obj_type.as_str() {
                    "view" => CollectionType::View,
                    _ => CollectionType::Table,
                };
                Collection {
                    namespace: namespace.clone(),
                    name,
                    collection_type,
                }
            })
            .collect();

        Ok(CollectionList {
            collections,
            total_count: total_count as u32,
        })
    }

    async fn list_triggers(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: TriggerListOptions,
    ) -> EngineResult<TriggerList> {
        let sqlite_session = self.get_session(session).await?;
        let pool = &sqlite_session.pool;

        let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

        let count_row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type = 'trigger'
              AND (? IS NULL OR name LIKE ?)
            "#,
        )
        .bind(&search_pattern)
        .bind(&search_pattern)
        .fetch_one(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let total_count = count_row.0;

        let mut query_str = r#"
            SELECT name, tbl_name, sql
            FROM sqlite_master
            WHERE type = 'trigger'
              AND (? IS NULL OR name LIKE ?)
            ORDER BY name
        "#
        .to_string();

        if let Some(limit) = options.page_size {
            query_str.push_str(&format!(" LIMIT {}", limit));
            if let Some(page) = options.page {
                let offset = (page.max(1) - 1) * limit;
                query_str.push_str(&format!(" OFFSET {}", offset));
            }
        }

        let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(&query_str)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_all(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let triggers = rows
            .into_iter()
            .map(|(name, table_name, sql)| {
                let sql_upper = sql.as_deref().unwrap_or("").to_uppercase();

                let timing = if sql_upper.contains("INSTEAD OF") {
                    TriggerTiming::InsteadOf
                } else if sql_upper.contains("BEFORE") {
                    TriggerTiming::Before
                } else {
                    TriggerTiming::After
                };

                let mut events = Vec::new();
                if sql_upper.contains("INSERT") { events.push(TriggerEvent::Insert); }
                if sql_upper.contains("UPDATE") { events.push(TriggerEvent::Update); }
                if sql_upper.contains("DELETE") { events.push(TriggerEvent::Delete); }
                if events.is_empty() { events.push(TriggerEvent::Insert); }

                Trigger {
                    namespace: namespace.clone(),
                    name,
                    table_name,
                    timing,
                    events,
                    enabled: true,
                    function_name: None,
                }
            })
            .collect();

        Ok(TriggerList {
            triggers,
            total_count: total_count as u32,
        })
    }

    fn supports_triggers(&self) -> bool {
        true
    }

    async fn create_database(
        &self,
        _session: SessionId,
        _name: &str,
        _options: Option<Value>,
    ) -> EngineResult<()> {
        // SQLite doesn't support creating databases within a connection
        // A new database is created by opening a new file
        Err(EngineError::not_supported(
            "SQLite databases are created by opening a new file path",
        ))
    }

    async fn drop_database(&self, _session: SessionId, _name: &str) -> EngineResult<()> {
        // SQLite doesn't support dropping databases
        Err(EngineError::not_supported(
            "SQLite databases are deleted by removing the file",
        ))
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
        _namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        let sqlite_session = self.get_session(session).await?;

        // Use pool for streaming
        let mut conn = sqlite_session
            .pool
            .acquire()
            .await
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        // Check if query returns rows
        let returns_rows = sql_safety::returns_rows(self.driver_id(), query)
            .unwrap_or_else(|_| sql_safety::is_select_prefix(query));

        if !returns_rows {
            // Fallback to normal execute
            let result = self
                .execute_in_namespace(session, None, query, query_id)
                .await?;
            let _ = sender
                .send(StreamEvent::Done(result.affected_rows.unwrap_or(0)))
                .await;
            return Ok(());
        }

        let mut stream = sqlx::query(query).fetch(&mut *conn);
        let mut columns_sent = false;
        let mut row_count = 0;
        let mut stream_error: Option<String> = None;

        while let Some(item) = stream.next().await {
            match item {
                Ok(sqlite_row) => {
                    if !columns_sent {
                        let columns = Self::get_column_info(&sqlite_row);
                        if sender.send(StreamEvent::Columns(columns)).await.is_err() {
                            break;
                        }
                        columns_sent = true;
                    }

                    let row = Self::convert_row(&sqlite_row);
                    if sender.send(StreamEvent::Row(row)).await.is_err() {
                        break;
                    }
                    row_count += 1;
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    let _ = sender.send(StreamEvent::Error(error_msg.clone())).await;
                    stream_error = Some(error_msg);
                    break;
                }
            }
        }

        if stream_error.is_none() {
            let _ = sender.send(StreamEvent::Done(row_count)).await;
        }

        if let Some(err) = stream_error {
            return Err(EngineError::execution_error(err));
        }

        Ok(())
    }

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
        _namespace: Option<Namespace>,
        query: &str,
        _query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let sqlite_session = self.get_session(session).await?;
        let start = Instant::now();

        let returns_rows = sql_safety::returns_rows(self.driver_id(), query)
            .unwrap_or_else(|_| sql_safety::is_select_prefix(query));

        let mut tx_guard = sqlite_session.transaction_conn.lock().await;

        let result = if let Some(ref mut conn) = *tx_guard {
            // Use dedicated transaction connection
            if returns_rows {
                let sqlite_rows: Vec<SqliteRow> = sqlx::query(query)
                    .fetch_all(&mut **conn)
                    .await
                    .map_err(|e| {
                        let msg = e.to_string();
                        if msg.contains("syntax") {
                            EngineError::syntax_error(msg)
                        } else {
                            EngineError::execution_error(msg)
                        }
                    })?;

                let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

                if sqlite_rows.is_empty() {
                    Ok(QueryResult {
                        columns: Vec::new(),
                        rows: Vec::new(),
                        affected_rows: None,
                        execution_time_ms,
                    })
                } else {
                    let columns = Self::get_column_info(&sqlite_rows[0]);
                    let rows: Vec<QRow> = sqlite_rows.iter().map(Self::convert_row).collect();

                    Ok(QueryResult {
                        columns,
                        rows,
                        affected_rows: None,
                        execution_time_ms,
                    })
                }
            } else {
                let result = sqlx::query(query).execute(&mut **conn).await.map_err(|e| {
                    let msg = e.to_string();
                    if msg.contains("syntax") {
                        EngineError::syntax_error(msg)
                    } else {
                        EngineError::execution_error(msg)
                    }
                })?;

                let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

                Ok(QueryResult::with_affected_rows(
                    result.rows_affected(),
                    execution_time_ms,
                ))
            }
        } else {
            // Use pool
            let mut conn = sqlite_session
                .pool
                .acquire()
                .await
                .map_err(|e| EngineError::connection_failed(e.to_string()))?;

            if returns_rows {
                let sqlite_rows: Vec<SqliteRow> = sqlx::query(query)
                    .fetch_all(&mut *conn)
                    .await
                    .map_err(|e| {
                        let msg = e.to_string();
                        if msg.contains("syntax") {
                            EngineError::syntax_error(msg)
                        } else {
                            EngineError::execution_error(msg)
                        }
                    })?;

                let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

                if sqlite_rows.is_empty() {
                    Ok(QueryResult {
                        columns: Vec::new(),
                        rows: Vec::new(),
                        affected_rows: None,
                        execution_time_ms,
                    })
                } else {
                    let columns = Self::get_column_info(&sqlite_rows[0]);
                    let rows: Vec<QRow> = sqlite_rows.iter().map(Self::convert_row).collect();

                    Ok(QueryResult {
                        columns,
                        rows,
                        affected_rows: None,
                        execution_time_ms,
                    })
                }
            } else {
                let result = sqlx::query(query).execute(&mut *conn).await.map_err(|e| {
                    let msg = e.to_string();
                    if msg.contains("syntax") {
                        EngineError::syntax_error(msg)
                    } else {
                        EngineError::execution_error(msg)
                    }
                })?;

                let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

                Ok(QueryResult::with_affected_rows(
                    result.rows_affected(),
                    execution_time_ms,
                ))
            }
        };

        result
    }

    async fn describe_table(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        let sqlite_session = self.get_session(session).await?;
        let pool = &sqlite_session.pool;

        // Get column info using PRAGMA table_info
        let table_ident = Self::quote_ident(table);
        let pragma_query = format!("PRAGMA table_info({})", table_ident);

        let column_rows: Vec<(i64, String, String, i64, Option<String>, i64)> =
            sqlx::query_as(&pragma_query)
                .fetch_all(pool)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let mut pk_columns: Vec<String> = Vec::new();
        let columns: Vec<TableColumn> = column_rows
            .into_iter()
            .map(|(_cid, name, data_type, notnull, dflt_value, pk)| {
                let is_primary_key = pk > 0;
                if is_primary_key {
                    pk_columns.push(name.clone());
                }
                TableColumn {
                    name,
                    data_type,
                    nullable: notnull == 0,
                    default_value: dflt_value,
                    is_primary_key,
                }
            })
            .collect();

        // Get foreign keys using PRAGMA foreign_key_list
        let fk_query = format!("PRAGMA foreign_key_list({})", table_ident);
        let fk_rows: Vec<(i64, i64, String, String, String, String, String, String)> =
            sqlx::query_as(&fk_query)
                .fetch_all(pool)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let foreign_keys: Vec<ForeignKey> = fk_rows
            .into_iter()
            .map(
                |(_id, _seq, ref_table, from_col, to_col, _on_update, _on_delete, _match)| {
                    ForeignKey {
                        column: from_col,
                        referenced_table: ref_table,
                        referenced_column: to_col,
                        referenced_schema: None,
                        referenced_database: None,
                        constraint_name: None,
                        is_virtual: false,
                    }
                },
            )
            .collect();

        // Get indexes using PRAGMA index_list
        let index_query = format!("PRAGMA index_list({})", table_ident);
        let index_list: Vec<(i64, String, i64, String, i64)> = sqlx::query_as(&index_query)
            .fetch_all(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let mut indexes: Vec<TableIndex> = Vec::new();
        for (_seq, index_name, is_unique, _origin, _partial) in index_list {
            // Get columns for this index
            let index_info_query = format!("PRAGMA index_info({})", Self::quote_ident(&index_name));
            let index_cols: Vec<(i64, i64, String)> = sqlx::query_as(&index_info_query)
                .fetch_all(pool)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let columns: Vec<String> = index_cols.into_iter().map(|(_, _, name)| name).collect();

            let is_primary = index_name.starts_with("sqlite_autoindex_");
            indexes.push(TableIndex {
                name: index_name,
                columns,
                is_unique: is_unique != 0,
                is_primary,
            });
        }

        // Get row count estimate
        let count_query = format!("SELECT COUNT(*) FROM {}", table_ident);
        let row_count: Option<(i64,)> = sqlx::query_as(&count_query)
            .fetch_optional(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let row_count_estimate = row_count.map(|(c,)| c as u64);

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
    }

    async fn preview_table(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let query = format!("SELECT * FROM {} LIMIT {}", Self::quote_ident(table), limit);
        self.execute(session, &query, QueryId::new()).await
    }

    async fn query_table(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        let sqlite_session = self.get_session(session).await?;
        let start = Instant::now();

        let table_ident = Self::quote_ident(table);

        let page = options.effective_page();
        let page_size = options.effective_page_size();
        let offset = options.offset();

        // Build WHERE clause from filters
        let mut where_clauses: Vec<String> = Vec::new();
        let mut bind_values: Vec<Value> = Vec::new();

        if let Some(filters) = &options.filters {
            for filter in filters {
                let col_ident = Self::quote_ident(&filter.column);

                let clause = match filter.operator {
                    FilterOperator::Eq => {
                        bind_values.push(filter.value.clone());
                        format!("{} = ?", col_ident)
                    }
                    FilterOperator::Neq => {
                        bind_values.push(filter.value.clone());
                        format!("{} != ?", col_ident)
                    }
                    FilterOperator::Gt => {
                        bind_values.push(filter.value.clone());
                        format!("{} > ?", col_ident)
                    }
                    FilterOperator::Gte => {
                        bind_values.push(filter.value.clone());
                        format!("{} >= ?", col_ident)
                    }
                    FilterOperator::Lt => {
                        bind_values.push(filter.value.clone());
                        format!("{} < ?", col_ident)
                    }
                    FilterOperator::Lte => {
                        bind_values.push(filter.value.clone());
                        format!("{} <= ?", col_ident)
                    }
                    FilterOperator::Like => {
                        bind_values.push(filter.value.clone());
                        format!("{} LIKE ?", col_ident)
                    }
                    FilterOperator::IsNull => format!("{} IS NULL", col_ident),
                    FilterOperator::IsNotNull => format!("{} IS NOT NULL", col_ident),
                };
                where_clauses.push(clause);
            }
        }

        // Handle search across text columns
        if let Some(ref search_term) = options.search {
            if !search_term.trim().is_empty() {
                // Get column info
                let pragma_query = format!("PRAGMA table_info({})", table_ident);
                let columns_rows: Vec<(i64, String, String, i64, Option<String>, i64)> =
                    sqlx::query_as(&pragma_query)
                        .fetch_all(&sqlite_session.pool)
                        .await
                        .map_err(|e| EngineError::execution_error(e.to_string()))?;

                let mut search_clauses: Vec<String> = Vec::new();
                for (_, col_name, data_type, _, _, _) in &columns_rows {
                    // Only search text-like columns
                    let is_text = data_type.to_uppercase().contains("TEXT")
                        || data_type.to_uppercase().contains("CHAR")
                        || data_type.to_uppercase().contains("VARCHAR")
                        || data_type.to_uppercase().contains("CLOB");

                    if is_text {
                        let col_ident = Self::quote_ident(col_name);
                        bind_values.push(Value::Text(format!("%{}%", search_term)));
                        search_clauses.push(format!("{} LIKE ?", col_ident));
                    }
                }

                if !search_clauses.is_empty() {
                    where_clauses.push(format!("({})", search_clauses.join(" OR ")));
                }
            }
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_clauses.join(" AND "))
        };

        // Build ORDER BY clause
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

        // Execute COUNT query for total rows
        let count_sql = format!("SELECT COUNT(*) AS cnt FROM {}{}", table_ident, where_sql);
        let mut count_query = sqlx::query(&count_sql);
        for val in &bind_values {
            count_query = Self::bind_param(count_query, val);
        }

        let count_row: SqliteRow = {
            let mut tx_guard = sqlite_session.transaction_conn.lock().await;
            if let Some(ref mut conn) = *tx_guard {
                count_query.fetch_one(&mut **conn).await
            } else {
                count_query.fetch_one(&sqlite_session.pool).await
            }
        }
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let total_rows: i64 = count_row
            .try_get("cnt")
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        let total_rows = total_rows.max(0) as u64;

        // Execute data query with pagination
        let data_sql = format!(
            "SELECT * FROM {}{}{} LIMIT {} OFFSET {}",
            table_ident, where_sql, order_sql, page_size, offset
        );

        let mut data_query = sqlx::query(&data_sql);
        for val in &bind_values {
            data_query = Self::bind_param(data_query, val);
        }

        let sqlite_rows: Vec<SqliteRow> = {
            let mut tx_guard = sqlite_session.transaction_conn.lock().await;
            if let Some(ref mut conn) = *tx_guard {
                data_query.fetch_all(&mut **conn).await
            } else {
                data_query.fetch_all(&sqlite_session.pool).await
            }
        }
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

        let result = if sqlite_rows.is_empty() {
            QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: None,
                execution_time_ms,
            }
        } else {
            let columns = Self::get_column_info(&sqlite_rows[0]);
            let rows: Vec<QRow> = sqlite_rows.iter().map(Self::convert_row).collect();
            QueryResult {
                columns,
                rows,
                affected_rows: None,
                execution_time_ms,
            }
        };

        Ok(PaginatedQueryResult::new(result, total_rows, page, page_size))
    }

    async fn peek_foreign_key(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        foreign_key: &ForeignKey,
        value: &Value,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let sqlite_session = self.get_session(session).await?;
        let limit = limit.max(1).min(50);

        let table_ref = Self::quote_ident(&foreign_key.referenced_table);
        let column_ref = Self::quote_ident(&foreign_key.referenced_column);
        let sql = format!(
            "SELECT * FROM {} WHERE {} = ? LIMIT {}",
            table_ref, column_ref, limit
        );

        let mut query = sqlx::query(&sql);
        query = Self::bind_param(query, value);

        let start = Instant::now();
        let mut tx_guard = sqlite_session.transaction_conn.lock().await;
        let sqlite_rows: Vec<SqliteRow> = if let Some(ref mut conn) = *tx_guard {
            query.fetch_all(&mut **conn).await
        } else {
            query.fetch_all(&sqlite_session.pool).await
        }
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

        if sqlite_rows.is_empty() {
            return Ok(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: None,
                execution_time_ms,
            });
        }

        let columns = Self::get_column_info(&sqlite_rows[0]);
        let rows: Vec<QRow> = sqlite_rows.iter().map(Self::convert_row).collect();

        Ok(QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms,
        })
    }

    async fn cancel(&self, _session: SessionId, _query_id: Option<QueryId>) -> EngineResult<()> {
        // SQLite doesn't support query cancellation via a separate connection
        // The sqlite3_interrupt API would need to be called on the same connection
        Err(EngineError::not_supported(
            "SQLite does not support query cancellation",
        ))
    }

    fn cancel_support(&self) -> CancelSupport {
        CancelSupport::None
    }

    // ==================== Transaction Methods ====================

    async fn begin_transaction(&self, session: SessionId) -> EngineResult<()> {
        let sqlite_session = self.get_session(session).await?;
        let mut tx = sqlite_session.transaction_conn.lock().await;

        if tx.is_some() {
            return Err(EngineError::transaction_error(
                "A transaction is already active on this session",
            ));
        }

        let mut conn = sqlite_session.pool.acquire().await.map_err(|e| {
            EngineError::connection_failed(format!(
                "Failed to acquire connection for transaction: {}",
                e
            ))
        })?;

        sqlx::query("BEGIN")
            .execute(&mut *conn)
            .await
            .map_err(|e| {
                EngineError::execution_error(format!("Failed to begin transaction: {}", e))
            })?;

        *tx = Some(conn);
        Ok(())
    }

    async fn commit(&self, session: SessionId) -> EngineResult<()> {
        let sqlite_session = self.get_session(session).await?;
        let mut tx = sqlite_session.transaction_conn.lock().await;

        let mut conn = tx
            .take()
            .ok_or_else(|| EngineError::transaction_error("No active transaction to commit"))?;

        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .map_err(|e| {
                EngineError::execution_error(format!("Failed to commit transaction: {}", e))
            })?;

        Ok(())
    }

    async fn rollback(&self, session: SessionId) -> EngineResult<()> {
        let sqlite_session = self.get_session(session).await?;
        let mut tx = sqlite_session.transaction_conn.lock().await;

        let mut conn = tx
            .take()
            .ok_or_else(|| EngineError::transaction_error("No active transaction to rollback"))?;

        sqlx::query("ROLLBACK")
            .execute(&mut *conn)
            .await
            .map_err(|e| {
                EngineError::execution_error(format!("Failed to rollback transaction: {}", e))
            })?;

        Ok(())
    }

    fn supports_transactions(&self) -> bool {
        true
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    // ==================== Mutation Methods ====================

    async fn insert_row(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        table: &str,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let sqlite_session = self.get_session(session).await?;

        let table_name = Self::quote_ident(table);

        let mut keys: Vec<&String> = data.columns.keys().collect();
        keys.sort();

        let sql = if keys.is_empty() {
            format!("INSERT INTO {} DEFAULT VALUES", table_name)
        } else {
            let cols_str = keys
                .iter()
                .map(|k| Self::quote_ident(k))
                .collect::<Vec<_>>()
                .join(", ");
            let params_str = vec!["?"; keys.len()].join(", ");
            format!(
                "INSERT INTO {} ({}) VALUES ({})",
                table_name, cols_str, params_str
            )
        };

        let mut query = sqlx::query(&sql);
        for k in &keys {
            let val = data.columns.get(*k).unwrap();
            query = Self::bind_param(query, val);
        }

        let start = Instant::now();
        let mut tx_guard = sqlite_session.transaction_conn.lock().await;
        let result = if let Some(ref mut conn) = *tx_guard {
            query.execute(&mut **conn).await
        } else {
            query.execute(&sqlite_session.pool).await
        };

        let result = result.map_err(|e| EngineError::execution_error(e.to_string()))?;

        Ok(QueryResult::with_affected_rows(
            result.rows_affected(),
            start.elapsed().as_micros() as f64 / 1000.0,
        ))
    }

    async fn update_row(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let sqlite_session = self.get_session(session).await?;

        if primary_key.columns.is_empty() {
            return Err(EngineError::execution_error(
                "Primary key required for update operations".to_string(),
            ));
        }

        if data.columns.is_empty() {
            return Ok(QueryResult::with_affected_rows(0, 0.0));
        }

        let table_name = Self::quote_ident(table);

        let mut data_keys: Vec<&String> = data.columns.keys().collect();
        data_keys.sort();

        let mut pk_keys: Vec<&String> = primary_key.columns.keys().collect();
        pk_keys.sort();

        // UPDATE table SET col1=?, col2=? WHERE pk1=? AND pk2=?
        let set_clauses: Vec<String> = data_keys
            .iter()
            .map(|k| format!("{}=?", Self::quote_ident(k)))
            .collect();

        let where_clauses: Vec<String> = pk_keys
            .iter()
            .map(|k| format!("{}=?", Self::quote_ident(k)))
            .collect();

        let sql = format!(
            "UPDATE {} SET {} WHERE {}",
            table_name,
            set_clauses.join(", "),
            where_clauses.join(" AND ")
        );

        let mut query = sqlx::query(&sql);

        // Bind data values
        for k in &data_keys {
            let val = data.columns.get(*k).unwrap();
            query = Self::bind_param(query, val);
        }

        // Bind PK values
        for k in &pk_keys {
            let val = primary_key.columns.get(*k).unwrap();
            query = Self::bind_param(query, val);
        }

        let start = Instant::now();
        let mut tx_guard = sqlite_session.transaction_conn.lock().await;
        let result = if let Some(ref mut conn) = *tx_guard {
            query.execute(&mut **conn).await
        } else {
            query.execute(&sqlite_session.pool).await
        };

        let result = result.map_err(|e| EngineError::execution_error(e.to_string()))?;

        Ok(QueryResult::with_affected_rows(
            result.rows_affected(),
            start.elapsed().as_micros() as f64 / 1000.0,
        ))
    }

    async fn delete_row(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
    ) -> EngineResult<QueryResult> {
        let sqlite_session = self.get_session(session).await?;

        if primary_key.columns.is_empty() {
            return Err(EngineError::execution_error(
                "Primary key required for delete operations".to_string(),
            ));
        }

        let table_name = Self::quote_ident(table);

        let mut pk_keys: Vec<&String> = primary_key.columns.keys().collect();
        pk_keys.sort();

        // DELETE FROM table WHERE pk1=?
        let where_clauses: Vec<String> = pk_keys
            .iter()
            .map(|k| format!("{}=?", Self::quote_ident(k)))
            .collect();

        let sql = format!(
            "DELETE FROM {} WHERE {}",
            table_name,
            where_clauses.join(" AND ")
        );

        let mut query = sqlx::query(&sql);
        for k in &pk_keys {
            let val = primary_key.columns.get(*k).unwrap();
            query = Self::bind_param(query, val);
        }

        let start = Instant::now();
        let mut tx_guard = sqlite_session.transaction_conn.lock().await;
        let result = if let Some(ref mut conn) = *tx_guard {
            query.execute(&mut **conn).await
        } else {
            query.execute(&sqlite_session.pool).await
        };

        let result = result.map_err(|e| EngineError::execution_error(e.to_string()))?;

        Ok(QueryResult::with_affected_rows(
            result.rows_affected(),
            start.elapsed().as_micros() as f64 / 1000.0,
        ))
    }

    fn supports_mutations(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_connect_disconnect() {
        let driver = SqliteDriver::new();
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let config = ConnectionConfig {
            driver: "sqlite".to_string(),
            host: db_path.to_string_lossy().to_string(),
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
    async fn test_memory_database() {
        let driver = SqliteDriver::new();

        let config = ConnectionConfig {
            driver: "sqlite".to_string(),
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

        // Create a table
        let result = driver
            .execute(
                session_id,
                "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert!(result.affected_rows.is_some());

        // Insert data
        let result = driver
            .execute(
                session_id,
                "INSERT INTO test (name) VALUES ('hello')",
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

        driver.disconnect(session_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_transactions() {
        let driver = SqliteDriver::new();

        let config = ConnectionConfig {
            driver: "sqlite".to_string(),
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
                "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
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
                "INSERT INTO test (name) VALUES ('tx_test')",
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
}

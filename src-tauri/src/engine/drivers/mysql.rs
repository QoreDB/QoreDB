//! MySQL Driver
//!
//! Implements the DataEngine trait for MySQL/MariaDB databases using SQLx.
//!
//! ## Transaction Handling
//!
//! Same architecture as PostgreSQL: dedicated connection acquired from pool
//! on BEGIN and released on COMMIT/ROLLBACK.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use rust_decimal::Decimal;
use sqlx::mysql::{MySql, MySqlConnectOptions, MySqlPool, MySqlPoolOptions, MySqlRow, MySqlSslMode};
use sqlx::pool::PoolConnection;
use sqlx::{Column, Executor, Row, TypeInfo};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::sql_safety;
use crate::engine::traits::DataEngine;
use crate::engine::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType, ColumnInfo,
    ConnectionConfig, Namespace, QueryId, QueryResult, Row as QRow, RowData, SessionId,
    TableColumn, TableIndex, TableSchema, Value, ForeignKey,
    TableQueryOptions, PaginatedQueryResult, SortDirection, FilterOperator,
};
use crate::engine::traits::{StreamEvent, StreamSender};
use futures::StreamExt;

pub struct MySqlSession {
    pub pool: MySqlPool, 
    pub transaction_conn: Mutex<Option<PoolConnection<MySql>>>,
    pub active_queries: Mutex<HashMap<QueryId, u64>>,
}

impl MySqlSession {
    pub fn new(pool: MySqlPool) -> Self {
        Self {
            pool,
            transaction_conn: Mutex::new(None),
            active_queries: Mutex::new(HashMap::new()),
        }
    }
}

pub struct MySqlDriver {
    sessions: Arc<RwLock<HashMap<SessionId, Arc<MySqlSession>>>>,
}

impl MySqlDriver {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn get_session(&self, session: SessionId) -> EngineResult<Arc<MySqlSession>> {
        let sessions = self.sessions.read().await;
        sessions
            .get(&session)
            .cloned()
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))
    }

    /// Helper to bind a Value to a MySQL query
    fn bind_param<'q>(
        query: sqlx::query::Query<'q, MySql, sqlx::mysql::MySqlArguments>,
        value: &'q Value,
    ) -> sqlx::query::Query<'q, MySql, sqlx::mysql::MySqlArguments> {
        match value {
            Value::Null => query.bind(Option::<String>::None),
            Value::Bool(b) => query.bind(b),
            Value::Int(i) => query.bind(i),
            Value::Float(f) => query.bind(f),
            Value::Text(s) => query.bind(s),
            Value::Bytes(b) => query.bind(b),
            Value::Json(j) => query.bind(j),
            // Fallback for arrays
            Value::Array(_) => query.bind(Option::<String>::None),
        }
    }

    async fn fetch_connection_id(
        conn: &mut PoolConnection<MySql>,
    ) -> EngineResult<u64> {
        sqlx::query_scalar("SELECT CONNECTION_ID()")
            .fetch_one(&mut **conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))
    }

    fn build_connect_options(config: &ConnectionConfig) -> MySqlConnectOptions {
        let mut opts = MySqlConnectOptions::new()
            .host(&config.host)
            .port(config.port)
            .username(&config.username)
            .password(&config.password)
            .ssl_mode(if config.ssl {
                MySqlSslMode::Required
            } else {
                MySqlSslMode::Disabled
            });

        if let Some(db) = config.database.as_deref() {
            let db = db.trim();
            if !db.is_empty() {
                opts = opts.database(db);
            }
        }

        opts
    }

    fn quote_ident(name: &str) -> String {
        format!("`{}`", name.replace('`', "``"))
    }

    async fn apply_namespace_on_conn(
        conn: &mut PoolConnection<MySql>,
        namespace: &Option<Namespace>,
        query: &str,
    ) -> EngineResult<()> {
        // Avoid overriding explicit USE statements.
        if query.trim_start().to_ascii_lowercase().starts_with("use ") {
            return Ok(());
        }
        if let Some(ns) = namespace {
            let db = ns.database.trim();
            if !db.is_empty() {
                let use_sql = format!("USE {}", Self::quote_ident(db));
                // Use simple query protocol for maximum compatibility.
                conn.execute(sqlx::raw_sql(&use_sql))
                    .await
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;
            }
        }
        Ok(())
    }

    /// Converts a SQLx row to our universal Row type
    fn convert_row(mysql_row: &MySqlRow) -> QRow {
        let values: Vec<Value> = mysql_row
            .columns()
            .iter()
            .map(|col| Self::extract_value(mysql_row, col.ordinal()))
            .collect();

        QRow { values }
    }

    /// Extracts a value from a MySqlRow at the given index
    fn extract_value(row: &MySqlRow, idx: usize) -> Value {
        // Try u64 first for BIGINT UNSIGNED columns
        if let Ok(v) = row.try_get::<Option<u64>, _>(idx) {
            return v.map(|u| Value::Int(u as i64)).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
            return v.map(Value::Int).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<i32>, _>(idx) {
            return v.map(|i| Value::Int(i as i64)).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<u32>, _>(idx) {
            return v.map(|u| Value::Int(u as i64)).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<i16>, _>(idx) {
            return v.map(|i| Value::Int(i as i64)).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<u16>, _>(idx) {
            return v.map(|u| Value::Int(u as i64)).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<i8>, _>(idx) {
            return v.map(|i| Value::Int(i as i64)).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<u8>, _>(idx) {
            return v.map(|u| Value::Int(u as i64)).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
            return v.map(Value::Bool).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
            return v.map(Value::Float).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<f32>, _>(idx) {
            return v.map(|f| Value::Float(f as f64)).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<Uuid>, _>(idx) {
            return v.map(|u| Value::Text(u.to_string())).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<Decimal>, _>(idx) {
            return v.map(|d| {
                use rust_decimal::prelude::ToPrimitive;
                Value::Float(d.to_f64().unwrap_or(0.0))
            }).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
            return v.map(Value::Text).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(idx) {
            return v.map(|dt| Value::Text(dt.to_rfc3339())).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<chrono::NaiveDateTime>, _>(idx) {
            return v.map(|dt| Value::Text(dt.format("%Y-%m-%d %H:%M:%S").to_string())).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<chrono::NaiveDate>, _>(idx) {
            return v.map(|d| Value::Text(d.format("%Y-%m-%d").to_string())).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<chrono::NaiveTime>, _>(idx) {
            return v.map(|t| Value::Text(t.format("%H:%M:%S").to_string())).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
            return v.map(Value::Bytes).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(idx) {
            return v.map(Value::Json).unwrap_or(Value::Null);
        }

        Value::Null
    }

    /// Gets column info from a MySqlRow
    fn get_column_info(row: &MySqlRow) -> Vec<ColumnInfo> {
        row.columns()
            .iter()
            .map(|col| ColumnInfo {
                name: col.name().to_string(),
                data_type: col.type_info().name().to_string(),
                nullable: true,
            })
            .collect()
    }
}

impl Default for MySqlDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for MySqlDriver {
    fn driver_id(&self) -> &'static str {
        "mysql"
    }

    fn driver_name(&self) -> &'static str {
        "MySQL / MariaDB"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        let opts = Self::build_connect_options(config);

        let pool = MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect_with(opts)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("Access denied") {
                    EngineError::auth_failed(msg)
                } else {
                    EngineError::connection_failed(msg)
                }
            })?;

        sqlx::query("SELECT 1")
            .execute(&pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        pool.close().await;
        Ok(())
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        let opts = Self::build_connect_options(config);
        let max_connections = config.pool_max_connections.unwrap_or(5);
        let min_connections = config.pool_min_connections.unwrap_or(0);
        let acquire_timeout = config.pool_acquire_timeout_secs.unwrap_or(30);

        let pool = MySqlPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .acquire_timeout(std::time::Duration::from_secs(acquire_timeout as u64))
            .connect_with(opts)
            .await
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        let session_id = SessionId::new();
        let session = Arc::new(MySqlSession::new(pool));

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
        let mysql_session = self.get_session(session).await?;
        let pool = &mysql_session.pool;

        tracing::info!("MySQL: Listing namespaces for session {}", session.0);

        let rows: Vec<(String,)> = sqlx::query_as("SELECT CAST(schema_name AS CHAR) FROM information_schema.schemata")
            .fetch_all(pool)
            .await
            .map_err(|e| {
                tracing::error!("MySQL: Failed to list namespaces: {}", e);
                EngineError::execution_error(e.to_string())
            })?;

        tracing::info!("MySQL: Found {} raw databases: {:?}", rows.len(), rows.iter().map(|(n,)| n).collect::<Vec<_>>());

        let system_dbs = ["information_schema", "mysql", "performance_schema", "sys"];
        let namespaces = rows.into_iter()
            .map(|(db,)| db)
            .filter(|db| !system_dbs.contains(&db.as_str()))
            .map(Namespace::new)
            .collect();

        Ok(namespaces)
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let mysql_session = self.get_session(session).await?;
        let pool = &mysql_session.pool;

        let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

        // 1. Get total count
        let count_query = r#"
            SELECT COUNT(*)
            FROM information_schema.TABLES
            WHERE TABLE_SCHEMA = ?
            AND (? IS NULL OR TABLE_NAME LIKE ?)
        "#;

        let count_row: (i64,) = sqlx::query_as(count_query)
            .bind(&namespace.database)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_one(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        
        let total_count = count_row.0;

        // 2. Get paginated results
        // Cast to CHAR to avoid BINARY type mismatch with Rust String
        let mut query_str = r#"
            SELECT CAST(TABLE_NAME AS CHAR) AS table_name, CAST(TABLE_TYPE AS CHAR) AS table_type
            FROM information_schema.TABLES
            WHERE TABLE_SCHEMA = ?
            AND (? IS NULL OR TABLE_NAME LIKE ?)
            ORDER BY TABLE_NAME
        "#.to_string();

        if let Some(limit) = options.page_size {
             query_str.push_str(&format!(" LIMIT {}", limit));
             if let Some(page) = options.page {
                 let offset = (page.max(1) - 1) * limit;
                 query_str.push_str(&format!(" OFFSET {}", offset));
             }
        }

        let rows: Vec<(String, String)> = sqlx::query_as(&query_str)
            .bind(&namespace.database)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_all(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let collections = rows
            .into_iter()
            .map(|(name, table_type)| {
                let collection_type = match table_type.as_str() {
                    "VIEW" => CollectionType::View,
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

    async fn create_database(&self, session: SessionId, name: &str, _options: Option<Value>) -> EngineResult<()> {
        let mysql_session = self.get_session(session).await?;
        let pool = &mysql_session.pool;

        // Basic validation
        if name.is_empty() || name.len() > 64 {
            return Err(EngineError::validation("Database name must be between 1 and 64 characters"));
        }

        // Identifier quoting with backticks for MySQL
        // Simple escape of backticks to avoid injection
        let escaped_name = name.replace('`', "``");
        let query = format!("CREATE DATABASE `{}`", escaped_name);

        sqlx::query(&query)
            .execute(pool)
            .await
            .map_err(|e| {
                tracing::error!("MySQL: Failed to create database: {}", e);
                let msg = e.to_string();
                if msg.contains("Access denied") {
                    EngineError::auth_failed(format!("Permission denied: {}", msg))
                } else if msg.contains("exists") {
                    EngineError::validation(format!("Database '{}' already exists", name))
                } else {
                    EngineError::execution_error(msg)
                }
            })?;

        tracing::info!("MySQL: Successfully created database '{}'", name);
        Ok(())
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        let mysql_session = self.get_session(session).await?;
        let pool = &mysql_session.pool;

        // Basic validation
        if name.is_empty() || name.len() > 64 {
            return Err(EngineError::validation("Database name must be between 1 and 64 characters"));
        }

        // Identifier quoting with backticks for MySQL
        let escaped_name = name.replace('`', "``");
        let query = format!("DROP DATABASE `{}`", escaped_name);

        sqlx::query(&query)
            .execute(pool)
            .await
            .map_err(|e| {
                tracing::error!("MySQL: Failed to drop database: {}", e);
                let msg = e.to_string();
                if msg.contains("Access denied") {
                    EngineError::auth_failed(format!("Permission denied: {}", msg))
                } else if msg.contains("doesn't exist") || msg.contains("Unknown database") {
                    EngineError::validation(format!("Database '{}' does not exist", name))
                } else {
                    EngineError::execution_error(msg)
                }
            })?;

        tracing::info!("MySQL: Successfully dropped database '{}'", name);
        Ok(())
    }

    /// Executes a query and returns the result
    /// 
    /// Routes to transaction connection if active, otherwise uses pool.
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
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        let mysql_session = self.get_session(session).await?;

        // Use pool for streaming
        let mut conn = mysql_session
                .pool
                .acquire()
                .await
                .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        Self::apply_namespace_on_conn(&mut conn, &namespace, query).await?;

        // Check if query returns rows
        let returns_rows = sql_safety::returns_rows(self.driver_id(), query)
            .unwrap_or_else(|_| sql_safety::is_select_prefix(query));

           if !returns_rows {
             // Fallback
               let result = self.execute_in_namespace(session, namespace, query, query_id).await?;
             let _ = sender.send(StreamEvent::Done(result.affected_rows.unwrap_or(0))).await;
             return Ok(());
        }

        let connection_id = Self::fetch_connection_id(&mut conn).await?;
        {
            let mut active = mysql_session.active_queries.lock().await;
            active.insert(query_id, connection_id);
        }

        let mut stream = sqlx::query(query).fetch(&mut *conn);
        let mut columns_sent = false;
        let mut row_count = 0;
        let mut stream_error: Option<String> = None;

        while let Some(item) = stream.next().await {
            match item {
                Ok(mysql_row) => {
                    if !columns_sent {
                        let columns = Self::get_column_info(&mysql_row);
                        if sender.send(StreamEvent::Columns(columns)).await.is_err() {
                            break;
                        }
                        columns_sent = true;
                    }

                    let row = Self::convert_row(&mysql_row);
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

        {
            let mut active = mysql_session.active_queries.lock().await;
            active.remove(&query_id);
        }

        // Only send Done if no error occurred
        if stream_error.is_none() {
            let _ = sender.send(StreamEvent::Done(row_count)).await;
        }

        // Return error if stream failed, so frontend knows about it
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
        self.execute_in_namespace(session, None, query, query_id).await
    }

    async fn execute_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let mysql_session = self.get_session(session).await?;
        let start = Instant::now();

        let returns_rows = sql_safety::returns_rows(self.driver_id(), query)
            .unwrap_or_else(|_| sql_safety::is_select_prefix(query));

        let mut tx_guard = mysql_session.transaction_conn.lock().await;
        let result = if let Some(ref mut conn) = *tx_guard {
            let connection_id = Self::fetch_connection_id(conn).await?;
            {
                let mut active = mysql_session.active_queries.lock().await;
                active.insert(query_id, connection_id);
            }

            Self::apply_namespace_on_conn(conn, &namespace, query).await?;

            let result = if returns_rows {
                let mysql_rows: Vec<MySqlRow> = sqlx::query(query)
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

                if mysql_rows.is_empty() {
                    Ok(QueryResult {
                        columns: Vec::new(),
                        rows: Vec::new(),
                        affected_rows: None,
                        execution_time_ms,
                    })
                } else {
                    let columns = Self::get_column_info(&mysql_rows[0]);
                    let rows: Vec<QRow> = mysql_rows.iter().map(Self::convert_row).collect();

                    Ok(QueryResult {
                        columns,
                        rows,
                        affected_rows: None,
                        execution_time_ms,
                    })
                }
            } else {
                // Use simple query protocol for DDL and other statements that may not be
                // supported via the prepared statement protocol on some MySQL/MariaDB versions.
                let result = conn
                    .execute(sqlx::raw_sql(query))
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

                Ok(QueryResult::with_affected_rows(
                    result.rows_affected(),
                    execution_time_ms,
                ))
            };

            let mut active = mysql_session.active_queries.lock().await;
            active.remove(&query_id);
            result
        } else {
            let mut conn = mysql_session
                .pool
                .acquire()
                .await
                .map_err(|e| EngineError::connection_failed(e.to_string()))?;
            let connection_id = Self::fetch_connection_id(&mut conn).await?;
            {
                let mut active = mysql_session.active_queries.lock().await;
                active.insert(query_id, connection_id);
            }

            Self::apply_namespace_on_conn(&mut conn, &namespace, query).await?;

            let result = if returns_rows {
                let mysql_rows: Vec<MySqlRow> = sqlx::query(query)
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

                if mysql_rows.is_empty() {
                    Ok(QueryResult {
                        columns: Vec::new(),
                        rows: Vec::new(),
                        affected_rows: None,
                        execution_time_ms,
                    })
                } else {
                    let columns = Self::get_column_info(&mysql_rows[0]);
                    let rows: Vec<QRow> = mysql_rows.iter().map(Self::convert_row).collect();

                    Ok(QueryResult {
                        columns,
                        rows,
                        affected_rows: None,
                        execution_time_ms,
                    })
                }
            } else {
                // Use simple query protocol for DDL and other statements that may not be
                // supported via the prepared statement protocol on some MySQL/MariaDB versions.
                let result = conn
                    .execute(sqlx::raw_sql(query))
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

                Ok(QueryResult::with_affected_rows(
                    result.rows_affected(),
                    execution_time_ms,
                ))
            };

            let mut active = mysql_session.active_queries.lock().await;
            active.remove(&query_id);
            result
        };

        result
    }

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        let mysql_session = self.get_session(session).await?;
        let pool = &mysql_session.pool;

        let database = &namespace.database;
        // Cast to CHAR to avoid BINARY type mismatch with Rust String
        let column_rows: Vec<(String, String, String, Option<String>, String)> = sqlx::query_as(
            r#"
            SELECT 
                CAST(c.COLUMN_NAME AS CHAR) AS column_name,
                CAST(c.COLUMN_TYPE AS CHAR) AS column_type,
                CAST(c.IS_NULLABLE AS CHAR) AS is_nullable,
                CAST(c.COLUMN_DEFAULT AS CHAR) AS column_default,
                CAST(c.COLUMN_KEY AS CHAR) AS column_key
            FROM information_schema.COLUMNS c
            WHERE c.TABLE_SCHEMA = ? AND c.TABLE_NAME = ?
            ORDER BY c.ORDINAL_POSITION
            "#,
        )
        .bind(database)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        // Build columns vec, collecting primary keys
        let mut pk_columns: Vec<String> = Vec::new();
        let columns: Vec<TableColumn> = column_rows
            .into_iter()
            .map(|(name, data_type, is_nullable, default_value, column_key)| {
                let is_primary_key = column_key == "PRI";
                if is_primary_key {
                    pk_columns.push(name.clone());
                }
                TableColumn {
                    name,
                    data_type,
                    nullable: is_nullable == "YES",
                    default_value,
                    is_primary_key,
                }
            })
            .collect();

        // Get foreign keys
        // Filter for REFERENCED_TABLE_NAME IS NOT NULL to find FKs
        let fk_rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
            r#"
            SELECT
                CAST(kcu.COLUMN_NAME AS CHAR) AS column_name,
                CAST(kcu.REFERENCED_TABLE_NAME AS CHAR) AS referenced_table,
                CAST(kcu.REFERENCED_COLUMN_NAME AS CHAR) AS referenced_column,
                CAST(kcu.REFERENCED_TABLE_SCHEMA AS CHAR) AS referenced_database,
                CAST(kcu.CONSTRAINT_NAME AS CHAR) AS constraint_name
            FROM information_schema.KEY_COLUMN_USAGE kcu
            WHERE kcu.TABLE_SCHEMA = ?
                AND kcu.TABLE_NAME = ?
                AND kcu.REFERENCED_TABLE_NAME IS NOT NULL
            "#,
        )
        .bind(database)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let foreign_keys: Vec<ForeignKey> = fk_rows
            .into_iter()
            .map(|(column, referenced_table, referenced_column, referenced_database, constraint_name)| ForeignKey {
                column,
                referenced_table,
                referenced_column,
                referenced_schema: None,
                referenced_database: Some(referenced_database),
                constraint_name: Some(constraint_name),
            })
            .collect();

        // Get row count estimate from table_rows (u64 for BIGINT UNSIGNED)
        let count_row: Option<(u64,)> = sqlx::query_as(
            r#"
            SELECT TABLE_ROWS
            FROM information_schema.TABLES
            WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
            "#,
        )
        .bind(database)
        .bind(table)
        .fetch_optional(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let row_count_estimate = count_row.map(|(c,)| c);

        // Get indexes
        let index_rows: Vec<(String, String, i32, i32)> = sqlx::query_as(
            r#"
            SELECT
                CAST(INDEX_NAME AS CHAR) AS name,
                CAST(COLUMN_NAME AS CHAR) AS column_name,
                CAST(NON_UNIQUE AS SIGNED) AS non_unique,
                CAST(SEQ_IN_INDEX AS SIGNED) AS seq_in_index
            FROM information_schema.STATISTICS
            WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
            ORDER BY INDEX_NAME, SEQ_IN_INDEX
            "#,
        )
        .bind(database)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        // Group by index name
        let mut index_map: std::collections::HashMap<String, (Vec<String>, bool, bool)> = std::collections::HashMap::new();
        for (name, column_name, non_unique, _seq) in index_rows {
            let is_unique = non_unique == 0;
            let is_primary = name == "PRIMARY";
            let entry = index_map.entry(name).or_insert_with(|| (Vec::new(), is_unique, is_primary));
            entry.0.push(column_name);
        }

        let indexes: Vec<TableIndex> = index_map
            .into_iter()
            .map(|(name, (columns, is_unique, is_primary))| TableIndex {
                name,
                columns,
                is_unique,
                is_primary,
            })
            .collect();

        Ok(TableSchema {
            columns,
            primary_key: if pk_columns.is_empty() { None } else { Some(pk_columns) },
            foreign_keys,
            row_count_estimate,
            indexes,
        })
    }

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        // Use backticks for MySQL identifier quoting
        let query = format!(
            "SELECT * FROM `{}`.`{}` LIMIT {}",
            namespace.database, table, limit
        );
        self.execute(session, &query, QueryId::new()).await
    }

    async fn query_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        let mysql_session = self.get_session(session).await?;
        let start = Instant::now();

        let db_ident = Self::quote_ident(&namespace.database);
        let table_ident = Self::quote_ident(table);
        let table_ref = format!("{}.{}", db_ident, table_ident);

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
                // Get column info to find text columns
                let columns_sql = "SELECT COLUMN_NAME, DATA_TYPE FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?";
                let columns_rows: Vec<MySqlRow> = {
                    let mut tx_guard = mysql_session.transaction_conn.lock().await;
                    if let Some(ref mut conn) = *tx_guard {
                        sqlx::query(columns_sql)
                            .bind(&namespace.database)
                            .bind(table)
                            .fetch_all(&mut **conn)
                            .await
                    } else {
                        sqlx::query(columns_sql)
                            .bind(&namespace.database)
                            .bind(table)
                            .fetch_all(&mysql_session.pool)
                            .await
                    }
                }
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

                let mut search_clauses: Vec<String> = Vec::new();
                for col_row in &columns_rows {
                    let col_name: String = col_row.try_get("COLUMN_NAME")
                        .map_err(|e| EngineError::execution_error(e.to_string()))?;
                    let data_type: String = col_row.try_get("DATA_TYPE")
                        .map_err(|e| EngineError::execution_error(e.to_string()))?;

                    // Only search text-like columns
                    let is_text = matches!(data_type.to_lowercase().as_str(),
                        "varchar" | "char" | "text" | "tinytext" | "mediumtext" | "longtext" | "enum" | "set"
                    );

                    if is_text {
                        let col_ident = Self::quote_ident(&col_name);
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
        let count_sql = format!("SELECT COUNT(*) AS cnt FROM {}{}", table_ref, where_sql);
        let mut count_query = sqlx::query(&count_sql);
        for val in &bind_values {
            count_query = Self::bind_param(count_query, val);
        }

        let count_row: MySqlRow = {
            let mut tx_guard = mysql_session.transaction_conn.lock().await;
            if let Some(ref mut conn) = *tx_guard {
                count_query.fetch_one(&mut **conn).await
            } else {
                count_query.fetch_one(&mysql_session.pool).await
            }
        }
        .map_err(|e| EngineError::execution_error(e.to_string()))?;
        
        let total_rows: i64 = count_row.try_get("cnt")
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        let total_rows = total_rows.max(0) as u64;

        // Execute data query with pagination
        let data_sql = format!(
            "SELECT * FROM {}{}{} LIMIT {} OFFSET {}",
            table_ref, where_sql, order_sql, page_size, offset
        );

        let mut data_query = sqlx::query(&data_sql);
        for val in &bind_values {
            data_query = Self::bind_param(data_query, val);
        }

        let mysql_rows: Vec<MySqlRow> = {
            let mut tx_guard = mysql_session.transaction_conn.lock().await;
            if let Some(ref mut conn) = *tx_guard {
                data_query.fetch_all(&mut **conn).await
            } else {
                data_query.fetch_all(&mysql_session.pool).await
            }
        }
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

        let result = if mysql_rows.is_empty() {
            QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: None,
                execution_time_ms,
            }
        } else {
            let columns = Self::get_column_info(&mysql_rows[0]);
            let rows: Vec<QRow> = mysql_rows.iter().map(Self::convert_row).collect();
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
        namespace: &Namespace,
        foreign_key: &ForeignKey,
        value: &Value,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let mysql_session = self.get_session(session).await?;
        let limit = limit.max(1).min(50);
        let database = foreign_key
            .referenced_database
            .as_deref()
            .unwrap_or(namespace.database.as_str());

        let table_ref = format!(
            "{}.{}",
            Self::quote_ident(database),
            Self::quote_ident(&foreign_key.referenced_table)
        );
        let column_ref = Self::quote_ident(&foreign_key.referenced_column);
        let sql = format!("SELECT * FROM {} WHERE {} = ? LIMIT {}", table_ref, column_ref, limit);

        let mut query = sqlx::query(&sql);
        query = Self::bind_param(query, value);

        let start = Instant::now();
        let mut tx_guard = mysql_session.transaction_conn.lock().await;
        let mysql_rows: Vec<MySqlRow> = if let Some(ref mut conn) = *tx_guard {
            query.fetch_all(&mut **conn).await
        } else {
            query.fetch_all(&mysql_session.pool).await
        }
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

        if mysql_rows.is_empty() {
            return Ok(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: None,
                execution_time_ms,
            });
        }

        let columns = Self::get_column_info(&mysql_rows[0]);
        let rows: Vec<QRow> = mysql_rows.iter().map(Self::convert_row).collect();

        Ok(QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms,
        })
    }

    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        let mysql_session = self.get_session(session).await?;

        let connection_ids: Vec<u64> = {
            let active = mysql_session.active_queries.lock().await;
            if let Some(qid) = query_id {
                match active.get(&qid) {
                    Some(id) => vec![*id],
                    None => return Err(EngineError::execution_error("Query not found")),
                }
            } else {
                active.values().copied().collect()
            }
        };

        if connection_ids.is_empty() {
            return Err(EngineError::execution_error("No active queries to cancel"));
        }

        let mut conn = mysql_session
            .pool
            .acquire()
            .await
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        for connection_id in connection_ids {
            let sql = format!("KILL QUERY {}", connection_id);
            let _ = sqlx::query(&sql)
                .execute(&mut *conn)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
        }

        Ok(())
    }

    fn cancel_support(&self) -> CancelSupport {
        CancelSupport::Driver
    }

    // ==================== Transaction Methods ====================

    async fn begin_transaction(&self, session: SessionId) -> EngineResult<()> {
        let mysql_session = self.get_session(session).await?;
        let mut tx = mysql_session.transaction_conn.lock().await;

        if tx.is_some() {
            return Err(EngineError::transaction_error(
                "A transaction is already active on this session"
            ));
        }

        let mut conn = mysql_session.pool.acquire().await
            .map_err(|e| EngineError::connection_failed(format!(
                "Failed to acquire connection for transaction: {}", e
            )))?;

        conn.execute(sqlx::raw_sql("START TRANSACTION"))
            .await
            .map_err(|e| EngineError::execution_error(format!(
                "Failed to begin transaction: {}", e
            )))?;

        *tx = Some(conn);
        Ok(())
    }

    async fn commit(&self, session: SessionId) -> EngineResult<()> {
        let mysql_session = self.get_session(session).await?;
        let mut tx = mysql_session.transaction_conn.lock().await;

        let mut conn = tx.take()
            .ok_or_else(|| EngineError::transaction_error(
                "No active transaction to commit"
            ))?;

        conn.execute(sqlx::raw_sql("COMMIT"))
            .await
            .map_err(|e| EngineError::execution_error(format!(
                "Failed to commit transaction: {}", e
            )))?;

        Ok(())
    }

    async fn rollback(&self, session: SessionId) -> EngineResult<()> {
        let mysql_session = self.get_session(session).await?;
        let mut tx = mysql_session.transaction_conn.lock().await;

        let mut conn = tx.take()
            .ok_or_else(|| EngineError::transaction_error(
                "No active transaction to rollback"
            ))?;

        conn.execute(sqlx::raw_sql("ROLLBACK"))
            .await
            .map_err(|e| EngineError::execution_error(format!(
                "Failed to rollback transaction: {}", e
            )))?;

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
        namespace: &Namespace,
        table: &str,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let mysql_session = self.get_session(session).await?;

        // 1. Build Query String
        // MySQL uses backticks for identifiers
        let table_name = format!("`{}`.`{}`", 
            namespace.database.replace("`", "``"), 
            table.replace("`", "``")
        );

        let mut keys: Vec<&String> = data.columns.keys().collect();
        keys.sort();

        let sql = if keys.is_empty() {
             // MySQL: INSERT INTO table () VALUES ()
             format!("INSERT INTO {} () VALUES ()", table_name)
        } else {
            let cols_str = keys.iter().map(|k| format!("`{}`", k.replace("`", "``"))).collect::<Vec<_>>().join(", ");
            let params_str = vec!["?"; keys.len()].join(", ");
            format!("INSERT INTO {} ({}) VALUES ({})", table_name, cols_str, params_str)
        };

        // 2. Prepare Query
        let mut query = sqlx::query(&sql);
        for k in &keys {
            let val = data.columns.get(*k).unwrap();
            query = Self::bind_param(query, val);
        }

        // 3. Execute
        let start = Instant::now();
        let mut tx_guard = mysql_session.transaction_conn.lock().await;
        let result = if let Some(ref mut conn) = *tx_guard {
             query.execute(&mut **conn).await
        } else {
             query.execute(&mysql_session.pool).await
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
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let mysql_session = self.get_session(session).await?;

        if primary_key.columns.is_empty() {
            return Err(EngineError::execution_error("Primary key required for update operations".to_string()));
        }

        if data.columns.is_empty() {
             return Ok(QueryResult::with_affected_rows(0, 0.0));
        }

        let table_name = format!("`{}`.`{}`", 
            namespace.database.replace("`", "``"), 
            table.replace("`", "``")
        );

        let mut data_keys: Vec<&String> = data.columns.keys().collect();
        data_keys.sort();

        let mut pk_keys: Vec<&String> = primary_key.columns.keys().collect();
        pk_keys.sort();

        // UPDATE table SET col1=?, col2=? WHERE pk1=? AND pk2=?
        let set_clauses: Vec<String> = data_keys.iter()
            .map(|k| format!("`{}`=?", k.replace("`", "``")))
            .collect();

        let where_clauses: Vec<String> = pk_keys.iter()
            .map(|k| format!("`{}`=?", k.replace("`", "``")))
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
        let mut tx_guard = mysql_session.transaction_conn.lock().await;
        let result = if let Some(ref mut conn) = *tx_guard {
             query.execute(&mut **conn).await
        } else {
             query.execute(&mysql_session.pool).await
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
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
    ) -> EngineResult<QueryResult> {
        let mysql_session = self.get_session(session).await?;

        if primary_key.columns.is_empty() {
            return Err(EngineError::execution_error("Primary key required for delete operations".to_string()));
        }

        let table_name = format!("`{}`.`{}`", 
            namespace.database.replace("`", "``"), 
            table.replace("`", "``")
        );

        let mut pk_keys: Vec<&String> = primary_key.columns.keys().collect();
        pk_keys.sort();

        // DELETE FROM table WHERE pk1=?
        let where_clauses: Vec<String> = pk_keys.iter()
            .map(|k| format!("`{}`=?", k.replace("`", "``")))
            .collect();

        let sql = format!("DELETE FROM {} WHERE {}", table_name, where_clauses.join(" AND "));

        let mut query = sqlx::query(&sql);
        for k in &pk_keys {
            let val = primary_key.columns.get(*k).unwrap();
            query = Self::bind_param(query, val);
        }

        let start = Instant::now();
        let mut tx_guard = mysql_session.transaction_conn.lock().await;
        let result = if let Some(ref mut conn) = *tx_guard {
             query.execute(&mut **conn).await
        } else {
             query.execute(&mysql_session.pool).await
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

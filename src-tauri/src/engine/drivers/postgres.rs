//! PostgreSQL Driver
//!
//! Implements the DataEngine trait for PostgreSQL databases using SQLx.
//!
//! ## Transaction Handling
//!
//! When a transaction is started via `begin_transaction()`, a dedicated connection
//! is acquired from the pool and held until `commit()` or `rollback()` is called.
//! All queries during the transaction are executed on this dedicated connection
//! to ensure proper isolation.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow, Postgres};
use sqlx::{Column, Row, TypeInfo};
use tokio::sync::{Mutex, RwLock};

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::sql_safety;
use crate::engine::traits::DataEngine;
use crate::engine::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType, ColumnInfo,
    ConnectionConfig, Namespace, QueryId, QueryResult, Row as QRow, RowData, SessionId,
    TableColumn, TableSchema, Value, ForeignKey
};
use crate::engine::traits::{StreamEvent, StreamSender};
use futures::StreamExt;

/// Holds the connection state for a PostgreSQL session.
pub struct PostgresSession {
    /// The connection pool for this session
    pub pool: PgPool,
    /// Dedicated connection when a transaction is active
    pub transaction_conn: Mutex<Option<PoolConnection<Postgres>>>,
    /// Active queries (query_id -> backend_pid)
    pub active_queries: Mutex<HashMap<QueryId, i32>>,
}

impl PostgresSession {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            transaction_conn: Mutex::new(None),
            active_queries: Mutex::new(HashMap::new()),
        }
    }
}

/// PostgreSQL driver implementation
pub struct PostgresDriver {
    sessions: Arc<RwLock<HashMap<SessionId, Arc<PostgresSession>>>>,
}

impl PostgresDriver {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn get_session(&self, session: SessionId) -> EngineResult<Arc<PostgresSession>> {
        let sessions = self.sessions.read().await;
        sessions
            .get(&session)
            .cloned()
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))
    }

    /// Helper to bind a Value to a Postgres query
    fn bind_param<'q>(
        query: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
        value: &'q Value,
    ) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
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

    /// Converts a SQLx row to our universal Row type
    fn convert_row(pg_row: &PgRow) -> QRow {
        let values: Vec<Value> = pg_row
            .columns()
            .iter()
            .map(|col| Self::extract_value(pg_row, col.ordinal()))
            .collect();

        QRow { values }
    }

    /// Extracts a value from a PgRow at the given index
    fn extract_value(row: &PgRow, idx: usize) -> Value {
        // Try to interpret value based on common types
        // We use try_get with Option<T> to handle NULLs gracefully
        
        if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
            return v.map(Value::Int).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<i32>, _>(idx) {
            return v.map(|i| Value::Int(i as i64)).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<i16>, _>(idx) {
            return v.map(|i| Value::Int(i as i64)).unwrap_or(Value::Null);
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
        if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
            return v.map(Value::Text).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
            return v.map(Value::Bytes).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(idx) {
            return v.map(Value::Json).unwrap_or(Value::Null);
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

        // Fallback or unknown types treated as null or string if possible
        Value::Null
    }
    
    /// Gets column info from a PgRow
    fn get_column_info(row: &PgRow) -> Vec<ColumnInfo> {
        row.columns()
            .iter()
            .map(|col| ColumnInfo {
                name: col.name().to_string(),
                data_type: col.type_info().name().to_string(),
                nullable: true, // Postgres doesn't easily expose nullability in metadata from rows
            })
            .collect()
    }

    async fn fetch_backend_pid(
        conn: &mut PoolConnection<Postgres>,
    ) -> EngineResult<i32> {
        sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut **conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))
    }
}

impl Default for PostgresDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for PostgresDriver {
    fn driver_id(&self) -> &'static str {
        "postgres"
    }

    fn driver_name(&self) -> &'static str {
        "PostgreSQL"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        let conn_str = Self::build_connection_string(config);

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(&conn_str)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("password authentication failed") {
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
        let conn_str = Self::build_connection_string(config);
        let max_connections = config.pool_max_connections.unwrap_or(5);
        let min_connections = config.pool_min_connections.unwrap_or(0);
        let acquire_timeout = config.pool_acquire_timeout_secs.unwrap_or(30);

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .acquire_timeout(std::time::Duration::from_secs(acquire_timeout as u64))
            .connect(&conn_str)
            .await
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        let session_id = SessionId::new();
        let session = Arc::new(PostgresSession::new(pool));

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
        let pg_session = self.get_session(session).await?;
        let pool = &pg_session.pool;

        // Get current database name
        let current_db: (String,) = sqlx::query_as("SELECT current_database()")
            .fetch_one(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        let db_name = current_db.0;

        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT nspname
            FROM pg_catalog.pg_namespace
            WHERE nspname NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
              AND nspname NOT LIKE 'pg_temp_%'
            ORDER BY nspname
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let namespaces = rows
            .into_iter()
            .map(|(name,)| Namespace::with_schema(&db_name, name))
            .collect();

        Ok(namespaces)
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let pg_session = self.get_session(session).await?;
        let pool = &pg_session.pool;

        let schema = namespace.schema.as_deref().unwrap_or("public");
        let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

        // 1. Get total count
        let count_query = r#"
            SELECT COUNT(*) 
            FROM information_schema.tables 
            WHERE table_schema = $1 
            AND table_type = 'BASE TABLE'
            AND ($2 IS NULL OR table_name LIKE $3)
        "#;

        let count_row: (i64,) = sqlx::query_as(count_query)
            .bind(schema)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_one(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        
        let total_count = count_row.0;

        // 2. Get paginated results
        let mut query_str = r#"
            SELECT table_name, table_type 
            FROM information_schema.tables 
            WHERE table_schema = $1
            AND ($2 IS NULL OR table_name LIKE $3)
            ORDER BY table_name
        "#.to_string();

        if let Some(limit) = options.page_size {
             query_str.push_str(&format!(" LIMIT {}", limit));
             if let Some(page) = options.page {
                 let offset = (page.max(1) - 1) * limit;
                 query_str.push_str(&format!(" OFFSET {}", offset));
             }
        }

        let rows: Vec<(String, String)> = sqlx::query_as(&query_str)
            .bind(schema)
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

    async fn execute_stream(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        let pg_session = self.get_session(session).await?;

        // Use pool for streaming to avoid locking transaction connection for long duration
        let mut conn = pg_session
                .pool
                .acquire()
                .await
                .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        // Check if query returns rows (select)
        let returns_rows = sql_safety::returns_rows(self.driver_id(), query)
            .unwrap_or_else(|_| sql_safety::is_select_prefix(query));

        if !returns_rows {
             // Fallback to normal execute and send "Done"
             let result = self.execute(session, query, query_id).await?;
             let _ = sender.send(StreamEvent::Done(result.affected_rows.unwrap_or(0))).await;
             return Ok(());
        }

        // Register active query
        let backend_pid = Self::fetch_backend_pid(&mut conn).await?;
        {
            let mut active = pg_session.active_queries.lock().await;
            active.insert(query_id, backend_pid);
        }

        let mut stream = sqlx::query(query).fetch(&mut *conn);
        let mut columns_sent = false;
        let mut row_count = 0;

        while let Some(item) = stream.next().await {
            match item {
                Ok(pg_row) => {
                    if !columns_sent {
                        let columns = Self::get_column_info(&pg_row);
                        if sender.send(StreamEvent::Columns(columns)).await.is_err() {
                            break; // Receiver dropped
                        }
                        columns_sent = true;
                    }

                    let row = Self::convert_row(&pg_row);
                    if sender.send(StreamEvent::Row(row)).await.is_err() {
                        break;
                    }
                    row_count += 1;
                }
                Err(e) => {
                    let _ = sender.send(StreamEvent::Error(e.to_string())).await;
                    break;
                }
            }
        }

        // Cleanup
        {
            let mut active = pg_session.active_queries.lock().await;
            active.remove(&query_id);
        }

        let _ = sender.send(StreamEvent::Done(row_count)).await;

        Ok(())
    }

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let pg_session = self.get_session(session).await?;
        let start = Instant::now();

        let returns_rows = sql_safety::returns_rows(self.driver_id(), query)
            .unwrap_or_else(|_| sql_safety::is_select_prefix(query));

        // Check for active transaction
        let mut tx_guard = pg_session.transaction_conn.lock().await;

        let result = if let Some(ref mut conn) = *tx_guard {
             // Use dedicated connection
             
             // Register active query
             let backend_pid = Self::fetch_backend_pid(conn).await?;
             {
                 let mut active = pg_session.active_queries.lock().await;
                 active.insert(query_id, backend_pid);
             }

             let result = if returns_rows {
                let pg_rows: Vec<PgRow> = sqlx::query(query)
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

                if pg_rows.is_empty() {
                    Ok(QueryResult {
                        columns: Vec::new(),
                        rows: Vec::new(),
                        affected_rows: None,
                        execution_time_ms,
                    })
                } else {
                    let columns = Self::get_column_info(&pg_rows[0]);
                    let rows: Vec<QRow> = pg_rows.iter().map(Self::convert_row).collect();

                    Ok(QueryResult {
                        columns,
                        rows,
                        affected_rows: None,
                        execution_time_ms,
                    })
                }
             } else {
                 let result = sqlx::query(query)
                    .execute(&mut **conn)
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

            let mut active = pg_session.active_queries.lock().await;
            active.remove(&query_id);
            result

        } else {
            // Use pool
             let mut conn = pg_session
                .pool
                .acquire()
                .await
                .map_err(|e| EngineError::connection_failed(e.to_string()))?;

             let backend_pid = Self::fetch_backend_pid(&mut conn).await?;
             {
                 let mut active = pg_session.active_queries.lock().await;
                 active.insert(query_id, backend_pid);
             }

             let result = if returns_rows {
                let pg_rows: Vec<PgRow> = sqlx::query(query)
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

                if pg_rows.is_empty() {
                    Ok(QueryResult {
                        columns: Vec::new(),
                        rows: Vec::new(),
                        affected_rows: None,
                        execution_time_ms,
                    })
                } else {
                    let columns = Self::get_column_info(&pg_rows[0]);
                    let rows: Vec<QRow> = pg_rows.iter().map(Self::convert_row).collect();

                    Ok(QueryResult {
                        columns,
                        rows,
                        affected_rows: None,
                        execution_time_ms,
                    })
                }
             } else {
                 let result = sqlx::query(query)
                    .execute(&mut *conn)
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

            let mut active = pg_session.active_queries.lock().await;
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
        let pg_session = self.get_session(session).await?;
        let pool = &pg_session.pool;

        let schema = namespace.schema.as_deref().unwrap_or("public");

        // Get column info
        let column_rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT 
                column_name::text,
                data_type::text,
                is_nullable::text,
                column_default::text
            FROM information_schema.columns
            WHERE table_schema = $1 AND table_name = $2
            ORDER BY ordinal_position
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        // Get primary key columns
        let pk_rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT a.attname::text
            FROM pg_index i
            JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
            JOIN pg_class c ON c.oid = i.indrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE i.indisprimary
              AND n.nspname = $1
              AND c.relname = $2
            ORDER BY array_position(i.indkey, a.attnum)
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let pk_columns: Vec<String> = pk_rows.into_iter().map(|(name,)| name).collect();

        // Get foreign keys
        // Note: This query joins information_schema views to find FK definitions matches
        let fk_rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT
                kcu.column_name::text,
                ccu.table_name::text AS foreign_table_name,
                ccu.column_name::text AS foreign_column_name,
                tc.constraint_name::text
            FROM
                information_schema.table_constraints AS tc
                JOIN information_schema.key_column_usage AS kcu
                  ON tc.constraint_name = kcu.constraint_name
                  AND tc.table_schema = kcu.table_schema
                JOIN information_schema.constraint_column_usage AS ccu
                  ON ccu.constraint_name = tc.constraint_name
                  AND ccu.table_schema = tc.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
                AND tc.table_schema = $1
                AND tc.table_name = $2
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let foreign_keys: Vec<ForeignKey> = fk_rows
            .into_iter()
            .map(|(column, referenced_table, referenced_column, constraint_name)| ForeignKey {
                column,
                referenced_table,
                referenced_column,
                constraint_name: constraint_name,
            })
            .collect();

        // Get columns vec
        let columns: Vec<TableColumn> = column_rows
            .into_iter()
            .map(|(name, data_type, is_nullable, default_value)| TableColumn {
                is_primary_key: pk_columns.contains(&name),
                name,
                data_type,
                nullable: is_nullable == "YES",
                default_value,
            })
            .collect();

        // Get row count estimate
        let count_row: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT reltuples::bigint
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = $1 AND c.relname = $2
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_optional(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let row_count_estimate = count_row.map(|(c,)| c as u64);

        Ok(TableSchema {
            columns,
            primary_key: if pk_columns.is_empty() { None } else { Some(pk_columns) },
            foreign_keys,
            row_count_estimate,
        })
    }

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let schema = namespace.schema.as_deref().unwrap_or("public");
        // Use quoted identifiers to handle special characters
        let query = format!(
            "SELECT * FROM \"{}\".\"{}\" LIMIT {}",
            schema, table, limit
        );
        self.execute(session, &query, QueryId::new()).await
    }

    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        let pg_session = self.get_session(session).await?;

        let backend_pids: Vec<i32> = {
            let active = pg_session.active_queries.lock().await;
            if let Some(qid) = query_id {
                match active.get(&qid) {
                    Some(pid) => vec![*pid],
                    None => return Err(EngineError::execution_error("Query not found")),
                }
            } else {
                active.values().copied().collect()
            }
        };

        if backend_pids.is_empty() {
            return Err(EngineError::execution_error("No active queries to cancel"));
        }

        let mut conn = pg_session
            .pool
            .acquire()
            .await
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        for pid in backend_pids {
            let _ = sqlx::query("SELECT pg_cancel_backend($1)")
                .bind(pid)
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
        let pg_session = self.get_session(session).await?;
        let mut tx = pg_session.transaction_conn.lock().await;

        // Check if a transaction is already active
        if tx.is_some() {
            return Err(EngineError::transaction_error(
                "A transaction is already active on this session"
            ));
        }

        // Acquire a dedicated connection from the pool
        let mut conn = pg_session.pool.acquire().await
            .map_err(|e| EngineError::connection_failed(format!(
                "Failed to acquire connection for transaction: {}", e
            )))?;

        // Execute BEGIN on the dedicated connection
        sqlx::query("BEGIN")
            .execute(&mut *conn)
            .await
            .map_err(|e| EngineError::execution_error(format!(
                "Failed to begin transaction: {}", e
            )))?;

        // Store the dedicated connection
        *tx = Some(conn);

        Ok(())
    }

    async fn commit(&self, session: SessionId) -> EngineResult<()> {
        let pg_session = self.get_session(session).await?;
        let mut tx = pg_session.transaction_conn.lock().await;

        // Get the dedicated connection, or error if no transaction active
        let mut conn = tx.take()
            .ok_or_else(|| EngineError::transaction_error(
                "No active transaction to commit"
            ))?;

        // Execute COMMIT on the dedicated connection
        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .map_err(|e| EngineError::execution_error(format!(
                "Failed to commit transaction: {}", e
            )))?;

        // Connection is automatically returned to the pool when dropped
        Ok(())
    }

    async fn rollback(&self, session: SessionId) -> EngineResult<()> {
        let pg_session = self.get_session(session).await?;
        let mut tx = pg_session.transaction_conn.lock().await;

        // Get the dedicated connection, or error if no transaction active
        let mut conn = tx.take()
            .ok_or_else(|| EngineError::transaction_error(
                "No active transaction to rollback"
            ))?;

        // Execute ROLLBACK on the dedicated connection
        sqlx::query("ROLLBACK")
            .execute(&mut *conn)
            .await
            .map_err(|e| EngineError::execution_error(format!(
                "Failed to rollback transaction: {}", e
            )))?;

        // Connection is automatically returned to the pool when dropped
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
        let pg_session = self.get_session(session).await?;

        // 1. Build Query String
        let table_name = if let Some(schema) = &namespace.schema {
            format!("\"{}\".\"{}\"", schema.replace("\"", "\"\""), table.replace("\"", "\"\""))
        } else {
            format!("\"{}\"", table.replace("\"", "\"\""))
        };

        let mut keys: Vec<&String> = data.columns.keys().collect();
        keys.sort();

        let sql = if keys.is_empty() {
            format!("INSERT INTO {} DEFAULT VALUES", table_name)
        } else {
            let cols_str = keys.iter().map(|k| format!("\"{}\"", k.replace("\"", "\"\""))).collect::<Vec<_>>().join(", ");
            let params_str = (1..=keys.len()).map(|i| format!("${}", i)).collect::<Vec<_>>().join(", ");
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
        let mut tx_guard = pg_session.transaction_conn.lock().await;
        let result = if let Some(ref mut conn) = *tx_guard {
             query.execute(&mut **conn).await
        } else {
             query.execute(&pg_session.pool).await
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
        let pg_session = self.get_session(session).await?;

        if primary_key.columns.is_empty() {
            return Err(EngineError::execution_error("Primary key required for update operations".to_string()));
        }

        if data.columns.is_empty() {
             // Nothing to update
             return Ok(QueryResult::with_affected_rows(0, 0.0));
        }

        let table_name = if let Some(schema) = &namespace.schema {
            format!("\"{}\".\"{}\"", schema.replace("\"", "\"\""), table.replace("\"", "\"\""))
        } else {
            format!("\"{}\"", table.replace("\"", "\"\""))
        };

        let mut data_keys: Vec<&String> = data.columns.keys().collect();
        data_keys.sort();

        let mut pk_keys: Vec<&String> = primary_key.columns.keys().collect();
        pk_keys.sort();

        // UPDATE table SET col1=$1, col2=$2 WHERE pk1=$3 AND pk2=$4
        let mut set_clauses = Vec::new();
        let mut i = 1;
        for k in &data_keys {
            set_clauses.push(format!("\"{}\"=${}", k.replace("\"", "\"\""), i));
            i += 1;
        }

        let mut where_clauses = Vec::new();
        for k in &pk_keys {
            where_clauses.push(format!("\"{}\"=${}", k.replace("\"", "\"\""), i));
            i += 1;
        }

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
        let mut tx_guard = pg_session.transaction_conn.lock().await;
        let result = if let Some(ref mut conn) = *tx_guard {
             query.execute(&mut **conn).await
        } else {
             query.execute(&pg_session.pool).await
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
        let pg_session = self.get_session(session).await?;

        if primary_key.columns.is_empty() {
            return Err(EngineError::execution_error("Primary key required for delete operations".to_string()));
        }

        let table_name = if let Some(schema) = &namespace.schema {
            format!("\"{}\".\"{}\"", schema.replace("\"", "\"\""), table.replace("\"", "\"\""))
        } else {
            format!("\"{}\"", table.replace("\"", "\"\""))
        };

        let mut pk_keys: Vec<&String> = primary_key.columns.keys().collect();
        pk_keys.sort();

        // DELETE FROM table WHERE pk1=$1
        let mut where_clauses = Vec::new();
        let mut i = 1;
        for k in &pk_keys {
            where_clauses.push(format!("\"{}\"=${}", k.replace("\"", "\"\""), i));
            i += 1;
        }

        let sql = format!("DELETE FROM {} WHERE {}", table_name, where_clauses.join(" AND "));

        let mut query = sqlx::query(&sql);
        for k in &pk_keys {
            let val = primary_key.columns.get(*k).unwrap();
            query = Self::bind_param(query, val);
        }

        let start = Instant::now();
        let mut tx_guard = pg_session.transaction_conn.lock().await;
        let result = if let Some(ref mut conn) = *tx_guard {
             query.execute(&mut **conn).await
        } else {
             query.execute(&pg_session.pool).await
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



    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_explain(&self) -> bool {
        true
    }

    fn supports_explain(&self) -> bool {
        true
    }
}

impl PostgresDriver {
    /// Builds a connection string from config
    fn build_connection_string(config: &ConnectionConfig) -> String {
        let db = config.database.as_deref().unwrap_or("postgres");
        let ssl_mode = if config.ssl { "require" } else { "disable" };

        format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}",
            config.username, config.password, config.host, config.port, db, ssl_mode
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_string_building() {
        let config = ConnectionConfig {
            driver: "postgres".to_string(),
            host: "localhost".to_string(),
            port: 5432,
            username: "user".to_string(),
            password: "pass".to_string(),
            database: Some("testdb".to_string()),
            ssl: false,
            environment: "development".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
        };

        let conn_str = PostgresDriver::build_connection_string(&config);
        assert!(conn_str.contains("localhost:5432"));
        assert!(conn_str.contains("testdb"));
        assert!(conn_str.contains("sslmode=disable"));
    }
}

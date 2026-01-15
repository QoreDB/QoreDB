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
use tokio::sync::Mutex;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::traits::DataEngine;
use crate::engine::types::{
    Collection, CollectionType, ColumnInfo, ConnectionConfig, Namespace, QueryResult,
    Row as QRow, RowData, SessionId, TableColumn, TableSchema, Value,
};

/// Holds the connection state for a PostgreSQL session.
///
/// A session always has a pool for regular operations.
/// When a transaction is active, a dedicated connection is held
/// to ensure all queries within the transaction use the same connection.
pub struct PostgresSession {
    /// The connection pool for this session
    pub pool: PgPool,
    /// Dedicated connection when a transaction is active
    /// This connection is acquired on BEGIN and released on COMMIT/ROLLBACK
    pub transaction_conn: Option<PoolConnection<Postgres>>,
}

impl PostgresSession {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            transaction_conn: None,
        }
    }

    /// Returns true if a transaction is currently active
    pub fn has_active_transaction(&self) -> bool {
        self.transaction_conn.is_some()
    }
}

/// PostgreSQL driver implementation
pub struct PostgresDriver {
    sessions: Arc<Mutex<HashMap<SessionId, PostgresSession>>>,
}

impl PostgresDriver {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Builds a connection string from config
    fn build_connection_string(config: &ConnectionConfig) -> String {
        let ssl_mode = if config.ssl { "require" } else { "disable" };
        let db = config.database.as_deref().unwrap_or("postgres");

        format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}",
            config.username, config.password, config.host, config.port, db, ssl_mode
        )
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
            // Fallback for arrays or other complex types not yet fully mapped
            Value::Array(_) => query.bind(Option::<String>::None),
        }
    }

    /// Extracts a value from a PgRow at the given index
    fn extract_value(row: &PgRow, idx: usize) -> Value {
        // IMPORTANT: Test integers BEFORE bool to avoid misinterpretation
        // Try different integer types in order of likelihood
        if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
            return v.map(Value::Int).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<i32>, _>(idx) {
            return v.map(|i| Value::Int(i as i64)).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<i16>, _>(idx) {
            return v.map(|i| Value::Int(i as i64)).unwrap_or(Value::Null);
        }
        // Bool AFTER integers
        if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
            return v.map(Value::Bool).unwrap_or(Value::Null);
        }
        // Floats
        if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
            return v.map(Value::Float).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<Option<f32>, _>(idx) {
            return v.map(|f| Value::Float(f as f64)).unwrap_or(Value::Null);
        }
        // String
        if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
            return v.map(Value::Text).unwrap_or(Value::Null);
        }
        // Date/Time types - convert to ISO 8601 string
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
        // Binary
        if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
            return v.map(Value::Bytes).unwrap_or(Value::Null);
        }
        // JSON/JSONB
        if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(idx) {
            return v.map(Value::Json).unwrap_or(Value::Null);
        }

        // Fallback: try to get as string representation
        Value::Null
    }

    /// Gets column info from a PgRow
    fn get_column_info(row: &PgRow) -> Vec<ColumnInfo> {
        row.columns()
            .iter()
            .map(|col| ColumnInfo {
                name: col.name().to_string(),
                data_type: col.type_info().name().to_string(),
                nullable: true, // SQLx doesn't expose nullability easily at runtime
            })
            .collect()
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
                if e.to_string().contains("password authentication failed") {
                    EngineError::auth_failed(e.to_string())
                } else {
                    EngineError::connection_failed(e.to_string())
                }
            })?;

        // Test with a simple query
        sqlx::query("SELECT 1")
            .execute(&pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        pool.close().await;
        Ok(())
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        let conn_str = Self::build_connection_string(config);

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect(&conn_str)
            .await
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        let session_id = SessionId::new();
        let session = PostgresSession::new(pool);

        let mut sessions = self.sessions.lock().await;
        sessions.insert(session_id, session);

        Ok(session_id)
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        let mut sessions = self.sessions.lock().await;

        if let Some(pg_session) = sessions.remove(&session) {
            pg_session.pool.close().await;
            Ok(())
        } else {
            Err(EngineError::session_not_found(session.0.to_string()))
        }
    }

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let sessions = self.sessions.lock().await;
        let pg_session = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;
        let pool = &pg_session.pool;

        // Get all schemas grouped by database
        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT current_database()::text as database, schema_name::text
            FROM information_schema.schemata
            WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
            ORDER BY schema_name
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let namespaces = rows
            .into_iter()
            .map(|(db, schema)| Namespace::with_schema(db, schema))
            .collect();

        Ok(namespaces)
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
    ) -> EngineResult<Vec<Collection>> {
        let sessions = self.sessions.lock().await;
        let pg_session = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;
        let pool = &pg_session.pool;

        let schema = namespace.schema.as_deref().unwrap_or("public");

        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT table_name::text, table_type::text
            FROM information_schema.tables
            WHERE table_schema = $1
            ORDER BY table_name
            "#,
        )
        .bind(schema)
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

        Ok(collections)
    }

    async fn execute(&self, session: SessionId, query: &str) -> EngineResult<QueryResult> {
        let mut sessions = self.sessions.lock().await;
        let pg_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let start = Instant::now();

        // Determine if this is a SELECT-like query
        let trimmed = query.trim().to_uppercase();
        let is_select = trimmed.starts_with("SELECT")
            || trimmed.starts_with("WITH")
            || trimmed.starts_with("SHOW")
            || trimmed.starts_with("EXPLAIN");

        // Route to transaction connection if active, otherwise use pool
        if let Some(ref mut conn) = pg_session.transaction_conn {
            // Execute on dedicated transaction connection
            if is_select {
                let pg_rows: Vec<PgRow> = sqlx::query(query)
                    .fetch_all(&mut **conn)
                    .await
                    .map_err(|e| {
                        let msg = e.to_string();
                        if msg.contains("syntax error") {
                            EngineError::syntax_error(msg)
                        } else {
                            EngineError::execution_error(msg)
                        }
                    })?;

                let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

                if pg_rows.is_empty() {
                    return Ok(QueryResult {
                        columns: Vec::new(),
                        rows: Vec::new(),
                        affected_rows: None,
                        execution_time_ms,
                    });
                }

                let columns = Self::get_column_info(&pg_rows[0]);
                let rows: Vec<QRow> = pg_rows.iter().map(Self::convert_row).collect();

                Ok(QueryResult {
                    columns,
                    rows,
                    affected_rows: None,
                    execution_time_ms,
                })
            } else {
                let result = sqlx::query(query)
                    .execute(&mut **conn)
                    .await
                    .map_err(|e| {
                        let msg = e.to_string();
                        if msg.contains("syntax error") {
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
            // No transaction active - use pool
            let pool = &pg_session.pool;

            if is_select {
                let pg_rows: Vec<PgRow> = sqlx::query(query)
                    .fetch_all(pool)
                    .await
                    .map_err(|e| {
                        let msg = e.to_string();
                        if msg.contains("syntax error") {
                            EngineError::syntax_error(msg)
                        } else {
                            EngineError::execution_error(msg)
                        }
                    })?;

                let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

                if pg_rows.is_empty() {
                    return Ok(QueryResult {
                        columns: Vec::new(),
                        rows: Vec::new(),
                        affected_rows: None,
                        execution_time_ms,
                    });
                }

                let columns = Self::get_column_info(&pg_rows[0]);
                let rows: Vec<QRow> = pg_rows.iter().map(Self::convert_row).collect();

                Ok(QueryResult {
                    columns,
                    rows,
                    affected_rows: None,
                    execution_time_ms,
                })
            } else {
                let result = sqlx::query(query).execute(pool).await.map_err(|e| {
                    let msg = e.to_string();
                    if msg.contains("syntax error") {
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
        }
    }

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        let sessions = self.sessions.lock().await;
        let pg_session = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;
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

        // Build columns vec
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
        self.execute(session, &query).await
    }

    async fn cancel(&self, session: SessionId) -> EngineResult<()> {
        // PostgreSQL cancellation requires pg_cancel_backend
        // For now, we just verify the session exists
        let sessions = self.sessions.lock().await;
        if sessions.contains_key(&session) {
            // TODO: Implement proper query cancellation with pg_cancel_backend
            Ok(())
        } else {
            Err(EngineError::session_not_found(session.0.to_string()))
        }
    }

    // ==================== Transaction Methods ====================

    async fn begin_transaction(&self, session: SessionId) -> EngineResult<()> {
        let mut sessions = self.sessions.lock().await;
        let pg_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        // Check if a transaction is already active
        if pg_session.transaction_conn.is_some() {
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
        pg_session.transaction_conn = Some(conn);

        Ok(())
    }

    async fn commit(&self, session: SessionId) -> EngineResult<()> {
        let mut sessions = self.sessions.lock().await;
        let pg_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        // Get the dedicated connection, or error if no transaction active
        let mut conn = pg_session.transaction_conn.take()
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
        let mut sessions = self.sessions.lock().await;
        let pg_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        // Get the dedicated connection, or error if no transaction active
        let mut conn = pg_session.transaction_conn.take()
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
        let mut sessions = self.sessions.lock().await;
        let pg_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

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
        let result = if let Some(ref mut conn) = pg_session.transaction_conn {
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
        let mut sessions = self.sessions.lock().await;
        let pg_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

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
        let result = if let Some(ref mut conn) = pg_session.transaction_conn {
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
        let mut sessions = self.sessions.lock().await;
        let pg_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

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
        let result = if let Some(ref mut conn) = pg_session.transaction_conn {
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
            environment: Some("development".to_string()),
            read_only: false,
            ssh_tunnel: None,
        };

        let conn_str = PostgresDriver::build_connection_string(&config);
        assert!(conn_str.contains("localhost:5432"));
        assert!(conn_str.contains("testdb"));
        assert!(conn_str.contains("sslmode=disable"));
    }
}

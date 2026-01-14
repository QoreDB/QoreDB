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
use sqlx::mysql::{MySql, MySqlPool, MySqlPoolOptions, MySqlRow};
use sqlx::pool::PoolConnection;
use sqlx::{Column, Row, TypeInfo};
use tokio::sync::Mutex;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::traits::DataEngine;
use crate::engine::types::{
    Collection, CollectionType, ColumnInfo, ConnectionConfig, Namespace, QueryResult,
    Row as QRow, RowData, SessionId, TableColumn, TableSchema, Value,
};

/// Holds the connection state for a MySQL session.
pub struct MySqlSession {
    /// The connection pool for this session
    pub pool: MySqlPool,
    /// Dedicated connection when a transaction is active
    pub transaction_conn: Option<PoolConnection<MySql>>,
}

impl MySqlSession {
    pub fn new(pool: MySqlPool) -> Self {
        Self {
            pool,
            transaction_conn: None,
        }
    }
}

/// MySQL driver implementation
pub struct MySqlDriver {
    sessions: Arc<Mutex<HashMap<SessionId, MySqlSession>>>,
}

impl MySqlDriver {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
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

    /// Builds a connection string from config
    fn build_connection_string(config: &ConnectionConfig) -> String {
        let db = config.database.as_deref().unwrap_or("mysql");
        let ssl_mode = if config.ssl { "REQUIRED" } else { "DISABLED" };

        format!(
            "mysql://{}:{}@{}:{}/{}?ssl-mode={}",
            config.username, config.password, config.host, config.port, db, ssl_mode
        )
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
        let conn_str = Self::build_connection_string(config);

        let pool = MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(&conn_str)
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
        let conn_str = Self::build_connection_string(config);

        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect(&conn_str)
            .await
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        let session_id = SessionId::new();
        let session = MySqlSession::new(pool);

        let mut sessions = self.sessions.lock().await;
        sessions.insert(session_id, session);

        Ok(session_id)
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        let mut sessions = self.sessions.lock().await;

        if let Some(mysql_session) = sessions.remove(&session) {
            mysql_session.pool.close().await;
            Ok(())
        } else {
            Err(EngineError::session_not_found(session.0.to_string()))
        }
    }

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let sessions = self.sessions.lock().await;
        let mysql_session = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;
        let pool = &mysql_session.pool;

        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT schema_name
            FROM information_schema.schemata
            WHERE schema_name NOT IN ('information_schema', 'mysql', 'performance_schema', 'sys')
            ORDER BY schema_name
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let namespaces = rows.into_iter().map(|(db,)| Namespace::new(db)).collect();

        Ok(namespaces)
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
    ) -> EngineResult<Vec<Collection>> {
        let sessions = self.sessions.lock().await;
        let mysql_session = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;
        let pool = &mysql_session.pool;

        // Cast to CHAR to avoid BINARY type mismatch with Rust String
        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT CAST(TABLE_NAME AS CHAR) AS table_name, CAST(TABLE_TYPE AS CHAR) AS table_type
            FROM information_schema.TABLES
            WHERE TABLE_SCHEMA = ?
            ORDER BY TABLE_NAME
            "#,
        )
        .bind(&namespace.database)
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

    /// Executes a query and returns the result
    /// 
    /// Routes to transaction connection if active, otherwise uses pool.
    async fn execute(&self, session: SessionId, query: &str) -> EngineResult<QueryResult> {
        let mut sessions = self.sessions.lock().await;
        let mysql_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let start = Instant::now();

        let trimmed = query.trim().to_uppercase();
        let is_select = trimmed.starts_with("SELECT")
            || trimmed.starts_with("SHOW")
            || trimmed.starts_with("DESCRIBE")
            || trimmed.starts_with("EXPLAIN");

        // Route to transaction connection if active, otherwise use pool
        if let Some(ref mut conn) = mysql_session.transaction_conn {
            // Execute on dedicated transaction connection
            if is_select {
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

                let execution_time_ms = start.elapsed().as_millis() as u64;

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

                let execution_time_ms = start.elapsed().as_millis() as u64;

                Ok(QueryResult::with_affected_rows(
                    result.rows_affected(),
                    execution_time_ms,
                ))
            }
        } else {
            // No transaction active - use pool
            let pool = &mysql_session.pool;

            if is_select {
                let mysql_rows: Vec<MySqlRow> = sqlx::query(query)
                    .fetch_all(pool)
                    .await
                    .map_err(|e| {
                        let msg = e.to_string();
                        if msg.contains("syntax") {
                            EngineError::syntax_error(msg)
                        } else {
                            EngineError::execution_error(msg)
                        }
                    })?;

                let execution_time_ms = start.elapsed().as_millis() as u64;

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
            } else {
                let result = sqlx::query(query).execute(pool).await.map_err(|e| {
                    let msg = e.to_string();
                    if msg.contains("syntax") {
                        EngineError::syntax_error(msg)
                    } else {
                        EngineError::execution_error(msg)
                    }
                })?;

                let execution_time_ms = start.elapsed().as_millis() as u64;

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
        let mysql_session = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;
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
        // Use backticks for MySQL identifier quoting
        let query = format!(
            "SELECT * FROM `{}`.`{}` LIMIT {}",
            namespace.database, table, limit
        );
        self.execute(session, &query).await
    }

    async fn cancel(&self, session: SessionId) -> EngineResult<()> {
        let sessions = self.sessions.lock().await;
        if sessions.contains_key(&session) {
            Ok(())
        } else {
            Err(EngineError::session_not_found(session.0.to_string()))
        }
    }

    // ==================== Transaction Methods ====================

    async fn begin_transaction(&self, session: SessionId) -> EngineResult<()> {
        let mut sessions = self.sessions.lock().await;
        let mysql_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        if mysql_session.transaction_conn.is_some() {
            return Err(EngineError::transaction_error(
                "A transaction is already active on this session"
            ));
        }

        let mut conn = mysql_session.pool.acquire().await
            .map_err(|e| EngineError::connection_failed(format!(
                "Failed to acquire connection for transaction: {}", e
            )))?;

        sqlx::query("START TRANSACTION")
            .execute(&mut *conn)
            .await
            .map_err(|e| EngineError::execution_error(format!(
                "Failed to begin transaction: {}", e
            )))?;

        mysql_session.transaction_conn = Some(conn);
        Ok(())
    }

    async fn commit(&self, session: SessionId) -> EngineResult<()> {
        let mut sessions = self.sessions.lock().await;
        let mysql_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let mut conn = mysql_session.transaction_conn.take()
            .ok_or_else(|| EngineError::transaction_error(
                "No active transaction to commit"
            ))?;

        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .map_err(|e| EngineError::execution_error(format!(
                "Failed to commit transaction: {}", e
            )))?;

        Ok(())
    }

    async fn rollback(&self, session: SessionId) -> EngineResult<()> {
        let mut sessions = self.sessions.lock().await;
        let mysql_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let mut conn = mysql_session.transaction_conn.take()
            .ok_or_else(|| EngineError::transaction_error(
                "No active transaction to rollback"
            ))?;

        sqlx::query("ROLLBACK")
            .execute(&mut *conn)
            .await
            .map_err(|e| EngineError::execution_error(format!(
                "Failed to rollback transaction: {}", e
            )))?;

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
        let mysql_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

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
        let result = if let Some(ref mut conn) = mysql_session.transaction_conn {
             query.execute(&mut **conn).await
        } else {
             query.execute(&mysql_session.pool).await
        };

        let result = result.map_err(|e| EngineError::execution_error(e.to_string()))?;
        
        Ok(QueryResult::with_affected_rows(
            result.rows_affected(),
            start.elapsed().as_millis() as u64,
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
        let mysql_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        if primary_key.columns.is_empty() {
            return Err(EngineError::execution_error("Primary key required for update operations".to_string()));
        }

        if data.columns.is_empty() {
             return Ok(QueryResult::with_affected_rows(0, 0));
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
        let result = if let Some(ref mut conn) = mysql_session.transaction_conn {
             query.execute(&mut **conn).await
        } else {
             query.execute(&mysql_session.pool).await
        };

        let result = result.map_err(|e| EngineError::execution_error(e.to_string()))?;
        
        Ok(QueryResult::with_affected_rows(
            result.rows_affected(),
            start.elapsed().as_millis() as u64,
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
        let mysql_session = sessions
            .get_mut(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

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
        let result = if let Some(ref mut conn) = mysql_session.transaction_conn {
             query.execute(&mut **conn).await
        } else {
             query.execute(&mysql_session.pool).await
        };

        let result = result.map_err(|e| EngineError::execution_error(e.to_string()))?;
        
        Ok(QueryResult::with_affected_rows(
            result.rows_affected(),
            start.elapsed().as_millis() as u64,
        ))
    }

    fn supports_mutations(&self) -> bool {
        true
    }
}

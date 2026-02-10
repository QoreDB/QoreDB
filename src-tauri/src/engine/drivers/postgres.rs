//! PostgreSQL Driver
//!
//! Implements the DataEngine trait for PostgreSQL databases using SQLx.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow, Postgres};
use sqlx::{Row};
use tokio::sync::{Mutex, RwLock};
use futures::StreamExt;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::drivers::postgres_utils::{
    bind_param, collect_enum_type_oids, convert_row, get_column_info, load_enum_labels,
    EnumLabelMap,
};
use crate::engine::sql_safety;
use crate::engine::traits::{DataEngine, StreamEvent, StreamSender};
use crate::engine::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType,
    ConnectionConfig, Namespace, QueryId, QueryResult, Row as QRow, RowData, SessionId,
    TableColumn, TableIndex, TableSchema, Value, ForeignKey,
    TableQueryOptions, PaginatedQueryResult, SortDirection, FilterOperator,
    Routine, RoutineList, RoutineListOptions, RoutineType,
};

pub struct PostgresSession {
    pub pool: PgPool,
    pub transaction_conn: Mutex<Option<PoolConnection<Postgres>>>,
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

    async fn create_pool(
        config: &ConnectionConfig,
        max_connections: u32,
        min_connections: u32,
        acquire_timeout_secs: u64,
        classify_auth_error: bool,
        run_test_query: bool,
    ) -> EngineResult<PgPool> {
        let conn_str = Self::build_connection_string(config);

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .acquire_timeout(std::time::Duration::from_secs(acquire_timeout_secs))
            .connect(&conn_str)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if classify_auth_error && msg.contains("password authentication failed") {
                    EngineError::auth_failed(msg)
                } else {
                    EngineError::connection_failed(msg)
                }
            })?;

        if run_test_query {
            sqlx::query("SELECT 1")
                .execute(&pool)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
        }

        Ok(pool)
    }

    async fn get_session(&self, session: SessionId) -> EngineResult<Arc<PostgresSession>> {
        let sessions = self.sessions.read().await;
        sessions
            .get(&session)
            .cloned()
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))
    }

    fn quote_ident(name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    async fn apply_namespace_on_conn(
        conn: &mut PoolConnection<Postgres>,
        namespace: &Option<Namespace>,
        query: &str,
        in_transaction: bool,
    ) -> EngineResult<()> {
        let lower = query.trim_start().to_ascii_lowercase();
        // Avoid overriding explicit search_path changes.
        if lower.starts_with("set ") && lower.contains("search_path") {
            return Ok(());
        }

        let schema = namespace
            .as_ref()
            .and_then(|ns| ns.schema.as_ref())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        if let Some(schema) = schema {
            let schema_sql = Self::quote_ident(schema);
            let list = if schema.eq_ignore_ascii_case("public") {
                schema_sql
            } else {
                format!("{}, public", schema_sql)
            };

            let set_sql = if in_transaction {
                format!("SET LOCAL search_path TO {}", list)
            } else {
                format!("SET search_path TO {}", list)
            };

            sqlx::query(&set_sql)
                .execute(&mut **conn)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
        }

        Ok(())
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
        let pool = Self::create_pool(config, 1, 0, 10, true, true).await?;
        pool.close().await;
        Ok(())
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        let max_connections = config.pool_max_connections.unwrap_or(5);
        let min_connections = config.pool_min_connections.unwrap_or(0);
        let acquire_timeout = config.pool_acquire_timeout_secs.unwrap_or(30);

        let pool = Self::create_pool(
            config,
            max_connections,
            min_connections,
            acquire_timeout as u64,
            false,
            false,
        )
        .await?;

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

        // 1. Get total count (tables + views + materialized views)
        let count_query = r#"
            SELECT COUNT(*) FROM (
                SELECT table_name AS name
                FROM information_schema.tables
                WHERE table_schema = $1
                AND ($2 IS NULL OR table_name LIKE $3)
                UNION ALL
                SELECT matviewname AS name
                FROM pg_matviews
                WHERE schemaname = $1
                AND ($2 IS NULL OR matviewname LIKE $3)
            ) combined
        "#;

        let count_row: (i64,) = sqlx::query_as(count_query)
            .bind(schema)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_one(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let total_count = count_row.0;

        // 2. Get paginated results (tables + views + materialized views)
        let mut query_str = r#"
            SELECT name, ctype FROM (
                SELECT table_name AS name,
                    CASE WHEN table_type = 'VIEW' THEN 'View' ELSE 'Table' END AS ctype
                FROM information_schema.tables
                WHERE table_schema = $1
                AND ($2 IS NULL OR table_name LIKE $3)
                UNION ALL
                SELECT matviewname AS name, 'MaterializedView' AS ctype
                FROM pg_matviews
                WHERE schemaname = $1
                AND ($2 IS NULL OR matviewname LIKE $3)
            ) combined ORDER BY name
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
            .map(|(name, ctype)| {
                let collection_type = match ctype.as_str() {
                    "View" => CollectionType::View,
                    "MaterializedView" => CollectionType::MaterializedView,
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

    async fn list_routines(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: RoutineListOptions,
    ) -> EngineResult<RoutineList> {
        let pg_session = self.get_session(session).await?;
        let pool = &pg_session.pool;

        let schema = namespace.schema.as_deref().unwrap_or("public");
        let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

        // Filter by routine type if specified
        let type_filter = match &options.routine_type {
            Some(RoutineType::Function) => Some("f"),
            Some(RoutineType::Procedure) => Some("p"),
            None => None,
        };

        // 1. Get total count
        let count_query = r#"
            SELECT COUNT(*)
            FROM pg_proc p
            JOIN pg_namespace n ON p.pronamespace = n.oid
            WHERE n.nspname = $1
            AND p.prokind IN ('f', 'p')
            AND ($2 IS NULL OR p.proname LIKE $3)
            AND ($4 IS NULL OR p.prokind = $4)
        "#;

        let count_row: (i64,) = sqlx::query_as(count_query)
            .bind(schema)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(&type_filter)
            .fetch_one(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let total_count = count_row.0;

        // 2. Get paginated results
        let mut query_str = r#"
            SELECT
                p.proname::text AS name,
                p.prokind::text AS kind,
                pg_get_function_identity_arguments(p.oid)::text AS args,
                pg_get_function_result(p.oid)::text AS return_type,
                l.lanname::text AS language
            FROM pg_proc p
            JOIN pg_namespace n ON p.pronamespace = n.oid
            LEFT JOIN pg_language l ON p.prolang = l.oid
            WHERE n.nspname = $1
            AND p.prokind IN ('f', 'p')
            AND ($2 IS NULL OR p.proname LIKE $3)
            AND ($4 IS NULL OR p.prokind = $4)
            ORDER BY p.proname
        "#.to_string();

        if let Some(limit) = options.page_size {
            query_str.push_str(&format!(" LIMIT {}", limit));
            if let Some(page) = options.page {
                let offset = (page.max(1) - 1) * limit;
                query_str.push_str(&format!(" OFFSET {}", offset));
            }
        }

        let rows: Vec<(String, String, String, Option<String>, Option<String>)> = sqlx::query_as(&query_str)
            .bind(schema)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(&type_filter)
            .fetch_all(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let routines = rows
            .into_iter()
            .map(|(name, kind, args, return_type, language)| {
                let routine_type = match kind.as_str() {
                    "p" => RoutineType::Procedure,
                    _ => RoutineType::Function,
                };
                Routine {
                    namespace: namespace.clone(),
                    name,
                    routine_type,
                    arguments: args,
                    return_type,
                    language,
                }
            })
            .collect();

        Ok(RoutineList {
            routines,
            total_count: total_count as u32,
        })
    }

    fn supports_routines(&self) -> bool {
        true
    }

    async fn create_database(&self, session: SessionId, name: &str, _options: Option<Value>) -> EngineResult<()> {
        let pg_session = self.get_session(session).await?;
        let pool = &pg_session.pool;

        // Basic validation
        if name.is_empty() || name.len() > 63 {
             return Err(EngineError::validation("Schema name must be between 1 and 63 characters"));
        }

        // Identifier quoting with double quotes for Postgres
        let escaped_name = name.replace('"', "\"\"");
        let query = format!("CREATE SCHEMA \"{}\"", escaped_name);

        sqlx::query(&query)
            .execute(pool)
            .await
            .map_err(|e| {
                tracing::error!("Postgres: Failed to create schema: {}", e);
                let msg = e.to_string();
                if msg.contains("permission denied") {
                    EngineError::auth_failed(format!("Permission denied: {}", msg))
                } else if msg.contains("exists") {
                    EngineError::validation(format!("Schema '{}' already exists", name))
                } else {
                    EngineError::execution_error(msg)
                }
            })?;

        Ok(())
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        let pg_session = self.get_session(session).await?;
        let pool = &pg_session.pool;

        // Basic validation
        if name.is_empty() || name.len() > 63 {
            return Err(EngineError::validation("Schema name must be between 1 and 63 characters"));
        }

        // Identifier quoting with double quotes for Postgres
        let escaped_name = name.replace('"', "\"\"");
        let query = format!("DROP SCHEMA \"{}\" CASCADE", escaped_name);

        sqlx::query(&query)
            .execute(pool)
            .await
            .map_err(|e| {
                tracing::error!("Postgres: Failed to drop schema: {}", e);
                let msg = e.to_string();
                if msg.contains("permission denied") {
                    EngineError::auth_failed(format!("Permission denied: {}", msg))
                } else if msg.contains("does not exist") {
                    EngineError::validation(format!("Schema '{}' does not exist", name))
                } else {
                    EngineError::execution_error(msg)
                }
            })?;

        tracing::info!("Postgres: Successfully dropped schema '{}'", name);
        Ok(())
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

        Self::apply_namespace_on_conn(&mut conn, &namespace, query, false).await?;

        // Check if query returns rows (select)
        let returns_rows = sql_safety::returns_rows(self.driver_id(), query)
            .unwrap_or_else(|_| sql_safety::is_select_prefix(query));

        if !returns_rows {
             // Fallback to normal execute and send "Done"
               let result = self.execute_in_namespace(session, namespace, query, query_id).await?;
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
        let mut stream_error: Option<String> = None;
        let mut enum_labels: EnumLabelMap = HashMap::new();

        while let Some(item) = stream.next().await {
            match item {
                Ok(pg_row) => {
                    if !columns_sent {
                        let columns = get_column_info(&pg_row);
                        if sender.send(StreamEvent::Columns(columns.clone())).await.is_err() {
                            break; // Receiver dropped
                        }
                        columns_sent = true;

                        // Collect enum type OIDs and load labels
                        let enum_oids = collect_enum_type_oids(pg_row.columns());
                        if !enum_oids.is_empty() {
                            // We need to load enum labels using the pool since our conn is busy with the stream
                            match load_enum_labels(&pg_session.pool, &enum_oids).await {
                                Ok(labels) => enum_labels = labels,
                                Err(e) => {
                                    tracing::warn!("Failed to load enum labels: {}", e);
                                }
                            }
                        }
                    }

                    let row = convert_row(&pg_row, &enum_labels);
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

        // Cleanup
        {
            let mut active = pg_session.active_queries.lock().await;
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

             Self::apply_namespace_on_conn(conn, &namespace, query, true).await?;

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
                    let columns = get_column_info(&pg_rows[0]);
                    // Load enum labels for this result set
                    let enum_oids = collect_enum_type_oids(pg_rows[0].columns());
                    let enum_labels = if !enum_oids.is_empty() {
                        load_enum_labels(&pg_session.pool, &enum_oids).await.unwrap_or_default()
                    } else {
                        HashMap::new()
                    };
                    let rows: Vec<QRow> = pg_rows.iter().map(|r| convert_row(r, &enum_labels)).collect();

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

             Self::apply_namespace_on_conn(&mut conn, &namespace, query, false).await?;

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
                    let columns = get_column_info(&pg_rows[0]);
                    // Load enum labels for this result set
                    let enum_oids = collect_enum_type_oids(pg_rows[0].columns());
                    let enum_labels = if !enum_oids.is_empty() {
                        load_enum_labels(&pg_session.pool, &enum_oids).await.unwrap_or_default()
                    } else {
                        HashMap::new()
                    };
                    let rows: Vec<QRow> = pg_rows.iter().map(|r| convert_row(r, &enum_labels)).collect();

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
        let fk_rows: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT
                kcu.column_name::text,
                ccu.table_name::text AS foreign_table_name,
                ccu.column_name::text AS foreign_column_name,
                ccu.table_schema::text AS foreign_table_schema,
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
            .map(|(column, referenced_table, referenced_column, referenced_schema, constraint_name)| ForeignKey {
                column,
                referenced_table,
                referenced_column,
                referenced_schema: Some(referenced_schema),
                referenced_database: None,
                constraint_name,
                is_virtual: false,
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

        // Row count: exact for small tables, estimate for large ones.
        // Prefer pg_stat_user_tables.n_live_tup, then pg_class.reltuples (can be stale).
        const SMALL_TABLE_MAX_ROWS: i64 = 100_000;
        const SMALL_TABLE_MAX_BYTES: i64 = 64 * 1024 * 1024;

        let stats_row: Option<(Option<i64>, Option<f64>, i64)> = sqlx::query_as(
            r#"
            SELECT
                s.n_live_tup::bigint AS n_live_tup,
                c.reltuples::double precision AS reltuples,
                pg_total_relation_size(c.oid)::bigint AS total_bytes
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid
            WHERE n.nspname = $1 AND c.relname = $2
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_optional(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let (n_live_tup, reltuples, total_bytes) = stats_row
            .map(|(n_live_tup, reltuples, total_bytes)| (n_live_tup, reltuples, total_bytes))
            .unwrap_or((None, None, 0));

        let estimate_rows: Option<i64> = if let Some(n_live) = n_live_tup {
            Some(n_live)
        } else if let Some(rel) = reltuples {
            if rel >= 0.0 {
                Some(rel.floor() as i64)
            } else {
                None
            }
        } else {
            None
        };

        let small_by_rows = estimate_rows.map(|v| v <= SMALL_TABLE_MAX_ROWS).unwrap_or(false);
        let small_by_bytes = total_bytes <= SMALL_TABLE_MAX_BYTES;
        let should_count_exact = small_by_rows || small_by_bytes;

        let row_count_estimate = if should_count_exact {
            let schema_ident = Self::quote_ident(schema);
            let table_ident = Self::quote_ident(table);
            let count_sql = format!("SELECT COUNT(*)::bigint FROM {}.{}", schema_ident, table_ident);
            let exact_count: i64 = sqlx::query_scalar(&count_sql)
                .fetch_one(pool)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            if exact_count < 0 {
                None
            } else {
                Some(exact_count as u64)
            }
        } else {
            estimate_rows.and_then(|c| if c < 0 { None } else { Some(c as u64) })
        };

        // Get indexes
        let index_rows: Vec<(String, Vec<String>, bool, bool)> = sqlx::query_as(
            r#"
            SELECT i.relname AS index_name,
                   array_agg(a.attname ORDER BY x.ordinality)::text[] AS columns,
                   ix.indisunique AS is_unique,
                   ix.indisprimary AS is_primary
            FROM pg_index ix
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN pg_class t ON t.oid = ix.indrelid
            JOIN pg_namespace n ON n.oid = t.relnamespace
            CROSS JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS x(attnum, ordinality)
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = x.attnum
            WHERE n.nspname = $1 AND t.relname = $2
            GROUP BY i.relname, ix.indisunique, ix.indisprimary
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let indexes: Vec<TableIndex> = index_rows
            .into_iter()
            .map(|(name, columns, is_unique, is_primary)| TableIndex {
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
        let schema = namespace.schema.as_deref().unwrap_or("public");
        // Use quoted identifiers to handle special characters
        let query = format!(
            "SELECT * FROM \"{}\".\"{}\" LIMIT {}",
            schema, table, limit
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
        let pg_session = self.get_session(session).await?;
        let start = Instant::now();

        let schema_name = namespace.schema.as_deref().unwrap_or("public");
        let schema_ident = Self::quote_ident(schema_name);
        let table_ident = Self::quote_ident(table);
        let table_ref = format!("{}.{}", schema_ident, table_ident);

        let page = options.effective_page();
        let page_size = options.effective_page_size();
        let offset = options.offset();

        // Build WHERE clause from filters
        let mut where_clauses: Vec<String> = Vec::new();
        let mut bind_values: Vec<Value> = Vec::new();

        if let Some(filters) = &options.filters {
            for filter in filters {
                let col_ident = Self::quote_ident(&filter.column);
                let param_idx = bind_values.len() + 1;

                let clause = match filter.operator {
                    FilterOperator::Eq => {
                        bind_values.push(filter.value.clone());
                        format!("{} = ${}", col_ident, param_idx)
                    }
                    FilterOperator::Neq => {
                        bind_values.push(filter.value.clone());
                        format!("{} != ${}", col_ident, param_idx)
                    }
                    FilterOperator::Gt => {
                        bind_values.push(filter.value.clone());
                        format!("{} > ${}", col_ident, param_idx)
                    }
                    FilterOperator::Gte => {
                        bind_values.push(filter.value.clone());
                        format!("{} >= ${}", col_ident, param_idx)
                    }
                    FilterOperator::Lt => {
                        bind_values.push(filter.value.clone());
                        format!("{} < ${}", col_ident, param_idx)
                    }
                    FilterOperator::Lte => {
                        bind_values.push(filter.value.clone());
                        format!("{} <= ${}", col_ident, param_idx)
                    }
                    FilterOperator::Like => {
                        bind_values.push(filter.value.clone());
                        format!("{} ILIKE ${}", col_ident, param_idx)
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
                let columns_sql = format!(
                    "SELECT column_name, data_type FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2"
                );
                let columns_rows: Vec<PgRow> = {
                    let mut tx_guard = pg_session.transaction_conn.lock().await;
                    if let Some(ref mut conn) = *tx_guard {
                        sqlx::query(&columns_sql)
                            .bind(schema_name)
                            .bind(table)
                            .fetch_all(&mut **conn)
                            .await
                    } else {
                        sqlx::query(&columns_sql)
                            .bind(schema_name)
                            .bind(table)
                            .fetch_all(&pg_session.pool)
                            .await
                    }
                }
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

                let mut search_clauses: Vec<String> = Vec::new();
                for col_row in &columns_rows {
                    let col_name: String = col_row.try_get("column_name")
                        .map_err(|e| EngineError::execution_error(e.to_string()))?;
                    let data_type: String = col_row.try_get("data_type")
                        .map_err(|e| EngineError::execution_error(e.to_string()))?;

                    // Only search text-like columns
                    let is_text = matches!(data_type.as_str(),
                        "text" | "character varying" | "character" | "varchar" | "char" | "name" | "citext"
                    );

                    if is_text {
                        let col_ident = Self::quote_ident(&col_name);
                        let param_idx = bind_values.len() + 1;
                        bind_values.push(Value::Text(format!("%{}%", search_term)));
                        search_clauses.push(format!("{} ILIKE ${}", col_ident, param_idx));
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
        let count_sql = format!("SELECT COUNT(*)::bigint AS cnt FROM {}{}", table_ref, where_sql);
        let mut count_query = sqlx::query(&count_sql);
        for val in &bind_values {
            count_query = bind_param(count_query, val);
        }

        let count_row: PgRow = {
            let mut tx_guard = pg_session.transaction_conn.lock().await;
            if let Some(ref mut conn) = *tx_guard {
                count_query.fetch_one(&mut **conn).await
            } else {
                count_query.fetch_one(&pg_session.pool).await
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
            data_query = bind_param(data_query, val);
        }

        let pg_rows: Vec<PgRow> = {
            let mut tx_guard = pg_session.transaction_conn.lock().await;
            if let Some(ref mut conn) = *tx_guard {
                data_query.fetch_all(&mut **conn).await
            } else {
                data_query.fetch_all(&pg_session.pool).await
            }
        }
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

        let result = if pg_rows.is_empty() {
            QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: None,
                execution_time_ms,
            }
        } else {
            let columns = get_column_info(&pg_rows[0]);
            // Load enum labels for this result set
            let enum_oids = collect_enum_type_oids(pg_rows[0].columns());
            let enum_labels = if !enum_oids.is_empty() {
                load_enum_labels(&pg_session.pool, &enum_oids).await.unwrap_or_default()
            } else {
                HashMap::new()
            };
            let rows: Vec<QRow> = pg_rows.iter().map(|r| convert_row(r, &enum_labels)).collect();
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
        let pg_session = self.get_session(session).await?;
        let limit = limit.max(1).min(50);
        let schema = foreign_key
            .referenced_schema
            .as_deref()
            .or(namespace.schema.as_deref())
            .unwrap_or("public");

        let table_ref = format!(
            "{}.{}",
            Self::quote_ident(schema),
            Self::quote_ident(&foreign_key.referenced_table)
        );
        let column_ref = Self::quote_ident(&foreign_key.referenced_column);
        let sql = format!("SELECT * FROM {} WHERE {} = $1 LIMIT {}", table_ref, column_ref, limit);

        let mut query = sqlx::query(&sql);
        query = bind_param(query, value);

        let start = Instant::now();
        let mut tx_guard = pg_session.transaction_conn.lock().await;
        let pg_rows: Vec<PgRow> = if let Some(ref mut conn) = *tx_guard {
            query.fetch_all(&mut **conn).await
        } else {
            query.fetch_all(&pg_session.pool).await
        }
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

        if pg_rows.is_empty() {
            return Ok(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: None,
                execution_time_ms,
            });
        }

        let columns = get_column_info(&pg_rows[0]);
        // Load enum labels for this result set
        let enum_oids = collect_enum_type_oids(pg_rows[0].columns());
        let enum_labels = if !enum_oids.is_empty() {
            load_enum_labels(&pg_session.pool, &enum_oids).await.unwrap_or_default()
        } else {
            HashMap::new()
        };
        let rows: Vec<QRow> = pg_rows.iter().map(|r| convert_row(r, &enum_labels)).collect();

        Ok(QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms,
        })
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
            query = bind_param(query, val);
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
            query = bind_param(query, val);
        }
        
        // Bind PK values
        for k in &pk_keys {
            let val = primary_key.columns.get(*k).unwrap();
            query = bind_param(query, val);
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
            query = bind_param(query, val);
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

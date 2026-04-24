// SPDX-License-Identifier: Apache-2.0

//! Shared PostgreSQL-compatible driver logic
//!
//! This module provides reusable building blocks for any database that speaks
//! the PostgreSQL wire protocol (PostgreSQL, CockroachDB, etc.).
//! It is intentionally NOT a full DataEngine implementation — each concrete
//! driver still implements the trait and calls into these helpers, choosing
//! which queries and behaviours to override.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow, Postgres};
use sqlx::Row;
use tokio::sync::{Mutex, RwLock};

use crate::drivers::postgres_utils::{
    bind_param, build_decoders, collect_enum_type_oids, columns_and_rows,
    convert_row_with_decoders, get_column_info, load_enum_labels, PgDecoder,
    EnumLabelMap,
};
use qore_core::error::{EngineError, EngineResult};
use qore_sql::safety;
use qore_core::traits::{StreamEvent, StreamSender};
use qore_core::types::{
    CancelSupport, ColumnInfo, ConnectionConfig, FilterOperator, ForeignKey, Namespace,
    PaginatedQueryResult, QueryId, QueryResult, Routine, RoutineDefinition, RoutineList,
    RoutineListOptions, RoutineOperationResult, RoutineType, RowData, SessionId,
    SortDirection, TableColumn, TableIndex, TableQueryOptions, TableSchema, Trigger,
    TriggerDefinition, TriggerEvent, TriggerList, TriggerListOptions, TriggerOperationResult,
    TriggerTiming, Value,
};

// =============================================================================
// Session
// =============================================================================

/// A session backed by a PgPool (works for any PG-compatible database).
pub struct PgCompatSession {
    pub pool: PgPool,
    pub transaction_conn: Mutex<Option<PoolConnection<Postgres>>>,
    pub active_queries: Mutex<HashMap<QueryId, i32>>,
}

impl PgCompatSession {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            transaction_conn: Mutex::new(None),
            active_queries: Mutex::new(HashMap::new()),
        }
    }
}

/// Convenience alias — drivers keep a map of these.
pub type SessionMap = Arc<RwLock<HashMap<SessionId, Arc<PgCompatSession>>>>;

pub fn new_session_map() -> SessionMap {
    Arc::new(RwLock::new(HashMap::new()))
}

// =============================================================================
// Pool & connection helpers
// =============================================================================

pub async fn create_pg_pool(
    conn_str: &str,
    max_connections: u32,
    min_connections: u32,
    acquire_timeout_secs: u64,
    classify_auth_error: bool,
    run_test_query: bool,
) -> EngineResult<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .min_connections(min_connections)
        .acquire_timeout(std::time::Duration::from_secs(acquire_timeout_secs))
        .connect(conn_str)
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

pub async fn get_session(
    sessions: &SessionMap,
    session: SessionId,
) -> EngineResult<Arc<PgCompatSession>> {
    let map = sessions.read().await;
    map.get(&session)
        .cloned()
        .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))
}

pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub async fn apply_namespace_on_conn(
    conn: &mut PoolConnection<Postgres>,
    namespace: &Option<Namespace>,
    query: &str,
    in_transaction: bool,
) -> EngineResult<()> {
    let lower = query.trim_start().to_ascii_lowercase();
    if lower.starts_with("set ") && lower.contains("search_path") {
        return Ok(());
    }

    let schema = namespace
        .as_ref()
        .and_then(|ns| ns.schema.as_ref())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    if let Some(schema) = schema {
        let schema_sql = quote_ident(schema);
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

pub async fn fetch_backend_pid(conn: &mut PoolConnection<Postgres>) -> EngineResult<i32> {
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut **conn)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))
}

// =============================================================================
// Test / Connect / Disconnect
// =============================================================================

pub async fn test_connection(conn_str: &str) -> EngineResult<()> {
    let pool = create_pg_pool(conn_str, 1, 0, 10, true, true).await?;
    pool.close().await;
    Ok(())
}

pub async fn connect(
    sessions: &SessionMap,
    config: &ConnectionConfig,
    conn_str: &str,
) -> EngineResult<SessionId> {
    let max = config.pool_max_connections.unwrap_or(5);
    let min = config.pool_min_connections.unwrap_or(0);
    let timeout = config.pool_acquire_timeout_secs.unwrap_or(30) as u64;

    let pool = create_pg_pool(conn_str, max, min, timeout, false, false).await?;

    let session_id = SessionId::new();
    let session = Arc::new(PgCompatSession::new(pool));

    let mut map = sessions.write().await;
    map.insert(session_id, session);

    Ok(session_id)
}

pub async fn disconnect(sessions: &SessionMap, session: SessionId) -> EngineResult<()> {
    let session = {
        let mut map = sessions.write().await;
        map.remove(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?
    };

    {
        let mut tx = session.transaction_conn.lock().await;
        tx.take();
    }

    session.pool.close().await;
    Ok(())
}

pub async fn ping(sessions: &SessionMap, session: SessionId) -> EngineResult<()> {
    let pg = get_session(sessions, session).await?;
    sqlx::query("SELECT 1")
        .execute(&pg.pool)
        .await
        .map_err(|e| EngineError::connection_failed(format!("Ping failed: {e}")))?;
    Ok(())
}

// =============================================================================
// Execute
// =============================================================================

pub async fn execute_in_namespace(
    sessions: &SessionMap,
    driver_id: &str,
    session: SessionId,
    namespace: Option<Namespace>,
    query: &str,
    query_id: QueryId,
) -> EngineResult<QueryResult> {
    let pg = get_session(sessions, session).await?;
    let start = Instant::now();

    let returns_rows = safety::returns_rows(driver_id, query)
        .unwrap_or_else(|_| safety::is_select_prefix(query));

    let mut tx_guard = pg.transaction_conn.lock().await;

    let result = if let Some(ref mut conn) = *tx_guard {
        let backend_pid = fetch_backend_pid(conn).await?;
        {
            let mut active = pg.active_queries.lock().await;
            active.insert(query_id, backend_pid);
        }

        apply_namespace_on_conn(conn, &namespace, query, true).await?;

        let result = if returns_rows {
            exec_rows_on_conn(conn, &pg.pool, query, start).await?
        } else {
            let r = sqlx::query(query)
                .execute(&mut **conn)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            QueryResult::with_affected_rows(
                r.rows_affected(),
                start.elapsed().as_micros() as f64 / 1000.0,
            )
        };

        {
            let mut active = pg.active_queries.lock().await;
            active.remove(&query_id);
        }
        result
    } else {
        drop(tx_guard);

        let mut conn = pg
            .pool
            .acquire()
            .await
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        let backend_pid = fetch_backend_pid(&mut conn).await?;
        {
            let mut active = pg.active_queries.lock().await;
            active.insert(query_id, backend_pid);
        }

        apply_namespace_on_conn(&mut conn, &namespace, query, false).await?;

        let result = if returns_rows {
            exec_rows_on_poolconn(&mut conn, &pg.pool, query, start).await?
        } else {
            let r = sqlx::query(query)
                .execute(&mut *conn)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            QueryResult::with_affected_rows(
                r.rows_affected(),
                start.elapsed().as_micros() as f64 / 1000.0,
            )
        };

        {
            let mut active = pg.active_queries.lock().await;
            active.remove(&query_id);
        }
        result
    };

    Ok(result)
}

/// Execute a SELECT on a transaction-owned connection
async fn exec_rows_on_conn(
    conn: &mut PoolConnection<Postgres>,
    pool: &PgPool,
    query: &str,
    start: Instant,
) -> EngineResult<QueryResult> {
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

    rows_to_result(pg_rows, pool, start).await
}

/// Execute a SELECT on a pool-acquired connection
async fn exec_rows_on_poolconn(
    conn: &mut PoolConnection<Postgres>,
    pool: &PgPool,
    query: &str,
    start: Instant,
) -> EngineResult<QueryResult> {
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

    rows_to_result(pg_rows, pool, start).await
}

async fn rows_to_result(
    pg_rows: Vec<PgRow>,
    pool: &PgPool,
    start: Instant,
) -> EngineResult<QueryResult> {
    let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;
    if pg_rows.is_empty() {
        return Ok(QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            affected_rows: None,
            execution_time_ms,
        });
    }
    let enum_oids = collect_enum_type_oids(pg_rows[0].columns());
    let enum_labels = if !enum_oids.is_empty() {
        load_enum_labels(pool, &enum_oids).await.unwrap_or_default()
    } else {
        HashMap::new()
    };
    let (columns, rows) = columns_and_rows(&pg_rows, &enum_labels);
    Ok(QueryResult {
        columns,
        rows,
        affected_rows: None,
        execution_time_ms,
    })
}

// =============================================================================
// Streaming
// =============================================================================

pub async fn execute_stream_in_namespace(
    sessions: &SessionMap,
    driver_id: &str,
    session: SessionId,
    namespace: Option<Namespace>,
    query: &str,
    query_id: QueryId,
    sender: StreamSender,
) -> EngineResult<()> {
    let pg = get_session(sessions, session).await?;

    let mut conn = pg
        .pool
        .acquire()
        .await
        .map_err(|e| EngineError::connection_failed(e.to_string()))?;

    apply_namespace_on_conn(&mut conn, &namespace, query, false).await?;

    let returns_rows = safety::returns_rows(driver_id, query)
        .unwrap_or_else(|_| safety::is_select_prefix(query));

    if !returns_rows {
        let result =
            execute_in_namespace(sessions, driver_id, session, namespace, query, query_id).await?;
        let _ = sender
            .send(StreamEvent::Done(result.affected_rows.unwrap_or(0)))
            .await;
        return Ok(());
    }

    let backend_pid = fetch_backend_pid(&mut conn).await?;
    {
        let mut active = pg.active_queries.lock().await;
        active.insert(query_id, backend_pid);
    }

    let mut stream = sqlx::query(query).fetch(&mut *conn);
    let mut columns_sent = false;
    let mut row_count = 0;
    let mut stream_error: Option<String> = None;
    let mut enum_labels: EnumLabelMap = HashMap::new();
    let mut decoders: Vec<PgDecoder> = Vec::new();
    let mut batch = Vec::with_capacity(500);

    while let Some(item) = stream.next().await {
        match item {
            Ok(pg_row) => {
                if !columns_sent {
                    let columns = get_column_info(&pg_row);
                    decoders = build_decoders(pg_row.columns());
                    if sender
                        .send(StreamEvent::Columns(columns.clone()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    columns_sent = true;

                    let enum_oids = collect_enum_type_oids(pg_row.columns());
                    if !enum_oids.is_empty() {
                        match load_enum_labels(&pg.pool, &enum_oids).await {
                            Ok(labels) => enum_labels = labels,
                            Err(e) => {
                                tracing::warn!("Failed to load enum labels: {}", e);
                            }
                        }
                    }
                }

                let row = convert_row_with_decoders(&pg_row, &decoders, &enum_labels);
                batch.push(row);
                row_count += 1;
                
                if batch.len() >= 500 {
                    if sender.send(StreamEvent::RowBatch(std::mem::take(&mut batch))).await.is_err() {
                        break;
                    }
                }
            }
            Err(e) => {
                let error_msg = e.to_string();
                let _ = sender.send(StreamEvent::Error(error_msg.clone())).await;
                stream_error = Some(error_msg);
                break;
            }
        }
    }
    
    if !batch.is_empty() {
        let _ = sender.send(StreamEvent::RowBatch(batch)).await;
    }

    {
        let mut active = pg.active_queries.lock().await;
        active.remove(&query_id);
    }

    if stream_error.is_none() {
        let _ = sender.send(StreamEvent::Done(row_count)).await;
    }

    if let Some(err) = stream_error {
        return Err(EngineError::execution_error(err));
    }

    Ok(())
}

// =============================================================================
// Cancel
// =============================================================================

pub async fn cancel(
    sessions: &SessionMap,
    session: SessionId,
    query_id: Option<QueryId>,
) -> EngineResult<()> {
    let pg = get_session(sessions, session).await?;

    let backend_pids: Vec<i32> = {
        let active = pg.active_queries.lock().await;
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

    let mut conn = pg
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

pub fn cancel_support() -> CancelSupport {
    CancelSupport::Driver
}

// =============================================================================
// Transactions
// =============================================================================

pub async fn begin_transaction(sessions: &SessionMap, session: SessionId) -> EngineResult<()> {
    let pg = get_session(sessions, session).await?;
    let mut tx = pg.transaction_conn.lock().await;

    if tx.is_some() {
        return Err(EngineError::transaction_error(
            "A transaction is already active on this session",
        ));
    }

    let mut conn = pg.pool.acquire().await.map_err(|e| {
        EngineError::connection_failed(format!(
            "Failed to acquire connection for transaction: {}",
            e
        ))
    })?;

    sqlx::query("BEGIN")
        .execute(&mut *conn)
        .await
        .map_err(|e| EngineError::execution_error(format!("Failed to begin transaction: {}", e)))?;

    *tx = Some(conn);
    Ok(())
}

pub async fn commit(sessions: &SessionMap, session: SessionId) -> EngineResult<()> {
    let pg = get_session(sessions, session).await?;
    let mut tx = pg.transaction_conn.lock().await;

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

pub async fn rollback(sessions: &SessionMap, session: SessionId) -> EngineResult<()> {
    let pg = get_session(sessions, session).await?;
    let mut tx = pg.transaction_conn.lock().await;

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

// =============================================================================
// Mutations
// =============================================================================

pub async fn insert_row(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    data: &RowData,
) -> EngineResult<QueryResult> {
    let pg = get_session(sessions, session).await?;

    let table_name = qualified_table_name(namespace, table);

    let mut keys: Vec<&String> = data.columns.keys().collect();
    keys.sort();

    let sql = if keys.is_empty() {
        format!("INSERT INTO {} DEFAULT VALUES", table_name)
    } else {
        let cols_str = keys
            .iter()
            .map(|k| quote_ident(k))
            .collect::<Vec<_>>()
            .join(", ");
        let params_str = (1..=keys.len())
            .map(|i| format!("${}", i))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table_name, cols_str, params_str
        )
    };

    let mut query = sqlx::query(&sql);
    for k in &keys {
        let val = data.columns.get(*k).unwrap();
        query = bind_param(query, val);
    }

    let start = Instant::now();
    let mut tx_guard = pg.transaction_conn.lock().await;
    let result = if let Some(ref mut conn) = *tx_guard {
        query.execute(&mut **conn).await
    } else {
        query.execute(&pg.pool).await
    };

    let result = result.map_err(|e| EngineError::execution_error(e.to_string()))?;
    Ok(QueryResult::with_affected_rows(
        result.rows_affected(),
        start.elapsed().as_micros() as f64 / 1000.0,
    ))
}

pub async fn update_row(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    primary_key: &RowData,
    data: &RowData,
) -> EngineResult<QueryResult> {
    let pg = get_session(sessions, session).await?;

    if primary_key.columns.is_empty() {
        return Err(EngineError::execution_error(
            "Primary key required for update operations".to_string(),
        ));
    }
    if data.columns.is_empty() {
        return Ok(QueryResult::with_affected_rows(0, 0.0));
    }

    let table_name = qualified_table_name(namespace, table);

    let mut data_keys: Vec<&String> = data.columns.keys().collect();
    data_keys.sort();
    let mut pk_keys: Vec<&String> = primary_key.columns.keys().collect();
    pk_keys.sort();

    let mut set_clauses = Vec::new();
    let mut i = 1;
    for k in &data_keys {
        set_clauses.push(format!("{}=${}", quote_ident(k), i));
        i += 1;
    }
    let mut where_clauses = Vec::new();
    for k in &pk_keys {
        where_clauses.push(format!("{}=${}", quote_ident(k), i));
        i += 1;
    }

    let sql = format!(
        "UPDATE {} SET {} WHERE {}",
        table_name,
        set_clauses.join(", "),
        where_clauses.join(" AND ")
    );

    let mut query = sqlx::query(&sql);
    for k in &data_keys {
        query = bind_param(query, data.columns.get(*k).unwrap());
    }
    for k in &pk_keys {
        query = bind_param(query, primary_key.columns.get(*k).unwrap());
    }

    let start = Instant::now();
    let mut tx_guard = pg.transaction_conn.lock().await;
    let result = if let Some(ref mut conn) = *tx_guard {
        query.execute(&mut **conn).await
    } else {
        query.execute(&pg.pool).await
    };

    let result = result.map_err(|e| EngineError::execution_error(e.to_string()))?;
    Ok(QueryResult::with_affected_rows(
        result.rows_affected(),
        start.elapsed().as_micros() as f64 / 1000.0,
    ))
}

pub async fn delete_row(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    primary_key: &RowData,
) -> EngineResult<QueryResult> {
    let pg = get_session(sessions, session).await?;

    if primary_key.columns.is_empty() {
        return Err(EngineError::execution_error(
            "Primary key required for delete operations".to_string(),
        ));
    }

    let table_name = qualified_table_name(namespace, table);

    let mut pk_keys: Vec<&String> = primary_key.columns.keys().collect();
    pk_keys.sort();

    let mut where_clauses = Vec::new();
    let mut i = 1;
    for k in &pk_keys {
        where_clauses.push(format!("{}=${}", quote_ident(k), i));
        i += 1;
    }

    let sql = format!(
        "DELETE FROM {} WHERE {}",
        table_name,
        where_clauses.join(" AND ")
    );

    let mut query = sqlx::query(&sql);
    for k in &pk_keys {
        query = bind_param(query, primary_key.columns.get(*k).unwrap());
    }

    let start = Instant::now();
    let mut tx_guard = pg.transaction_conn.lock().await;
    let result = if let Some(ref mut conn) = *tx_guard {
        query.execute(&mut **conn).await
    } else {
        query.execute(&pg.pool).await
    };

    let result = result.map_err(|e| EngineError::execution_error(e.to_string()))?;
    Ok(QueryResult::with_affected_rows(
        result.rows_affected(),
        start.elapsed().as_micros() as f64 / 1000.0,
    ))
}

// =============================================================================
// Peek FK
// =============================================================================

pub async fn peek_foreign_key(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    foreign_key: &ForeignKey,
    value: &Value,
    limit: u32,
) -> EngineResult<QueryResult> {
    let pg = get_session(sessions, session).await?;
    let limit = limit.max(1).min(50);
    let schema = foreign_key
        .referenced_schema
        .as_deref()
        .or(namespace.schema.as_deref())
        .unwrap_or("public");

    let table_ref = format!(
        "{}.{}",
        quote_ident(schema),
        quote_ident(&foreign_key.referenced_table)
    );
    let column_ref = quote_ident(&foreign_key.referenced_column);
    let sql = format!(
        "SELECT * FROM {} WHERE {} = $1 LIMIT {}",
        table_ref, column_ref, limit
    );

    let mut query = sqlx::query(&sql);
    query = bind_param(query, value);

    let start = Instant::now();
    let mut tx_guard = pg.transaction_conn.lock().await;
    let pg_rows: Vec<PgRow> = if let Some(ref mut conn) = *tx_guard {
        query.fetch_all(&mut **conn).await
    } else {
        query.fetch_all(&pg.pool).await
    }
    .map_err(|e| EngineError::execution_error(e.to_string()))?;

    rows_to_result(pg_rows, &pg.pool, start).await
}

// =============================================================================
// Query Table (paginated)
// =============================================================================

pub async fn query_table(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    options: TableQueryOptions,
) -> EngineResult<PaginatedQueryResult> {
    let pg = get_session(sessions, session).await?;
    let start = Instant::now();

    let schema_name = namespace.schema.as_deref().unwrap_or("public");
    let schema_ident = quote_ident(schema_name);
    let table_ident = quote_ident(table);
    let table_ref = format!("{}.{}", schema_ident, table_ident);

    let page = options.effective_page();
    let page_size = options.effective_page_size();
    let offset = options.offset();

    // Build WHERE clause from filters
    let mut where_clauses: Vec<String> = Vec::new();
    let mut bind_values: Vec<Value> = Vec::new();

    if let Some(filters) = &options.filters {
        for filter in filters {
            let col_ident = quote_ident(&filter.column);
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
                FilterOperator::Regex => {
                    bind_values.push(filter.value.clone());
                    // Postgres: `~` is case-sensitive, `~*` is case-insensitive.
                    // Other regex flags are not natively exposed — they are
                    // embedded in the pattern itself (e.g. `(?i)` / `(?s)`).
                    let op = if filter
                        .options
                        .regex_flags
                        .as_deref()
                        .map(|f| f.contains('i'))
                        .unwrap_or(false)
                    {
                        "~*"
                    } else {
                        "~"
                    };
                    format!("{} {} ${}", col_ident, op, param_idx)
                }
                FilterOperator::Text => {
                    bind_values.push(filter.value.clone());
                    let lang = filter
                        .options
                        .text_language
                        .as_deref()
                        .unwrap_or("english");
                    // Cast the column to text to cover non-text types gracefully.
                    format!(
                        "to_tsvector('{}', {}::text) @@ plainto_tsquery('{}', ${})",
                        lang.replace('\'', "''"),
                        col_ident,
                        lang.replace('\'', "''"),
                        param_idx
                    )
                }
            };
            where_clauses.push(clause);
        }
    }

    // Handle search across all columns
    if let Some(ref search_term) = options.search {
        if !search_term.trim().is_empty() {
            let columns_sql = "SELECT column_name, data_type FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2";
            let columns_rows: Vec<PgRow> = {
                let mut tx_guard = pg.transaction_conn.lock().await;
                if let Some(ref mut conn) = *tx_guard {
                    sqlx::query(columns_sql)
                        .bind(schema_name)
                        .bind(table)
                        .fetch_all(&mut **conn)
                        .await
                } else {
                    sqlx::query(columns_sql)
                        .bind(schema_name)
                        .bind(table)
                        .fetch_all(&pg.pool)
                        .await
                }
            }
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut search_clauses: Vec<String> = Vec::new();
            for col_row in &columns_rows {
                let col_name: String = col_row
                    .try_get("column_name")
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;
                let data_type: String = col_row
                    .try_get("data_type")
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;

                let is_unsearchable =
                    matches!(data_type.as_str(), "bytea" | "tsvector" | "tsquery");
                if is_unsearchable {
                    continue;
                }

                let col_ident = quote_ident(&col_name);
                let param_idx = bind_values.len() + 1;
                bind_values.push(Value::Text(format!("%{}%", search_term)));

                let is_text = matches!(
                    data_type.as_str(),
                    "text"
                        | "character varying"
                        | "character"
                        | "varchar"
                        | "char"
                        | "name"
                        | "citext"
                );
                if is_text {
                    search_clauses.push(format!("{} ILIKE ${}", col_ident, param_idx));
                } else {
                    search_clauses.push(format!("{}::text ILIKE ${}", col_ident, param_idx));
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

    let order_sql = if let Some(sort_col) = &options.sort_column {
        let sort_ident = quote_ident(sort_col);
        let direction = match options.sort_direction.unwrap_or_default() {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        };
        format!(" ORDER BY {} {}", sort_ident, direction)
    } else {
        String::new()
    };

    // COUNT
    let count_sql = format!(
        "SELECT COUNT(*)::bigint AS cnt FROM {}{}",
        table_ref, where_sql
    );
    let mut count_query = sqlx::query(&count_sql);
    for val in &bind_values {
        count_query = bind_param(count_query, val);
    }

    let count_row: PgRow = {
        let mut tx_guard = pg.transaction_conn.lock().await;
        if let Some(ref mut conn) = *tx_guard {
            count_query.fetch_one(&mut **conn).await
        } else {
            count_query.fetch_one(&pg.pool).await
        }
    }
    .map_err(|e| EngineError::execution_error(e.to_string()))?;

    let total_rows: i64 = count_row
        .try_get("cnt")
        .map_err(|e| EngineError::execution_error(e.to_string()))?;
    let total_rows = total_rows.max(0) as u64;

    // DATA
    let data_sql = format!(
        "SELECT * FROM {}{}{} LIMIT {} OFFSET {}",
        table_ref, where_sql, order_sql, page_size, offset
    );

    let mut data_query = sqlx::query(&data_sql);
    for val in &bind_values {
        data_query = bind_param(data_query, val);
    }

    let pg_rows: Vec<PgRow> = {
        let mut tx_guard = pg.transaction_conn.lock().await;
        if let Some(ref mut conn) = *tx_guard {
            data_query.fetch_all(&mut **conn).await
        } else {
            data_query.fetch_all(&pg.pool).await
        }
    }
    .map_err(|e| EngineError::execution_error(e.to_string()))?;

    let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

    let result = if pg_rows.is_empty() {
        let col_meta_sql = "SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2 ORDER BY ordinal_position";
        let col_meta_rows: Vec<PgRow> = {
            let mut tx_guard = pg.transaction_conn.lock().await;
            if let Some(ref mut conn) = *tx_guard {
                sqlx::query(col_meta_sql)
                    .bind(schema_name)
                    .bind(table)
                    .fetch_all(&mut **conn)
                    .await
            } else {
                sqlx::query(col_meta_sql)
                    .bind(schema_name)
                    .bind(table)
                    .fetch_all(&pg.pool)
                    .await
            }
        }
        .unwrap_or_default();

        let columns: Vec<ColumnInfo> = col_meta_rows
            .iter()
            .filter_map(|r| {
                let name: String = r.try_get("column_name").ok()?;
                let data_type: String = r.try_get("data_type").ok()?;
                let is_nullable: String = r.try_get("is_nullable").ok()?;
                Some(ColumnInfo {
                    name,
                    data_type,
                    nullable: is_nullable == "YES",
                })
            })
            .collect();

        QueryResult {
            columns,
            rows: Vec::new(),
            affected_rows: None,
            execution_time_ms,
        }
    } else {
        let enum_oids = collect_enum_type_oids(pg_rows[0].columns());
        let enum_labels = if !enum_oids.is_empty() {
            load_enum_labels(&pg.pool, &enum_oids)
                .await
                .unwrap_or_default()
        } else {
            HashMap::new()
        };
        let (columns, rows) = columns_and_rows(&pg_rows, &enum_labels);
        QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms,
        }
    };

    Ok(PaginatedQueryResult::new(
        result, total_rows, page, page_size,
    ))
}

// =============================================================================
// Describe Table
// =============================================================================

pub async fn describe_table_core(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    use_pg_stat: bool,
) -> EngineResult<TableSchema> {
    let pg = get_session(sessions, session).await?;
    let pool = &pg.pool;
    let schema = namespace.schema.as_deref().unwrap_or("public");

    // Columns
    let column_rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT column_name::text, data_type::text, is_nullable::text, column_default::text
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

    // Primary keys
    let pk_rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT a.attname::text
        FROM pg_index i
        JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
        JOIN pg_class c ON c.oid = i.indrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE i.indisprimary AND n.nspname = $1 AND c.relname = $2
        ORDER BY array_position(i.indkey, a.attnum)
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| EngineError::execution_error(e.to_string()))?;

    let pk_columns: Vec<String> = pk_rows.into_iter().map(|(n,)| n).collect();

    // Foreign keys
    let fk_rows: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT
            kcu.column_name::text,
            ccu.table_name::text AS foreign_table_name,
            ccu.column_name::text AS foreign_column_name,
            ccu.table_schema::text AS foreign_table_schema,
            tc.constraint_name::text
        FROM information_schema.table_constraints AS tc
        JOIN information_schema.key_column_usage AS kcu
            ON tc.constraint_name = kcu.constraint_name AND tc.table_schema = kcu.table_schema
        JOIN information_schema.constraint_column_usage AS ccu
            ON ccu.constraint_name = tc.constraint_name AND ccu.table_schema = tc.table_schema
        WHERE tc.constraint_type = 'FOREIGN KEY'
            AND tc.table_schema = $1 AND tc.table_name = $2
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| EngineError::execution_error(e.to_string()))?;

    let foreign_keys: Vec<ForeignKey> = fk_rows
        .into_iter()
        .map(
            |(col, ref_table, ref_col, ref_schema, constraint_name)| ForeignKey {
                column: col,
                referenced_table: ref_table,
                referenced_column: ref_col,
                referenced_schema: Some(ref_schema),
                referenced_database: None,
                constraint_name,
                is_virtual: false,
            },
        )
        .collect();

    let columns: Vec<TableColumn> = column_rows
        .into_iter()
        .map(
            |(name, data_type, is_nullable, default_value)| TableColumn {
                is_primary_key: pk_columns.contains(&name),
                name,
                data_type,
                nullable: is_nullable == "YES",
                default_value,
            },
        )
        .collect();

    // Row count estimation
    const SMALL_TABLE_MAX_ROWS: i64 = 100_000;
    const SMALL_TABLE_MAX_BYTES: i64 = 64 * 1024 * 1024;

    let (estimate_rows, total_bytes) = if use_pg_stat {
        // PostgreSQL: use pg_stat_user_tables + pg_class
        let stats: Option<(Option<i64>, Option<f64>, i64)> = sqlx::query_as(
            r#"
            SELECT s.n_live_tup::bigint, c.reltuples::double precision,
                   pg_total_relation_size(c.oid)::bigint
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

        let (n_live_tup, reltuples, total_bytes) = stats.unwrap_or((None, None, 0));
        let est = n_live_tup.or_else(|| {
            reltuples.and_then(|r| {
                if r >= 0.0 {
                    Some(r.floor() as i64)
                } else {
                    None
                }
            })
        });
        (est, total_bytes)
    } else {
        // CockroachDB: pg_stat_user_tables may not be populated reliably
        let stats: Option<(Option<f64>, i64)> = sqlx::query_as(
            r#"
            SELECT c.reltuples::double precision,
                   pg_total_relation_size(c.oid)::bigint
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

        let (reltuples, total_bytes) = stats.unwrap_or((None, 0));
        let est = reltuples.and_then(|r| {
            if r >= 0.0 {
                Some(r.floor() as i64)
            } else {
                None
            }
        });
        (est, total_bytes)
    };

    let small_by_rows = estimate_rows
        .map(|v| v <= SMALL_TABLE_MAX_ROWS)
        .unwrap_or(false);
    let small_by_bytes = total_bytes <= SMALL_TABLE_MAX_BYTES;
    let should_count_exact = small_by_rows || small_by_bytes;

    let row_count_estimate = if should_count_exact {
        let count_sql = format!(
            "SELECT COUNT(*)::bigint FROM {}.{}",
            quote_ident(schema),
            quote_ident(table)
        );
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

    // Indexes
    let index_rows: Vec<(String, Vec<String>, bool, bool, Option<String>)> = sqlx::query_as(
        r#"
        SELECT i.relname AS index_name,
               array_agg(a.attname ORDER BY x.ordinality)::text[] AS columns,
               ix.indisunique AS is_unique,
               ix.indisprimary AS is_primary,
               am.amname AS index_type
        FROM pg_index ix
        JOIN pg_class i ON i.oid = ix.indexrelid
        JOIN pg_class t ON t.oid = ix.indrelid
        JOIN pg_namespace n ON n.oid = t.relnamespace
        JOIN pg_am am ON am.oid = i.relam
        CROSS JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS x(attnum, ordinality)
        JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = x.attnum
        WHERE n.nspname = $1 AND t.relname = $2
        GROUP BY i.relname, ix.indisunique, ix.indisprimary, am.amname
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| EngineError::execution_error(e.to_string()))?;

    let indexes: Vec<TableIndex> = index_rows
        .into_iter()
        .map(|(name, columns, is_unique, is_primary, index_type)| TableIndex {
            name,
            columns,
            is_unique,
            is_primary,
            index_type,
        })
        .collect();

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

// =============================================================================
// Routines
// =============================================================================

pub async fn list_routines(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    options: RoutineListOptions,
) -> EngineResult<RoutineList> {
    let pg = get_session(sessions, session).await?;
    let pool = &pg.pool;
    let schema = namespace.schema.as_deref().unwrap_or("public");
    let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

    let type_filter = match &options.routine_type {
        Some(RoutineType::Function) => Some("f"),
        Some(RoutineType::Procedure) => Some("p"),
        None => None,
    };

    let count_row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM pg_proc p JOIN pg_namespace n ON p.pronamespace = n.oid
        WHERE n.nspname = $1 AND p.prokind IN ('f', 'p')
        AND ($2 IS NULL OR p.proname LIKE $3)
        AND ($4 IS NULL OR p.prokind = $4)
        "#,
    )
    .bind(schema)
    .bind(&search_pattern)
    .bind(&search_pattern)
    .bind(&type_filter)
    .fetch_one(pool)
    .await
    .map_err(|e| EngineError::execution_error(e.to_string()))?;

    let mut query_str = r#"
        SELECT p.proname::text, p.prokind::text,
               pg_get_function_identity_arguments(p.oid)::text,
               pg_get_function_result(p.oid)::text,
               l.lanname::text
        FROM pg_proc p JOIN pg_namespace n ON p.pronamespace = n.oid
        LEFT JOIN pg_language l ON p.prolang = l.oid
        WHERE n.nspname = $1 AND p.prokind IN ('f', 'p')
        AND ($2 IS NULL OR p.proname LIKE $3)
        AND ($4 IS NULL OR p.prokind = $4)
        ORDER BY p.proname
    "#
    .to_string();

    if let Some(limit) = options.page_size {
        query_str.push_str(&format!(" LIMIT {}", limit));
        if let Some(page) = options.page {
            query_str.push_str(&format!(" OFFSET {}", (page.max(1) - 1) * limit));
        }
    }

    let rows: Vec<(String, String, String, Option<String>, Option<String>)> =
        sqlx::query_as(&query_str)
            .bind(schema)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(&type_filter)
            .fetch_all(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

    let routines = rows
        .into_iter()
        .map(|(name, kind, args, return_type, language)| Routine {
            namespace: namespace.clone(),
            name,
            routine_type: if kind == "p" {
                RoutineType::Procedure
            } else {
                RoutineType::Function
            },
            arguments: args,
            return_type,
            language,
        })
        .collect();

    Ok(RoutineList {
        routines,
        total_count: count_row.0 as u32,
    })
}

pub async fn get_routine_definition(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    routine_name: &str,
    routine_type: RoutineType,
    arguments: Option<&str>,
) -> EngineResult<RoutineDefinition> {
    let pg = get_session(sessions, session).await?;
    let pool = &pg.pool;
    let schema = namespace.schema.as_deref().unwrap_or("public");

    let kind_filter = match routine_type {
        RoutineType::Function => "f",
        RoutineType::Procedure => "p",
    };

    let query = if arguments.is_some() {
        r#"
        SELECT p.proname::text, pg_get_functiondef(p.oid)::text, l.lanname::text,
               pg_get_function_identity_arguments(p.oid)::text, pg_get_function_result(p.oid)::text
        FROM pg_proc p JOIN pg_namespace n ON p.pronamespace = n.oid
        LEFT JOIN pg_language l ON p.prolang = l.oid
        WHERE n.nspname = $1 AND p.proname = $2 AND p.prokind = $3
        AND pg_get_function_identity_arguments(p.oid) = $4
        LIMIT 1
        "#
    } else {
        r#"
        SELECT p.proname::text, pg_get_functiondef(p.oid)::text, l.lanname::text,
               pg_get_function_identity_arguments(p.oid)::text, pg_get_function_result(p.oid)::text
        FROM pg_proc p JOIN pg_namespace n ON p.pronamespace = n.oid
        LEFT JOIN pg_language l ON p.prolang = l.oid
        WHERE n.nspname = $1 AND p.proname = $2 AND p.prokind = $3
        AND ($4::text IS NULL)
        LIMIT 1
        "#
    };

    let args_bind = arguments.unwrap_or("");

    let row: (
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
    ) = sqlx::query_as(query)
        .bind(schema)
        .bind(routine_name)
        .bind(kind_filter)
        .bind(args_bind)
        .fetch_optional(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?
        .ok_or_else(|| {
            EngineError::execution_error(format!(
                "Routine '{}' not found in schema '{}'",
                routine_name, schema
            ))
        })?;

    let (name, def, lang, args, ret) = row;
    Ok(RoutineDefinition {
        name,
        namespace: namespace.clone(),
        routine_type,
        definition: def
            .unwrap_or_else(|| format!("-- Could not retrieve definition for {}", routine_name)),
        language: lang,
        arguments: args,
        return_type: ret,
    })
}

pub async fn drop_routine(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    routine_name: &str,
    routine_type: RoutineType,
    arguments: Option<&str>,
) -> EngineResult<RoutineOperationResult> {
    let pg = get_session(sessions, session).await?;
    let schema = namespace.schema.as_deref().unwrap_or("public");

    let type_keyword = match routine_type {
        RoutineType::Function => "FUNCTION",
        RoutineType::Procedure => "PROCEDURE",
    };
    let args_clause = arguments.unwrap_or("");
    let sql = format!(
        "DROP {} \"{}\".\"{}\"({})",
        type_keyword, schema, routine_name, args_clause
    );

    let start = Instant::now();
    sqlx::query(&sql)
        .execute(&pg.pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

    Ok(RoutineOperationResult {
        success: true,
        executed_command: sql,
        message: None,
        execution_time_ms: start.elapsed().as_millis() as f64,
    })
}

// =============================================================================
// Triggers
// =============================================================================

pub async fn list_triggers(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    options: TriggerListOptions,
) -> EngineResult<TriggerList> {
    let pg = get_session(sessions, session).await?;
    let pool = &pg.pool;
    let schema = namespace.schema.as_deref().unwrap_or("public");
    let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

    let count_row: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(DISTINCT t.tgname)
        FROM pg_trigger t
        JOIN pg_class c ON t.tgrelid = c.oid
        JOIN pg_namespace n ON c.relnamespace = n.oid
        WHERE n.nspname = $1 AND NOT t.tgisinternal
        AND ($2::text IS NULL OR t.tgname::text ILIKE $2)
        "#,
    )
    .bind(schema)
    .bind(&search_pattern)
    .fetch_one(pool)
    .await
    .map_err(|e| EngineError::execution_error(e.to_string()))?;

    let mut query_str = r#"
        SELECT t.tgname::text, c.relname::text, t.tgtype::int,
               t.tgenabled::text, p.proname::text
        FROM pg_trigger t
        JOIN pg_class c ON t.tgrelid = c.oid
        JOIN pg_namespace n ON c.relnamespace = n.oid
        JOIN pg_proc p ON t.tgfoid = p.oid
        WHERE n.nspname = $1 AND NOT t.tgisinternal
        AND ($2::text IS NULL OR t.tgname::text ILIKE $2)
        ORDER BY t.tgname
    "#
    .to_string();

    if let Some(limit) = options.page_size {
        query_str.push_str(&format!(" LIMIT {}", limit));
        if let Some(page) = options.page {
            query_str.push_str(&format!(" OFFSET {}", (page.max(1) - 1) * limit));
        }
    }

    let rows: Vec<(String, String, i32, String, String)> = sqlx::query_as(&query_str)
        .bind(schema)
        .bind(&search_pattern)
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

    let triggers = rows
        .into_iter()
        .map(
            |(name, table_name, tg_type, enabled_char, function_name)| Trigger {
                namespace: namespace.clone(),
                name,
                table_name,
                timing: decode_trigger_timing(tg_type),
                events: decode_trigger_events(tg_type),
                enabled: enabled_char != "D",
                function_name: Some(function_name),
            },
        )
        .collect();

    Ok(TriggerList {
        triggers,
        total_count: count_row.0 as u32,
    })
}

pub async fn get_trigger_definition(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    trigger_name: &str,
) -> EngineResult<TriggerDefinition> {
    let pg = get_session(sessions, session).await?;
    let schema = namespace.schema.as_deref().unwrap_or("public");

    let row: (String, String, i32, String, String, String) = sqlx::query_as(
        r#"
        SELECT t.tgname::text, c.relname::text, t.tgtype::int,
               t.tgenabled::text, p.proname::text, pg_get_triggerdef(t.oid)::text
        FROM pg_trigger t
        JOIN pg_class c ON t.tgrelid = c.oid
        JOIN pg_namespace n ON c.relnamespace = n.oid
        JOIN pg_proc p ON t.tgfoid = p.oid
        WHERE n.nspname = $1 AND t.tgname = $2 AND NOT t.tgisinternal
        LIMIT 1
        "#,
    )
    .bind(schema)
    .bind(trigger_name)
    .fetch_optional(&pg.pool)
    .await
    .map_err(|e| EngineError::execution_error(e.to_string()))?
    .ok_or_else(|| EngineError::execution_error("Trigger not found"))?;

    let (name, table_name, tg_type, enabled_char, function_name, definition) = row;

    Ok(TriggerDefinition {
        name,
        namespace: namespace.clone(),
        table_name,
        timing: decode_trigger_timing(tg_type),
        events: decode_trigger_events(tg_type),
        definition,
        enabled: enabled_char != "D",
        function_name: Some(function_name),
    })
}

pub async fn drop_trigger(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    trigger_name: &str,
    table_name: &str,
) -> EngineResult<TriggerOperationResult> {
    let pg = get_session(sessions, session).await?;
    let schema = namespace.schema.as_deref().unwrap_or("public");

    let sql = format!(
        "DROP TRIGGER {} ON {}.{}",
        quote_ident(trigger_name),
        quote_ident(schema),
        quote_ident(table_name)
    );

    let start = Instant::now();
    sqlx::query(&sql)
        .execute(&pg.pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

    Ok(TriggerOperationResult {
        success: true,
        executed_command: sql,
        message: None,
        execution_time_ms: start.elapsed().as_millis() as f64,
    })
}

pub async fn toggle_trigger(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    trigger_name: &str,
    table_name: &str,
    enable: bool,
) -> EngineResult<TriggerOperationResult> {
    let pg = get_session(sessions, session).await?;
    let schema = namespace.schema.as_deref().unwrap_or("public");

    let action = if enable { "ENABLE" } else { "DISABLE" };
    let sql = format!(
        "ALTER TABLE {}.{} {} TRIGGER {}",
        quote_ident(schema),
        quote_ident(table_name),
        action,
        quote_ident(trigger_name)
    );

    let start = Instant::now();
    sqlx::query(&sql)
        .execute(&pg.pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

    Ok(TriggerOperationResult {
        success: true,
        executed_command: sql,
        message: None,
        execution_time_ms: start.elapsed().as_millis() as f64,
    })
}

// =============================================================================
// Schema operations
// =============================================================================

pub async fn create_schema(
    sessions: &SessionMap,
    session: SessionId,
    name: &str,
    driver_label: &str,
) -> EngineResult<()> {
    let pg = get_session(sessions, session).await?;

    if name.is_empty() || name.len() > 63 {
        return Err(EngineError::validation(
            "Schema name must be between 1 and 63 characters",
        ));
    }

    let query = format!("CREATE SCHEMA {}", quote_ident(name));
    sqlx::query(&query).execute(&pg.pool).await.map_err(|e| {
        tracing::error!("{}: Failed to create schema: {}", driver_label, e);
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

pub async fn drop_schema(
    sessions: &SessionMap,
    session: SessionId,
    name: &str,
    driver_label: &str,
) -> EngineResult<()> {
    let pg = get_session(sessions, session).await?;

    if name.is_empty() || name.len() > 63 {
        return Err(EngineError::validation(
            "Schema name must be between 1 and 63 characters",
        ));
    }

    let query = format!("DROP SCHEMA {} CASCADE", quote_ident(name));
    sqlx::query(&query).execute(&pg.pool).await.map_err(|e| {
        tracing::error!("{}: Failed to drop schema: {}", driver_label, e);
        let msg = e.to_string();
        if msg.contains("permission denied") {
            EngineError::auth_failed(format!("Permission denied: {}", msg))
        } else if msg.contains("does not exist") {
            EngineError::validation(format!("Schema '{}' does not exist", name))
        } else {
            EngineError::execution_error(msg)
        }
    })?;

    tracing::info!("{}: Successfully dropped schema '{}'", driver_label, name);
    Ok(())
}

// =============================================================================
// Internal helpers
// =============================================================================

fn qualified_table_name(namespace: &Namespace, table: &str) -> String {
    if let Some(schema) = &namespace.schema {
        format!("{}.{}", quote_ident(schema), quote_ident(table))
    } else {
        quote_ident(table)
    }
}

fn decode_trigger_timing(tg_type: i32) -> TriggerTiming {
    if tg_type & (1 << 6) != 0 {
        TriggerTiming::InsteadOf
    } else if tg_type & (1 << 1) != 0 {
        TriggerTiming::Before
    } else {
        TriggerTiming::After
    }
}

fn decode_trigger_events(tg_type: i32) -> Vec<TriggerEvent> {
    let mut events = Vec::new();
    if tg_type & (1 << 2) != 0 {
        events.push(TriggerEvent::Insert);
    }
    if tg_type & (1 << 3) != 0 {
        events.push(TriggerEvent::Delete);
    }
    if tg_type & (1 << 4) != 0 {
        events.push(TriggerEvent::Update);
    }
    if tg_type & (1 << 5) != 0 {
        events.push(TriggerEvent::Truncate);
    }
    events
}

// =============================================================================
// Connection string builder
// =============================================================================

pub fn build_pg_connection_string(config: &ConnectionConfig, default_db: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

    let db = config.database.as_deref().unwrap_or(default_db);

    // Use explicit ssl_mode if provided, otherwise fall back to boolean
    let ssl_mode = config
        .ssl_mode
        .as_deref()
        .unwrap_or(if config.ssl { "require" } else { "disable" });

    let encoded_user = utf8_percent_encode(&config.username, NON_ALPHANUMERIC);
    let encoded_pass = utf8_percent_encode(&config.password, NON_ALPHANUMERIC);

    format!(
        "postgres://{}:{}@{}:{}/{}?sslmode={}",
        encoded_user, encoded_pass, config.host, config.port, db, ssl_mode
    )
}

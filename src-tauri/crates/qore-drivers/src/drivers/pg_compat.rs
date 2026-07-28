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
use sqlx::Row;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow, Postgres};
use tokio::sync::{Mutex, RwLock};

use crate::drivers::postgres_utils::{
    EnumLabelMap, PgDecoder, bind_param, build_decoders, collect_enum_type_oids, columns_and_rows,
    convert_row_with_decoders, get_column_info, load_enum_labels,
};
use qore_core::cursor::KeysetPlan;
use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::{StreamEvent, StreamSender};
use qore_core::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType, ColumnInfo,
    ConnectionConfig, FilterOperator, ForeignKey, MaintenanceMessage, MaintenanceMessageLevel,
    MaintenanceOperationInfo, MaintenanceOperationType, MaintenanceRequest, MaintenanceResult,
    Namespace, PaginatedQueryResult, QueryId, QueryResult, Routine, RoutineDefinition, RoutineList,
    RoutineListOptions, RoutineOperationResult, RoutineType, RowData, SearchMode, SessionId,
    SortDirection, TableColumn, TableIndex, TableQueryOptions, TableSchema, Trigger,
    TriggerDefinition, TriggerEvent, TriggerList, TriggerListOptions, TriggerOperationResult,
    TriggerTiming, TruncateAllResult, Value,
};
use qore_sql::safety;

// Session

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

// Pool and connection helpers

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

// Maintenance
//
// Shared VACUUM / ANALYZE / REINDEX / CLUSTER support for full PostgreSQL
// servers. CockroachDB only supports ANALYZE and keeps its own override.

/// The maintenance operations supported by a full PostgreSQL server.
pub fn maintenance_operations() -> Vec<MaintenanceOperationInfo> {
    vec![
        MaintenanceOperationInfo {
            operation: MaintenanceOperationType::Vacuum,
            is_heavy: false,
            has_options: true,
        },
        MaintenanceOperationInfo {
            operation: MaintenanceOperationType::Analyze,
            is_heavy: false,
            has_options: false,
        },
        MaintenanceOperationInfo {
            operation: MaintenanceOperationType::Reindex,
            is_heavy: true,
            has_options: false,
        },
        MaintenanceOperationInfo {
            operation: MaintenanceOperationType::Cluster,
            is_heavy: true,
            has_options: true,
        },
    ]
}

/// Runs a VACUUM / ANALYZE / REINDEX / CLUSTER operation on a table for any
/// full PostgreSQL server (PostgreSQL, Neon, Supabase, TimescaleDB).
pub async fn run_maintenance(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    request: &MaintenanceRequest,
) -> EngineResult<MaintenanceResult> {
    let pg = get_session(sessions, session).await?;
    let schema = namespace.schema.as_deref().unwrap_or("public");
    let qualified_table = format!("{}.{}", quote_ident(schema), quote_ident(table));

    let sql = match request.operation {
        MaintenanceOperationType::Vacuum => {
            let full = if request.options.full.unwrap_or(false) {
                "FULL "
            } else {
                ""
            };
            let analyze = if request.options.with_analyze.unwrap_or(false) {
                "ANALYZE "
            } else {
                ""
            };
            let verbose = if request.options.verbose.unwrap_or(false) {
                "VERBOSE "
            } else {
                ""
            };
            format!("VACUUM {full}{analyze}{verbose}{qualified_table}")
        }
        MaintenanceOperationType::Analyze => {
            format!("ANALYZE {qualified_table}")
        }
        MaintenanceOperationType::Reindex => {
            format!("REINDEX TABLE {qualified_table}")
        }
        MaintenanceOperationType::Cluster => {
            if let Some(ref idx) = request.options.index_name {
                format!("CLUSTER {qualified_table} USING {}", quote_ident(idx))
            } else {
                format!("CLUSTER {qualified_table}")
            }
        }
        _ => {
            return Err(EngineError::not_supported(
                "Operation not supported for PostgreSQL",
            ));
        }
    };

    let start = Instant::now();
    // VACUUM cannot run inside a transaction, so always run on the pool directly.
    sqlx::query(&sql)
        .execute(&pg.pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

    let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

    Ok(MaintenanceResult {
        executed_command: sql,
        messages: vec![MaintenanceMessage {
            level: MaintenanceMessageLevel::Info,
            text: "Operation completed successfully".into(),
        }],
        execution_time_ms,
        success: true,
    })
}

/// Truncates all base tables in a namespace. When `namespace.schema` is set,
/// only that schema is truncated; otherwise every user schema in the database
/// is truncated. A single `TRUNCATE ... RESTART IDENTITY CASCADE` lets the
/// engine resolve foreign-key dependencies.
pub async fn truncate_all(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    driver_label: &str,
) -> EngineResult<TruncateAllResult> {
    let pg = get_session(sessions, session).await?;

    let tables: Vec<(String, String)> = if let Some(schema) = namespace.schema.as_deref() {
        sqlx::query_as::<_, (String, String)>(
            "SELECT table_schema, table_name FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type = 'BASE TABLE' \
             ORDER BY table_name",
        )
        .bind(schema)
        .fetch_all(&pg.pool)
        .await
    } else {
        sqlx::query_as::<_, (String, String)>(
            "SELECT table_schema, table_name FROM information_schema.tables \
             WHERE table_type = 'BASE TABLE' \
             AND table_schema NOT IN ('pg_catalog', 'information_schema') \
             AND table_schema NOT LIKE 'pg_toast%' \
             ORDER BY table_schema, table_name",
        )
        .fetch_all(&pg.pool)
        .await
    }
    .map_err(|e| EngineError::execution_error(e.to_string()))?;

    if tables.is_empty() {
        return Ok(TruncateAllResult {
            executed_command: String::new(),
            truncated_tables: Vec::new(),
            messages: vec![MaintenanceMessage {
                level: MaintenanceMessageLevel::Info,
                text: "No tables to truncate".into(),
            }],
            execution_time_ms: 0.0,
            success: true,
        });
    }

    let qualified: Vec<String> = tables
        .iter()
        .map(|(schema, table)| format!("{}.{}", quote_ident(schema), quote_ident(table)))
        .collect();
    let truncated_tables: Vec<String> = tables
        .iter()
        .map(|(schema, table)| format!("{schema}.{table}"))
        .collect();
    let sql = format!(
        "TRUNCATE TABLE {} RESTART IDENTITY CASCADE",
        qualified.join(", ")
    );

    let start = Instant::now();
    sqlx::query(&sql).execute(&pg.pool).await.map_err(|e| {
        tracing::error!("{}: Failed to truncate all tables: {}", driver_label, e);
        EngineError::execution_error(e.to_string())
    })?;
    let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

    let count = truncated_tables.len();
    Ok(TruncateAllResult {
        executed_command: sql,
        truncated_tables,
        messages: vec![MaintenanceMessage {
            level: MaintenanceMessageLevel::Info,
            text: format!("Truncated {count} table(s)"),
        }],
        execution_time_ms,
        success: true,
    })
}

pub async fn apply_namespace_on_conn(
    conn: &mut PoolConnection<Postgres>,
    driver_id: &str,
    namespace: &Option<Namespace>,
    query: &str,
    in_transaction: bool,
) -> EngineResult<()> {
    let lower = query.trim_start().to_ascii_lowercase();
    if lower.starts_with("use ") || (lower.starts_with("set ") && lower.contains("search_path")) {
        return Ok(());
    }

    if let Some(statement) = namespace_statement(driver_id, namespace.as_ref(), in_transaction) {
        sqlx::query(&statement)
            .execute(&mut **conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
    }

    Ok(())
}

fn namespace_statement(
    driver_id: &str,
    namespace: Option<&Namespace>,
    in_transaction: bool,
) -> Option<String> {
    let namespace = namespace?;
    if driver_id.eq_ignore_ascii_case("motherduck") {
        let database = quote_ident(&namespace.database);
        let target = namespace
            .schema
            .as_deref()
            .map(|schema| format!("{database}.{}", quote_ident(schema)))
            .unwrap_or(database);
        return Some(format!("USE {target}"));
    }

    let schema = namespace
        .schema
        .as_deref()
        .map(str::trim)
        .filter(|schema| !schema.is_empty())?;
    let schema_sql = quote_ident(schema);
    let search_path = if schema.eq_ignore_ascii_case("public") {
        schema_sql
    } else {
        format!("{schema_sql}, public")
    };
    let scope = if in_transaction { " LOCAL" } else { "" };
    Some(format!("SET{scope} search_path TO {search_path}"))
}

async fn track_query(pg: &PgCompatSession, query_id: Option<QueryId>, backend_pid: i32) {
    if let Some(qid) = query_id {
        pg.active_queries.lock().await.insert(qid, backend_pid);
    }
}

async fn untrack_query(pg: &PgCompatSession, query_id: Option<QueryId>) {
    if let Some(qid) = query_id {
        pg.active_queries.lock().await.remove(&qid);
    }
}

pub async fn fetch_backend_pid(conn: &mut PoolConnection<Postgres>) -> EngineResult<i32> {
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut **conn)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))
}

// Connection lifecycle

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
    let max = config.pool_max_connections.unwrap_or(10);
    let min = config.pool_min_connections.unwrap_or(2).min(max);
    let timeout = config.pool_acquire_timeout_secs.unwrap_or(15) as u64;

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
        // Best-effort ROLLBACK on the transaction-owned connection before we
        // drop it. PostgreSQL otherwise keeps the open transaction (and its
        // locks) until the server `idle_in_transaction_session_timeout`
        // expires, which is typically minutes (cf. audit B3-H2).
        let mut tx = session.transaction_conn.lock().await;
        if let Some(mut conn) = tx.take() {
            if let Err(e) = sqlx::query("ROLLBACK").execute(&mut *conn).await {
                tracing::warn!(?e, "ROLLBACK on disconnect failed");
            }
        }
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

// Execute

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

    let returns_rows =
        safety::returns_rows(driver_id, query).unwrap_or_else(|_| safety::is_select_prefix(query));

    let mut tx_guard = pg.transaction_conn.lock().await;

    let result = if let Some(ref mut conn) = *tx_guard {
        let backend_pid = fetch_backend_pid(conn).await?;
        {
            let mut active = pg.active_queries.lock().await;
            active.insert(query_id, backend_pid);
        }

        apply_namespace_on_conn(conn, driver_id, &namespace, query, true).await?;

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

        apply_namespace_on_conn(&mut conn, driver_id, &namespace, query, false).await?;

        let result = if returns_rows {
            exec_rows_on_conn(&mut conn, &pg.pool, query, start).await?
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

// Streaming

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

    apply_namespace_on_conn(&mut conn, driver_id, &namespace, query, false).await?;

    let returns_rows =
        safety::returns_rows(driver_id, query).unwrap_or_else(|_| safety::is_select_prefix(query));

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
                    if sender
                        .send(StreamEvent::RowBatch(std::mem::replace(
                            &mut batch,
                            Vec::with_capacity(500),
                        )))
                        .await
                        .is_err()
                    {
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

// Cancel

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

    // `pg_cancel_backend` returns `false` when the PID is unknown or already
    // gone. The original code threw the return value away, so the frontend
    // saw a successful cancel for queries that never received the signal —
    // and there was no fallback (`pg_terminate_backend`) if cancellation
    // simply didn't take (cf. audit B3-H1). We surface the per-PID outcome
    // and escalate to `pg_terminate_backend` after a brief grace window
    // when `pg_cancel_backend` says the PID was rejected.
    let mut failures: Vec<i32> = Vec::new();
    for pid in &backend_pids {
        let cancelled: bool = sqlx::query_scalar("SELECT pg_cancel_backend($1)")
            .bind(*pid)
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        if !cancelled {
            tracing::warn!(
                pid = pid,
                "pg_cancel_backend returned false; escalating to pg_terminate_backend"
            );
            let terminated: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)")
                .bind(*pid)
                .fetch_one(&mut *conn)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            if !terminated {
                failures.push(*pid);
            }
        }
    }

    if !failures.is_empty() {
        return Err(EngineError::execution_error(format!(
            "Failed to cancel {} backend pid(s): {:?}",
            failures.len(),
            failures
        )));
    }

    Ok(())
}

pub fn cancel_support() -> CancelSupport {
    CancelSupport::Driver
}

// Transactions

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

// Mutations

pub async fn insert_row(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    data: &RowData,
) -> EngineResult<QueryResult> {
    insert_row_with_qualification(sessions, session, namespace, table, data, false).await
}

pub async fn insert_row_duckdb(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    data: &RowData,
) -> EngineResult<QueryResult> {
    insert_row_with_qualification(sessions, session, namespace, table, data, true).await
}

async fn insert_row_with_qualification(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    data: &RowData,
    include_database: bool,
) -> EngineResult<QueryResult> {
    let pg = get_session(sessions, session).await?;

    let table_name = qualified_table_name(namespace, table, include_database);

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
    update_row_with_qualification(
        sessions,
        session,
        namespace,
        table,
        primary_key,
        data,
        false,
    )
    .await
}

pub async fn update_row_duckdb(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    primary_key: &RowData,
    data: &RowData,
) -> EngineResult<QueryResult> {
    update_row_with_qualification(sessions, session, namespace, table, primary_key, data, true)
        .await
}

async fn update_row_with_qualification(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    primary_key: &RowData,
    data: &RowData,
    include_database: bool,
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

    let table_name = qualified_table_name(namespace, table, include_database);

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
    delete_row_with_qualification(sessions, session, namespace, table, primary_key, false).await
}

pub async fn delete_row_duckdb(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    primary_key: &RowData,
) -> EngineResult<QueryResult> {
    delete_row_with_qualification(sessions, session, namespace, table, primary_key, true).await
}

async fn delete_row_with_qualification(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    primary_key: &RowData,
    include_database: bool,
) -> EngineResult<QueryResult> {
    let pg = get_session(sessions, session).await?;

    if primary_key.columns.is_empty() {
        return Err(EngineError::execution_error(
            "Primary key required for delete operations".to_string(),
        ));
    }

    let table_name = qualified_table_name(namespace, table, include_database);

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

// Peek FK

pub async fn peek_foreign_key(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    foreign_key: &ForeignKey,
    value: &Value,
    limit: u32,
) -> EngineResult<QueryResult> {
    peek_foreign_key_with_qualification(
        sessions,
        session,
        namespace,
        foreign_key,
        value,
        limit,
        false,
    )
    .await
}

pub async fn peek_foreign_key_duckdb(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    foreign_key: &ForeignKey,
    value: &Value,
    limit: u32,
) -> EngineResult<QueryResult> {
    peek_foreign_key_with_qualification(
        sessions,
        session,
        namespace,
        foreign_key,
        value,
        limit,
        true,
    )
    .await
}

async fn peek_foreign_key_with_qualification(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    foreign_key: &ForeignKey,
    value: &Value,
    limit: u32,
    include_database: bool,
) -> EngineResult<QueryResult> {
    let pg = get_session(sessions, session).await?;
    let limit = limit.max(1).min(50);
    let schema = foreign_key
        .referenced_schema
        .as_deref()
        .or(namespace.schema.as_deref())
        .unwrap_or(if include_database { "main" } else { "public" });

    let table_ref = if include_database {
        let database = foreign_key
            .referenced_database
            .as_deref()
            .unwrap_or(&namespace.database);
        format!(
            "{}.{}.{}",
            quote_ident(database),
            quote_ident(schema),
            quote_ident(&foreign_key.referenced_table)
        )
    } else {
        format!(
            "{}.{}",
            quote_ident(schema),
            quote_ident(&foreign_key.referenced_table)
        )
    };
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

// Query Table (paginated)

pub async fn query_table(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    options: TableQueryOptions,
) -> EngineResult<PaginatedQueryResult> {
    query_table_with_dialect(sessions, session, namespace, table, options, false).await
}

/// PostgreSQL-wire table browsing using DuckDB SQL semantics. MotherDuck's
/// transport is PostgreSQL-compatible, but regex and text-search expressions
/// must still use DuckDB functions rather than PostgreSQL FTS operators.
pub async fn query_table_duckdb(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    options: TableQueryOptions,
) -> EngineResult<PaginatedQueryResult> {
    query_table_with_dialect(sessions, session, namespace, table, options, true).await
}

async fn query_table_with_dialect(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    options: TableQueryOptions,
    duckdb_dialect: bool,
) -> EngineResult<PaginatedQueryResult> {
    let pg = get_session(sessions, session).await?;
    let start = Instant::now();

    let schema_name =
        namespace
            .schema
            .as_deref()
            .unwrap_or(if duckdb_dialect { "main" } else { "public" });
    let table_ref = qualified_table_name(namespace, table, duckdb_dialect);

    let page = options.effective_page();
    let page_size = options.effective_page_size();
    let fetch_size = options.fetch_size();
    let offset = options.offset();

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
                    // Cast to text so substring search works on every column type
                    // (numbers, booleans, dates…), not just text columns. Mirrors
                    // the global-search behavior below.
                    bind_values.push(filter.value.clone());
                    format!("{}::text ILIKE ${}", col_ident, param_idx)
                }
                FilterOperator::IsNull => format!("{} IS NULL", col_ident),
                FilterOperator::IsNotNull => format!("{} IS NOT NULL", col_ident),
                FilterOperator::Regex => {
                    filter.value.as_text().ok_or_else(|| {
                        EngineError::syntax_error(
                            "regex operator requires a string value in 'value'",
                        )
                    })?;
                    bind_values.push(filter.value.clone());
                    let flags = filter.options.sanitized_regex_flags();
                    if duckdb_dialect {
                        if flags.is_empty() {
                            format!("regexp_matches({}::VARCHAR, ${})", col_ident, param_idx)
                        } else {
                            format!(
                                "regexp_matches({}::VARCHAR, ${}, '{}')",
                                col_ident, param_idx, flags
                            )
                        }
                    } else {
                        let op = if flags.contains('i') { "~*" } else { "~" };
                        format!("{} {} ${}", col_ident, op, param_idx)
                    }
                }
                FilterOperator::Text => {
                    let term = filter.value.as_text().ok_or_else(|| {
                        EngineError::syntax_error(
                            "text operator requires a string value in 'value'",
                        )
                    })?;
                    if duckdb_dialect {
                        bind_values.push(Value::Text(format!("%{term}%")));
                        format!("{}::VARCHAR ILIKE ${}", col_ident, param_idx)
                    } else {
                        bind_values.push(filter.value.clone());
                        let lang = filter.options.sanitized_text_language("english");
                        // `lang` is guaranteed to be `[a-z_]{1,32}`, safe to
                        // interpolate into the SQL function call.
                        format!(
                            "to_tsvector('{}', {}::text) @@ plainto_tsquery('{}', ${})",
                            lang, col_ident, lang, param_idx
                        )
                    }
                }
            };
            where_clauses.push(clause);
        }
    }

    if let Some(search_term) = options.effective_search() {
        // A caller-supplied scope removes the catalog round-trip this branch
        // would otherwise pay on every page and every debounced keystroke. The
        // cast is unconditional: the pattern is unanchored, so no index applies
        // to a text column either, and the driver has no column types here.
        if let Some(scope) = options.effective_search_columns() {
            let anchored = options.effective_search_mode() == SearchMode::StartsWith;
            let mut search_clauses: Vec<String> = Vec::new();
            for col_name in scope {
                bind_values.push(Value::Text(options.search_pattern(search_term)));
                // Anchored mode drops both the cast and the case folding: either
                // one makes the expression stop matching an index on the column.
                search_clauses.push(if anchored {
                    format!("{} LIKE ${}", quote_ident(col_name), bind_values.len())
                } else {
                    format!(
                        "{}::text ILIKE ${}",
                        quote_ident(col_name),
                        bind_values.len()
                    )
                });
            }
            if !search_clauses.is_empty() {
                where_clauses.push(format!("({})", search_clauses.join(" OR ")));
            }
        } else {
            let columns_sql = "SELECT column_name, data_type FROM information_schema.columns WHERE table_catalog = $1 AND table_schema = $2 AND table_name = $3";
            let columns_rows: Vec<PgRow> = {
                let mut tx_guard = pg.transaction_conn.lock().await;
                if let Some(ref mut conn) = *tx_guard {
                    sqlx::query(columns_sql)
                        .bind(&namespace.database)
                        .bind(schema_name)
                        .bind(table)
                        .fetch_all(&mut **conn)
                        .await
                } else {
                    sqlx::query(columns_sql)
                        .bind(&namespace.database)
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

                let normalized_type = data_type.to_ascii_uppercase();
                let is_unsearchable = if duckdb_dialect {
                    normalized_type.contains("BLOB")
                } else {
                    matches!(data_type.as_str(), "bytea" | "tsvector" | "tsquery")
                };
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

    // Keyset needs a total order. The caller supplies the unique key — it holds
    // the schema already — so the driver reads no catalog, not even on the
    // first page. Without a unique key there is no total order and `OFFSET`
    // stays the honest answer.
    let keyset = options
        .keyset_applies()
        .then(|| {
            KeysetPlan::new(
                options.sort_column.as_deref(),
                matches!(options.sort_direction, Some(SortDirection::Desc)),
                options.effective_keyset_columns(),
            )
        })
        .flatten();

    // Cursor binds live apart from `bind_values`: the count must stay a count of
    // the filtered set, not of what is left after the boundary.
    let mut cursor_values: Vec<Value> = Vec::new();
    let mut cursor_sql = String::new();
    if let (Some(plan), Some(encoded)) = (keyset.as_ref(), options.cursor.as_deref()) {
        let base = bind_values.len();
        cursor_sql = plan.predicate(
            |col| quote_ident(col),
            |index| format!("${}", base + index + 1),
        );
        cursor_values = plan.decode(encoded)?.values;
    }

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

    // MotherDuck speaks the PostgreSQL wire but stores columnar data, where an
    // exact count is cheap enough to answer an estimate request outright.
    let exact_count = options.wants_exact_total() || (duckdb_dialect && options.wants_any_total());

    let total_rows = if exact_count {
        let count_sql = format!(
            "SELECT COUNT(*)::bigint AS cnt FROM {}{}",
            table_ref, where_sql
        );
        let mut count_query = sqlx::query(&count_sql);
        for val in &bind_values {
            count_query = bind_param(count_query, val);
        }

        // Pinned to a single connection, whose backend PID is registered under
        // the caller's query id: without it `cancel` has nothing to target and
        // an accidental count on a huge table runs to completion.
        let count_row: PgRow = {
            let mut tx_guard = pg.transaction_conn.lock().await;
            if let Some(ref mut conn) = *tx_guard {
                let pid = fetch_backend_pid(conn).await?;
                track_query(&pg, options.query_id, pid).await;
                let outcome = count_query.fetch_one(&mut **conn).await;
                untrack_query(&pg, options.query_id).await;
                outcome
            } else {
                drop(tx_guard);
                let mut conn = pg
                    .pool
                    .acquire()
                    .await
                    .map_err(|e| EngineError::connection_failed(e.to_string()))?;
                let pid = fetch_backend_pid(&mut conn).await?;
                track_query(&pg, options.query_id, pid).await;
                let outcome = count_query.fetch_one(&mut *conn).await;
                untrack_query(&pg, options.query_id).await;
                outcome
            }
        }
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let total_rows: i64 = count_row
            .try_get("cnt")
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        Some(total_rows.max(0) as u64)
    } else {
        None
    };

    let data_sql = if let Some(plan) = keyset.as_ref() {
        let data_where = match (where_sql.is_empty(), cursor_sql.is_empty()) {
            (_, true) => where_sql.clone(),
            (true, false) => format!(" WHERE {}", cursor_sql),
            (false, false) => format!("{} AND {}", where_sql, cursor_sql),
        };
        format!(
            "SELECT * FROM {} {} ORDER BY {} LIMIT {}",
            table_ref,
            data_where,
            plan.order_by(|col| quote_ident(col)),
            fetch_size
        )
    } else {
        format!(
            "SELECT * FROM {}{}{} LIMIT {} OFFSET {}",
            table_ref, where_sql, order_sql, fetch_size, offset
        )
    };

    let mut data_query = sqlx::query(&data_sql);
    for val in &bind_values {
        data_query = bind_param(data_query, val);
    }
    for val in &cursor_values {
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
        let col_meta_sql = "SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE table_catalog = $1 AND table_schema = $2 AND table_name = $3 ORDER BY ordinal_position";
        let col_meta_rows: Vec<PgRow> = {
            let mut tx_guard = pg.transaction_conn.lock().await;
            if let Some(ref mut conn) = *tx_guard {
                sqlx::query(col_meta_sql)
                    .bind(&namespace.database)
                    .bind(schema_name)
                    .bind(table)
                    .fetch_all(&mut **conn)
                    .await
            } else {
                sqlx::query(col_meta_sql)
                    .bind(&namespace.database)
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
                    name: name.into(),
                    data_type: data_type.into(),
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

    let mut paginated =
        PaginatedQueryResult::from_optional_total(result, total_rows, page, page_size);

    if let Some(plan) = keyset.as_ref() {
        // Minted from the last row actually returned, so the over-fetched row
        // that `from_optional_total` trimmed never becomes a boundary — it
        // would skip one row per page.
        let next_cursor = paginated
            .has_more
            .then(|| next_cursor_for(&paginated.result, plan))
            .flatten();
        paginated = paginated.with_keyset(next_cursor);
    }

    if !exact_count && options.wants_any_total() && options.estimate_matches_scope() {
        let (estimate, as_of) = pg_row_estimate(&pg.pool, schema_name, table).await;
        return Ok(paginated.with_estimate(estimate, as_of));
    }

    Ok(paginated)
}

/// Cursor for the page after `result`, or `None` when it cannot be built.
fn next_cursor_for(result: &QueryResult, plan: &KeysetPlan) -> Option<String> {
    let last = result.rows.last()?;
    let columns: Vec<String> = result
        .columns
        .iter()
        .map(|col| col.name.to_string())
        .collect();
    plan.mint(&columns, &last.values)
}

/// Planner row estimate from the catalog, with the freshness of the statistics
/// behind it. `reltuples` is -1 on a table that was never analyzed, and stale
/// after a bulk load — hence the timestamp travelling with the number.
async fn pg_row_estimate(pool: &PgPool, schema: &str, table: &str) -> (Option<u64>, Option<i64>) {
    let row: Option<(f32, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
        r#"
        SELECT c.reltuples,
               GREATEST(s.last_analyze, s.last_autoanalyze) AS analyzed_at
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        LEFT JOIN pg_stat_all_tables s ON s.relid = c.oid
        WHERE n.nspname = $1 AND c.relname = $2
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match row {
        Some((reltuples, analyzed_at)) if reltuples >= 0.0 => (
            Some(reltuples as u64),
            analyzed_at.map(|ts| ts.timestamp_millis()),
        ),
        _ => (None, None),
    }
}

// Describe Table

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
    let column_rows: Vec<(String, String, String, Option<String>, String)> = sqlx::query_as(
        r#"
        SELECT column_name::text, data_type::text, is_nullable::text, column_default::text,
               is_identity::text
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
            |(name, data_type, is_nullable, default_value, is_identity)| {
                let is_auto_increment = is_identity == "YES"
                    || default_value
                        .as_deref()
                        .is_some_and(|d| d.contains("nextval("));
                TableColumn {
                    is_primary_key: pk_columns.contains(&name),
                    name,
                    data_type,
                    nullable: is_nullable == "YES",
                    default_value,
                    is_auto_increment,
                }
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
        .map(
            |(name, columns, is_unique, is_primary, index_type)| TableIndex {
                name,
                columns,
                is_unique,
                is_primary,
                index_type,
            },
        )
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

// Namespaces & collections (default PG-compat implementation, matviews included)

/// Default `list_namespaces` implementation: lists every non-system schema in
/// the current database. Drivers with stricter exclusion lists (e.g. CockroachDB)
/// keep their own override.
pub async fn list_namespaces_default(
    sessions: &SessionMap,
    session: SessionId,
) -> EngineResult<Vec<Namespace>> {
    let pg = get_session(sessions, session).await?;
    let pool = &pg.pool;

    let current_db: (String,) = sqlx::query_as("SELECT current_database()")
        .fetch_one(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

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

    Ok(rows
        .into_iter()
        .map(|(name,)| Namespace::with_schema(&current_db.0, name))
        .collect())
}

/// Default `list_collections` implementation: tables, views and materialized
/// views. Drivers without matview support (CockroachDB) keep their own override.
pub async fn list_collections_default(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    options: CollectionListOptions,
) -> EngineResult<CollectionList> {
    let pg = get_session(sessions, session).await?;
    let pool = &pg.pool;

    let schema = namespace.schema.as_deref().unwrap_or("public");
    let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

    let count_row: (i64,) = sqlx::query_as(
        r#"
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
        "#,
    )
    .bind(schema)
    .bind(&search_pattern)
    .bind(&search_pattern)
    .fetch_one(pool)
    .await
    .map_err(|e| EngineError::execution_error(e.to_string()))?;

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
        total_count: count_row.0 as u32,
    })
}

// Routines

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
        "DROP {} {}.{}({})",
        type_keyword,
        quote_ident(schema),
        quote_ident(routine_name),
        args_clause
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

// Triggers

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

// Schema operations

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

// Internal helpers

fn qualified_table_name(namespace: &Namespace, table: &str, include_database: bool) -> String {
    if include_database {
        let schema = namespace.schema.as_deref().unwrap_or("main");
        format!(
            "{}.{}.{}",
            quote_ident(&namespace.database),
            quote_ident(schema),
            quote_ident(table)
        )
    } else if let Some(schema) = &namespace.schema {
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

// Connection string builder

pub fn build_pg_connection_string(config: &ConnectionConfig, default_db: &str) -> String {
    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

    let db = config.database.as_deref().unwrap_or(default_db);

    // Explicit ssl_mode wins; otherwise derive a sslmode from the boolean.
    let ssl_mode =
        config
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duckdb_table_names_include_catalog_and_schema() {
        let namespace = Namespace::with_schema("analytics", "sales");
        assert_eq!(
            qualified_table_name(&namespace, "orders", true),
            "\"analytics\".\"sales\".\"orders\""
        );
    }

    #[test]
    fn postgres_table_names_remain_schema_qualified() {
        let namespace = Namespace::with_schema("application", "public");
        assert_eq!(
            qualified_table_name(&namespace, "users", false),
            "\"public\".\"users\""
        );
    }

    #[test]
    fn motherduck_namespace_selects_catalog_and_schema() {
        let namespace = Namespace::with_schema("analytics", "sales");
        assert_eq!(
            namespace_statement("motherduck", Some(&namespace), false).as_deref(),
            Some("USE \"analytics\".\"sales\"")
        );
    }

    #[test]
    fn postgres_namespace_keeps_search_path_behavior() {
        let namespace = Namespace::with_schema("application", "private");
        assert_eq!(
            namespace_statement("postgres", Some(&namespace), true).as_deref(),
            Some("SET LOCAL search_path TO \"private\", public")
        );
    }
}

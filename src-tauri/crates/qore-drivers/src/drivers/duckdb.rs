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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use ::duckdb::{
    AccessMode, Config, Connection, InterruptHandle, Statement,
    core::{LogicalTypeHandle, LogicalTypeId},
    params_from_iter,
    types::{TimeUnit, Value as DuckValue},
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, SecondsFormat, Utc};
use tokio::sync::RwLock;

use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::{DataEngine, StreamEvent, StreamSender};
use qore_core::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType, ColumnInfo,
    ConnectionConfig, FilterOperator, ForeignKey, MaintenanceMessage, MaintenanceMessageLevel,
    MaintenanceOperationInfo, MaintenanceOperationType, MaintenanceRequest, MaintenanceResult,
    Namespace, PaginatedQueryResult, QueryId, QueryResult, Routine, RoutineDefinition, RoutineList,
    RoutineListOptions, RoutineOperationResult, RoutineType, Row as QRow, RowData, Sequence,
    SequenceDefinition, SequenceList, SequenceListOptions, SequenceOperationResult, SessionId,
    SortDirection, TableColumn, TableIndex, TableQueryOptions, TableSchema, TruncateAllResult,
    Value,
};
use qore_sql::safety;

/// Holds the connection state for a DuckDB session.
pub struct DuckDbSession {
    /// The DuckDB connection, protected by a std Mutex (Connection is !Sync).
    conn: std::sync::Mutex<Connection>,
    transaction_active: AtomicBool,
    /// DuckDB's interrupt handle does not require acquiring the connection
    /// mutex, so cancellation remains possible while a query is executing.
    interrupt: Arc<InterruptHandle>,
    active_query: std::sync::Mutex<Option<QueryId>>,
    /// The file path to the database (or ":memory:").
    pub db_path: String,
}

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

    fn open_connection(config: &ConnectionConfig) -> EngineResult<Connection> {
        let path = config.host.trim();
        let access_mode = if config.read_only {
            AccessMode::ReadOnly
        } else {
            AccessMode::ReadWrite
        };
        let flags = Config::default()
            .access_mode(access_mode)
            .and_then(|config| config.custom_user_agent("QoreDB"))
            .map_err(|e| {
                EngineError::connection_failed(format!("Invalid DuckDB configuration: {e}"))
            })?;

        if path == ":memory:" || path == "duckdb::memory:" {
            if config.read_only {
                return Err(EngineError::connection_failed(
                    "DuckDB in-memory databases cannot be opened in read-only mode",
                ));
            }
            Connection::open_in_memory_with_flags(flags).map_err(|e| {
                EngineError::connection_failed(format!("Failed to open DuckDB in-memory: {e}"))
            })
        } else {
            Connection::open_with_flags(path, flags).map_err(|e| {
                EngineError::connection_failed(format!(
                    "Failed to open DuckDB file '{}': {e}",
                    path
                ))
            })
        }
    }

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

    /// Runs a cancellable query. The active query is registered only after
    /// acquiring the serialized connection, avoiding queued queries replacing
    /// the identifier of the statement that is actually running.
    async fn with_query_conn<F, R>(
        session: &Arc<DuckDbSession>,
        query_id: QueryId,
        f: F,
    ) -> EngineResult<R>
    where
        F: FnOnce(&Connection) -> EngineResult<R> + Send + 'static,
        R: Send + 'static,
    {
        let session = Arc::clone(session);
        tokio::task::spawn_blocking(move || {
            let conn = session.conn.lock().map_err(|e| {
                EngineError::internal(format!("Failed to lock DuckDB connection: {e}"))
            })?;
            let _active_query = ActiveQueryGuard::start(Arc::clone(&session), query_id)?;
            f(&conn)
        })
        .await
        .map_err(|e| EngineError::internal(format!("DuckDB task panicked: {e}")))?
    }
}

struct ActiveQueryGuard {
    session: Arc<DuckDbSession>,
    query_id: QueryId,
}

impl ActiveQueryGuard {
    fn start(session: Arc<DuckDbSession>, query_id: QueryId) -> EngineResult<Self> {
        let mut active = session.active_query.lock().map_err(|e| {
            EngineError::internal(format!("Failed to register active DuckDB query: {e}"))
        })?;
        *active = Some(query_id);
        drop(active);
        Ok(Self { session, query_id })
    }
}

impl Drop for ActiveQueryGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.session.active_query.lock() {
            if *active == Some(self.query_id) {
                *active = None;
            }
        }
    }
}

impl Default for DuckDbDriver {
    fn default() -> Self {
        Self::new()
    }
}

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
        // duckdb-rs exposes List/Array values when reading but currently does
        // not implement binding them. Keep this as a bound text parameter;
        // DuckDB coerces the JSON/list literal to the destination LIST/ARRAY
        // type without interpolating user-controlled data into SQL.
        Value::Array(_) => DuckValue::Text(value.to_json().to_string()),
    }
}

fn duckdb_value_to_qoredb(row: &::duckdb::Row<'_>, idx: usize) -> ::duckdb::Result<Value> {
    Ok(duckdb_owned_value_to_qoredb(row.get_ref(idx)?.to_owned()))
}

/// Converts DuckDB's complete dynamic value surface without silently turning
/// unsupported or high-precision values into NULL. Values that have no native
/// QoreDB scalar representation (128-bit integers, decimals, temporal values,
/// intervals and enums) use their lossless textual form. Nested values remain
/// structured as arrays / JSON objects.
fn duckdb_owned_value_to_qoredb(value: DuckValue) -> Value {
    match value {
        DuckValue::Null => Value::Null,
        DuckValue::Boolean(value) => Value::Bool(value),
        DuckValue::TinyInt(value) => Value::Int(value as i64),
        DuckValue::SmallInt(value) => Value::Int(value as i64),
        DuckValue::Int(value) => Value::Int(value as i64),
        DuckValue::BigInt(value) => Value::Int(value),
        DuckValue::HugeInt(value) => i64::try_from(value)
            .map(Value::Int)
            .unwrap_or_else(|_| Value::Text(value.to_string())),
        DuckValue::UTinyInt(value) => Value::Int(value as i64),
        DuckValue::USmallInt(value) => Value::Int(value as i64),
        DuckValue::UInt(value) => Value::Int(value as i64),
        DuckValue::UBigInt(value) => i64::try_from(value)
            .map(Value::Int)
            .unwrap_or_else(|_| Value::Text(value.to_string())),
        DuckValue::Float(value) => Value::Float(value as f64),
        DuckValue::Double(value) => Value::Float(value),
        DuckValue::Decimal(value) => Value::Text(value.to_string()),
        DuckValue::Timestamp(unit, value) => Value::Text(format_timestamp(unit, value)),
        DuckValue::Text(value) => Value::Text(value),
        DuckValue::Blob(value) => Value::Bytes(value),
        DuckValue::Date32(days) => Value::Text(format_date(days)),
        DuckValue::Time64(unit, value) => Value::Text(format_time(unit, value)),
        DuckValue::Interval {
            months,
            days,
            nanos,
        } => Value::Text(format!("{months} months {days} days {nanos} nanoseconds")),
        DuckValue::List(values) | DuckValue::Array(values) => Value::Array(
            values
                .into_iter()
                .map(duckdb_owned_value_to_qoredb)
                .collect(),
        ),
        DuckValue::Enum(value) => Value::Text(value),
        DuckValue::Struct(values) => {
            let object = values
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        duckdb_owned_value_to_qoredb(value.clone()).to_json(),
                    )
                })
                .collect();
            Value::Json(serde_json::Value::Object(object))
        }
        DuckValue::Map(values) => Value::Array(
            values
                .iter()
                .map(|(key, value)| {
                    Value::Array(vec![
                        duckdb_owned_value_to_qoredb(key.clone()),
                        duckdb_owned_value_to_qoredb(value.clone()),
                    ])
                })
                .collect(),
        ),
        DuckValue::Union(value) => duckdb_owned_value_to_qoredb(*value),
    }
}

fn duckdb_string_list(value: DuckValue) -> EngineResult<Vec<String>> {
    match value {
        DuckValue::Null => Ok(Vec::new()),
        DuckValue::List(values) | DuckValue::Array(values) => values
            .into_iter()
            .map(|value| match value {
                DuckValue::Text(value) | DuckValue::Enum(value) => Ok(value),
                other => Err(EngineError::execution_error(format!(
                    "Expected a DuckDB string-list item, got {other:?}"
                ))),
            })
            .collect(),
        other => Err(EngineError::execution_error(format!(
            "Expected a DuckDB string list, got {other:?}"
        ))),
    }
}

fn time_unit_micros(unit: TimeUnit, value: i64) -> i128 {
    let factor = match unit {
        TimeUnit::Second => 1_000_000_i128,
        TimeUnit::Millisecond => 1_000,
        TimeUnit::Microsecond => 1,
        TimeUnit::Nanosecond => return i128::from(value).div_euclid(1_000),
    };
    i128::from(value) * factor
}

fn format_timestamp(unit: TimeUnit, value: i64) -> String {
    let micros = time_unit_micros(unit, value);
    let seconds = micros.div_euclid(1_000_000);
    let subsec_micros = micros.rem_euclid(1_000_000) as u32;
    i64::try_from(seconds)
        .ok()
        .and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds, subsec_micros * 1_000))
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Micros, true))
        .unwrap_or_else(|| format!("{value} {unit:?} since 1970-01-01"))
}

fn format_date(days: i32) -> String {
    NaiveDate::from_ymd_opt(1970, 1, 1)
        .and_then(|epoch| epoch.checked_add_signed(Duration::days(days as i64)))
        .map(|value| value.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| format!("{days} days since 1970-01-01"))
}

fn format_time(unit: TimeUnit, value: i64) -> String {
    let micros_per_day = 86_400_000_000_i128;
    let micros = time_unit_micros(unit, value).rem_euclid(micros_per_day);
    let seconds = (micros / 1_000_000) as u32;
    let nanos = (micros.rem_euclid(1_000_000) as u32) * 1_000;
    NaiveTime::from_num_seconds_from_midnight_opt(seconds, nanos)
        .map(|value| value.format("%H:%M:%S%.6f").to_string())
        .unwrap_or_else(|| format!("{value} {unit:?}"))
}

fn logical_type_name(logical_type: &LogicalTypeHandle) -> String {
    if let Some(alias) = logical_type.get_alias() {
        if !alias.is_empty() {
            return alias;
        }
    }

    match logical_type.id() {
        LogicalTypeId::Invalid => "INVALID".into(),
        LogicalTypeId::Boolean => "BOOLEAN".into(),
        LogicalTypeId::Tinyint => "TINYINT".into(),
        LogicalTypeId::Smallint => "SMALLINT".into(),
        LogicalTypeId::Integer => "INTEGER".into(),
        LogicalTypeId::Bigint => "BIGINT".into(),
        LogicalTypeId::UTinyint => "UTINYINT".into(),
        LogicalTypeId::USmallint => "USMALLINT".into(),
        LogicalTypeId::UInteger => "UINTEGER".into(),
        LogicalTypeId::UBigint => "UBIGINT".into(),
        LogicalTypeId::Float => "FLOAT".into(),
        LogicalTypeId::Double => "DOUBLE".into(),
        LogicalTypeId::Timestamp => "TIMESTAMP".into(),
        LogicalTypeId::Date => "DATE".into(),
        LogicalTypeId::Time => "TIME".into(),
        LogicalTypeId::Interval => "INTERVAL".into(),
        LogicalTypeId::Hugeint => "HUGEINT".into(),
        LogicalTypeId::Varchar => "VARCHAR".into(),
        LogicalTypeId::Blob => "BLOB".into(),
        LogicalTypeId::Decimal => format!(
            "DECIMAL({}, {})",
            logical_type.decimal_width(),
            logical_type.decimal_scale()
        ),
        LogicalTypeId::TimestampS => "TIMESTAMP_S".into(),
        LogicalTypeId::TimestampMs => "TIMESTAMP_MS".into(),
        LogicalTypeId::TimestampNs => "TIMESTAMP_NS".into(),
        LogicalTypeId::Enum => "ENUM".into(),
        LogicalTypeId::List => format!("{}[]", logical_type_name(&logical_type.child(0))),
        LogicalTypeId::Struct => {
            let fields = (0..logical_type.num_children())
                .map(|idx| {
                    format!(
                        "{} {}",
                        logical_type.child_name(idx),
                        logical_type_name(&logical_type.child(idx))
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("STRUCT({fields})")
        }
        LogicalTypeId::Map => format!(
            "MAP({}, {})",
            logical_type_name(&logical_type.child(0)),
            logical_type_name(&logical_type.child(1))
        ),
        LogicalTypeId::Uuid => "UUID".into(),
        LogicalTypeId::Union => {
            let members = (0..logical_type.num_children())
                .map(|idx| {
                    format!(
                        "{} {}",
                        logical_type.child_name(idx),
                        logical_type_name(&logical_type.child(idx))
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("UNION({members})")
        }
        LogicalTypeId::Bit => "BIT".into(),
        LogicalTypeId::TimeTZ => "TIME WITH TIME ZONE".into(),
        LogicalTypeId::TimestampTZ => "TIMESTAMP WITH TIME ZONE".into(),
        LogicalTypeId::UHugeint => "UHUGEINT".into(),
        LogicalTypeId::Array => format!("{}[]", logical_type_name(&logical_type.child(0))),
        LogicalTypeId::Any => "ANY".into(),
        LogicalTypeId::Bignum => "BIGNUM".into(),
        LogicalTypeId::SqlNull => "NULL".into(),
        LogicalTypeId::StringLiteral => "STRING_LITERAL".into(),
        LogicalTypeId::IntegerLiteral => "INTEGER_LITERAL".into(),
        LogicalTypeId::TimeNs => "TIME_NS".into(),
        LogicalTypeId::Variant => "VARIANT".into(),
        LogicalTypeId::Unsupported => format!("UNKNOWN({})", logical_type.raw_id()),
        _ => format!("UNKNOWN({})", logical_type.raw_id()),
    }
}

fn column_info(stmt: &Statement<'_>) -> Vec<ColumnInfo> {
    (0..stmt.column_count())
        .map(|idx| ColumnInfo {
            name: stmt
                .column_name(idx)
                .map(|name| name.as_str().into())
                .unwrap_or_else(|_| format!("col_{idx}").into()),
            data_type: logical_type_name(&stmt.column_logical_type(idx)).into(),
            nullable: true,
        })
        .collect()
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

    // duckdb crate: column_count/column_name panic before execution; read column count off the Row inside the closure.
    let rows_iter = stmt
        .query_map([], |row| {
            let col_count = row.as_ref().column_count();
            let values = (0..col_count)
                .map(|i| duckdb_value_to_qoredb(row, i))
                .collect::<::duckdb::Result<Vec<_>>>()?;
            Ok(QRow { values })
        })
        .map_err(|e| classify_error(e.to_string()))?;

    let mut rows = Vec::new();
    for row_result in rows_iter {
        let row = row_result.map_err(|e| EngineError::execution_error(e.to_string()))?;
        rows.push(row);
    }

    let columns = column_info(&stmt);

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
        let interrupt = conn.interrupt_handle();

        let session_id = SessionId::new();
        let session = Arc::new(DuckDbSession {
            conn: std::sync::Mutex::new(conn),
            transaction_active: AtomicBool::new(false),
            interrupt,
            active_query: std::sync::Mutex::new(None),
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
        Ok(())
    }

    async fn ping(&self, session: SessionId) -> EngineResult<()> {
        let duck_session = self.get_session(session).await?;
        Self::with_conn(&duck_session, |conn| {
            conn.execute("SELECT 1", [])
                .map_err(|e| EngineError::connection_failed(format!("Ping failed: {e}")))?;
            Ok(())
        })
        .await
    }

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
                let schema_name = row.map_err(|e| EngineError::execution_error(e.to_string()))?;
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
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());

        Self::with_conn(&duck_session, move |conn| {
            let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

            // Build a unified Vec<String> param list to keep closure types consistent across branches.
            let mut count_sql = String::from(
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = ?1",
            );
            let mut count_params: Vec<String> = vec![schema_name.clone()];
            if let Some(ref pattern) = search_pattern {
                count_sql.push_str(" AND table_name ILIKE ?2");
                count_params.push(pattern.clone());
            }

            let total_count: i64 = conn
                .query_row(&count_sql, params_from_iter(count_params.iter()), |row| {
                    row.get(0)
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut data_sql = String::from(
                "SELECT table_name, table_type FROM information_schema.tables \
                 WHERE table_schema = ?1",
            );

            let mut params: Vec<String> = vec![schema_name.clone()];
            if let Some(ref pattern) = search_pattern {
                data_sql.push_str(" AND table_name ILIKE ?2");
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

    fn supports_routines(&self) -> bool {
        // DuckDB user-defined scalar and table macros are its persistent SQL
        // routine surface. It does not expose stored procedures.
        true
    }

    async fn list_routines(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: RoutineListOptions,
    ) -> EngineResult<RoutineList> {
        let duck_session = self.get_session(session).await?;
        let namespace = namespace.clone();
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());

        if matches!(options.routine_type, Some(RoutineType::Procedure)) {
            return Ok(RoutineList {
                routines: Vec::new(),
                total_count: 0,
            });
        }

        Self::with_conn(&duck_session, move |conn| {
            let search = options.search.as_ref().map(|value| format!("%{value}%"));
            let mut where_sql = String::from(
                "schema_name = ?1 AND internal = false AND macro_definition IS NOT NULL",
            );
            let mut params = vec![schema_name.clone()];
            if let Some(pattern) = &search {
                where_sql.push_str(" AND function_name ILIKE ?2");
                params.push(pattern.clone());
            }

            let total_count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM duckdb_functions() WHERE {where_sql}"),
                    params_from_iter(params.iter()),
                    |row| row.get(0),
                )
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut sql = format!(
                "SELECT function_name, function_type, parameters, parameter_types, return_type \
                 FROM duckdb_functions() WHERE {where_sql} ORDER BY function_name, function_oid"
            );
            if let Some(page_size) = options.page_size {
                sql.push_str(&format!(" LIMIT {page_size}"));
                let page = options.page.unwrap_or(1).max(1);
                sql.push_str(&format!(" OFFSET {}", (page - 1) * page_size));
            }

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            let rows = stmt
                .query_map(params_from_iter(params.iter()), |row| {
                    let name: String = row.get(0)?;
                    let function_type: String = row.get(1)?;
                    let parameters = row.get_ref(2)?.to_owned();
                    let parameter_types = row.get_ref(3)?.to_owned();
                    let return_type: Option<String> = row.get(4)?;
                    Ok((
                        name,
                        function_type,
                        parameters,
                        parameter_types,
                        return_type,
                    ))
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut routines = Vec::new();
            for row in rows {
                let (name, function_type, parameters, parameter_types, return_type) =
                    row.map_err(|e| EngineError::execution_error(e.to_string()))?;
                let parameters = duckdb_string_list(parameters)?;
                let parameter_types = duckdb_string_list(parameter_types)?;
                routines.push(Routine {
                    namespace: namespace.clone(),
                    name,
                    routine_type: RoutineType::Function,
                    arguments: format_macro_arguments(&parameters, &parameter_types),
                    return_type: if function_type.contains("table") {
                        Some("TABLE".into())
                    } else {
                        return_type
                    },
                    language: Some("SQL macro".into()),
                });
            }

            Ok(RoutineList {
                routines,
                total_count: total_count.max(0) as u32,
            })
        })
        .await
    }

    async fn get_routine_definition(
        &self,
        session: SessionId,
        namespace: &Namespace,
        routine_name: &str,
        routine_type: RoutineType,
        arguments: Option<&str>,
    ) -> EngineResult<RoutineDefinition> {
        if routine_type == RoutineType::Procedure {
            return Err(EngineError::not_supported(
                "DuckDB does not support stored procedures",
            ));
        }

        let duck_session = self.get_session(session).await?;
        let namespace = namespace.clone();
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());
        let routine_name = routine_name.to_string();
        let requested_arguments = arguments.map(str::to_string);

        Self::with_conn(&duck_session, move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT function_type, parameters, parameter_types, return_type, \
                            macro_definition \
                     FROM duckdb_functions() \
                     WHERE schema_name = ?1 AND function_name = ?2 \
                       AND internal = false AND macro_definition IS NOT NULL \
                     ORDER BY function_oid",
                )
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            let rows = stmt
                .query_map([&schema_name, &routine_name], |row| {
                    let function_type: String = row.get(0)?;
                    let parameters = row.get_ref(1)?.to_owned();
                    let parameter_types = row.get_ref(2)?.to_owned();
                    let return_type: Option<String> = row.get(3)?;
                    let definition: String = row.get(4)?;
                    Ok((
                        function_type,
                        parameters,
                        parameter_types,
                        return_type,
                        definition,
                    ))
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut candidates = Vec::new();
            for row in rows {
                let (function_type, parameters, parameter_types, return_type, definition) =
                    row.map_err(|e| EngineError::execution_error(e.to_string()))?;
                candidates.push((
                    function_type,
                    duckdb_string_list(parameters)?,
                    duckdb_string_list(parameter_types)?,
                    return_type,
                    definition,
                ));
            }
            let candidate = candidates
                .into_iter()
                .find(|(_, parameters, parameter_types, _, _)| {
                    requested_arguments.as_ref().is_none_or(|requested| {
                        format_macro_arguments(parameters, parameter_types) == *requested
                    })
                })
                .ok_or_else(|| {
                    EngineError::execution_error(format!(
                        "DuckDB macro {}.{} was not found",
                        schema_name, routine_name
                    ))
                })?;
            let (function_type, parameters, parameter_types, return_type, body) = candidate;
            let rendered_arguments = format_macro_arguments(&parameters, &parameter_types);
            let table_keyword = if function_type.contains("table") {
                " TABLE"
            } else {
                ""
            };
            let definition = format!(
                "CREATE MACRO {}.{}({}) AS{} {};",
                Self::quote_ident(&schema_name),
                Self::quote_ident(&routine_name),
                rendered_arguments,
                table_keyword,
                body
            );

            Ok(RoutineDefinition {
                name: routine_name,
                namespace,
                routine_type: RoutineType::Function,
                definition,
                language: Some("SQL macro".into()),
                arguments: rendered_arguments,
                return_type: if function_type.contains("table") {
                    Some("TABLE".into())
                } else {
                    return_type
                },
            })
        })
        .await
    }

    async fn drop_routine(
        &self,
        session: SessionId,
        namespace: &Namespace,
        routine_name: &str,
        routine_type: RoutineType,
        _arguments: Option<&str>,
    ) -> EngineResult<RoutineOperationResult> {
        if routine_type == RoutineType::Procedure {
            return Err(EngineError::not_supported(
                "DuckDB does not support stored procedures",
            ));
        }
        let duck_session = self.get_session(session).await?;
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());
        let routine_name = routine_name.to_string();

        Self::with_conn(&duck_session, move |conn| {
            let sql = format!(
                "DROP MACRO {}.{}",
                Self::quote_ident(&schema_name),
                Self::quote_ident(&routine_name)
            );
            let start = Instant::now();
            conn.execute(&sql, [])
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            Ok(RoutineOperationResult {
                success: true,
                executed_command: sql,
                message: Some("DuckDB macro dropped successfully".into()),
                execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
            })
        })
        .await
    }

    fn supports_sequences(&self) -> bool {
        true
    }

    async fn list_sequences(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: SequenceListOptions,
    ) -> EngineResult<SequenceList> {
        let duck_session = self.get_session(session).await?;
        let namespace = namespace.clone();
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());

        Self::with_conn(&duck_session, move |conn| {
            let search = options.search.as_ref().map(|value| format!("%{value}%"));
            let mut where_sql = String::from("schema_name = ?1");
            let mut params = vec![schema_name];
            if let Some(pattern) = &search {
                where_sql.push_str(" AND sequence_name ILIKE ?2");
                params.push(pattern.clone());
            }
            let total_count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM duckdb_sequences() WHERE {where_sql}"),
                    params_from_iter(params.iter()),
                    |row| row.get(0),
                )
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut sql = format!(
                "SELECT sequence_name, start_value, min_value, max_value, increment_by, cycle \
                 FROM duckdb_sequences() WHERE {where_sql} ORDER BY sequence_name"
            );
            if let Some(page_size) = options.page_size {
                sql.push_str(&format!(" LIMIT {page_size}"));
                let page = options.page.unwrap_or(1).max(1);
                sql.push_str(&format!(" OFFSET {}", (page - 1) * page_size));
            }
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            let rows = stmt
                .query_map(params_from_iter(params.iter()), |row| {
                    Ok(Sequence {
                        namespace: namespace.clone(),
                        name: row.get(0)?,
                        data_type: "BIGINT".into(),
                        start_value: row.get(1)?,
                        min_value: row.get(2)?,
                        max_value: row.get(3)?,
                        increment: row.get(4)?,
                        cycle: row.get(5)?,
                        // DuckDB sequences do not expose or support a CACHE option.
                        cache_size: 0,
                    })
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            let mut sequences = Vec::new();
            for row in rows {
                sequences.push(row.map_err(|e| EngineError::execution_error(e.to_string()))?);
            }
            Ok(SequenceList {
                sequences,
                total_count: total_count.max(0) as u32,
            })
        })
        .await
    }

    async fn get_sequence_definition(
        &self,
        session: SessionId,
        namespace: &Namespace,
        sequence_name: &str,
    ) -> EngineResult<SequenceDefinition> {
        let duck_session = self.get_session(session).await?;
        let namespace = namespace.clone();
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());
        let sequence_name = sequence_name.to_string();

        Self::with_conn(&duck_session, move |conn| {
            let definition: String = conn
                .query_row(
                    "SELECT sql FROM duckdb_sequences() \
                     WHERE schema_name = ?1 AND sequence_name = ?2",
                    [&schema_name, &sequence_name],
                    |row| row.get(0),
                )
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            Ok(SequenceDefinition {
                name: sequence_name,
                namespace,
                definition,
            })
        })
        .await
    }

    async fn drop_sequence(
        &self,
        session: SessionId,
        namespace: &Namespace,
        sequence_name: &str,
    ) -> EngineResult<SequenceOperationResult> {
        let duck_session = self.get_session(session).await?;
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());
        let sequence_name = sequence_name.to_string();

        Self::with_conn(&duck_session, move |conn| {
            let sql = format!(
                "DROP SEQUENCE {}.{}",
                Self::quote_ident(&schema_name),
                Self::quote_ident(&sequence_name)
            );
            let start = Instant::now();
            conn.execute(&sql, [])
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            Ok(SequenceOperationResult {
                success: true,
                executed_command: sql,
                message: Some("DuckDB sequence dropped successfully".into()),
                execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
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
                    is_auto_increment: default_value
                        .as_deref()
                        .is_some_and(|value| value.to_ascii_lowercase().contains("nextval(")),
                    default_value,
                    is_primary_key: false,
                });
            }

            if columns.is_empty() {
                return Err(EngineError::execution_error(format!(
                    "DuckDB table or view {}.{} was not found",
                    schema_name, table
                )));
            }

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

            for col in &mut columns {
                if pk_columns.contains(&col.name) {
                    col.is_primary_key = true;
                }
            }

            let mut foreign_keys: Vec<ForeignKey> = Vec::new();
            if let Ok(mut fk_stmt) = conn.prepare(
                "SELECT \
                     unnest(constraint_column_names) as from_col, \
                     unnest(referenced_column_names) as ref_col, \
                     referenced_table, \
                     constraint_name \
                 FROM duckdb_constraints() \
                 WHERE schema_name = ?1 AND table_name = ?2 \
                 AND constraint_type = 'FOREIGN KEY'",
            ) {
                if let Ok(fk_rows) = fk_stmt.query_map([&schema_name, &table], |row| {
                    let from_col: String = row.get(0)?;
                    let ref_col: String = row.get(1)?;
                    let ref_table: String = row.get(2)?;
                    let constraint_name: Option<String> = row.get(3)?;
                    Ok((from_col, ref_col, ref_table, constraint_name))
                }) {
                    for row in fk_rows {
                        if let Ok((from_col, ref_col, ref_table, constraint_name)) = row {
                            foreign_keys.push(ForeignKey {
                                column: from_col,
                                referenced_table: ref_table,
                                referenced_column: ref_col,
                                referenced_schema: Some(schema_name.clone()),
                                referenced_database: None,
                                constraint_name,
                                is_virtual: false,
                            });
                        }
                    }
                }
            }

            let mut indexes: Vec<TableIndex> = Vec::new();
            if let Ok(mut constraint_stmt) = conn.prepare(
                "SELECT constraint_name, constraint_type, constraint_column_names \
                 FROM duckdb_constraints() \
                 WHERE schema_name = ?1 AND table_name = ?2 \
                   AND constraint_type IN ('PRIMARY KEY', 'UNIQUE')",
            ) {
                if let Ok(constraint_rows) =
                    constraint_stmt.query_map([&schema_name, &table], |row| {
                        let name: String = row.get(0)?;
                        let constraint_type: String = row.get(1)?;
                        let columns = row.get_ref(2)?.to_owned();
                        Ok((name, constraint_type, columns))
                    })
                {
                    for row in constraint_rows.flatten() {
                        let columns = duckdb_string_list(row.2)?;
                        indexes.push(TableIndex {
                            name: row.0,
                            columns,
                            is_unique: true,
                            is_primary: row.1 == "PRIMARY KEY",
                            index_type: Some("ART".into()),
                        });
                    }
                }
            }
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
                            // duckdb_indexes() doesn't expose the column list directly — parse CREATE INDEX.
                            let idx_columns = extract_index_columns(sql.as_deref());
                            indexes.push(TableIndex {
                                name,
                                columns: idx_columns,
                                is_unique,
                                is_primary: false,
                                index_type: Some("ART".into()),
                            });
                        }
                    }
                }
            }

            let row_count_estimate = conn
                .query_row(
                    "SELECT estimated_size FROM duckdb_tables() \
                     WHERE schema_name = ?1 AND table_name = ?2",
                    [&schema_name, &table],
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
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        // Block INSTALL / LOAD / ATTACH / COPY ... TO / PRAGMA
        // enable_external_access. Without this filter, a single statement in
        // the editor can install `httpfs` and then exfiltrate data over HTTP
        // or write to arbitrary local paths (cf. audit B4-C3).
        if let Some(danger) = safety::classify_duckdb_script_dangerous(query) {
            return Err(EngineError::not_supported(danger.reason()));
        }
        let duck_session = self.get_session(session).await?;
        let query = query.to_string();
        let returns_rows = safety::split_sql_statements("duckdb", &query)
            .ok()
            .and_then(|statements| statements.last().cloned())
            .map(|statement| {
                safety::returns_rows("duckdb", &statement)
                    .unwrap_or_else(|_| safety::is_select_prefix(&statement))
            })
            .unwrap_or_else(|| safety::is_select_prefix(&query));

        Self::with_query_conn(&duck_session, query_id, move |conn| {
            if let Some(ns) = &namespace {
                let schema = ns.schema.as_deref().unwrap_or(&ns.database);
                conn.execute(
                    &format!("SET schema = '{}'", schema.replace('\'', "''")),
                    [],
                )
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
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        if let Some(danger) = safety::classify_duckdb_script_dangerous(query) {
            return Err(EngineError::not_supported(danger.reason()));
        }
        let duck_session = self.get_session(session).await?;
        let query = query.to_string();

        let returns_rows = safety::split_sql_statements("duckdb", &query)
            .ok()
            .and_then(|statements| statements.last().cloned())
            .map(|statement| {
                safety::returns_rows("duckdb", &statement)
                    .unwrap_or_else(|_| safety::is_select_prefix(&statement))
            })
            .unwrap_or_else(|| safety::is_select_prefix(&query));

        if !returns_rows {
            let result = self
                .execute_in_namespace(session, namespace, &query, query_id)
                .await?;
            let _ = sender
                .send(StreamEvent::Done(result.affected_rows.unwrap_or(0)))
                .await;
            return Ok(());
        }

        Self::with_query_conn(&duck_session, query_id, move |conn| {
            if let Some(ns) = &namespace {
                let schema = ns.schema.as_deref().unwrap_or(&ns.database);
                conn.execute(
                    &format!("SET schema = '{}'", schema.replace('\'', "''")),
                    [],
                )
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            }

            let mut stmt = conn
                .prepare(&query)
                .map_err(|e| classify_error(e.to_string()))?;

            let mut rows = stmt.query([]).map_err(|e| classify_error(e.to_string()))?;
            let columns = rows
                .as_ref()
                .map(column_info)
                .ok_or_else(|| EngineError::execution_error("DuckDB returned no statement"))?;

            if sender.blocking_send(StreamEvent::Columns(columns)).is_err() {
                return Ok(());
            }

            let mut row_count = 0_u64;
            let mut batch = Vec::with_capacity(500);
            while let Some(row) = rows
                .next()
                .map_err(|e| EngineError::execution_error(e.to_string()))?
            {
                let col_count = row.as_ref().column_count();
                let values = (0..col_count)
                    .map(|idx| duckdb_value_to_qoredb(row, idx))
                    .collect::<::duckdb::Result<Vec<_>>>()
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;
                batch.push(QRow { values });
                row_count += 1;
                if batch.len() >= 500 {
                    if sender
                        .blocking_send(StreamEvent::RowBatch(std::mem::replace(
                            &mut batch,
                            Vec::with_capacity(500),
                        )))
                        .is_err()
                    {
                        return Ok(());
                    }
                }
            }
            if !batch.is_empty() {
                let _ = sender.blocking_send(StreamEvent::RowBatch(batch));
            }

            let _ = sender.blocking_send(StreamEvent::Done(row_count));
            Ok(())
        })
        .await
    }

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
        let fetch_size = options.fetch_size();
        let offset = options.offset();

        Self::with_conn(&duck_session, move |conn| {
            let start = Instant::now();
            let table_ref = format!(
                "{}.{}",
                Self::quote_ident(&schema_name),
                Self::quote_ident(&table)
            );

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
                            // CAST to VARCHAR so substring search works on every
                            // column type (numbers, booleans, dates…), not just text.
                            bind_values.push(value_to_duckdb(&filter.value));
                            format!("CAST({} AS VARCHAR) ILIKE ?", col_ident)
                        }
                        FilterOperator::IsNull => format!("{} IS NULL", col_ident),
                        FilterOperator::IsNotNull => format!("{} IS NOT NULL", col_ident),
                        FilterOperator::Regex => {
                            filter.value.as_text().ok_or_else(|| {
                                EngineError::syntax_error(
                                    "regex operator requires a string value in 'value'",
                                )
                            })?;
                            bind_values.push(value_to_duckdb(&filter.value));
                            // sanitized_regex_flags restricts flags to `imxs` so the literal is safe to interpolate.
                            let flags_lit = filter.options.sanitized_regex_flags();
                            if flags_lit.is_empty() {
                                format!("regexp_matches({}::VARCHAR, ?)", col_ident)
                            } else {
                                format!(
                                    "regexp_matches({}::VARCHAR, ?, '{}')",
                                    col_ident, flags_lit
                                )
                            }
                        }
                        FilterOperator::Text => {
                            // DuckDB has no full-text index; fall back to a case-insensitive substring match.
                            // The filter bar UI warns on absence of a text index.
                            let term = filter.value.as_text().ok_or_else(|| {
                                EngineError::syntax_error(
                                    "text operator requires a string value in 'value'",
                                )
                            })?;
                            bind_values.push(value_to_duckdb(&qore_core::types::Value::Text(
                                format!("%{}%", term),
                            )));
                            format!("{}::VARCHAR ILIKE ?", col_ident)
                        }
                    };
                    where_clauses.push(clause);
                }
            }

            if let Some(ref search_term) = options.search {
                if !search_term.trim().is_empty() {
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
                                    if upper.contains("BLOB") {
                                        continue;
                                    }

                                    let col_ident = Self::quote_ident(&col_name);
                                    bind_values.push(DuckValue::Text(format!("%{}%", search_term)));

                                    if upper.contains("VARCHAR")
                                        || upper.contains("TEXT")
                                        || upper.contains("CHAR")
                                    {
                                        search_clauses.push(format!("{} ILIKE ?", col_ident));
                                    } else {
                                        search_clauses.push(format!(
                                            "CAST({} AS VARCHAR) ILIKE ?",
                                            col_ident
                                        ));
                                    }
                                }
                            }
                            if !search_clauses.is_empty() {
                                where_clauses.push(format!("({})", search_clauses.join(" OR ")));
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

            let total_rows = if options.wants_exact_total() {
                let count_sql = format!("SELECT COUNT(*) AS cnt FROM {}{}", table_ref, where_sql);
                let total_rows: i64 = conn
                    .query_row(&count_sql, params_from_iter(bind_values.iter()), |row| {
                        row.get(0)
                    })
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;
                Some(total_rows.max(0) as u64)
            } else {
                None
            };

            let data_sql = format!(
                "SELECT * FROM {}{}{} LIMIT {} OFFSET {}",
                table_ref, where_sql, order_sql, fetch_size, offset
            );

            let mut stmt = conn
                .prepare(&data_sql)
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            // duckdb crate: column_count/column_name panic before execution — collect first.
            let rows_iter = stmt
                .query_map(params_from_iter(bind_values.iter()), |row| {
                    let col_count = row.as_ref().column_count();
                    let values = (0..col_count)
                        .map(|i| duckdb_value_to_qoredb(row, i))
                        .collect::<::duckdb::Result<Vec<_>>>()?;
                    Ok(QRow { values })
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut rows = Vec::new();
            for row_result in rows_iter {
                let row = row_result.map_err(|e| EngineError::execution_error(e.to_string()))?;
                rows.push(row);
            }

            let columns = column_info(&stmt);

            let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

            let result = QueryResult {
                columns,
                rows,
                affected_rows: None,
                execution_time_ms,
            };

            Ok(PaginatedQueryResult::from_optional_total(
                result, total_rows, page, page_size,
            ))
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
        let schema_name = foreign_key.referenced_schema.clone().unwrap_or_else(|| {
            namespace
                .schema
                .clone()
                .unwrap_or_else(|| namespace.database.clone())
        });
        let ref_table = foreign_key.referenced_table.clone();
        let ref_column = foreign_key.referenced_column.clone();
        let duck_value = value_to_duckdb(value);

        Self::with_conn(&duck_session, move |conn| {
            let sql = format!(
                "SELECT * FROM {}.{} WHERE {} IS NOT DISTINCT FROM ?1 LIMIT {}",
                Self::quote_ident(&schema_name),
                Self::quote_ident(&ref_table),
                Self::quote_ident(&ref_column),
                limit
            );

            let start = Instant::now();
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            // duckdb crate: column_count/column_name panic before execution — collect first.
            let rows_iter = stmt
                .query_map([&duck_value], |row| {
                    let col_count = row.as_ref().column_count();
                    let values = (0..col_count)
                        .map(|i| duckdb_value_to_qoredb(row, i))
                        .collect::<::duckdb::Result<Vec<_>>>()?;
                    Ok(QRow { values })
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            let mut rows = Vec::new();
            for row_result in rows_iter {
                let row = row_result.map_err(|e| EngineError::execution_error(e.to_string()))?;
                rows.push(row);
            }

            let columns = column_info(&stmt);

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

    async fn begin_transaction(&self, session: SessionId) -> EngineResult<()> {
        let duck_session = self.get_session(session).await?;

        if duck_session.transaction_active.load(Ordering::Acquire) {
            return Err(EngineError::transaction_error(
                "A transaction is already active on this session",
            ));
        }

        Self::with_conn(&duck_session, |conn| {
            conn.execute("BEGIN TRANSACTION", []).map_err(|e| {
                EngineError::execution_error(format!("Failed to begin transaction: {e}"))
            })?;
            Ok(())
        })
        .await?;

        duck_session
            .transaction_active
            .store(true, Ordering::Release);
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
            conn.execute("COMMIT", []).map_err(|e| {
                EngineError::execution_error(format!("Failed to commit transaction: {e}"))
            })?;
            Ok(())
        })
        .await?;

        duck_session
            .transaction_active
            .store(false, Ordering::Release);
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
            conn.execute("ROLLBACK", []).map_err(|e| {
                EngineError::execution_error(format!("Failed to rollback transaction: {e}"))
            })?;
            Ok(())
        })
        .await?;

        duck_session
            .transaction_active
            .store(false, Ordering::Release);
        Ok(())
    }

    fn supports_transactions(&self) -> bool {
        true
    }

    fn supports_ssh(&self) -> bool {
        // DuckDB is embedded and opens a local file directly; there is no
        // remote socket for QoreDB's SSH tunnel layer to forward.
        false
    }

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
                .map(|(i, k)| {
                    format!(
                        "{} IS NOT DISTINCT FROM ?{}",
                        Self::quote_ident(k),
                        data_keys.len() + i + 1
                    )
                })
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
                .map(|(i, k)| format!("{} IS NOT DISTINCT FROM ?{}", Self::quote_ident(k), i + 1))
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

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_explain(&self) -> bool {
        true
    }

    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        let duck_session = self.get_session(session).await?;
        let active = duck_session.active_query.lock().map_err(|e| {
            EngineError::internal(format!("Failed to inspect active DuckDB query: {e}"))
        })?;
        let should_interrupt = match (query_id, *active) {
            (None, Some(_)) => true,
            (Some(requested), Some(active)) => requested == active,
            _ => false,
        };
        drop(active);

        if should_interrupt {
            duck_session.interrupt.interrupt();
        }
        Ok(())
    }

    fn cancel_support(&self) -> CancelSupport {
        CancelSupport::Driver
    }

    fn supports_maintenance(&self) -> bool {
        true
    }

    async fn list_maintenance_operations(
        &self,
        _session: SessionId,
        _namespace: &Namespace,
        _table: &str,
    ) -> EngineResult<Vec<MaintenanceOperationInfo>> {
        Ok(vec![MaintenanceOperationInfo {
            operation: MaintenanceOperationType::Analyze,
            is_heavy: false,
            has_options: false,
        }])
    }

    async fn run_maintenance(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        request: &MaintenanceRequest,
    ) -> EngineResult<MaintenanceResult> {
        let duckdb_session = self.get_session(session).await?;
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());
        let table = table.to_string();

        let sql = match request.operation {
            MaintenanceOperationType::Analyze => format!(
                "ANALYZE {}.{}",
                Self::quote_ident(&schema_name),
                Self::quote_ident(&table)
            ),
            _ => {
                return Err(EngineError::not_supported(
                    "Operation not supported for DuckDB",
                ));
            }
        };

        Self::with_conn(&duckdb_session, move |conn| {
            let start = Instant::now();
            conn.execute_batch(&sql)
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            Ok(MaintenanceResult {
                executed_command: sql,
                messages: vec![MaintenanceMessage {
                    level: MaintenanceMessageLevel::Info,
                    text: "Operation completed successfully".into(),
                }],
                execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
                success: true,
            })
        })
        .await
    }

    fn supports_truncate_all(&self) -> bool {
        true
    }

    async fn truncate_all(
        &self,
        session: SessionId,
        namespace: &Namespace,
    ) -> EngineResult<TruncateAllResult> {
        let duckdb_session = self.get_session(session).await?;
        if duckdb_session.transaction_active.load(Ordering::Acquire) {
            return Err(EngineError::transaction_error(
                "Cannot truncate all DuckDB tables inside an active transaction",
            ));
        }
        let schema_name = namespace
            .schema
            .clone()
            .unwrap_or_else(|| namespace.database.clone());

        Self::with_conn(&duckdb_session, move |conn| {
            let mut table_stmt = conn
                .prepare(
                    "SELECT table_name FROM information_schema.tables \
                     WHERE table_schema = ?1 AND table_type = 'BASE TABLE' \
                     ORDER BY table_name",
                )
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            let table_rows = table_stmt
                .query_map([&schema_name], |row| row.get::<_, String>(0))
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            let mut tables = Vec::new();
            for row in table_rows {
                tables.push(row.map_err(|e| EngineError::execution_error(e.to_string()))?);
            }

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

            // Delete referencing tables before referenced tables so DuckDB's
            // immediate foreign-key enforcement remains satisfied.
            let mut edges: HashMap<String, Vec<String>> = HashMap::new();
            let mut indegree: HashMap<String, usize> =
                tables.iter().cloned().map(|table| (table, 0)).collect();
            let mut fk_stmt = conn
                .prepare(
                    "SELECT table_name, referenced_table FROM duckdb_constraints() \
                     WHERE schema_name = ?1 AND constraint_type = 'FOREIGN KEY'",
                )
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            let fk_rows = fk_stmt
                .query_map([&schema_name], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| EngineError::execution_error(e.to_string()))?;
            for row in fk_rows {
                let (child, parent) =
                    row.map_err(|e| EngineError::execution_error(e.to_string()))?;
                if child != parent
                    && indegree.contains_key(&child)
                    && indegree.contains_key(&parent)
                {
                    edges.entry(child).or_default().push(parent.clone());
                    *indegree.entry(parent).or_default() += 1;
                }
            }
            let has_foreign_keys = !edges.is_empty();

            let mut ready: Vec<String> = indegree
                .iter()
                .filter(|(_, degree)| **degree == 0)
                .map(|(table, _)| table.clone())
                .collect();
            ready.sort_by(|a, b| b.cmp(a));
            let mut ordered = Vec::with_capacity(tables.len());
            while let Some(table) = ready.pop() {
                ordered.push(table.clone());
                if let Some(parents) = edges.get(&table) {
                    for parent in parents {
                        if let Some(degree) = indegree.get_mut(parent) {
                            *degree -= 1;
                            if *degree == 0 {
                                ready.push(parent.clone());
                                ready.sort_by(|a, b| b.cmp(a));
                            }
                        }
                    }
                }
            }
            // Cyclic foreign keys are uncommon in DuckDB; keep deterministic
            // behavior and let DuckDB return the precise constraint error.
            for table in &tables {
                if !ordered.contains(table) {
                    ordered.push(table.clone());
                }
            }

            let start = Instant::now();
            // DuckDB keeps deleted FK references visible until commit. As a
            // result, deleting child then parent in one transaction still
            // fails. Use ordered autocommit only for FK graphs; schemas with
            // no FKs retain all-or-nothing transactional behavior.
            if !has_foreign_keys {
                conn.execute_batch("BEGIN TRANSACTION")
                    .map_err(|e| EngineError::transaction_error(e.to_string()))?;
            }
            let mut executed = Vec::with_capacity(ordered.len());
            for table in &ordered {
                let sql = format!(
                    "TRUNCATE {}.{}",
                    Self::quote_ident(&schema_name),
                    Self::quote_ident(table)
                );
                if let Err(error) = conn.execute(&sql, []) {
                    if !has_foreign_keys {
                        let _ = conn.execute_batch("ROLLBACK");
                    }
                    return Err(EngineError::execution_error(error.to_string()));
                }
                executed.push(sql);
            }
            if !has_foreign_keys {
                if let Err(error) = conn.execute_batch("COMMIT") {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(EngineError::transaction_error(error.to_string()));
                }
            }

            Ok(TruncateAllResult {
                executed_command: executed.join(";\n"),
                truncated_tables: ordered,
                messages: vec![if has_foreign_keys {
                    MaintenanceMessage {
                        level: MaintenanceMessageLevel::Warning,
                        text: format!(
                            "Truncated {} DuckDB tables in foreign-key dependency order; \
                             DuckDB does not allow this operation in one transaction",
                            tables.len()
                        ),
                    }
                } else {
                    MaintenanceMessage {
                        level: MaintenanceMessageLevel::Info,
                        text: format!("Truncated {} DuckDB tables", tables.len()),
                    }
                }],
                execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
                success: true,
            })
        })
        .await
    }
}

fn format_macro_arguments(parameters: &[String], parameter_types: &[String]) -> String {
    parameters
        .iter()
        .enumerate()
        .map(|(idx, parameter)| {
            let data_type = parameter_types
                .get(idx)
                .map(String::as_str)
                .unwrap_or("ANY");
            if data_type.eq_ignore_ascii_case("ANY") || data_type.eq_ignore_ascii_case("UNKNOWN") {
                parameter.clone()
            } else {
                format!("{parameter} {data_type}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Extracts column names from a CREATE INDEX SQL statement.
fn extract_index_columns(sql: Option<&str>) -> Vec<String> {
    let Some(sql) = sql else {
        return Vec::new();
    };

    // Match the `... ON table (col1, col2, ...)` tail of a CREATE INDEX statement.
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

#[cfg(test)]
mod tests {
    use super::*;
    use qore_core::types::{ColumnFilter, CountMode, FilterOptions};
    use std::path::Path;
    use tokio::sync::mpsc;

    fn test_config(path: impl AsRef<Path>, read_only: bool) -> ConnectionConfig {
        ConnectionConfig {
            driver: "duckdb".to_string(),
            host: path.as_ref().to_string_lossy().into_owned(),
            port: 0,
            username: String::new(),
            password: String::new(),
            database: None,
            ssl: false,
            ssl_mode: None,
            environment: "development".to_string(),
            read_only,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
        }
    }

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
            ssl_mode: None,
            environment: "development".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
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
            ssl_mode: None,
            environment: "development".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
        };

        let session_id = driver.connect(&config).await.unwrap();

        let result = driver
            .execute(
                session_id,
                "CREATE TABLE test (id INTEGER PRIMARY KEY, name VARCHAR, value DOUBLE)",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert!(result.affected_rows.is_some());

        let result = driver
            .execute(
                session_id,
                "INSERT INTO test VALUES (1, 'hello', 3.14)",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert_eq!(result.affected_rows, Some(1));

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
            ssl_mode: None,
            environment: "development".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
        };

        let session_id = driver.connect(&config).await.unwrap();

        let namespaces = driver.list_namespaces(session_id).await.unwrap();
        // DuckDB always exposes the built-in 'main' schema.
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
            ssl_mode: None,
            environment: "development".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
        };

        let session_id = driver.connect(&config).await.unwrap();

        driver
            .execute(
                session_id,
                "CREATE TABLE test (id INTEGER PRIMARY KEY, name VARCHAR)",
                QueryId::new(),
            )
            .await
            .unwrap();

        driver.begin_transaction(session_id).await.unwrap();

        driver
            .execute(
                session_id,
                "INSERT INTO test VALUES (1, 'tx_test')",
                QueryId::new(),
            )
            .await
            .unwrap();

        driver.rollback(session_id).await.unwrap();

        let result = driver
            .execute(session_id, "SELECT * FROM test", QueryId::new())
            .await
            .unwrap();
        assert_eq!(result.rows.len(), 0);

        driver.disconnect(session_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_advanced_types_and_column_metadata() {
        let driver = DuckDbDriver::new();
        let session = driver
            .connect(&test_config(":memory:", false))
            .await
            .unwrap();

        let result = driver
            .execute(
                session,
                "SELECT \
                    12::TINYINT AS tiny, \
                    18446744073709551615::UBIGINT AS ubig, \
                    123456789012345678901234567890::HUGEINT AS huge, \
                    123.450::DECIMAL(10, 3) AS decimal_value, \
                    DATE '2024-02-29' AS date_value, \
                    TIME '12:34:56.123456' AS time_value, \
                    TIMESTAMP '2024-02-29 12:34:56.123456' AS timestamp_value, \
                    INTERVAL '2 months 3 days 4 microseconds' AS interval_value, \
                    [1, NULL, 3] AS list_value, \
                    {'name': 'duck', 'score': 42} AS struct_value, \
                    MAP {'a': 1, 'b': 2} AS map_value",
                QueryId::new(),
            )
            .await
            .unwrap();

        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.columns.len(), 11);
        assert_eq!(result.columns[0].data_type.as_str(), "TINYINT");
        assert_eq!(result.columns[1].data_type.as_str(), "UBIGINT");

        let values = &result.rows[0].values;
        assert!(matches!(values[0], Value::Int(12)));
        assert!(matches!(&values[1], Value::Text(value) if value == "18446744073709551615"));
        assert!(
            matches!(&values[2], Value::Text(value) if value == "123456789012345678901234567890")
        );
        assert!(matches!(&values[3], Value::Text(value) if value == "123.450"));
        assert!(matches!(&values[4], Value::Text(value) if value == "2024-02-29"));
        assert!(matches!(&values[8], Value::Array(value) if value.len() == 3));
        assert!(matches!(&values[9], Value::Json(value) if value["name"] == "duck"));
        assert!(matches!(&values[10], Value::Array(value) if value.len() == 2));
    }

    #[tokio::test]
    async fn test_multi_statement_script_returns_final_result_and_checks_every_statement() {
        let driver = DuckDbDriver::new();
        let session = driver
            .connect(&test_config(":memory:", false))
            .await
            .unwrap();

        let result = driver
            .execute(
                session,
                "CREATE TABLE scripted(id INTEGER); \
                 INSERT INTO scripted VALUES (1), (2); \
                 SELECT sum(id) AS total FROM scripted",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert!(matches!(result.rows[0].values[0], Value::Int(3)));

        let error = driver
            .execute(
                session,
                "SELECT ';' AS harmless; INSTALL httpfs",
                QueryId::new(),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("INSTALL is blocked"));
    }

    #[tokio::test]
    async fn test_dml_returning_call_and_pragma_results_are_preserved() {
        let driver = DuckDbDriver::new();
        let session = driver
            .connect(&test_config(":memory:", false))
            .await
            .unwrap();

        driver
            .execute(
                session,
                "CREATE TABLE returning_values(id INTEGER PRIMARY KEY, value VARCHAR)",
                QueryId::new(),
            )
            .await
            .unwrap();

        let inserted = driver
            .execute(
                session,
                "INSERT INTO returning_values VALUES (1, 'first') RETURNING id, value",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert_eq!(inserted.rows.len(), 1);
        assert!(matches!(inserted.rows[0].values[0], Value::Int(1)));

        let updated = driver
            .execute(
                session,
                "UPDATE returning_values SET value = 'updated' RETURNING value",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert!(matches!(&updated.rows[0].values[0], Value::Text(value) if value == "updated"));

        let deleted = driver
            .execute(
                session,
                "DELETE FROM returning_values RETURNING id",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert!(matches!(deleted.rows[0].values[0], Value::Int(1)));

        let call = driver
            .execute(session, "CALL pragma_version()", QueryId::new())
            .await
            .unwrap();
        assert!(!call.columns.is_empty());
        assert!(!call.rows.is_empty());

        let pragma = driver
            .execute(session, "PRAGMA database_size", QueryId::new())
            .await
            .unwrap();
        assert!(!pragma.columns.is_empty());
        assert!(!pragma.rows.is_empty());
    }

    #[tokio::test]
    async fn test_table_editor_crud_filters_search_and_array_values() {
        let driver = DuckDbDriver::new();
        let session = driver
            .connect(&test_config(":memory:", false))
            .await
            .unwrap();
        let namespace = Namespace::new("main");
        driver
            .execute(
                session,
                "CREATE TABLE editor_rows(\
                    id INTEGER PRIMARY KEY, \
                    name VARCHAR, \
                    score DOUBLE, \
                    tags INTEGER[]\
                )",
                QueryId::new(),
            )
            .await
            .unwrap();

        let inserted = driver
            .insert_row(
                session,
                &namespace,
                "editor_rows",
                &RowData::new()
                    .with_column("id", Value::Int(1))
                    .with_column("name", Value::Text("Alpha".into()))
                    .with_column("score", Value::Float(12.5))
                    .with_column(
                        "tags",
                        Value::Array(vec![Value::Int(2), Value::Int(4), Value::Int(8)]),
                    ),
            )
            .await
            .unwrap();
        assert_eq!(inserted.affected_rows, Some(1));

        driver
            .insert_row(
                session,
                &namespace,
                "editor_rows",
                &RowData::new()
                    .with_column("id", Value::Int(2))
                    .with_column("name", Value::Text("Beta".into()))
                    .with_column("score", Value::Float(4.0))
                    .with_column("tags", Value::Array(vec![Value::Int(1)])),
            )
            .await
            .unwrap();

        let browse_page = driver
            .query_table(
                session,
                &namespace,
                "editor_rows",
                TableQueryOptions {
                    page: Some(1),
                    page_size: Some(1),
                    sort_column: Some("score".into()),
                    sort_direction: Some(SortDirection::Desc),
                    count_mode: Some(CountMode::None),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(browse_page.total_rows, 2);
        assert_eq!(browse_page.result.rows.len(), 1);
        assert!(!browse_page.total_rows_exact);
        assert!(browse_page.has_more);

        let page = driver
            .query_table(
                session,
                &namespace,
                "editor_rows",
                TableQueryOptions {
                    page: Some(1),
                    page_size: Some(1),
                    sort_column: Some("score".into()),
                    sort_direction: Some(SortDirection::Desc),
                    filters: Some(vec![ColumnFilter {
                        column: "score".into(),
                        operator: FilterOperator::Gt,
                        value: Value::Float(10.0),
                        options: FilterOptions::default(),
                    }]),
                    search: Some("alp".into()),
                    count_mode: Some(CountMode::None),
                },
            )
            .await
            .unwrap();
        assert_eq!(page.total_rows, 1);
        assert!(!page.total_rows_exact);
        assert!(!page.has_more);
        assert_eq!(page.result.rows.len(), 1);
        assert!(matches!(&page.result.rows[0].values[1], Value::Text(value) if value == "Alpha"));
        assert!(
            matches!(&page.result.rows[0].values[3], Value::Array(values) if values.len() == 3)
        );

        let updated = driver
            .update_row(
                session,
                &namespace,
                "editor_rows",
                &RowData::new().with_column("id", Value::Int(1)),
                &RowData::new().with_column("name", Value::Text("Gamma".into())),
            )
            .await
            .unwrap();
        assert_eq!(updated.affected_rows, Some(1));

        let deleted = driver
            .delete_row(
                session,
                &namespace,
                "editor_rows",
                &RowData::new().with_column("id", Value::Int(2)),
            )
            .await
            .unwrap();
        assert_eq!(deleted.affected_rows, Some(1));

        let remaining = driver
            .execute(
                session,
                "SELECT name, tags FROM editor_rows",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert_eq!(remaining.rows.len(), 1);
        assert!(matches!(&remaining.rows[0].values[0], Value::Text(value) if value == "Gamma"));
        assert!(!driver.capabilities().supports_ssh);
    }

    #[tokio::test]
    async fn test_streaming_emits_metadata_and_incremental_batches() {
        let driver = DuckDbDriver::new();
        let session = driver
            .connect(&test_config(":memory:", false))
            .await
            .unwrap();
        let (sender, mut receiver) = mpsc::channel(8);

        driver
            .execute_stream(
                session,
                "SELECT i FROM range(1201) AS values(i)",
                QueryId::new(),
                sender,
            )
            .await
            .unwrap();

        let mut saw_columns = false;
        let mut batch_sizes = Vec::new();
        let mut done = None;
        while let Some(event) = receiver.recv().await {
            match event {
                StreamEvent::Columns(columns) => {
                    saw_columns = true;
                    assert_eq!(columns[0].data_type.as_str(), "BIGINT");
                }
                StreamEvent::RowBatch(rows) => batch_sizes.push(rows.len()),
                StreamEvent::Done(rows) => {
                    done = Some(rows);
                    break;
                }
                StreamEvent::Row(_) | StreamEvent::Error(_) => {}
            }
        }
        assert!(saw_columns);
        assert_eq!(batch_sizes, vec![500, 500, 201]);
        assert_eq!(done, Some(1201));
    }

    #[tokio::test]
    async fn test_macros_and_sequences_are_full_schema_objects() {
        let driver = DuckDbDriver::new();
        let session = driver
            .connect(&test_config(":memory:", false))
            .await
            .unwrap();
        let namespace = Namespace::new("main");
        driver
            .execute(
                session,
                "CREATE MACRO main.add_two(value INTEGER) AS value + 2; \
                 CREATE SEQUENCE main.order_ids START 5 INCREMENT 2 MAXVALUE 99 CYCLE",
                QueryId::new(),
            )
            .await
            .unwrap();

        let routines = driver
            .list_routines(session, &namespace, RoutineListOptions::default())
            .await
            .unwrap();
        assert_eq!(routines.total_count, 1);
        assert_eq!(routines.routines[0].name, "add_two");
        let routine = driver
            .get_routine_definition(session, &namespace, "add_two", RoutineType::Function, None)
            .await
            .unwrap();
        assert!(routine.definition.contains("CREATE MACRO"));
        assert!(routine.definition.contains('2'));
        let macro_result = driver
            .execute(session, "SELECT add_two(40)", QueryId::new())
            .await
            .unwrap();
        assert!(matches!(macro_result.rows[0].values[0], Value::Int(42)));

        let sequences = driver
            .list_sequences(session, &namespace, SequenceListOptions::default())
            .await
            .unwrap();
        assert_eq!(sequences.total_count, 1);
        assert_eq!(sequences.sequences[0].start_value, 5);
        assert_eq!(sequences.sequences[0].increment, 2);
        assert!(sequences.sequences[0].cycle);
        let sequence = driver
            .get_sequence_definition(session, &namespace, "order_ids")
            .await
            .unwrap();
        assert!(sequence.definition.contains("CREATE SEQUENCE"));

        driver
            .drop_routine(session, &namespace, "add_two", RoutineType::Function, None)
            .await
            .unwrap();
        driver
            .drop_sequence(session, &namespace, "order_ids")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_read_only_connection_is_enforced_by_duckdb() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("readonly.duckdb");
        let driver = DuckDbDriver::new();

        let write_session = driver.connect(&test_config(&db_path, false)).await.unwrap();
        driver
            .execute(
                write_session,
                "CREATE TABLE values_table(value INTEGER); INSERT INTO values_table VALUES (42)",
                QueryId::new(),
            )
            .await
            .unwrap();
        driver.disconnect(write_session).await.unwrap();

        let read_session = driver.connect(&test_config(&db_path, true)).await.unwrap();
        let result = driver
            .execute(
                read_session,
                "SELECT value FROM values_table",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert!(matches!(result.rows[0].values[0], Value::Int(42)));
        assert!(
            driver
                .execute(
                    read_session,
                    "INSERT INTO values_table VALUES (43)",
                    QueryId::new(),
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_describe_table_and_truncate_all_with_foreign_keys() {
        let driver = DuckDbDriver::new();
        let session = driver
            .connect(&test_config(":memory:", false))
            .await
            .unwrap();
        let namespace = Namespace::new("main");
        driver
            .execute(
                session,
                "CREATE SEQUENCE item_ids; \
                 CREATE TABLE parent( \
                    id BIGINT PRIMARY KEY DEFAULT nextval('item_ids'), \
                    code VARCHAR UNIQUE \
                 ); \
                 CREATE TABLE child( \
                    id INTEGER PRIMARY KEY, \
                    parent_id BIGINT REFERENCES parent(id) \
                 ); \
                 CREATE INDEX child_parent_idx ON child(parent_id); \
                 INSERT INTO parent(code) VALUES ('p'); \
                 INSERT INTO child VALUES (1, 1)",
                QueryId::new(),
            )
            .await
            .unwrap();

        let schema = driver
            .describe_table(session, &namespace, "parent")
            .await
            .unwrap();
        assert_eq!(schema.primary_key.as_deref(), Some(&["id".to_string()][..]));
        assert!(schema.columns[0].is_auto_increment);
        assert!(schema.indexes.iter().any(|index| index.is_primary));
        assert!(schema.indexes.iter().any(|index| index.is_unique));

        let truncated = driver.truncate_all(session, &namespace).await.unwrap();
        assert_eq!(truncated.truncated_tables.len(), 2);
        let counts = driver
            .execute(
                session,
                "SELECT (SELECT count(*) FROM parent), (SELECT count(*) FROM child)",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert!(
            counts.rows[0]
                .values
                .iter()
                .all(|value| matches!(value, Value::Int(0)))
        );
    }

    #[tokio::test]
    async fn test_running_query_can_be_cancelled_and_connection_reused() {
        let driver = Arc::new(DuckDbDriver::new());
        let session = driver
            .connect(&test_config(":memory:", false))
            .await
            .unwrap();
        let query_id = QueryId::new();
        let query_driver = Arc::clone(&driver);
        let task = tokio::spawn(async move {
            query_driver
                .execute(
                    session,
                    "SELECT sum(sin(i)) FROM range(1000000000) AS values(i)",
                    query_id,
                )
                .await
        });

        for _ in 0..100 {
            let active = driver
                .get_session(session)
                .await
                .unwrap()
                .active_query
                .lock()
                .unwrap()
                .is_some();
            if active {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        driver.cancel(session, Some(query_id)).await.unwrap();
        let cancelled = tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .expect("DuckDB cancellation timed out")
            .unwrap();
        assert!(cancelled.is_err());

        let result = driver
            .execute(session, "SELECT 1", QueryId::new())
            .await
            .unwrap();
        assert!(matches!(result.rows[0].values[0], Value::Int(1)));
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

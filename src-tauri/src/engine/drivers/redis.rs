//! Redis Driver
//!
//! Implements the DataEngine trait for Redis using the redis-rs crate.
//! Redis is a key-value store; this driver maps keys as "collections" and
//! displays their contents in type-specific tabular formats.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use futures::future::{AbortHandle, Abortable};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use tokio::sync::{Mutex, RwLock};

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::traits::DataEngine;
use crate::engine::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType, ColumnInfo,
    ConnectionConfig, Namespace, QueryId, QueryResult, Row as QRow, SessionId, TableColumn,
    TableSchema, Value, TableQueryOptions, PaginatedQueryResult,
};

/// Holds a Redis connection and session metadata
pub struct RedisSession {
    pub connection: Mutex<redis::aio::MultiplexedConnection>,
    pub current_db: AtomicU16,
}

/// Redis driver implementation
pub struct RedisDriver {
    sessions: Arc<RwLock<HashMap<SessionId, Arc<RedisSession>>>>,
    active_queries: Arc<Mutex<HashMap<QueryId, (SessionId, AbortHandle)>>>,
}

impl RedisDriver {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            active_queries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Builds a Redis connection URL from config
    fn build_connection_string(config: &ConnectionConfig) -> String {
        let scheme = if config.ssl { "rediss" } else { "redis" };
        let db = config
            .database
            .as_deref()
            .and_then(|d| d.parse::<u16>().ok())
            .unwrap_or(0);

        if !config.username.is_empty() || !config.password.is_empty() {
            let user = if config.username.is_empty() {
                String::new()
            } else {
                Self::encode_userinfo_component(&config.username)
            };
            let password = Self::encode_userinfo_component(&config.password);
            format!(
                "{}://{}:{}@{}:{}/{}",
                scheme, user, password, config.host, config.port, db
            )
        } else {
            format!("{}://{}:{}/{}", scheme, config.host, config.port, db)
        }
    }

    /// Creates a multiplexed connection and pings it
    async fn create_connection_and_ping(
        config: &ConnectionConfig,
    ) -> EngineResult<(redis::aio::MultiplexedConnection, u16)> {
        let conn_str = Self::build_connection_string(config);
        let client = redis::Client::open(conn_str)
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("AUTH") || msg.contains("NOAUTH") || msg.contains("invalid password") {
                    EngineError::auth_failed(msg)
                } else {
                    EngineError::connection_failed(msg)
                }
            })?;

        // PING to verify
        redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .map_err(|e| EngineError::connection_failed(format!("PING failed: {}", e)))?;

        let db = config
            .database
            .as_deref()
            .and_then(|d| d.parse::<u16>().ok())
            .unwrap_or(0);

        Ok((conn, db))
    }

    async fn get_session(&self, session: SessionId) -> EngineResult<Arc<RedisSession>> {
        let sessions = self.sessions.read().await;
        sessions
            .get(&session)
            .cloned()
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))
    }

    fn parse_db_index(database: &str) -> u16 {
        database.trim_start_matches("db").parse().unwrap_or(0)
    }

    fn encode_userinfo_component(value: &str) -> String {
        utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
    }

    async fn select_db(
        conn: &mut redis::aio::MultiplexedConnection,
        db_index: u16,
    ) -> EngineResult<()> {
        redis::cmd("SELECT")
            .arg(db_index)
            .query_async::<String>(conn)
            .await
            .map_err(|e| EngineError::execution_error(format!("SELECT db{}: {}", db_index, e)))?;
        Ok(())
    }

    async fn execute_with_target_db(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
        target_db: Option<u16>,
    ) -> EngineResult<QueryResult> {
        let redis_session = self.get_session(session).await?;
        let redis_session = Arc::clone(&redis_session);

        let (abort_handle, abort_reg) = AbortHandle::new_pair();
        {
            let mut active = self.active_queries.lock().await;
            active.insert(query_id, (session, abort_handle));
        }

        let query = query.to_string();
        let result = Abortable::new(
            async move { Self::execute_with_lock(redis_session, query, target_db).await },
            abort_reg,
        )
        .await;

        {
            let mut active = self.active_queries.lock().await;
            active.remove(&query_id);
        }

        match result {
            Ok(inner) => inner,
            Err(_) => Err(EngineError::Cancelled),
        }
    }

    async fn execute_with_lock(
        redis_session: Arc<RedisSession>,
        query: String,
        target_db: Option<u16>,
    ) -> EngineResult<QueryResult> {
        let start = Instant::now();
        let parts = Self::parse_command(&query)?;

        let cmd_name = parts[0].to_ascii_uppercase();
        let args = &parts[1..];

        let mut conn = redis_session.connection.lock().await;

        if let Some(db_index) = target_db {
            Self::select_db(&mut *conn, db_index).await?;
            redis_session.current_db.store(db_index, Ordering::Relaxed);
        }

        let mut cmd = redis::cmd(&cmd_name);
        for arg in args {
            cmd.arg(arg);
        }

        let value: redis::Value = cmd
            .query_async(&mut *conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        // Track explicit SELECT commands to keep current_db in sync.
        if cmd_name == "SELECT" {
            if let Some(db_str) = args.first() {
                if let Ok(db_index) = db_str.parse::<u16>() {
                    redis_session.current_db.store(db_index, Ordering::Relaxed);
                }
            }
        }

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;
        Ok(Self::format_execute_result(value, execution_time_ms))
    }

    /// Gets the Redis type of a key
    async fn key_type(
        conn: &mut redis::aio::MultiplexedConnection,
        key: &str,
    ) -> EngineResult<String> {
        let type_str: String = redis::cmd("TYPE")
            .arg(key)
            .query_async(conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        Ok(type_str)
    }

    /// Gets the TTL of a key (-1 = no expiry, -2 = key doesn't exist)
    async fn key_ttl(
        conn: &mut redis::aio::MultiplexedConnection,
        key: &str,
    ) -> EngineResult<i64> {
        let ttl: i64 = redis::cmd("TTL")
            .arg(key)
            .query_async(conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
        Ok(ttl)
    }

    /// Reads string key value
    async fn read_string(
        conn: &mut redis::aio::MultiplexedConnection,
        key: &str,
    ) -> EngineResult<QueryResult> {
        let start = Instant::now();
        let value: redis::Value = redis::cmd("GET")
            .arg(key)
            .query_async(conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let columns = vec![ColumnInfo {
            name: "value".to_string(),
            data_type: "string".to_string(),
            nullable: false,
        }];

        let val = Self::redis_value_to_value(&value);

        let rows = vec![QRow {
            values: vec![val],
        }];

        Ok(QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
        })
    }

    /// Reads hash key value
    async fn read_hash(
        conn: &mut redis::aio::MultiplexedConnection,
        key: &str,
    ) -> EngineResult<QueryResult> {
        let start = Instant::now();
        let fields: redis::Value = redis::cmd("HGETALL")
            .arg(key)
            .query_async(conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let columns = vec![
            ColumnInfo {
                name: "field".to_string(),
                data_type: "string".to_string(),
                nullable: false,
            },
            ColumnInfo {
                name: "value".to_string(),
                data_type: "string".to_string(),
                nullable: false,
            },
        ];

        let rows: Vec<QRow> = match fields {
            redis::Value::Array(pairs) => {
                let mut out = Vec::new();
                let mut iter = pairs.iter();
                while let (Some(field), Some(value)) = (iter.next(), iter.next()) {
                    out.push(QRow {
                        values: vec![
                            Value::Text(Self::redis_value_to_string(field)),
                            Self::redis_value_to_value(value),
                        ],
                    });
                }
                out
            }
            redis::Value::Map(pairs) => pairs
                .iter()
                .map(|(field, value)| QRow {
                    values: vec![
                        Value::Text(Self::redis_value_to_string(field)),
                        Self::redis_value_to_value(value),
                    ],
                })
                .collect(),
            _ => Vec::new(),
        };

        Ok(QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
        })
    }

    async fn read_hash_page(
        conn: &mut redis::aio::MultiplexedConnection,
        key: &str,
        offset: usize,
        limit: usize,
    ) -> EngineResult<Vec<QRow>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut rows = Vec::new();
        let mut cursor: u64 = 0;
        let mut seen: usize = 0;

        loop {
            let (next_cursor, chunk): (u64, Vec<(Vec<u8>, Vec<u8>)>) = redis::cmd("HSCAN")
                .arg(key)
                .arg(cursor)
                .arg("COUNT")
                .arg(500)
                .query_async(conn)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            for (field_bytes, value_bytes) in chunk {
                if seen >= offset && rows.len() < limit {
                    let field = Self::redis_value_to_string(&redis::Value::BulkString(field_bytes));
                    let value = Self::redis_value_to_value(&redis::Value::BulkString(value_bytes));
                    rows.push(QRow {
                        values: vec![Value::Text(field), value],
                    });
                }
                seen += 1;
                if rows.len() >= limit {
                    break;
                }
            }

            if next_cursor == 0 || rows.len() >= limit {
                break;
            }
            cursor = next_cursor;
        }

        Ok(rows)
    }

    /// Reads list key value
    async fn read_list(
        conn: &mut redis::aio::MultiplexedConnection,
        key: &str,
        offset: i64,
        limit: i64,
    ) -> EngineResult<QueryResult> {
        let start = Instant::now();
        let stop = if limit > 0 { offset + limit - 1 } else { -1 };
        let values: redis::Value = redis::cmd("LRANGE")
            .arg(key)
            .arg(offset)
            .arg(stop)
            .query_async(conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let columns = vec![
            ColumnInfo {
                name: "index".to_string(),
                data_type: "integer".to_string(),
                nullable: false,
            },
            ColumnInfo {
                name: "value".to_string(),
                data_type: "string".to_string(),
                nullable: false,
            },
        ];

        let rows: Vec<QRow> = match values {
            redis::Value::Array(items) => items
                .iter()
                .enumerate()
                .map(|(i, value)| QRow {
                    values: vec![
                        Value::Int((offset as usize + i) as i64),
                        Self::redis_value_to_value(value),
                    ],
                })
                .collect(),
            _ => Vec::new(),
        };

        Ok(QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
        })
    }

    /// Reads set key value
    async fn read_set(
        conn: &mut redis::aio::MultiplexedConnection,
        key: &str,
    ) -> EngineResult<QueryResult> {
        let start = Instant::now();
        let members: redis::Value = redis::cmd("SMEMBERS")
            .arg(key)
            .query_async(conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let columns = vec![ColumnInfo {
            name: "member".to_string(),
            data_type: "string".to_string(),
            nullable: false,
        }];

        let rows: Vec<QRow> = match members {
            redis::Value::Array(items) => items
                .iter()
                .map(|member| QRow {
                    values: vec![Self::redis_value_to_value(member)],
                })
                .collect(),
            redis::Value::Set(items) => items
                .iter()
                .map(|member| QRow {
                    values: vec![Self::redis_value_to_value(member)],
                })
                .collect(),
            _ => Vec::new(),
        };

        Ok(QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
        })
    }

    async fn read_set_page(
        conn: &mut redis::aio::MultiplexedConnection,
        key: &str,
        offset: usize,
        limit: usize,
    ) -> EngineResult<Vec<QRow>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut rows = Vec::new();
        let mut cursor: u64 = 0;
        let mut seen: usize = 0;

        loop {
            let (next_cursor, chunk): (u64, Vec<Vec<u8>>) = redis::cmd("SSCAN")
                .arg(key)
                .arg(cursor)
                .arg("COUNT")
                .arg(500)
                .query_async(conn)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            for member_bytes in chunk {
                if seen >= offset && rows.len() < limit {
                    rows.push(QRow {
                        values: vec![Self::redis_value_to_value(&redis::Value::BulkString(
                            member_bytes,
                        ))],
                    });
                }
                seen += 1;
                if rows.len() >= limit {
                    break;
                }
            }

            if next_cursor == 0 || rows.len() >= limit {
                break;
            }
            cursor = next_cursor;
        }

        Ok(rows)
    }

    /// Reads sorted set key value
    async fn read_zset(
        conn: &mut redis::aio::MultiplexedConnection,
        key: &str,
        offset: i64,
        limit: i64,
    ) -> EngineResult<QueryResult> {
        let start = Instant::now();
        let stop = if limit > 0 { offset + limit - 1 } else { -1 };
        let members: Vec<(String, f64)> = redis::cmd("ZRANGE")
            .arg(key)
            .arg(offset)
            .arg(stop)
            .arg("WITHSCORES")
            .query_async(conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let columns = vec![
            ColumnInfo {
                name: "member".to_string(),
                data_type: "string".to_string(),
                nullable: false,
            },
            ColumnInfo {
                name: "score".to_string(),
                data_type: "float".to_string(),
                nullable: false,
            },
        ];

        let rows: Vec<QRow> = members
            .into_iter()
            .map(|(member, score)| QRow {
                values: vec![Value::Text(member), Value::Float(score)],
            })
            .collect();

        Ok(QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
        })
    }

    /// Reads stream entries
    async fn read_stream(
        conn: &mut redis::aio::MultiplexedConnection,
        key: &str,
        offset: usize,
        limit: usize,
    ) -> EngineResult<QueryResult> {
        let start = Instant::now();

        if limit == 0 {
            return Ok(QueryResult {
                columns: vec![
                    ColumnInfo {
                        name: "id".to_string(),
                        data_type: "string".to_string(),
                        nullable: false,
                    },
                    ColumnInfo {
                        name: "data".to_string(),
                        data_type: "json".to_string(),
                        nullable: false,
                    },
                ],
                rows: Vec::new(),
                affected_rows: None,
                execution_time_ms: 0.0,
            });
        }

        // Redis streams paginate by ID, not numeric offset.
        // We fetch up to offset+limit entries and slice in-memory to keep page semantics stable.
        let fetch_count = offset.saturating_add(limit);

        let result: redis::Value = redis::cmd("XRANGE")
            .arg(key)
            .arg("-")
            .arg("+")
            .arg("COUNT")
            .arg(fetch_count)
            .query_async(conn)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let columns = vec![
            ColumnInfo {
                name: "id".to_string(),
                data_type: "string".to_string(),
                nullable: false,
            },
            ColumnInfo {
                name: "data".to_string(),
                data_type: "json".to_string(),
                nullable: false,
            },
        ];

        let rows = Self::paginate_stream_rows(Self::parse_stream_entries(&result), offset, limit);

        Ok(QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
        })
    }

    /// Parses XRANGE result into rows
    fn parse_stream_entries(value: &redis::Value) -> Vec<QRow> {
        let mut rows = Vec::new();

        if let redis::Value::Array(entries) = value {
            for entry in entries {
                if let redis::Value::Array(parts) = entry {
                    if parts.len() >= 2 {
                        let id = Self::redis_value_to_string(&parts[0]);
                        let data = Self::redis_value_to_json_object(&parts[1]);
                        rows.push(QRow {
                            values: vec![Value::Text(id), Value::Json(data)],
                        });
                    }
                }
            }
        }

        rows
    }

    fn paginate_stream_rows(rows: Vec<QRow>, offset: usize, limit: usize) -> Vec<QRow> {
        rows.into_iter().skip(offset).take(limit).collect()
    }

    /// Converts a redis::Value to a String
    fn redis_value_to_string(value: &redis::Value) -> String {
        match value {
            redis::Value::BulkString(bytes) => String::from_utf8_lossy(bytes).to_string(),
            redis::Value::SimpleString(s) => s.clone(),
            redis::Value::Int(i) => i.to_string(),
            redis::Value::Double(f) => f.to_string(),
            redis::Value::Boolean(b) => b.to_string(),
            redis::Value::Nil => "(nil)".to_string(),
            redis::Value::Okay => "OK".to_string(),
            _ => format!("{:?}", value),
        }
    }

    /// Converts array of field-value pairs into a JSON object
    fn redis_value_to_json_object(value: &redis::Value) -> serde_json::Value {
        let mut map = serde_json::Map::new();

        if let redis::Value::Array(pairs) = value {
            let mut iter = pairs.iter();
            while let (Some(key), Some(val)) = (iter.next(), iter.next()) {
                let k = Self::redis_value_to_string(key);
                let v = Self::redis_value_to_string(val);
                // Try to parse as JSON value
                let json_v = if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&v) {
                    parsed
                } else {
                    serde_json::Value::String(v)
                };
                map.insert(k, json_v);
            }
        }

        serde_json::Value::Object(map)
    }

    /// Converts a redis::Value to a QoreDB Value
    fn redis_value_to_value(value: &redis::Value) -> Value {
        match value {
            redis::Value::Nil => Value::Null,
            redis::Value::Int(i) => Value::Int(*i),
            redis::Value::Double(f) => Value::Float(*f),
            redis::Value::Boolean(b) => Value::Bool(*b),
            redis::Value::BulkString(bytes) => {
                match String::from_utf8(bytes.clone()) {
                    Ok(s) => {
                        // Try JSON parsing
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&s) {
                            Value::Json(json)
                        } else {
                            Value::Text(s)
                        }
                    }
                    Err(_) => Value::Bytes(bytes.clone()),
                }
            }
            redis::Value::SimpleString(s) | redis::Value::VerbatimString { text: s, .. } => {
                Value::Text(s.clone())
            }
            redis::Value::Okay => Value::Text("OK".to_string()),
            redis::Value::Array(arr) => {
                let values: Vec<Value> = arr.iter().map(Self::redis_value_to_value).collect();
                Value::Array(values)
            }
            redis::Value::Map(pairs) => {
                let mut map = serde_json::Map::new();
                for (k, v) in pairs {
                    let key = Self::redis_value_to_string(k);
                    let val = match Self::redis_value_to_value(v) {
                        Value::Text(s) => serde_json::Value::String(s),
                        Value::Int(i) => serde_json::Value::Number(serde_json::Number::from(i)),
                        Value::Float(f) => {
                            if let Some(n) = serde_json::Number::from_f64(f) {
                                serde_json::Value::Number(n)
                            } else {
                                serde_json::Value::String(f.to_string())
                            }
                        }
                        Value::Bool(b) => serde_json::Value::Bool(b),
                        Value::Null => serde_json::Value::Null,
                        Value::Json(j) => j,
                        other => serde_json::to_value(other).unwrap_or(serde_json::Value::Null),
                    };
                    map.insert(key, val);
                }
                Value::Json(serde_json::Value::Object(map))
            }
            redis::Value::Set(items) => {
                let values: Vec<Value> = items.iter().map(Self::redis_value_to_value).collect();
                Value::Array(values)
            }
            redis::Value::Attribute { data, .. } => Self::redis_value_to_value(data),
            redis::Value::BigNumber(big) => Value::Text(big.to_string()),
            redis::Value::ServerError(err) => Value::Text(format!("ERROR: {}", err.details().unwrap_or("unknown"))),
            redis::Value::Push { data, .. } => {
                let values: Vec<Value> = data.iter().map(Self::redis_value_to_value).collect();
                Value::Array(values)
            }
        }
    }

    /// Formats a redis::Value as QueryResult for execute()
    fn format_execute_result(value: redis::Value, execution_time_ms: f64) -> QueryResult {
        match &value {
            redis::Value::Nil => QueryResult {
                columns: vec![ColumnInfo {
                    name: "result".to_string(),
                    data_type: "string".to_string(),
                    nullable: true,
                }],
                rows: vec![QRow {
                    values: vec![Value::Null],
                }],
                affected_rows: None,
                execution_time_ms,
            },
            redis::Value::Okay => QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: Some(1),
                execution_time_ms,
            },
            redis::Value::Int(i) => QueryResult {
                columns: vec![ColumnInfo {
                    name: "result".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                }],
                rows: vec![QRow {
                    values: vec![Value::Int(*i)],
                }],
                affected_rows: None,
                execution_time_ms,
            },
            redis::Value::Array(arr) => {
                let columns = vec![ColumnInfo {
                    name: "value".to_string(),
                    data_type: "string".to_string(),
                    nullable: true,
                }];
                let rows: Vec<QRow> = arr
                    .iter()
                    .map(|v| QRow {
                        values: vec![Self::redis_value_to_value(v)],
                    })
                    .collect();
                QueryResult {
                    columns,
                    rows,
                    affected_rows: None,
                    execution_time_ms,
                }
            }
            _ => {
                let columns = vec![ColumnInfo {
                    name: "result".to_string(),
                    data_type: "string".to_string(),
                    nullable: true,
                }];
                let rows = vec![QRow {
                    values: vec![Self::redis_value_to_value(&value)],
                }];
                QueryResult {
                    columns,
                    rows,
                    affected_rows: None,
                    execution_time_ms,
                }
            }
        }
    }

    /// Parses a Redis command string into command name + arguments.
    /// Supports quoted strings (single and double quotes).
    fn parse_command(input: &str) -> EngineResult<Vec<String>> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(EngineError::syntax_error("Empty command"));
        }

        let mut parts = Vec::new();
        let mut current = String::new();
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut escape_next = false;

        for ch in trimmed.chars() {
            if escape_next {
                current.push(ch);
                escape_next = false;
                continue;
            }

            match ch {
                '\\' if in_double_quote => {
                    escape_next = true;
                }
                '\'' if !in_double_quote => {
                    in_single_quote = !in_single_quote;
                }
                '"' if !in_single_quote => {
                    in_double_quote = !in_double_quote;
                }
                ' ' | '\t' if !in_single_quote && !in_double_quote => {
                    if !current.is_empty() {
                        parts.push(current.clone());
                        current.clear();
                    }
                }
                _ => {
                    current.push(ch);
                }
            }
        }

        if !current.is_empty() {
            parts.push(current);
        }

        if in_single_quote || in_double_quote {
            return Err(EngineError::syntax_error("Unterminated quoted string"));
        }

        if parts.is_empty() {
            return Err(EngineError::syntax_error("Empty command"));
        }

        Ok(parts)
    }
}

impl Default for RedisDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for RedisDriver {
    fn driver_id(&self) -> &'static str {
        "redis"
    }

    fn driver_name(&self) -> &'static str {
        "Redis"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        let _ = Self::create_connection_and_ping(config).await?;
        Ok(())
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        let (conn, db) = Self::create_connection_and_ping(config).await?;

        let session_id = SessionId::new();
        let redis_session = Arc::new(RedisSession {
            connection: Mutex::new(conn),
            current_db: AtomicU16::new(db),
        });

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id, redis_session);

        Ok(session_id)
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        let mut sessions = self.sessions.write().await;
        sessions
            .remove(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;
        Ok(())
    }

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let redis_session = self.get_session(session).await?;
        let mut conn = redis_session.connection.lock().await;

        // Try to get the configured number of databases
        let db_count: u16 = match redis::cmd("CONFIG")
            .arg("GET")
            .arg("databases")
            .query_async::<Vec<String>>(&mut *conn)
            .await
        {
            Ok(vals) if vals.len() >= 2 => vals[1].parse().unwrap_or(16),
            _ => 16, // Default Redis config
        };

        let mut namespaces = Vec::new();

        for db in 0..db_count {
            // SELECT the database
            let select_ok: Result<String, _> = redis::cmd("SELECT")
                .arg(db)
                .query_async(&mut *conn)
                .await;

            if select_ok.is_err() {
                continue;
            }

            // Check if it has any keys
            let dbsize: Result<i64, _> = redis::cmd("DBSIZE").query_async(&mut *conn).await;

            match dbsize {
                Ok(size) if size > 0 => {
                    namespaces.push(Namespace::new(format!("db{}", db)));
                }
                Ok(_) => {
                    // Empty database — still show db0 always
                    if db == 0 {
                        namespaces.push(Namespace::new("db0".to_string()));
                    }
                }
                Err(_) => continue,
            }
        }

        // Restore the original database selection
        let _ = redis::cmd("SELECT")
            .arg(redis_session.current_db.load(Ordering::Relaxed))
            .query_async::<String>(&mut *conn)
            .await;

        // If no namespaces found, always show db0
        if namespaces.is_empty() {
            namespaces.push(Namespace::new("db0".to_string()));
        }

        Ok(namespaces)
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let redis_session = self.get_session(session).await?;
        let mut conn = redis_session.connection.lock().await;

        // SELECT the right database
        let db_index = Self::parse_db_index(&namespace.database);

        redis::cmd("SELECT")
            .arg(db_index)
            .query_async::<String>(&mut *conn)
            .await
            .map_err(|e| EngineError::execution_error(format!("SELECT db{}: {}", db_index, e)))?;

        // SCAN all keys
        let pattern = if let Some(ref search) = options.search {
            if search.is_empty() {
                "*".to_string()
            } else {
                format!("*{}*", search)
            }
        } else {
            "*".to_string()
        };

        let mut all_keys: Vec<String> = Vec::new();
        let mut cursor: u64 = 0;

        loop {
            let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(500)
                .query_async(&mut *conn)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

            all_keys.extend(keys);
            cursor = next_cursor;

            if cursor == 0 {
                break;
            }

            // Safety limit
            if all_keys.len() > 100_000 {
                break;
            }
        }

        all_keys.sort();
        all_keys.dedup();

        let total_count = all_keys.len();

        // Apply pagination
        let paginated = if let Some(limit) = options.page_size {
            let page = options.page.unwrap_or(1).max(1);
            let offset = ((page - 1) * limit) as usize;
            let limit = limit as usize;

            if offset >= all_keys.len() {
                Vec::new()
            } else {
                all_keys.into_iter().skip(offset).take(limit).collect()
            }
        } else {
            all_keys
        };

        let collections = paginated
            .into_iter()
            .map(|name| Collection {
                namespace: namespace.clone(),
                name,
                collection_type: CollectionType::Collection,
            })
            .collect();

        Ok(CollectionList {
            collections,
            total_count: total_count as u32,
        })
    }

    async fn create_database(
        &self,
        _session: SessionId,
        _name: &str,
        _options: Option<Value>,
    ) -> EngineResult<()> {
        Err(EngineError::not_supported(
            "Redis databases are numbered (0-15) and cannot be created or dropped",
        ))
    }

    async fn drop_database(&self, _session: SessionId, _name: &str) -> EngineResult<()> {
        Err(EngineError::not_supported(
            "Redis databases are numbered (0-15) and cannot be created or dropped",
        ))
    }

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        self.execute_with_target_db(session, query, query_id, None).await
    }

    async fn execute_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let target_db = namespace
            .as_ref()
            .map(|ns| Self::parse_db_index(&ns.database));
        self.execute_with_target_db(session, query, query_id, target_db)
            .await
    }

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        let redis_session = self.get_session(session).await?;
        let mut conn = redis_session.connection.lock().await;

        // SELECT database
        let db_index = Self::parse_db_index(&namespace.database);

        redis::cmd("SELECT")
            .arg(db_index)
            .query_async::<String>(&mut *conn)
            .await
            .map_err(|e| EngineError::execution_error(format!("SELECT db{}: {}", db_index, e)))?;

        let key = table;
        let type_str = Self::key_type(&mut conn, key).await?;
        let ttl = Self::key_ttl(&mut conn, key).await?;

        // Get encoding
        let encoding: String = redis::cmd("OBJECT")
            .arg("ENCODING")
            .arg(key)
            .query_async(&mut *conn)
            .await
            .unwrap_or_else(|_| "unknown".to_string());

        // Build columns based on type
        let columns = match type_str.as_str() {
            "string" => vec![TableColumn {
                name: "value".to_string(),
                data_type: "string".to_string(),
                nullable: false,
                default_value: None,
                is_primary_key: false,
            }],
            "hash" => vec![
                TableColumn {
                    name: "field".to_string(),
                    data_type: "string".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                },
                TableColumn {
                    name: "value".to_string(),
                    data_type: "string".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                },
            ],
            "list" => vec![
                TableColumn {
                    name: "index".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                },
                TableColumn {
                    name: "value".to_string(),
                    data_type: "string".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                },
            ],
            "set" => vec![TableColumn {
                name: "member".to_string(),
                data_type: "string".to_string(),
                nullable: false,
                default_value: None,
                is_primary_key: false,
            }],
            "zset" => vec![
                TableColumn {
                    name: "member".to_string(),
                    data_type: "string".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                },
                TableColumn {
                    name: "score".to_string(),
                    data_type: "float".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                },
            ],
            "stream" => vec![
                TableColumn {
                    name: "id".to_string(),
                    data_type: "string".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                },
                TableColumn {
                    name: "data".to_string(),
                    data_type: "json".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                },
            ],
            _ => vec![TableColumn {
                name: "value".to_string(),
                data_type: type_str.clone(),
                nullable: true,
                default_value: None,
                is_primary_key: false,
            }],
        };

        // Get element count
        let element_count: Option<u64> = match type_str.as_str() {
            "string" => Some(1),
            "hash" => redis::cmd("HLEN")
                .arg(key)
                .query_async::<u64>(&mut *conn)
                .await
                .ok(),
            "list" => redis::cmd("LLEN")
                .arg(key)
                .query_async::<u64>(&mut *conn)
                .await
                .ok(),
            "set" => redis::cmd("SCARD")
                .arg(key)
                .query_async::<u64>(&mut *conn)
                .await
                .ok(),
            "zset" => redis::cmd("ZCARD")
                .arg(key)
                .query_async::<u64>(&mut *conn)
                .await
                .ok(),
            "stream" => redis::cmd("XLEN")
                .arg(key)
                .query_async::<u64>(&mut *conn)
                .await
                .ok(),
            _ => None,
        };

        let ttl_display = if ttl == -1 {
            "no expiry".to_string()
        } else if ttl == -2 {
            "key not found".to_string()
        } else {
            format!("{}s", ttl)
        };

        Ok(TableSchema {
            columns,
            primary_key: None,
            foreign_keys: Vec::new(),
            row_count_estimate: element_count,
            indexes: vec![crate::engine::types::TableIndex {
                name: format!("type:{} | encoding:{} | ttl:{}", type_str, encoding, ttl_display),
                columns: vec![key.to_string()],
                is_unique: false,
                is_primary: false,
            }],
        })
    }

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let redis_session = self.get_session(session).await?;
        let mut conn = redis_session.connection.lock().await;

        // SELECT database
        let db_index = Self::parse_db_index(&namespace.database);

        redis::cmd("SELECT")
            .arg(db_index)
            .query_async::<String>(&mut *conn)
            .await
            .map_err(|e| EngineError::execution_error(format!("SELECT db{}: {}", db_index, e)))?;

        let key = table;
        let type_str = Self::key_type(&mut conn, key).await?;

        match type_str.as_str() {
            "string" => Self::read_string(&mut conn, key).await,
            "hash" => Self::read_hash(&mut conn, key).await,
            "list" => Self::read_list(&mut conn, key, 0, limit as i64).await,
            "set" => Self::read_set(&mut conn, key).await,
            "zset" => Self::read_zset(&mut conn, key, 0, limit as i64).await,
            "stream" => Self::read_stream(&mut conn, key, 0, limit as usize).await,
            "none" => Err(EngineError::execution_error(format!(
                "Key '{}' does not exist",
                key
            ))),
            other => Err(EngineError::not_supported(format!(
                "Unsupported Redis type: {}",
                other
            ))),
        }
    }

    async fn query_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        let redis_session = self.get_session(session).await?;
        let mut conn = redis_session.connection.lock().await;

        // SELECT database
        let db_index = Self::parse_db_index(&namespace.database);

        redis::cmd("SELECT")
            .arg(db_index)
            .query_async::<String>(&mut *conn)
            .await
            .map_err(|e| EngineError::execution_error(format!("SELECT db{}: {}", db_index, e)))?;

        let key = table;
        let type_str = Self::key_type(&mut conn, key).await?;
        let page = options.effective_page();
        let page_size = options.effective_page_size();
        let offset = options.offset() as i64;

        match type_str.as_str() {
            "string" => {
                let result = Self::read_string(&mut conn, key).await?;
                Ok(PaginatedQueryResult::new(result, 1, page, page_size))
            }
            "hash" => {
                let start = Instant::now();
                let total: u64 = redis::cmd("HLEN")
                    .arg(key)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;
                let rows =
                    Self::read_hash_page(&mut conn, key, offset as usize, page_size as usize)
                        .await?;
                let result = QueryResult {
                    columns: vec![
                        ColumnInfo {
                            name: "field".to_string(),
                            data_type: "string".to_string(),
                            nullable: false,
                        },
                        ColumnInfo {
                            name: "value".to_string(),
                            data_type: "string".to_string(),
                            nullable: false,
                        },
                    ],
                    rows,
                    affected_rows: None,
                    execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
                };
                Ok(PaginatedQueryResult::new(result, total, page, page_size))
            }
            "list" => {
                let total: u64 = redis::cmd("LLEN")
                    .arg(key)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;
                let result =
                    Self::read_list(&mut conn, key, offset, page_size as i64).await?;
                Ok(PaginatedQueryResult::new(result, total, page, page_size))
            }
            "set" => {
                let start = Instant::now();
                let total: u64 = redis::cmd("SCARD")
                    .arg(key)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;
                let rows =
                    Self::read_set_page(&mut conn, key, offset as usize, page_size as usize)
                        .await?;
                let result = QueryResult {
                    columns: vec![ColumnInfo {
                        name: "member".to_string(),
                        data_type: "string".to_string(),
                        nullable: false,
                    }],
                    rows,
                    affected_rows: None,
                    execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
                };
                Ok(PaginatedQueryResult::new(result, total, page, page_size))
            }
            "zset" => {
                let total: u64 = redis::cmd("ZCARD")
                    .arg(key)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;
                let result =
                    Self::read_zset(&mut conn, key, offset, page_size as i64).await?;
                Ok(PaginatedQueryResult::new(result, total, page, page_size))
            }
            "stream" => {
                let total: u64 = redis::cmd("XLEN")
                    .arg(key)
                    .query_async(&mut *conn)
                    .await
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;
                let result =
                    Self::read_stream(&mut conn, key, offset as usize, page_size as usize).await?;
                Ok(PaginatedQueryResult::new(result, total, page, page_size))
            }
            "none" => Err(EngineError::execution_error(format!(
                "Key '{}' does not exist",
                key
            ))),
            other => Err(EngineError::not_supported(format!(
                "Unsupported Redis type: {}",
                other
            ))),
        }
    }

    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        let _ = self.get_session(session).await?;

        let mut active = self.active_queries.lock().await;

        if let Some(qid) = query_id {
            if let Some((sid, handle)) = active.get(&qid) {
                if *sid == session {
                    handle.abort();
                    active.remove(&qid);
                    return Ok(());
                }
            }
            return Err(EngineError::execution_error("Query not found"));
        }

        let to_cancel: Vec<QueryId> = active
            .iter()
            .filter_map(|(qid, (sid, _))| if *sid == session { Some(*qid) } else { None })
            .collect();

        for qid in to_cancel {
            if let Some((_, handle)) = active.remove(&qid) {
                handle.abort();
            }
        }

        Ok(())
    }

    fn cancel_support(&self) -> CancelSupport {
        CancelSupport::BestEffort
    }

    fn supports_schema(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_connection_string_simple() {
        let config = ConnectionConfig {
            driver: "redis".to_string(),
            host: "localhost".to_string(),
            port: 6379,
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

        let conn_str = RedisDriver::build_connection_string(&config);
        assert_eq!(conn_str, "redis://localhost:6379/0");
    }

    #[test]
    fn test_build_connection_string_with_auth() {
        let config = ConnectionConfig {
            driver: "redis".to_string(),
            host: "redis.example.com".to_string(),
            port: 6380,
            username: "default".to_string(),
            password: "secret".to_string(),
            database: Some("2".to_string()),
            ssl: true,
            environment: "production".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
        };

        let conn_str = RedisDriver::build_connection_string(&config);
        assert_eq!(conn_str, "rediss://default:secret@redis.example.com:6380/2");
    }

    #[test]
    fn test_build_connection_string_encodes_credentials() {
        let config = ConnectionConfig {
            driver: "redis".to_string(),
            host: "localhost".to_string(),
            port: 6379,
            username: "user:name".to_string(),
            password: "p@ss/wo:rd".to_string(),
            database: Some("1".to_string()),
            ssl: false,
            environment: "development".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
        };

        let conn_str = RedisDriver::build_connection_string(&config);
        assert_eq!(
            conn_str,
            "redis://user%3Aname:p%40ss%2Fwo%3Ard@localhost:6379/1"
        );
    }

    #[test]
    fn test_parse_command_simple() {
        let parts = RedisDriver::parse_command("GET mykey").unwrap();
        assert_eq!(parts, vec!["GET", "mykey"]);
    }

    #[test]
    fn test_parse_command_quoted() {
        let parts = RedisDriver::parse_command(r#"SET "my key" "hello world""#).unwrap();
        assert_eq!(parts, vec!["SET", "my key", "hello world"]);
    }

    #[test]
    fn test_parse_command_single_quoted() {
        let parts = RedisDriver::parse_command("SET 'my key' 'hello world'").unwrap();
        assert_eq!(parts, vec!["SET", "my key", "hello world"]);
    }

    #[test]
    fn test_parse_command_empty() {
        assert!(RedisDriver::parse_command("").is_err());
        assert!(RedisDriver::parse_command("   ").is_err());
    }

    #[test]
    fn test_parse_command_unterminated_quote() {
        assert!(RedisDriver::parse_command(r#"SET "unterminated"#).is_err());
    }

    #[test]
    fn test_parse_command_multiple_args() {
        let parts = RedisDriver::parse_command("HSET myhash field1 value1 field2 value2").unwrap();
        assert_eq!(
            parts,
            vec!["HSET", "myhash", "field1", "value1", "field2", "value2"]
        );
    }

    #[test]
    fn test_parse_db_index_variants() {
        assert_eq!(RedisDriver::parse_db_index("db0"), 0);
        assert_eq!(RedisDriver::parse_db_index("db15"), 15);
        assert_eq!(RedisDriver::parse_db_index("7"), 7);
        assert_eq!(RedisDriver::parse_db_index("invalid"), 0);
    }

    #[test]
    fn test_paginate_stream_rows_applies_offset() {
        let stream = redis::Value::Array(vec![
            redis::Value::Array(vec![
                redis::Value::BulkString(b"1-0".to_vec()),
                redis::Value::Array(vec![
                    redis::Value::BulkString(b"field".to_vec()),
                    redis::Value::BulkString(b"value-1".to_vec()),
                ]),
            ]),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"2-0".to_vec()),
                redis::Value::Array(vec![
                    redis::Value::BulkString(b"field".to_vec()),
                    redis::Value::BulkString(b"value-2".to_vec()),
                ]),
            ]),
            redis::Value::Array(vec![
                redis::Value::BulkString(b"3-0".to_vec()),
                redis::Value::Array(vec![
                    redis::Value::BulkString(b"field".to_vec()),
                    redis::Value::BulkString(b"value-3".to_vec()),
                ]),
            ]),
        ]);

        let rows = RedisDriver::parse_stream_entries(&stream);
        let page = RedisDriver::paginate_stream_rows(rows, 1, 1);

        assert_eq!(page.len(), 1);
        match page[0].values.first() {
            Some(Value::Text(id)) => assert_eq!(id, "2-0"),
            other => panic!("expected stream id text, got {:?}", other),
        }
    }
}

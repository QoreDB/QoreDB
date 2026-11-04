// SPDX-License-Identifier: BUSL-1.1

//! Federation execution manager.
//!
//! Orchestrates the full federation pipeline:
//! parse → plan → fetch from sources → load into DuckDB → execute → return results.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use tokio::time::{Duration, timeout};
use tracing::instrument;

use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::{StreamEvent, StreamSender};
use qore_core::types::{ColumnInfo, QueryId, QueryResult, Row, SessionId, Value};
use qore_drivers::session_manager::SessionManager;

use super::duckdb_engine::DuckDbEngine;
use super::planner::{build_plan, build_source_query, strip_masked_predicates};
use super::types::{
    ConnectionAliasMap, FederationMetadata, FederationPlan, FederationQueryOptions,
    SourceFetchPlan, SourceFetchResult,
};

/// Timeout per source fetch (30 seconds).
const SOURCE_FETCH_TIMEOUT_MS: u64 = 30_000;

/// Default global timeout for the full federation pipeline (60 seconds).
const DEFAULT_GLOBAL_TIMEOUT_MS: u64 = 60_000;

/// TTL for cached source fetches (60 seconds).
const SOURCE_CACHE_TTL: Duration = Duration::from_secs(60);

/// Maximum total memory footprint for cached federation source results (64 MB).
const SOURCE_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Maximum number of cached source tables kept simultaneously.
const SOURCE_CACHE_MAX_ENTRIES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SourceCacheKey {
    session_id: SessionId,
    database: String,
    schema: Option<String>,
    table: String,
    source_query: String,
    row_limit: u64,
    masking_fingerprint: String,
}

#[derive(Debug, Clone)]
struct SourceCacheEntry {
    result: QueryResult,
    inserted_at: Instant,
    approx_bytes: usize,
}

#[derive(Debug)]
struct FederationSourceCache {
    entries: HashMap<SourceCacheKey, SourceCacheEntry>,
    total_bytes: usize,
    ttl: Duration,
    max_bytes: usize,
    max_entries: usize,
}

impl FederationSourceCache {
    fn new(ttl: Duration, max_bytes: usize, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            total_bytes: 0,
            ttl,
            max_bytes,
            max_entries,
        }
    }

    fn evict_expired(&mut self, now: Instant) {
        let ttl = self.ttl;
        let mut freed = 0usize;
        self.entries.retain(|_, entry| {
            if now.duration_since(entry.inserted_at) >= ttl {
                freed = freed.saturating_add(entry.approx_bytes);
                false
            } else {
                true
            }
        });
        self.total_bytes = self.total_bytes.saturating_sub(freed);
    }

    fn get(&mut self, key: &SourceCacheKey) -> Option<QueryResult> {
        let now = Instant::now();
        self.evict_expired(now);
        self.entries.get(key).map(|entry| entry.result.clone())
    }

    fn insert(&mut self, key: SourceCacheKey, result: QueryResult) {
        let now = Instant::now();
        self.evict_expired(now);

        let approx_bytes = estimate_query_result_bytes(&result);
        if approx_bytes > self.max_bytes {
            return;
        }

        if let Some(prev) = self.entries.remove(&key) {
            self.total_bytes = self.total_bytes.saturating_sub(prev.approx_bytes);
        }

        while (!self.entries.is_empty())
            && (self.total_bytes.saturating_add(approx_bytes) > self.max_bytes
                || self.entries.len() >= self.max_entries)
        {
            let oldest_key = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.inserted_at)
                .map(|(k, _)| k.clone());
            if let Some(k) = oldest_key {
                if let Some(removed) = self.entries.remove(&k) {
                    self.total_bytes = self.total_bytes.saturating_sub(removed.approx_bytes);
                }
            } else {
                break;
            }
        }

        self.total_bytes = self.total_bytes.saturating_add(approx_bytes);
        self.entries.insert(
            key,
            SourceCacheEntry {
                result,
                inserted_at: now,
                approx_bytes,
            },
        );
    }

    fn invalidate_session(&mut self, session_id: SessionId) {
        let mut freed = 0usize;
        self.entries.retain(|k, entry| {
            if k.session_id == session_id {
                freed = freed.saturating_add(entry.approx_bytes);
                false
            } else {
                true
            }
        });
        self.total_bytes = self.total_bytes.saturating_sub(freed);
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.total_bytes = 0;
    }
}

fn global_source_cache() -> &'static Mutex<FederationSourceCache> {
    static CACHE: OnceLock<Mutex<FederationSourceCache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(FederationSourceCache::new(
            SOURCE_CACHE_TTL,
            SOURCE_CACHE_MAX_BYTES,
            SOURCE_CACHE_MAX_ENTRIES,
        ))
    })
}

/// Clears all entries in the short-lived federation source cache.
pub fn clear_federation_source_cache() {
    if let Ok(mut cache) = global_source_cache().lock() {
        cache.clear();
    }
}

/// Invalidates all cached federation source entries for a specific session.
pub fn invalidate_session_source_cache(session_id: SessionId) {
    if let Ok(mut cache) = global_source_cache().lock() {
        cache.invalidate_session(session_id);
    }
}

fn estimate_query_result_bytes(result: &QueryResult) -> usize {
    let mut bytes = 64usize;
    for col in &result.columns {
        bytes = bytes.saturating_add(col.name.len() + col.data_type.len() + 32);
    }
    for row in &result.rows {
        bytes = bytes.saturating_add(24);
        for val in &row.values {
            bytes = bytes.saturating_add(estimate_value_bytes(val));
        }
    }
    bytes
}

fn estimate_value_bytes(val: &Value) -> usize {
    match val {
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => 16,
        Value::Text(s) => 24 + s.len(),
        Value::Bytes(b) => 24 + b.len(),
        Value::Json(v) => 32 + v.to_string().len(),
        Value::Array(items) => 24 + items.iter().map(estimate_value_bytes).sum::<usize>(),
    }
}

/// Executes a federation query end-to-end.
///
/// Returns `(QueryResult, FederationMetadata)` for batch mode.
#[instrument(skip(session_manager, alias_map, options), fields(query_len = sql.len()))]
pub async fn execute_federation(
    sql: &str,
    alias_map: &ConnectionAliasMap,
    session_manager: &Arc<SessionManager>,
    options: &FederationQueryOptions,
) -> EngineResult<(QueryResult, FederationMetadata)> {
    let total_start = Instant::now();
    let global_timeout = options.timeout_ms.unwrap_or(DEFAULT_GLOBAL_TIMEOUT_MS);

    let result = timeout(
        Duration::from_millis(global_timeout),
        execute_federation_inner(sql, alias_map, session_manager, options),
    )
    .await;

    match result {
        Ok(inner_result) => {
            let mut result = inner_result?;
            result.1.total_time_ms = total_start.elapsed().as_secs_f64() * 1000.0;
            Ok(result)
        }
        Err(_) => Err(EngineError::Timeout {
            timeout_ms: global_timeout,
        }),
    }
}

/// Executes a federation query with streaming results.
#[instrument(skip(session_manager, alias_map, options, sender), fields(query_len = sql.len()))]
pub async fn execute_federation_stream(
    sql: &str,
    alias_map: &ConnectionAliasMap,
    session_manager: &Arc<SessionManager>,
    options: &FederationQueryOptions,
    sender: StreamSender,
) -> EngineResult<FederationMetadata> {
    let total_start = Instant::now();
    let global_timeout = options.timeout_ms.unwrap_or(DEFAULT_GLOBAL_TIMEOUT_MS);

    let result = timeout(
        Duration::from_millis(global_timeout),
        execute_federation_stream_inner(sql, alias_map, session_manager, options, sender),
    )
    .await;

    match result {
        Ok(inner_result) => {
            let mut meta = inner_result?;
            meta.total_time_ms = total_start.elapsed().as_secs_f64() * 1000.0;
            Ok(meta)
        }
        Err(_) => Err(EngineError::Timeout {
            timeout_ms: global_timeout,
        }),
    }
}

/// Inner implementation for batch federation execution.
async fn execute_federation_inner(
    sql: &str,
    alias_map: &ConnectionAliasMap,
    session_manager: &Arc<SessionManager>,
    options: &FederationQueryOptions,
) -> EngineResult<(QueryResult, FederationMetadata)> {
    let row_limit = options.row_limit_per_source;
    let refresh_sources = options.refresh_sources.unwrap_or(false);

    let mut plan = build_plan(sql, alias_map, row_limit, false)?;
    let (engine, fetch_results, duckdb_start) =
        prepare_duckdb(&mut plan, session_manager, refresh_sources).await?;

    let query_result = engine.execute_query(&plan.duckdb_query)?;
    let duckdb_time_ms = duckdb_start.elapsed().as_secs_f64() * 1000.0;

    let warnings = build_row_limit_warnings(&fetch_results);

    let metadata = FederationMetadata {
        source_results: fetch_results,
        duckdb_time_ms,
        total_time_ms: 0.0, // set by caller
        warnings,
    };

    Ok((query_result, metadata))
}

/// Inner implementation for streaming federation execution.
async fn execute_federation_stream_inner(
    sql: &str,
    alias_map: &ConnectionAliasMap,
    session_manager: &Arc<SessionManager>,
    options: &FederationQueryOptions,
    sender: StreamSender,
) -> EngineResult<FederationMetadata> {
    let row_limit = options.row_limit_per_source;
    let refresh_sources = options.refresh_sources.unwrap_or(false);

    let mut plan = build_plan(sql, alias_map, row_limit, true)?;
    let (engine, fetch_results, duckdb_start) =
        prepare_duckdb(&mut plan, session_manager, refresh_sources).await?;

    // DuckDB statement/row handles are not Send: execute sync, then stream out.
    let (columns, rows) = engine.execute_query_for_stream(&plan.duckdb_query)?;
    let duckdb_time_ms = duckdb_start.elapsed().as_secs_f64() * 1000.0;

    let _ = sender.send(StreamEvent::Columns(columns)).await;
    let row_count = rows.len() as u64;
    for row in rows {
        if sender.send(StreamEvent::Row(row)).await.is_err() {
            break; // receiver dropped (cancelled)
        }
    }
    let _ = sender.send(StreamEvent::Done(row_count)).await;

    let warnings = build_row_limit_warnings(&fetch_results);

    Ok(FederationMetadata {
        source_results: fetch_results,
        duckdb_time_ms,
        total_time_ms: 0.0,
        warnings,
    })
}

/// Row-limit warnings for any sources that hit their per-source cap. Shared by
/// the batch and streaming paths (cf. dédup D28).
fn build_row_limit_warnings(fetch_results: &[SourceFetchResult]) -> Vec<String> {
    fetch_results
        .iter()
        .filter(|r| r.row_limit_hit)
        .map(|r| {
            format!(
                "Source '{}.{}' returned the maximum {} rows. Results may be incomplete.",
                r.alias, r.table, r.row_count
            )
        })
        .collect()
}

/// Fetches every source, flattens MongoDB documents to per-key columns, and
/// loads the results into a fresh in-memory DuckDB engine. Shared setup for the
/// batch and streaming execution paths (cf. dédup D28). The returned `Instant`
/// is captured after the source fetch (mirroring the original timing) so the
/// caller's `duckdb_time_ms` still covers load + query only.
async fn prepare_duckdb(
    plan: &mut FederationPlan,
    session_manager: &Arc<SessionManager>,
    refresh_sources: bool,
) -> EngineResult<(DuckDbEngine, Vec<SourceFetchResult>, Instant)> {
    // Strip any pushdown predicates that reference masked columns on the source session.
    for source in &mut plan.sources {
        if let Some(masking) = session_manager.masking(source.session_id).await {
            strip_masked_predicates(source, &masking.config);
        }
        if refresh_sources {
            invalidate_session_source_cache(source.session_id);
        }
    }

    let (source_results, fetch_results) =
        fetch_all_sources(plan, session_manager, refresh_sources).await?;

    let source_results: Vec<QueryResult> = source_results
        .into_iter()
        .zip(plan.sources.iter())
        .map(|(result, source)| {
            if matches!(source.driver_id.as_str(), "mongodb" | "documentdb") {
                flatten_mongo_documents(result)
            } else {
                result
            }
        })
        .collect();

    let duckdb_start = Instant::now();
    let engine = DuckDbEngine::new()?;

    for (source, result) in plan.sources.iter().zip(source_results.iter()) {
        engine.create_temp_table(&source.table_ref.local_alias, &result.columns, &result.rows)?;
        engine.insert_batch(&source.table_ref.local_alias, &result.rows, &result.columns)?;
    }
    engine.disable_external_access()?;

    Ok((engine, fetch_results, duckdb_start))
}

/// Fetches data from all sources in parallel using tokio::spawn.
///
/// Returns (Vec<QueryResult>, Vec<SourceFetchResult>) in the same order as `plan.sources`.
async fn fetch_all_sources(
    plan: &FederationPlan,
    session_manager: &Arc<SessionManager>,
    refresh_sources: bool,
) -> EngineResult<(Vec<QueryResult>, Vec<SourceFetchResult>)> {
    let mut handles = Vec::with_capacity(plan.sources.len());

    for source in &plan.sources {
        let sm = Arc::clone(session_manager);
        let source = source.clone();

        handles.push(tokio::spawn(async move {
            fetch_single_source(&source, &sm, refresh_sources).await
        }));
    }

    let mut query_results = Vec::with_capacity(handles.len());
    let mut fetch_results = Vec::with_capacity(handles.len());

    for (i, handle) in handles.into_iter().enumerate() {
        let (result, fetch_meta) = handle
            .await
            .map_err(|e| EngineError::internal(format!("Source fetch task panicked: {e}")))?
            .map_err(|e| {
                let source = &plan.sources[i];
                EngineError::execution_error(format!(
                    "Failed to fetch from '{}.{}': {}",
                    source.table_ref.connection_alias, source.table_ref.table, e
                ))
            })?;

        query_results.push(result);
        fetch_results.push(fetch_meta);
    }

    Ok((query_results, fetch_results))
}

/// Fetches data from a single source table (or returns a short-lived cache hit).
async fn fetch_single_source(
    source: &SourceFetchPlan,
    session_manager: &Arc<SessionManager>,
    refresh_sources: bool,
) -> EngineResult<(QueryResult, SourceFetchResult)> {
    let start = Instant::now();

    let masking = session_manager.masking(source.session_id).await;
    let masking_fingerprint = masking
        .as_ref()
        .map(|m| {
            format!(
                "{}:{}",
                m.connection_id,
                serde_json::to_string(&m.config).unwrap_or_default()
            )
        })
        .unwrap_or_default();

    let query = build_source_query(source);
    let cache_key = SourceCacheKey {
        session_id: source.session_id,
        database: source.table_ref.namespace.database.clone(),
        schema: source.table_ref.namespace.schema.clone(),
        table: source.table_ref.table.clone(),
        source_query: query.clone(),
        row_limit: source.row_limit,
        masking_fingerprint,
    };

    if !refresh_sources {
        if let Ok(mut cache) = global_source_cache().lock() {
            if let Some(cached_result) = cache.get(&cache_key) {
                let row_count = cached_result.rows.len() as u64;
                let fetch_time_ms = start.elapsed().as_secs_f64() * 1000.0;
                let row_limit_hit = row_count >= source.row_limit;
                let fetch_result = SourceFetchResult {
                    alias: source.table_ref.connection_alias.clone(),
                    table: source.table_ref.table.clone(),
                    row_count,
                    fetch_time_ms,
                    row_limit_hit,
                    projected_columns: source.columns.clone(),
                    pushed_predicates: source.pushdown_predicates.clone(),
                    cache_hit: true,
                };
                return Ok((cached_result, fetch_result));
            }
        }
    }

    let driver = session_manager.get_driver(source.session_id).await?;
    let query_id = QueryId::new();

    let namespace = Some(source.table_ref.namespace.clone());

    let mut result = timeout(
        Duration::from_millis(SOURCE_FETCH_TIMEOUT_MS),
        driver.execute_in_namespace(source.session_id, namespace, &query, query_id),
    )
    .await
    .map_err(|_| EngineError::Timeout {
        timeout_ms: SOURCE_FETCH_TIMEOUT_MS,
    })??;

    // Masked before DuckDB sees them, so the federated query cannot filter or
    // join on the raw values; masked values are text whatever the source type.
    if let Some(masking) = masking {
        masking.apply(Some(&source.table_ref.table), &mut result);
        for column in result.columns.iter_mut().filter(|c| c.masked) {
            column.data_type = "text".into();
        }
    }

    if let Ok(mut cache) = global_source_cache().lock() {
        cache.insert(cache_key, result.clone());
    }

    let row_count = result.rows.len() as u64;
    let fetch_time_ms = start.elapsed().as_secs_f64() * 1000.0;
    let row_limit_hit = row_count >= source.row_limit;

    let fetch_result = SourceFetchResult {
        alias: source.table_ref.connection_alias.clone(),
        table: source.table_ref.table.clone(),
        row_count,
        fetch_time_ms,
        row_limit_hit,
        projected_columns: source.columns.clone(),
        pushed_predicates: source.pushdown_predicates.clone(),
        cache_hit: false,
    };

    Ok((result, fetch_result))
}

/// Flattens a MongoDB QueryResult from a single `document` JSON column
/// into individual columns, one per top-level key found across all documents.
///
/// This allows DuckDB to reference MongoDB fields directly (e.g. `l.profileId`)
/// instead of requiring JSON extraction on a single `document` column.
fn flatten_mongo_documents(result: QueryResult) -> QueryResult {
    if result.columns.len() != 1 || result.columns[0].name != "document" {
        return result;
    }

    if result.rows.is_empty() {
        return result;
    }

    // Collect unique keys across all documents, preserving discovery order.
    let mut column_names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for row in &result.rows {
        if let Some(Value::Json(serde_json::Value::Object(map))) = row.values.first() {
            for key in map.keys() {
                if seen.insert(key.clone()) {
                    column_names.push(key.clone());
                }
            }
        }
    }

    if column_names.is_empty() {
        return result;
    }

    // MongoDB fields are dynamically typed, so every flattened column is VARCHAR.
    let columns: Vec<ColumnInfo> = column_names
        .iter()
        .map(|name| ColumnInfo {
            name: name.as_str().into(),
            data_type: "VARCHAR".into(),
            nullable: true,
            masked: false,
        })
        .collect();

    let rows: Vec<Row> = result
        .rows
        .iter()
        .map(|row| {
            let map = match row.values.first() {
                Some(Value::Json(serde_json::Value::Object(m))) => Some(m),
                _ => None,
            };

            let values: Vec<Value> = column_names
                .iter()
                .map(|key| {
                    map.and_then(|m| m.get(key))
                        .map(json_value_to_engine_value)
                        .unwrap_or(Value::Null)
                })
                .collect();

            Row { values }
        })
        .collect();

    QueryResult {
        columns,
        rows,
        affected_rows: result.affected_rows,
        execution_time_ms: result.execution_time_ms,
    }
}

/// Converts a serde_json::Value to an engine Value for DuckDB insertion.
fn json_value_to_engine_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Text(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::Text(s.clone()),
        // Objects and arrays serialize back to JSON for DuckDB's VARCHAR column.
        _ => Value::Text(v.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result(rows: usize) -> QueryResult {
        QueryResult {
            columns: vec![ColumnInfo {
                name: "email".into(),
                data_type: "text".into(),
                nullable: false,
                masked: false,
            }],
            rows: (0..rows)
                .map(|i| Row {
                    values: vec![Value::Text(format!("user{i}@example.com"))],
                })
                .collect(),
            affected_rows: None,
            execution_time_ms: 1.0,
        }
    }

    fn sample_key(session_id: SessionId, table: &str, masking_fp: &str) -> SourceCacheKey {
        SourceCacheKey {
            session_id,
            database: "public".to_string(),
            schema: None,
            table: table.to_string(),
            source_query: format!("SELECT \"email\" FROM \"{table}\" LIMIT 100"),
            row_limit: 100,
            masking_fingerprint: masking_fp.to_string(),
        }
    }

    #[test]
    fn source_cache_hits_and_invalidates_by_session_or_masking() {
        let mut cache = FederationSourceCache::new(Duration::from_secs(60), 1024 * 1024, 8);
        let session = SessionId::new();
        let key = sample_key(session, "users", "");

        assert!(cache.get(&key).is_none());
        cache.insert(key.clone(), sample_result(3));
        assert_eq!(cache.get(&key).unwrap().rows.len(), 3);

        // Changing masking fingerprint misses the cache.
        let masked_key = sample_key(session, "users", "conn-1:masked");
        assert!(cache.get(&masked_key).is_none());

        // Session invalidation removes the entry.
        cache.invalidate_session(session);
        assert!(cache.get(&key).is_none());
        assert_eq!(cache.total_bytes, 0);
    }

    #[test]
    fn source_cache_evicts_oldest_when_max_entries_exceeded() {
        let mut cache = FederationSourceCache::new(Duration::from_secs(60), 1024 * 1024, 2);
        let session = SessionId::new();
        let k1 = sample_key(session, "t1", "");
        let k2 = sample_key(session, "t2", "");
        let k3 = sample_key(session, "t3", "");

        cache.insert(k1.clone(), sample_result(1));
        cache.insert(k2.clone(), sample_result(1));
        cache.insert(k3.clone(), sample_result(1));

        assert_eq!(cache.entries.len(), 2);
        assert!(cache.get(&k1).is_none());
        assert!(cache.get(&k2).is_some());
        assert!(cache.get(&k3).is_some());
    }
}

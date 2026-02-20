// SPDX-License-Identifier: BUSL-1.1

//! Federation execution manager.
//!
//! Orchestrates the full federation pipeline:
//! parse → plan → fetch from sources → load into DuckDB → execute → return results.

use std::sync::Arc;
use std::time::Instant;

use tokio::time::{timeout, Duration};
use tracing::instrument;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::session_manager::SessionManager;
use crate::engine::traits::{StreamEvent, StreamSender};
use crate::engine::types::{QueryId, QueryResult};

use super::duckdb_engine::DuckDbEngine;
use super::planner::{build_plan, build_source_query};
use super::types::{
    ConnectionAliasMap, FederationMetadata, FederationPlan, FederationQueryOptions,
    SourceFetchPlan, SourceFetchResult,
};

/// Timeout per source fetch (30 seconds).
const SOURCE_FETCH_TIMEOUT_MS: u64 = 30_000;

/// Default global timeout for the full federation pipeline (60 seconds).
const DEFAULT_GLOBAL_TIMEOUT_MS: u64 = 60_000;

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

    // Step 1: Build plan
    let plan = build_plan(sql, alias_map, row_limit, false)?;

    // Step 2: Fetch from all sources in parallel
    let (source_results, fetch_results) =
        fetch_all_sources(&plan, session_manager).await?;

    // Step 3: Load into DuckDB and execute
    let duckdb_start = Instant::now();
    let engine = DuckDbEngine::new()?;

    for (source, result) in plan.sources.iter().zip(source_results.iter()) {
        engine.create_temp_table(&source.table_ref.local_alias, &result.columns)?;
        engine.insert_batch(&source.table_ref.local_alias, &result.rows, &result.columns)?;
    }

    let query_result = engine.execute_query(&plan.duckdb_query)?;
    let duckdb_time_ms = duckdb_start.elapsed().as_secs_f64() * 1000.0;

    // Build metadata
    let warnings: Vec<String> = fetch_results
        .iter()
        .filter(|r| r.row_limit_hit)
        .map(|r| {
            format!(
                "Source '{}.{}' returned the maximum {} rows. Results may be incomplete.",
                r.alias, r.table, r.row_count
            )
        })
        .collect();

    let metadata = FederationMetadata {
        source_results: fetch_results,
        duckdb_time_ms,
        total_time_ms: 0.0, // Set by caller
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

    // Step 1: Build plan
    let plan = build_plan(sql, alias_map, row_limit, true)?;

    // Step 2: Fetch from all sources in parallel
    let (source_results, fetch_results) =
        fetch_all_sources(&plan, session_manager).await?;

    // Step 3: Load into DuckDB and stream
    let duckdb_start = Instant::now();
    let engine = DuckDbEngine::new()?;

    for (source, result) in plan.sources.iter().zip(source_results.iter()) {
        engine.create_temp_table(&source.table_ref.local_alias, &result.columns)?;
        engine.insert_batch(&source.table_ref.local_alias, &result.rows, &result.columns)?;
    }

    // Execute synchronously (DuckDB types are not Send/Sync), then stream results
    let (columns, rows) = engine.execute_query_for_stream(&plan.duckdb_query)?;
    let duckdb_time_ms = duckdb_start.elapsed().as_secs_f64() * 1000.0;

    // Stream results through the channel
    let _ = sender.send(StreamEvent::Columns(columns)).await;
    let row_count = rows.len() as u64;
    for row in rows {
        if sender.send(StreamEvent::Row(row)).await.is_err() {
            break; // Receiver dropped (cancelled)
        }
    }
    let _ = sender.send(StreamEvent::Done(row_count)).await;

    let warnings: Vec<String> = fetch_results
        .iter()
        .filter(|r| r.row_limit_hit)
        .map(|r| {
            format!(
                "Source '{}.{}' returned the maximum {} rows. Results may be incomplete.",
                r.alias, r.table, r.row_count
            )
        })
        .collect();

    Ok(FederationMetadata {
        source_results: fetch_results,
        duckdb_time_ms,
        total_time_ms: 0.0,
        warnings,
    })
}

/// Fetches data from all sources in parallel using tokio::spawn.
///
/// Returns (Vec<QueryResult>, Vec<SourceFetchResult>) in the same order as `plan.sources`.
async fn fetch_all_sources(
    plan: &FederationPlan,
    session_manager: &Arc<SessionManager>,
) -> EngineResult<(Vec<QueryResult>, Vec<SourceFetchResult>)> {
    let mut handles = Vec::with_capacity(plan.sources.len());

    for source in &plan.sources {
        let sm = Arc::clone(session_manager);
        let source = source.clone();

        handles.push(tokio::spawn(async move {
            fetch_single_source(&source, &sm).await
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

/// Fetches data from a single source table.
async fn fetch_single_source(
    source: &SourceFetchPlan,
    session_manager: &Arc<SessionManager>,
) -> EngineResult<(QueryResult, SourceFetchResult)> {
    let start = Instant::now();

    let driver = session_manager.get_driver(source.session_id).await?;
    let query = build_source_query(source);
    let query_id = QueryId::new();

    let namespace = Some(source.table_ref.namespace.clone());

    let result = timeout(
        Duration::from_millis(SOURCE_FETCH_TIMEOUT_MS),
        driver.execute_in_namespace(source.session_id, namespace, &query, query_id),
    )
    .await
    .map_err(|_| EngineError::Timeout {
        timeout_ms: SOURCE_FETCH_TIMEOUT_MS,
    })??;

    let row_count = result.rows.len() as u64;
    let fetch_time_ms = start.elapsed().as_secs_f64() * 1000.0;
    let row_limit_hit = row_count >= source.row_limit;

    let fetch_result = SourceFetchResult {
        alias: source.table_ref.connection_alias.clone(),
        table: source.table_ref.table.clone(),
        row_count,
        fetch_time_ms,
        row_limit_hit,
    };

    Ok((result, fetch_result))
}

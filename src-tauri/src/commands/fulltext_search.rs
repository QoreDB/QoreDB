// SPDX-License-Identifier: Apache-2.0

//! Full-text Search Tauri Commands
//!
//! Provides global search functionality across all tables and columns in a database.
//! Uses driver-specific optimized strategies with automatic fallback.
//!
//! Features:
//! - Auto-detection of full-text indexes
//! - Caching of table capabilities
//! - Parallel table search with timeouts
//! - Progressive result streaming

use futures::stream::{self, StreamExt};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tokio::time::timeout;
use tracing::{debug, instrument, warn};

use crate::engine::fulltext_strategy::{
    get_capability_cache, get_search_strategy, FulltextIndexInfo, SearchMethod,
    TableSearchOptions,
};
use crate::engine::types::{CollectionListOptions, CollectionType, Namespace, QueryId, Value};

/// Maximum number of tables to search in parallel
const MAX_PARALLEL_TABLES: usize = 5;

/// Default timeout per table search (milliseconds)
const DEFAULT_TABLE_TIMEOUT_MS: u64 = 5000;

/// A single match found during full-text search
#[derive(Debug, Clone, Serialize)]
pub struct FulltextMatch {
    pub namespace: Namespace,
    pub table_name: String,
    pub column_name: String,
    pub value_preview: String,
    pub row_preview: Vec<(String, Value)>,
}

/// Statistics about the search execution
#[derive(Debug, Clone, Serialize)]
pub struct SearchStats {
    pub native_fulltext_count: u32,
    pub pattern_match_count: u32,
    pub timeout_count: u32,
    pub error_count: u32,
}

/// Response for full-text search
#[derive(Debug, Serialize)]
pub struct FulltextSearchResponse {
    pub success: bool,
    pub matches: Vec<FulltextMatch>,
    pub total_matches: u64,
    pub tables_searched: u32,
    pub search_time_ms: f64,
    pub error: Option<String>,
    pub truncated: bool,
    pub stats: SearchStats,
}

/// Progressive search event for streaming results
#[derive(Debug, Clone, Serialize)]
pub struct SearchProgressEvent {
    pub tables_searched: u32,
    pub total_tables: u32,
    pub matches_found: u32,
    pub current_table: Option<String>,
}

/// Options for full-text search
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FulltextSearchOptions {
    pub max_results_per_table: Option<u32>,
    pub max_total_results: Option<u32>,
    pub case_sensitive: Option<bool>,
    pub namespaces: Option<Vec<Namespace>>,
    pub tables: Option<Vec<String>>,
    pub timeout_per_table_ms: Option<u64>,
    pub max_parallel: Option<usize>,
    /// Enable streaming of results (emit events as results come in)
    pub stream_results: Option<bool>,
}

fn parse_session_id(id: &str) -> Result<crate::engine::types::SessionId, String> {
    let uuid = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(crate::engine::types::SessionId(uuid))
}

fn is_text_type(data_type: &str) -> bool {
    let dt = data_type.to_lowercase();
    dt.contains("char")
        || dt.contains("text")
        || dt.contains("varchar")
        || dt.contains("string")
        || dt.contains("clob")
        || dt.contains("name")
        || dt.contains("uuid")
        || dt.contains("json")
        || dt.contains("xml")
        || dt.contains("enum")
        || dt == "string"
        || dt == "objectid"
}

fn is_system_namespace(namespace: &Namespace) -> bool {
    let db_lower = namespace.database.to_lowercase();
    matches!(
        db_lower.as_str(),
        "information_schema"
            | "performance_schema"
            | "mysql"
            | "sys"
            | "pg_catalog"
            | "pg_toast"
            | "admin"
            | "local"
            | "config"
    )
}

fn is_system_table(table_name: &str) -> bool {
    let lower = table_name.to_lowercase();
    lower.starts_with("pg_")
        || lower.starts_with("sql_")
        || lower.starts_with("information_schema")
        || lower.starts_with("_")
}

fn value_to_preview(value: &Value, max_len: usize) -> String {
    let s = match value {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Text(t) => t.clone(),
        Value::Bytes(_) => "[binary]".to_string(),
        Value::Json(j) => j.to_string(),
        Value::Array(a) => format!("[{} items]", a.len()),
    };

    if s.len() > max_len {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    } else {
        s
    }
}

fn value_contains(value: &Value, search_term: &str, case_sensitive: bool) -> bool {
    let text = match value {
        Value::Text(t) => t.clone(),
        Value::Json(j) => j.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Bytes(bytes) => String::from_utf8_lossy(bytes).to_string(),
        Value::Array(a) => format!("[{} items]", a.len()),
        Value::Null => return false,
    };

    if case_sensitive {
        text.contains(search_term)
    } else {
        text.to_lowercase().contains(&search_term.to_lowercase())
    }
}

/// Result of searching a single table
struct TableSearchResult {
    matches: Vec<FulltextMatch>,
    method: SearchMethod,
    timed_out: bool,
    error: Option<String>,
}

/// Performs a full-text search across all tables and columns in the database
#[tauri::command]
#[instrument(skip(state, app_handle), fields(session_id = %session_id, search_term_len = search_term.len()))]
pub async fn fulltext_search(
    state: State<'_, crate::SharedState>,
    app_handle: AppHandle,
    session_id: String,
    search_term: String,
    options: Option<FulltextSearchOptions>,
) -> Result<FulltextSearchResponse, String> {
    let start_time = std::time::Instant::now();

    // Validate search term
    let search_term = search_term.trim();
    if search_term.is_empty() {
        return Ok(empty_response("Search term cannot be empty"));
    }

    if search_term.len() < 2 {
        return Ok(empty_response("Search term must be at least 2 characters"));
    }

    let opts = options.unwrap_or_default();
    let max_per_table = opts.max_results_per_table.unwrap_or(10).min(50);
    let max_total = opts.max_total_results.unwrap_or(100).min(500);
    let case_sensitive = opts.case_sensitive.unwrap_or(false);
    let table_timeout_ms = opts.timeout_per_table_ms.unwrap_or(DEFAULT_TABLE_TIMEOUT_MS);
    let max_parallel = opts.max_parallel.unwrap_or(MAX_PARALLEL_TABLES).min(10);
    let stream_results = opts.stream_results.unwrap_or(false);

    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };

    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(error_response(
                &e.to_string(),
                start_time.elapsed().as_secs_f64() * 1000.0,
            ));
        }
    };

    let driver_id = driver.driver_id();
    let search_strategy = get_search_strategy(driver_id);
    let capability_cache = get_capability_cache();

    // Get namespaces to search
    let namespaces = if let Some(ns) = opts.namespaces {
        ns
    } else {
        match driver.list_namespaces(session).await {
            Ok(ns) => ns,
            Err(e) => {
                return Ok(error_response(
                    &format!("Failed to list namespaces: {}", e),
                    start_time.elapsed().as_secs_f64() * 1000.0,
                ));
            }
        }
    };

    // Collect all tables to search with their capabilities
    let mut tables_to_search: Vec<(Namespace, String, Vec<String>)> = Vec::new();

    let is_sqlite = driver.driver_id().eq_ignore_ascii_case("sqlite");

    for namespace in namespaces {
        if is_system_namespace(&namespace) {
            continue;
        }

        let collections = match driver
            .list_collections(session, &namespace, CollectionListOptions::default())
            .await
        {
            Ok(c) => c,
            Err(_) => continue,
        };

        for collection in collections.collections {
            if !matches!(
                collection.collection_type,
                CollectionType::Table | CollectionType::Collection
            ) {
                continue;
            }

            if is_system_table(&collection.name) {
                continue;
            }

            if let Some(ref tables) = opts.tables {
                if !tables
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(&collection.name))
                {
                    continue;
                }
            }

            let schema = match driver
                .describe_table(session, &namespace, &collection.name)
                .await
            {
                Ok(s) => s,
                Err(_) => continue,
            };

            let text_columns: Vec<String> = if is_sqlite {
                schema.columns.iter().map(|c| c.name.clone()).collect()
            } else {
                schema
                    .columns
                    .iter()
                    .filter(|c| is_text_type(&c.data_type))
                    .map(|c| c.name.clone())
                    .collect()
            };

            if !text_columns.is_empty() {
                tables_to_search.push((namespace.clone(), collection.name, text_columns));
            }
        }
    }

    let total_tables = tables_to_search.len() as u32;

    if total_tables == 0 {
        return Ok(FulltextSearchResponse {
            success: true,
            matches: vec![],
            total_matches: 0,
            tables_searched: 0,
            search_time_ms: start_time.elapsed().as_secs_f64() * 1000.0,
            error: None,
            truncated: false,
            stats: SearchStats {
                native_fulltext_count: 0,
                pattern_match_count: 0,
                timeout_count: 0,
                error_count: 0,
            },
        });
    }

    // Search options
    let search_options = TableSearchOptions {
        search_term: search_term.to_string(),
        case_sensitive,
        max_results: max_per_table,
        timeout_ms: Some(table_timeout_ms),
        prefer_native: true,
    };

    // Counter for streaming progress
    let tables_searched_counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let matches_found_counter = Arc::new(std::sync::atomic::AtomicU32::new(0));

    let driver_ref = &driver;
    let strategy_ref = search_strategy.as_ref();
    let search_options_ref = &search_options;
    let capability_cache_ref = &capability_cache;
    let app_handle_ref = &app_handle;
    let session_id_ref = &session_id;
    let tables_searched_counter_ref = &tables_searched_counter;
    let matches_found_counter_ref = &matches_found_counter;

    // Search tables in parallel
    let results: Vec<TableSearchResult> = stream::iter(tables_to_search)
        .map(|(namespace, table_name, text_columns)| async move {
            let text_column_set: HashSet<String> =
                text_columns.iter().map(|c| c.to_lowercase()).collect();
            // Check cache first
            let capability = if let Some(cached) =
                capability_cache_ref.get(&namespace, &table_name).await
            {
                debug!("Using cached capability for {}.{}", namespace.database, table_name);
                cached
            } else {
                // Detect full-text indexes
                let detected_indexes = detect_fulltext_indexes(
                    driver_ref,
                    session,
                    strategy_ref,
                    &namespace,
                    &table_name,
                )
                .await;

                let capability =
                    strategy_ref.build_capability(&text_columns, &detected_indexes, None);

                // Cache the result
                capability_cache_ref
                    .set(&namespace, &table_name, capability.clone())
                    .await;

                capability
            };

            // Build and execute query
            let (query, method) = strategy_ref.build_search_query(
                &namespace,
                &table_name,
                &capability,
                search_options_ref,
            );
            if is_sqlite {
                debug!(
                    "SQLite search query for {}.{} ({} cols): {}",
                    namespace.database,
                    table_name,
                    text_columns.len(),
                    query
                );
            }

            let query_id = QueryId::new();
            let search_future =
                driver_ref.execute_in_namespace(session, Some(namespace.clone()), &query, query_id);

            let timeout_duration = Duration::from_millis(table_timeout_ms);
            let result = timeout(timeout_duration, search_future).await;

            // Update progress counter
            tables_searched_counter_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            let table_result = match result {
                Ok(Ok(query_result)) => {
                    if is_sqlite {
                        debug!(
                            "SQLite search result {}.{}: {} rows, {} cols",
                            namespace.database,
                            table_name,
                            query_result.rows.len(),
                            query_result.columns.len()
                        );
                    }
                    let mut matches = Vec::new();

                    for row in query_result.rows {
                        for (idx, col_info) in query_result.columns.iter().enumerate() {
                            if let Some(value) = row.values.get(idx) {
                                let col_name = col_info.name.to_lowercase();
                                let is_searchable =
                                    text_column_set.contains(&col_name)
                                        || is_text_type(&col_info.data_type);
                                if is_searchable
                                    && value_contains(
                                        value,
                                        &search_options_ref.search_term,
                                        case_sensitive,
                                    )
                                {
                                    let row_preview: Vec<(String, Value)> = query_result
                                        .columns
                                        .iter()
                                        .zip(row.values.iter())
                                        .map(|(c, v)| (c.name.clone(), v.clone()))
                                        .collect();

                                    matches.push(FulltextMatch {
                                        namespace: namespace.clone(),
                                        table_name: table_name.clone(),
                                        column_name: col_info.name.clone(),
                                        value_preview: value_to_preview(value, 100),
                                        row_preview,
                                    });

                                    if matches.len() >= max_per_table as usize {
                                        break;
                                    }
                                }
                            }
                        }
                        if matches.len() >= max_per_table as usize {
                            break;
                        }
                    }

                    // Update matches counter
                    matches_found_counter_ref
                        .fetch_add(matches.len() as u32, std::sync::atomic::Ordering::SeqCst);

                    TableSearchResult {
                        matches,
                        method,
                        timed_out: false,
                        error: None,
                    }
                }
                Ok(Err(e)) => {
                    warn!(
                        "Search error in {}.{}: {}",
                        namespace.database, table_name, e
                    );
                    TableSearchResult {
                        matches: vec![],
                        method,
                        timed_out: false,
                        error: Some(e.to_string()),
                    }
                }
                Err(_) => {
                    warn!("Search timeout in {}.{}", namespace.database, table_name);
                    TableSearchResult {
                        matches: vec![],
                        method,
                        timed_out: true,
                        error: None,
                    }
                }
            };

            // Emit progress event if streaming
            if stream_results {
                let progress = SearchProgressEvent {
                    tables_searched: tables_searched_counter_ref
                        .load(std::sync::atomic::Ordering::SeqCst),
                    total_tables,
                    matches_found: matches_found_counter_ref
                        .load(std::sync::atomic::Ordering::SeqCst),
                    current_table: Some(format!("{}.{}", namespace.database, table_name)),
                };
                let _ = app_handle_ref.emit(
                    &format!("fulltext_search_progress:{}", session_id_ref),
                    progress,
                );
            }

            table_result
        })
        .buffer_unordered(max_parallel)
        .collect()
        .await;

    // Aggregate results
    let mut all_matches: Vec<FulltextMatch> = Vec::new();
    let mut stats = SearchStats {
        native_fulltext_count: 0,
        pattern_match_count: 0,
        timeout_count: 0,
        error_count: 0,
    };
    let mut truncated = false;

    for result in results {
        match result.method {
            SearchMethod::NativeFulltext => stats.native_fulltext_count += 1,
            SearchMethod::PatternMatch => stats.pattern_match_count += 1,
            SearchMethod::Hybrid => {
                stats.native_fulltext_count += 1;
                stats.pattern_match_count += 1;
            }
        }

        if result.timed_out {
            stats.timeout_count += 1;
        }

        if result.error.is_some() {
            stats.error_count += 1;
        }

        for m in result.matches {
            if all_matches.len() >= max_total as usize {
                truncated = true;
                break;
            }
            all_matches.push(m);
        }

        if truncated {
            break;
        }
    }

    let total_matches = all_matches.len() as u64;
    let search_time_ms = start_time.elapsed().as_secs_f64() * 1000.0;

    debug!(
        "Search completed: {} matches in {:.0}ms (native: {}, pattern: {}, timeouts: {})",
        total_matches,
        search_time_ms,
        stats.native_fulltext_count,
        stats.pattern_match_count,
        stats.timeout_count
    );

    Ok(FulltextSearchResponse {
        success: true,
        matches: all_matches,
        total_matches,
        tables_searched: total_tables,
        search_time_ms,
        error: None,
        truncated,
        stats,
    })
}

/// Detect full-text indexes for a table
async fn detect_fulltext_indexes(
    driver: &Arc<dyn crate::engine::traits::DataEngine>,
    session: crate::engine::types::SessionId,
    strategy: &dyn crate::engine::fulltext_strategy::FulltextSearchStrategy,
    namespace: &Namespace,
    table_name: &str,
) -> Vec<FulltextIndexInfo> {
    // Get index detection query from strategy
    let Some(detection_query) = strategy.build_index_detection_query(namespace, table_name) else {
        return Vec::new();
    };

    // Execute detection query
    let query_id = QueryId::new();
    let result = match driver
        .execute_in_namespace(session, Some(namespace.clone()), &detection_query, query_id)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            debug!(
                "Index detection failed for {}.{}: {}",
                namespace.database, table_name, e
            );
            return Vec::new();
        }
    };

    // Convert rows to Vec<Vec<Value>> format
    let rows: Vec<Vec<Value>> = result.rows.into_iter().map(|r| r.values).collect();
    let columns: Vec<String> = result.columns.into_iter().map(|c| c.name).collect();

    // Parse results using strategy
    strategy.parse_index_detection_result(&rows, &columns)
}

fn empty_response(error: &str) -> FulltextSearchResponse {
    FulltextSearchResponse {
        success: false,
        matches: vec![],
        total_matches: 0,
        tables_searched: 0,
        search_time_ms: 0.0,
        error: Some(error.to_string()),
        truncated: false,
        stats: SearchStats {
            native_fulltext_count: 0,
            pattern_match_count: 0,
            timeout_count: 0,
            error_count: 0,
        },
    }
}

fn error_response(error: &str, time_ms: f64) -> FulltextSearchResponse {
    FulltextSearchResponse {
        success: false,
        matches: vec![],
        total_matches: 0,
        tables_searched: 0,
        search_time_ms: time_ms,
        error: Some(error.to_string()),
        truncated: false,
        stats: SearchStats {
            native_fulltext_count: 0,
            pattern_match_count: 0,
            timeout_count: 0,
            error_count: 0,
        },
    }
}

impl Default for FulltextSearchOptions {
    fn default() -> Self {
        Self {
            max_results_per_table: None,
            max_total_results: None,
            case_sensitive: None,
            namespaces: None,
            tables: None,
            timeout_per_table_ms: None,
            max_parallel: None,
            stream_results: None,
        }
    }
}

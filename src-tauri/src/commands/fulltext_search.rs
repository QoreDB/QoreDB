//! Full-text Search Tauri Commands
//!
//! Provides global search functionality across all tables and columns in a database.
//! Uses driver-specific optimized strategies with automatic fallback.

use futures::stream::{self, StreamExt};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;
use tokio::time::timeout;
use tracing::{instrument, warn};

use crate::engine::fulltext_strategy::{
    get_search_strategy, SearchMethod, TableSearchCapability, TableSearchOptions,
};
use crate::engine::types::{CollectionListOptions, CollectionType, Namespace, QueryId, Value};

/// Maximum number of tables to search in parallel
const MAX_PARALLEL_TABLES: usize = 5;

/// Default timeout per table search (milliseconds)
const DEFAULT_TABLE_TIMEOUT_MS: u64 = 5000;

/// A single match found during full-text search
#[derive(Debug, Clone, Serialize)]
pub struct FulltextMatch {
    /// The namespace (database/schema) containing the match
    pub namespace: Namespace,
    /// The table name where the match was found
    pub table_name: String,
    /// The column name where the match was found
    pub column_name: String,
    /// Preview of the matching value
    pub value_preview: String,
    /// The full row data for context
    pub row_preview: Vec<(String, Value)>,
}

/// Statistics about the search execution
#[derive(Debug, Clone, Serialize)]
pub struct SearchStats {
    /// Number of tables that used native full-text search
    pub native_fulltext_count: u32,
    /// Number of tables that used pattern matching (LIKE)
    pub pattern_match_count: u32,
    /// Number of tables that timed out
    pub timeout_count: u32,
    /// Number of tables that had errors
    pub error_count: u32,
}

/// Response for full-text search
#[derive(Debug, Serialize)]
pub struct FulltextSearchResponse {
    pub success: bool,
    /// All matches found, grouped by table/column
    pub matches: Vec<FulltextMatch>,
    /// Total number of matches found
    pub total_matches: u64,
    /// Number of tables searched
    pub tables_searched: u32,
    /// Time taken in milliseconds
    pub search_time_ms: f64,
    /// Error message if any
    pub error: Option<String>,
    /// Whether the search was cancelled due to limits
    pub truncated: bool,
    /// Detailed statistics
    pub stats: SearchStats,
}

/// Options for full-text search
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FulltextSearchOptions {
    /// Maximum results per table (default: 10)
    pub max_results_per_table: Option<u32>,
    /// Maximum total results (default: 100)
    pub max_total_results: Option<u32>,
    /// Case sensitive search (default: false)
    pub case_sensitive: Option<bool>,
    /// Specific namespaces to search (if empty, search all)
    pub namespaces: Option<Vec<Namespace>>,
    /// Specific tables to search (if empty, search all)
    pub tables: Option<Vec<String>>,
    /// Timeout per table in milliseconds (default: 5000)
    pub timeout_per_table_ms: Option<u64>,
    /// Maximum tables to search in parallel (default: 5)
    pub max_parallel: Option<usize>,
}

fn parse_session_id(id: &str) -> Result<crate::engine::types::SessionId, String> {
    let uuid = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(crate::engine::types::SessionId(uuid))
}

/// Check if a data type is text-like (searchable)
fn is_text_type(data_type: &str) -> bool {
    let dt = data_type.to_lowercase();

    // Common SQL text types
    if dt.contains("char")
        || dt.contains("text")
        || dt.contains("varchar")
        || dt.contains("string")
        || dt.contains("clob")
        || dt.contains("name")
        || dt.contains("uuid")
        || dt.contains("json")
        || dt.contains("xml")
        || dt.contains("enum")
    {
        return true;
    }

    // MongoDB types
    if dt == "string" || dt == "objectid" {
        return true;
    }

    false
}

/// Check if a namespace should be skipped (system databases)
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

/// Check if a table should be skipped (system tables)
fn is_system_table(table_name: &str) -> bool {
    let lower = table_name.to_lowercase();
    lower.starts_with("pg_")
        || lower.starts_with("sql_")
        || lower.starts_with("information_schema")
        || lower.starts_with("_")
}

/// Extract a preview string from a Value
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
        format!("{}...", &s[..max_len - 3])
    } else {
        s
    }
}

/// Check if a value contains the search term
fn value_contains(value: &Value, search_term: &str, case_sensitive: bool) -> bool {
    let text = match value {
        Value::Text(t) => t.clone(),
        Value::Json(j) => j.to_string(),
        _ => return false,
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
#[instrument(skip(state), fields(session_id = %session_id, search_term_len = search_term.len()))]
pub async fn fulltext_search(
    state: State<'_, crate::SharedState>,
    session_id: String,
    search_term: String,
    options: Option<FulltextSearchOptions>,
) -> Result<FulltextSearchResponse, String> {
    let start_time = std::time::Instant::now();

    // Validate search term
    let search_term = search_term.trim();
    if search_term.is_empty() {
        return Ok(FulltextSearchResponse {
            success: false,
            matches: vec![],
            total_matches: 0,
            tables_searched: 0,
            search_time_ms: 0.0,
            error: Some("Search term cannot be empty".to_string()),
            truncated: false,
            stats: SearchStats {
                native_fulltext_count: 0,
                pattern_match_count: 0,
                timeout_count: 0,
                error_count: 0,
            },
        });
    }

    if search_term.len() < 2 {
        return Ok(FulltextSearchResponse {
            success: false,
            matches: vec![],
            total_matches: 0,
            tables_searched: 0,
            search_time_ms: 0.0,
            error: Some("Search term must be at least 2 characters".to_string()),
            truncated: false,
            stats: SearchStats {
                native_fulltext_count: 0,
                pattern_match_count: 0,
                timeout_count: 0,
                error_count: 0,
            },
        });
    }

    let opts = options.unwrap_or(FulltextSearchOptions {
        max_results_per_table: None,
        max_total_results: None,
        case_sensitive: None,
        namespaces: None,
        tables: None,
        timeout_per_table_ms: None,
        max_parallel: None,
    });

    let max_per_table = opts.max_results_per_table.unwrap_or(10).min(50);
    let max_total = opts.max_total_results.unwrap_or(100).min(500);
    let case_sensitive = opts.case_sensitive.unwrap_or(false);
    let table_timeout_ms = opts.timeout_per_table_ms.unwrap_or(DEFAULT_TABLE_TIMEOUT_MS);
    let max_parallel = opts.max_parallel.unwrap_or(MAX_PARALLEL_TABLES).min(10);

    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };

    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(FulltextSearchResponse {
                success: false,
                matches: vec![],
                total_matches: 0,
                tables_searched: 0,
                search_time_ms: start_time.elapsed().as_secs_f64() * 1000.0,
                error: Some(e.to_string()),
                truncated: false,
                stats: SearchStats {
                    native_fulltext_count: 0,
                    pattern_match_count: 0,
                    timeout_count: 0,
                    error_count: 0,
                },
            });
        }
    };

    let driver_id = driver.driver_id();
    let search_strategy = get_search_strategy(driver_id);

    // Get namespaces to search
    let namespaces = if let Some(ns) = opts.namespaces {
        ns
    } else {
        match driver.list_namespaces(session).await {
            Ok(ns) => ns,
            Err(e) => {
                return Ok(FulltextSearchResponse {
                    success: false,
                    matches: vec![],
                    total_matches: 0,
                    tables_searched: 0,
                    search_time_ms: start_time.elapsed().as_secs_f64() * 1000.0,
                    error: Some(format!("Failed to list namespaces: {}", e)),
                    truncated: false,
                    stats: SearchStats {
                        native_fulltext_count: 0,
                        pattern_match_count: 0,
                        timeout_count: 0,
                        error_count: 0,
                    },
                });
            }
        }
    };

    // Collect all tables to search
    let mut tables_to_search: Vec<(Namespace, String, Vec<String>)> = Vec::new();

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

            // Get table schema to find text columns
            let schema = match driver
                .describe_table(session, &namespace, &collection.name)
                .await
            {
                Ok(s) => s,
                Err(_) => continue,
            };

            let text_columns: Vec<String> = schema
                .columns
                .iter()
                .filter(|c| is_text_type(&c.data_type))
                .map(|c| c.name.clone())
                .collect();

            if !text_columns.is_empty() {
                tables_to_search.push((namespace.clone(), collection.name, text_columns));
            }
        }
    }

    let tables_searched = tables_to_search.len() as u32;

    // Search tables in parallel with timeout
    let search_options = TableSearchOptions {
        search_term: search_term.to_string(),
        case_sensitive,
        max_results: max_per_table,
        timeout_ms: Some(table_timeout_ms),
        prefer_native: true,
    };

    let driver_ref = &driver;
    let strategy_ref = search_strategy.as_ref();
    let search_options_ref = &search_options;

    let results: Vec<TableSearchResult> = stream::iter(tables_to_search)
        .map(|(namespace, table_name, text_columns)| async move {
            // Analyze table capabilities
            let capability = match strategy_ref
                .analyze_table(&namespace, &table_name, &text_columns)
                .await
            {
                Ok(cap) => cap,
                Err(_) => TableSearchCapability {
                    searchable_columns: text_columns
                        .iter()
                        .map(|name| crate::engine::fulltext_strategy::ColumnSearchInfo {
                            name: name.clone(),
                            data_type: "text".to_string(),
                            has_fulltext_index: false,
                            fulltext_index_name: None,
                        })
                        .collect(),
                    recommended_method: SearchMethod::PatternMatch,
                    estimated_rows: None,
                },
            };

            // Build query using strategy
            let (query, method) =
                strategy_ref.build_search_query(&namespace, &table_name, &capability, search_options_ref);

            // Execute with timeout
            let query_id = QueryId::new();
            let search_future = driver_ref.execute_in_namespace(
                session,
                Some(namespace.clone()),
                &query,
                query_id,
            );

            let timeout_duration = Duration::from_millis(table_timeout_ms);
            let result = timeout(timeout_duration, search_future).await;

            match result {
                Ok(Ok(query_result)) => {
                    // Process matching rows
                    let mut matches = Vec::new();

                    for row in query_result.rows {
                        for (idx, col_info) in query_result.columns.iter().enumerate() {
                            if let Some(value) = row.values.get(idx) {
                                if is_text_type(&col_info.data_type)
                                    && value_contains(value, &search_options_ref.search_term, case_sensitive)
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

                    TableSearchResult {
                        matches,
                        method,
                        timed_out: false,
                        error: None,
                    }
                }
                Ok(Err(e)) => {
                    warn!("Search error in {}.{}: {}", namespace.database, table_name, e);
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
            }
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

    Ok(FulltextSearchResponse {
        success: true,
        matches: all_matches,
        total_matches,
        tables_searched,
        search_time_ms,
        error: None,
        truncated,
        stats,
    })
}

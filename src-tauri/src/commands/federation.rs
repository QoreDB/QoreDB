// SPDX-License-Identifier: BUSL-1.1

//! Cross-Database Federation Tauri Commands
//!
//! Commands for executing federated queries across multiple database connections.
//! Federation is a Pro feature — Core builds return an explicit error.

use std::collections::HashMap;

use serde::Serialize;
use tauri::State;

use crate::SharedState;

// ─── Core stubs (compiled when pro feature is disabled) ──────

#[cfg(not(feature = "pro"))]
const PRO_REQUIRED: &str = "Cross-database federation requires a Pro license.";

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn execute_federation_query(
    _state: State<'_, SharedState>,
    _window: tauri::Window,
    _query: String,
    _alias_map: HashMap<String, String>,
    _options: Option<serde_json::Value>,
) -> Result<FederationQueryResponse, String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn list_federation_sources(
    _state: State<'_, SharedState>,
) -> Result<Vec<serde_json::Value>, String> {
    Err(PRO_REQUIRED.to_string())
}

// ─── Response types (always compiled) ────────────────────────

/// Response for federation queries. Extends QueryResponse with federation metadata.
#[derive(Debug, Serialize)]
pub struct FederationQueryResponse {
    pub success: bool,
    pub result: Option<crate::engine::types::QueryResult>,
    pub error: Option<String>,
    pub query_id: Option<String>,
    pub federation: Option<FederationMeta>,
}

/// Metadata about the federation execution, serialized for the frontend.
#[derive(Debug, Serialize)]
pub struct FederationMeta {
    pub source_results: Vec<SourceFetchInfo>,
    pub duckdb_time_ms: f64,
    pub total_time_ms: f64,
    pub warnings: Vec<String>,
}

/// Per-source fetch info for the frontend.
#[derive(Debug, Serialize)]
pub struct SourceFetchInfo {
    pub alias: String,
    pub table: String,
    pub row_count: u64,
    pub fetch_time_ms: f64,
    pub row_limit_hit: bool,
}

// ─── Pro implementation ──────────────────────────────────────

#[cfg(feature = "pro")]
use std::sync::Arc;

#[cfg(feature = "pro")]
use tauri::Emitter;

#[cfg(feature = "pro")]
use uuid::Uuid;

#[cfg(feature = "pro")]
use crate::engine::traits::StreamEvent;
#[cfg(feature = "pro")]
use crate::engine::types::SessionId;
#[cfg(feature = "pro")]
use crate::federation::manager;
#[cfg(feature = "pro")]
use crate::federation::types::{
    AliasEntry, ConnectionAliasMap, FederationQueryOptions, FederationSource,
};

#[cfg(feature = "pro")]
fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {e}"))?;
    Ok(SessionId(uuid))
}

/// Executes a cross-database federation query.
///
/// The frontend provides:
/// - `query`: The SQL with 3-part identifiers (e.g., `prod_pg.public.users`)
/// - `alias_map`: mapping from alias to session_id
/// - `options`: timeout, streaming, row limit
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn execute_federation_query(
    state: State<'_, SharedState>,
    window: tauri::Window,
    query: String,
    alias_map: HashMap<String, String>,
    options: Option<FederationQueryOptions>,
) -> Result<FederationQueryResponse, String> {
    let options = options.unwrap_or(FederationQueryOptions {
        timeout_ms: None,
        stream: None,
        query_id: None,
        row_limit_per_source: None,
    });

    let query_id = options
        .query_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let streaming = options.stream.unwrap_or(false);

    // Resolve alias_map (alias -> session_id string) to ConnectionAliasMap
    let resolved_map = {
        let app_state = state.lock().await;
        resolve_alias_map(&alias_map, &app_state.session_manager).await?
    };

    let session_manager = {
        let app_state = state.lock().await;
        Arc::clone(&app_state.session_manager)
    };

    if streaming {
        // Streaming mode: send events via Tauri, return empty response
        let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(1024);

        let query_clone = query.clone();
        let _query_id_clone = query_id.clone();
        let options_clone = options.clone();
        let resolved_map_clone = resolved_map.clone();
        let sm = Arc::clone(&session_manager);

        // Spawn the federation execution
        let handle = tokio::spawn(async move {
            manager::execute_federation_stream(
                &query_clone,
                &resolved_map_clone,
                &sm,
                &options_clone,
                tx,
            )
            .await
        });

        // Spawn the event forwarder
        let window_clone = window.clone();
        let qid = query_id.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::Columns(cols) => {
                        let _ = window_clone.emit(&format!("query_stream_columns:{qid}"), &cols);
                    }
                    StreamEvent::Row(row) => {
                        let _ = window_clone.emit(&format!("query_stream_row:{qid}"), &row);
                    }
                    StreamEvent::Error(err) => {
                        let _ = window_clone.emit(&format!("query_stream_error:{qid}"), &err);
                    }
                    StreamEvent::Done(count) => {
                        let _ = window_clone.emit(&format!("query_stream_done:{qid}"), &count);
                    }
                }
            }
        });

        // Wait for the federation to complete
        let meta = handle
            .await
            .map_err(|e| format!("Federation task panicked: {e}"))?
            .map_err(|e| e.to_string())?;

        Ok(FederationQueryResponse {
            success: true,
            result: None, // Streamed via events
            error: None,
            query_id: Some(query_id),
            federation: Some(convert_metadata(&meta)),
        })
    } else {
        // Batch mode: return full result
        match manager::execute_federation(&query, &resolved_map, &session_manager, &options).await {
            Ok((result, meta)) => Ok(FederationQueryResponse {
                success: true,
                result: Some(result),
                error: None,
                query_id: Some(query_id),
                federation: Some(convert_metadata(&meta)),
            }),
            Err(e) => Ok(FederationQueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                query_id: Some(query_id),
                federation: None,
            }),
        }
    }
}

/// Lists all active connections available as federation sources.
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn list_federation_sources(
    state: State<'_, SharedState>,
) -> Result<Vec<FederationSource>, String> {
    let app_state = state.lock().await;
    let sessions = app_state.session_manager.list_sessions().await;

    let mut sources = Vec::new();
    for (session_id, display_name) in sessions {
        // Get the driver ID for this session
        if let Ok(driver) = app_state.session_manager.get_driver(session_id).await {
            let alias = normalize_alias(&display_name);
            sources.push(FederationSource {
                alias,
                session_id: session_id.0.to_string(),
                driver: driver.driver_id().to_string(),
                display_name,
            });
        }
    }

    Ok(sources)
}

// ─── Helper functions ────────────────────────────────────────

#[cfg(feature = "pro")]
async fn resolve_alias_map(
    alias_map: &HashMap<String, String>,
    session_manager: &Arc<crate::engine::SessionManager>,
) -> Result<ConnectionAliasMap, String> {
    let mut resolved = ConnectionAliasMap::new();

    for (alias, session_id_str) in alias_map {
        let session_id = parse_session_id(session_id_str)?;

        // Verify session exists
        if !session_manager.session_exists(session_id).await {
            return Err(format!("Session '{session_id_str}' not found for alias '{alias}'"));
        }

        let driver = session_manager
            .get_driver(session_id)
            .await
            .map_err(|e| format!("Failed to get driver for '{alias}': {e}"))?;

        let display_name = session_manager
            .get_session_info(session_id)
            .await
            .unwrap_or_else(|| alias.clone());

        resolved.insert(
            alias.clone(),
            AliasEntry {
                session_id,
                driver_id: driver.driver_id().to_string(),
                display_name,
            },
        );
    }

    Ok(resolved)
}

#[cfg(feature = "pro")]
fn convert_metadata(meta: &crate::federation::types::FederationMetadata) -> FederationMeta {
    FederationMeta {
        source_results: meta
            .source_results
            .iter()
            .map(|s| SourceFetchInfo {
                alias: s.alias.clone(),
                table: s.table.clone(),
                row_count: s.row_count,
                fetch_time_ms: s.fetch_time_ms,
                row_limit_hit: s.row_limit_hit,
            })
            .collect(),
        duckdb_time_ms: meta.duckdb_time_ms,
        total_time_ms: meta.total_time_ms,
        warnings: meta.warnings.clone(),
    }
}

/// Normalizes a connection display name into a SQL-safe alias.
/// e.g., "Production PostgreSQL (SSH)" -> "production_postgresql_ssh"
#[cfg(feature = "pro")]
fn normalize_alias(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
        // Collapse multiple underscores
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "pro")]
    use super::normalize_alias;

    #[cfg(feature = "pro")]
    #[test]
    fn normalizes_display_names() {
        assert_eq!(normalize_alias("Production PostgreSQL"), "production_postgresql");
        assert_eq!(normalize_alias("my-db (SSH)"), "my_db_ssh");
        assert_eq!(normalize_alias("user@host:5432/db"), "user_host_5432_db");
        assert_eq!(normalize_alias("Simple"), "simple");
    }
}

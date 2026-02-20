// SPDX-License-Identifier: BUSL-1.1

//! Types for the Cross-Database Federation engine.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::engine::types::{Namespace, SessionId};

/// A reference to a table from a specific database connection.
#[derive(Debug, Clone)]
pub struct FederatedTableRef {
    /// Connection alias (normalized saved connection name, e.g., "prod_pg")
    pub connection_alias: String,
    /// The namespace (database + optional schema)
    pub namespace: Namespace,
    /// Table name
    pub table: String,
    /// Local alias used in the DuckDB rewritten query (e.g., "__fed_users_0")
    pub local_alias: String,
}

/// Source fetch plan for a single federated table.
#[derive(Debug, Clone)]
pub struct SourceFetchPlan {
    /// The federated table reference
    pub table_ref: FederatedTableRef,
    /// Session ID of the connection to fetch from
    pub session_id: SessionId,
    /// Driver ID (e.g., "postgres", "mysql", "mongodb")
    pub driver_id: String,
    /// Columns needed (None = SELECT *)
    pub columns: Option<Vec<String>>,
    /// WHERE predicates that can be pushed down to the source
    pub pushdown_predicates: Vec<String>,
    /// Row limit safety cap (default: 100_000)
    pub row_limit: u64,
}

/// The complete execution plan for a federation query.
#[derive(Debug, Clone)]
pub struct FederationPlan {
    /// All source tables to fetch
    pub sources: Vec<SourceFetchPlan>,
    /// The rewritten SQL to execute on DuckDB (with local temp table names)
    pub duckdb_query: String,
    /// Original user query (for audit logging)
    pub original_query: String,
    /// Whether streaming is requested
    pub streaming: bool,
}

/// Mapping from connection alias to session info.
pub type ConnectionAliasMap = HashMap<String, AliasEntry>;

/// Entry in the connection alias map.
#[derive(Debug, Clone)]
pub struct AliasEntry {
    pub session_id: SessionId,
    pub driver_id: String,
    pub display_name: String,
}

/// Default row limit per source table.
pub const DEFAULT_ROW_LIMIT: u64 = 100_000;

/// Options for a federation query execution.
#[derive(Debug, Clone, Deserialize)]
pub struct FederationQueryOptions {
    /// Query timeout in milliseconds (default: 60_000)
    pub timeout_ms: Option<u64>,
    /// Enable streaming results
    pub stream: Option<bool>,
    /// Query ID for tracking/cancellation
    pub query_id: Option<String>,
    /// Row limit per source table (default: 100_000)
    pub row_limit_per_source: Option<u64>,
}

/// A federation source exposed to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct FederationSource {
    /// SQL-safe alias for use in queries
    pub alias: String,
    /// Active session ID
    pub session_id: String,
    /// Driver type (e.g., "postgres", "mysql", "mongodb")
    pub driver: String,
    /// Human-readable connection name
    pub display_name: String,
}

/// Result metadata for a federation query source fetch.
#[derive(Debug, Clone, Serialize)]
pub struct SourceFetchResult {
    /// Connection alias
    pub alias: String,
    /// Table name
    pub table: String,
    /// Number of rows fetched
    pub row_count: u64,
    /// Fetch duration in milliseconds
    pub fetch_time_ms: f64,
    /// Whether the row limit was hit
    pub row_limit_hit: bool,
}

/// Extended federation query response with source metadata.
#[derive(Debug, Clone, Serialize)]
pub struct FederationMetadata {
    /// Per-source fetch results
    pub source_results: Vec<SourceFetchResult>,
    /// DuckDB execution time in milliseconds
    pub duckdb_time_ms: f64,
    /// Total pipeline time in milliseconds
    pub total_time_ms: f64,
    /// Warnings (e.g., row limit hits)
    pub warnings: Vec<String>,
}

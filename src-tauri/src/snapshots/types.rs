// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::engine::types::{ColumnInfo, Namespace, QueryResult, Row};

/// Metadata about a snapshot (returned in list operations, without the full data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMeta {
    /// Unique identifier (UUID)
    pub id: String,
    /// User-facing name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// SQL query or table name that produced this data
    pub source: String,
    /// Source type: "query" or "table"
    pub source_type: String,
    /// Connection name at time of capture
    pub connection_name: Option<String>,
    /// Driver used (postgres, mysql, etc.)
    pub driver: Option<String>,
    /// Namespace at time of capture
    pub namespace: Option<Namespace>,
    /// Column definitions
    pub columns: Vec<ColumnInfo>,
    /// Number of rows captured
    pub row_count: usize,
    /// Timestamp of creation (ISO 8601)
    pub created_at: String,
    /// File size in bytes (populated when listing)
    #[serde(default)]
    pub file_size: u64,
}

/// Full snapshot with data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub meta: SnapshotMeta,
    pub rows: Vec<Row>,
}

impl Snapshot {
    /// Convert the snapshot data into a QueryResult for display or diff
    pub fn to_query_result(&self) -> QueryResult {
        QueryResult {
            columns: self.meta.columns.clone(),
            rows: self.rows.clone(),
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }
}

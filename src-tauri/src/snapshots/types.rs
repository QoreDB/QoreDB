// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::engine::types::{ColumnInfo, Namespace, QueryResult, Row};

/// Metadata about a snapshot (returned in list operations, without the full data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMeta {
    /// UUID v4 identifier.
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// SQL query text or table name that produced this data.
    pub source: String,
    /// `"query"` or `"table"`.
    pub source_type: String,
    pub connection_name: Option<String>,
    pub driver: Option<String>,
    pub namespace: Option<Namespace>,
    pub columns: Vec<ColumnInfo>,
    pub row_count: usize,
    /// ISO 8601 timestamp of creation.
    pub created_at: String,
    /// File size in bytes, populated when listing.
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
    pub fn to_query_result(&self) -> QueryResult {
        QueryResult {
            columns: self.meta.columns.clone(),
            rows: self.rows.clone(),
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }
}

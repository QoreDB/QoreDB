// SPDX-License-Identifier: BUSL-1.1

//! Data Time-Travel Types
//!
//! Type definitions for the changelog, timeline, temporal diff, and configuration.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::engine::types::Namespace;

// ─── Changelog Entry ───────────────────────────────────────────────────────

/// A single row-level change record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    /// Unique identifier for this change
    pub id: Uuid,
    /// UTC timestamp of the mutation
    pub timestamp: DateTime<Utc>,
    /// Session ID (active connection)
    pub session_id: String,
    /// Driver that executed the mutation
    pub driver_id: String,
    /// Namespace (database + optional schema)
    pub namespace: Namespace,
    /// Table name
    pub table_name: String,
    /// Operation type
    pub operation: ChangeOperation,
    /// Primary key columns and their values
    pub primary_key: HashMap<String, serde_json::Value>,
    /// Row state BEFORE the mutation (None for INSERT)
    pub before: Option<HashMap<String, serde_json::Value>>,
    /// Row state AFTER the mutation (None for DELETE)
    pub after: Option<HashMap<String, serde_json::Value>>,
    /// Columns that changed (empty for INSERT/DELETE)
    pub changed_columns: Vec<String>,
    /// Connection display name (for filtering by "user")
    pub connection_name: Option<String>,
    /// Environment (development/staging/production)
    pub environment: String,
}

/// Type of mutation operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeOperation {
    Insert,
    Update,
    Delete,
}

impl std::fmt::Display for ChangeOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeOperation::Insert => write!(f, "INSERT"),
            ChangeOperation::Update => write!(f, "UPDATE"),
            ChangeOperation::Delete => write!(f, "DELETE"),
        }
    }
}

// ─── Timeline ──────────────────────────────────────────────────────────────

/// An aggregated timeline event (groups changelog entries by timestamp bucket).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    /// Operation type
    pub operation: ChangeOperation,
    /// Number of rows affected
    pub row_count: usize,
    /// Session that made the mutation
    pub session_id: String,
    /// Connection display name
    pub connection_name: Option<String>,
    /// Primary key summary (for single-row events)
    pub primary_key: Option<HashMap<String, serde_json::Value>>,
    /// Changelog entry ID (for drill-down)
    pub entry_id: Uuid,
}

// ─── Temporal Diff ─────────────────────────────────────────────────────────

/// Result of comparing a table between two points in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDiff {
    /// Column names
    pub columns: Vec<String>,
    /// Rows with their change status
    pub rows: Vec<TemporalDiffRow>,
    /// Statistics
    pub stats: TemporalDiffStats,
}

/// A single row in a temporal diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDiffRow {
    /// Primary key of the row
    pub primary_key: HashMap<String, serde_json::Value>,
    /// State at T1 (None if the row didn't exist yet)
    pub state_at_t1: Option<HashMap<String, serde_json::Value>>,
    /// State at T2 (None if the row was deleted)
    pub state_at_t2: Option<HashMap<String, serde_json::Value>>,
    /// Columns modified between T1 and T2
    pub changed_columns: Vec<String>,
    /// Row status
    pub status: DiffRowStatus,
}

/// Status of a row in a temporal diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffRowStatus {
    Added,
    Modified,
    Removed,
}

/// Statistics for a temporal diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDiffStats {
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    pub total_changes: usize,
}

// ─── Configuration ─────────────────────────────────────────────────────────

/// Configuration for the Time-Travel feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeTravelConfig {
    /// Enable/disable change capture
    pub enabled: bool,
    /// Maximum entries in the changelog file (triggers rotation)
    pub max_entries: usize,
    /// Retention period in days (0 = unlimited)
    pub retention_days: u32,
    /// Maximum changelog file size in MB
    pub max_file_size_mb: u64,
    /// Tables excluded from capture (exact names)
    pub excluded_tables: Vec<String>,
    /// Only capture mutations in production environments
    pub production_only: bool,
}

impl Default for TimeTravelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 50_000,
            retention_days: 30,
            max_file_size_mb: 500,
            excluded_tables: vec![],
            production_only: false,
        }
    }
}

// ─── Filters ───────────────────────────────────────────────────────────────

/// Filters for querying the changelog.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChangelogFilter {
    pub table_name: Option<String>,
    pub namespace: Option<Namespace>,
    pub operation: Option<ChangeOperation>,
    pub session_id: Option<String>,
    pub connection_name: Option<String>,
    pub environment: Option<String>,
    pub from_timestamp: Option<DateTime<Utc>>,
    pub to_timestamp: Option<DateTime<Utc>>,
    pub primary_key_search: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

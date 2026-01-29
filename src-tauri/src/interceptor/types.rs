//! Interceptor Types
//!
//! Type definitions for the Universal Query Interceptor system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Environment classification for connections
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Development,
    Staging,
    Production,
}

impl Default for Environment {
    fn default() -> Self {
        Self::Development
    }
}

/// Query operation type for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryOperationType {
    Select,
    Insert,
    Update,
    Delete,
    Create,
    Alter,
    Drop,
    Truncate,
    Grant,
    Revoke,
    Execute,
    Other,
}

impl Default for QueryOperationType {
    fn default() -> Self {
        Self::Other
    }
}

impl QueryOperationType {
    /// Returns true if this operation modifies data
    pub fn is_mutation(&self) -> bool {
        !matches!(self, Self::Select)
    }

    /// Returns true if this operation is potentially destructive
    pub fn is_destructive(&self) -> bool {
        matches!(self, Self::Drop | Self::Truncate | Self::Delete | Self::Alter)
    }
}

/// Action to take when a safety rule matches
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyAction {
    /// Block the query entirely
    Block,
    /// Allow but emit a warning
    Warn,
    /// Require explicit user confirmation
    RequireConfirmation,
}

impl Default for SafetyAction {
    fn default() -> Self {
        Self::Block
    }
}

/// A custom safety rule for blocking or warning on certain queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyRule {
    /// Unique identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description of what this rule does
    #[serde(default)]
    pub description: String,
    /// Whether the rule is currently active
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Environments where this rule applies
    pub environments: Vec<Environment>,
    /// Operation types this rule applies to (empty = all)
    #[serde(default)]
    pub operations: Vec<QueryOperationType>,
    /// Action to take when rule matches
    pub action: SafetyAction,
    /// Optional regex pattern to match against query text
    #[serde(default)]
    pub pattern: Option<String>,
    /// Whether this is a built-in rule (cannot be deleted)
    #[serde(default)]
    pub builtin: bool,
}

fn default_true() -> bool {
    true
}

/// Result of a safety check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyCheckResult {
    /// Whether the query is allowed to proceed
    pub allowed: bool,
    /// The action that should be taken
    pub action: SafetyAction,
    /// Rule that triggered (if any)
    pub triggered_rule: Option<String>,
    /// Human-readable message
    pub message: Option<String>,
    /// Whether confirmation is required
    pub requires_confirmation: bool,
}

impl SafetyCheckResult {
    pub fn allowed() -> Self {
        Self {
            allowed: true,
            action: SafetyAction::Warn, // no-op
            triggered_rule: None,
            message: None,
            requires_confirmation: false,
        }
    }

    pub fn blocked(rule_id: String, message: String) -> Self {
        Self {
            allowed: false,
            action: SafetyAction::Block,
            triggered_rule: Some(rule_id),
            message: Some(message),
            requires_confirmation: false,
        }
    }

    pub fn needs_confirmation(rule_id: String, message: String) -> Self {
        Self {
            allowed: false,
            action: SafetyAction::RequireConfirmation,
            triggered_rule: Some(rule_id),
            message: Some(message),
            requires_confirmation: true,
        }
    }

    pub fn warning(rule_id: String, message: String) -> Self {
        Self {
            allowed: true,
            action: SafetyAction::Warn,
            triggered_rule: Some(rule_id),
            message: Some(message),
            requires_confirmation: false,
        }
    }
}

/// An entry in the audit log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// Unique identifier
    pub id: String,
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    /// Session ID that executed the query
    pub session_id: String,
    /// The query that was executed
    pub query: String,
    /// Truncated query for display (first N chars)
    pub query_preview: String,
    /// Environment where query was executed
    pub environment: Environment,
    /// Type of operation
    pub operation_type: QueryOperationType,
    /// Database/schema name
    #[serde(default)]
    pub database: Option<String>,
    /// Whether the query succeeded
    pub success: bool,
    /// Error message if failed
    #[serde(default)]
    pub error: Option<String>,
    /// Execution time in milliseconds
    pub execution_time_ms: f64,
    /// Number of rows affected/returned
    #[serde(default)]
    pub row_count: Option<i64>,
    /// Whether the query was blocked by safety rules
    #[serde(default)]
    pub blocked: bool,
    /// Safety rule that blocked/warned (if any)
    #[serde(default)]
    pub safety_rule: Option<String>,
    /// Driver ID (postgres, mysql, etc.)
    pub driver_id: String,
}

impl AuditLogEntry {
    pub fn new(
        session_id: String,
        query: String,
        environment: Environment,
        driver_id: String,
    ) -> Self {
        let mut preview = query.chars().take(100).collect::<String>();
        let truncated = query.chars().skip(100).next().is_some();
        if truncated {
            preview.push_str("...");
        }

        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            session_id,
            query,
            query_preview: preview,
            environment,
            operation_type: QueryOperationType::Other,
            database: None,
            success: false,
            error: None,
            execution_time_ms: 0.0,
            row_count: None,
            blocked: false,
            safety_rule: None,
            driver_id,
        }
    }
}

/// Profiling metrics for query performance
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfilingMetrics {
    /// Total number of queries
    pub total_queries: u64,
    /// Number of successful queries
    pub successful_queries: u64,
    /// Number of failed queries
    pub failed_queries: u64,
    /// Number of blocked queries
    pub blocked_queries: u64,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: f64,
    /// Average execution time in milliseconds
    pub avg_execution_time_ms: f64,
    /// Minimum execution time
    pub min_execution_time_ms: f64,
    /// Maximum execution time
    pub max_execution_time_ms: f64,
    /// 50th percentile (median) execution time
    pub p50_execution_time_ms: f64,
    /// 95th percentile execution time
    pub p95_execution_time_ms: f64,
    /// 99th percentile execution time
    pub p99_execution_time_ms: f64,
    /// Number of slow queries (above threshold)
    pub slow_query_count: u64,
    /// Queries by operation type
    pub by_operation_type: std::collections::HashMap<String, u64>,
    /// Queries by environment
    pub by_environment: std::collections::HashMap<String, u64>,
    /// Start of the profiling period
    pub period_start: DateTime<Utc>,
}

impl ProfilingMetrics {
    pub fn new() -> Self {
        Self {
            period_start: Utc::now(),
            min_execution_time_ms: f64::MAX,
            ..Default::default()
        }
    }
}

/// A slow query entry for detailed analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowQueryEntry {
    /// Unique identifier
    pub id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// The query
    pub query: String,
    /// Execution time in milliseconds
    pub execution_time_ms: f64,
    /// Environment
    pub environment: Environment,
    /// Database name
    #[serde(default)]
    pub database: Option<String>,
    /// Number of rows
    #[serde(default)]
    pub row_count: Option<i64>,
    /// Driver ID
    pub driver_id: String,
}

/// Configuration for the interceptor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptorConfig {
    /// Whether audit logging is enabled
    #[serde(default = "default_true")]
    pub audit_enabled: bool,
    /// Whether profiling is enabled
    #[serde(default = "default_true")]
    pub profiling_enabled: bool,
    /// Whether safety rules are enabled
    #[serde(default = "default_true")]
    pub safety_enabled: bool,
    /// Threshold for slow query detection (milliseconds)
    #[serde(default = "default_slow_threshold")]
    pub slow_query_threshold_ms: u64,
    /// Maximum number of audit log entries to retain
    #[serde(default = "default_max_audit_entries")]
    pub max_audit_entries: usize,
    /// Maximum number of slow query entries to retain
    #[serde(default = "default_max_slow_queries")]
    pub max_slow_queries: usize,
    /// Custom safety rules
    #[serde(default)]
    pub safety_rules: Vec<SafetyRule>,
    /// Built-in rule enabled overrides
    #[serde(default)]
    pub builtin_rule_overrides: Vec<BuiltinRuleOverride>,
}

/// Persisted enabled state for built-in rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinRuleOverride {
    pub id: String,
    pub enabled: bool,
}

fn default_slow_threshold() -> u64 {
    1000 // 1 second
}

fn default_max_audit_entries() -> usize {
    10000
}

fn default_max_slow_queries() -> usize {
    100
}

impl Default for InterceptorConfig {
    fn default() -> Self {
        Self {
            audit_enabled: true,
            profiling_enabled: true,
            safety_enabled: true,
            slow_query_threshold_ms: default_slow_threshold(),
            max_audit_entries: default_max_audit_entries(),
            max_slow_queries: default_max_slow_queries(),
            safety_rules: Vec::new(),
            builtin_rule_overrides: Vec::new(),
        }
    }
}

/// Query context passed through the interceptor pipeline
#[derive(Debug, Clone)]
pub struct QueryContext {
    /// Session ID
    pub session_id: String,
    /// The query being executed
    pub query: String,
    /// Environment
    pub environment: Environment,
    /// Driver ID
    pub driver_id: String,
    /// Database/schema name
    pub database: Option<String>,
    /// Detected operation type
    pub operation_type: QueryOperationType,
    /// Whether the query is a mutation
    pub is_mutation: bool,
    /// Whether the query is dangerous (DDL, etc.)
    pub is_dangerous: bool,
    /// Whether user has acknowledged dangerous query
    pub acknowledged: bool,
    /// Whether connection is read-only
    pub read_only: bool,
}

/// Result of query execution for post-processing
#[derive(Debug, Clone)]
pub struct QueryExecutionResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution time in milliseconds
    pub execution_time_ms: f64,
    /// Number of rows affected/returned
    pub row_count: Option<i64>,
}

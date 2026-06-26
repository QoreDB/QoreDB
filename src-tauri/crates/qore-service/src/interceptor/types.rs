// SPDX-License-Identifier: Apache-2.0

//! Interceptor Types
//!
//! Type definitions for the Universal Query Interceptor system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Environment classification for connections
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    #[default]
    Development,
    Staging,
    Production,
}

/// Maps a free-form environment label (as stored on a connection) to the
/// [`Environment`] enum; unknown values fall back to `Development`. Shared by
/// the query/mutation pipelines and every command that builds an interceptor
/// context (cf. dédup D3).
pub fn map_environment(env: &str) -> Environment {
    match env {
        "production" => Environment::Production,
        "staging" => Environment::Staging,
        _ => Environment::Development,
    }
}

/// Query operation type for classification
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    #[default]
    Other,
}

impl QueryOperationType {
    /// Returns true if this operation modifies data
    pub fn is_mutation(&self) -> bool {
        !matches!(self, Self::Select)
    }

    /// Returns true if this operation is potentially destructive
    pub fn is_destructive(&self) -> bool {
        matches!(
            self,
            Self::Drop | Self::Truncate | Self::Delete | Self::Alter
        )
    }
}

/// Action to take when a safety rule matches
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyAction {
    /// Block the query entirely
    #[default]
    Block,
    /// Allow but emit a warning
    Warn,
    /// Require explicit user confirmation
    RequireConfirmation,
}

/// A custom safety rule for blocking or warning on certain queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyRule {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyCheckResult {
    pub allowed: bool,
    pub action: SafetyAction,
    pub triggered_rule: Option<String>,
    pub message: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub query: String,
    /// Truncated query for display (first N chars)
    pub query_preview: String,
    pub environment: Environment,
    pub operation_type: QueryOperationType,
    #[serde(default)]
    pub database: Option<String>,
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
    /// Execution time in milliseconds
    pub execution_time_ms: f64,
    /// Number of rows affected/returned
    #[serde(default)]
    pub row_count: Option<i64>,
    #[serde(default)]
    pub blocked: bool,
    #[serde(default)]
    pub safety_rule: Option<String>,
    /// Driver ID (postgres, mysql, etc.)
    pub driver_id: String,
    /// Stable fingerprint (hex prefix) computed from the normalized query.
    /// `None` for entries persisted before fingerprinting was introduced.
    #[serde(default)]
    pub fingerprint: Option<String>,
}

impl AuditLogEntry {
    pub fn new(
        session_id: String,
        query: String,
        environment: Environment,
        driver_id: String,
    ) -> Self {
        use super::fingerprint::fingerprint_query;
        use super::redaction::redact_query;

        let redacted = redact_query(&query, &driver_id);
        let mut preview = redacted.chars().take(100).collect::<String>();
        let truncated = redacted.chars().nth(100).is_some();
        if truncated {
            preview.push_str("...");
        }

        let fingerprint = fingerprint_query(&redacted, &driver_id);

        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            session_id,
            query: redacted,
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
            fingerprint: Some(fingerprint),
        }
    }
}

/// Profiling metrics for query performance
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfilingMetrics {
    pub total_queries: u64,
    pub successful_queries: u64,
    pub failed_queries: u64,
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
    pub by_operation_type: std::collections::HashMap<String, u64>,
    pub by_environment: std::collections::HashMap<String, u64>,
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
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub query: String,
    /// Execution time in milliseconds
    pub execution_time_ms: f64,
    pub environment: Environment,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub row_count: Option<i64>,
    pub driver_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptorConfig {
    #[serde(default = "default_true")]
    pub audit_enabled: bool,
    #[serde(default = "default_true")]
    pub profiling_enabled: bool,
    #[serde(default = "default_true")]
    pub safety_enabled: bool,
    /// Threshold for slow query detection (milliseconds)
    #[serde(default = "default_slow_threshold")]
    pub slow_query_threshold_ms: u64,
    #[serde(default = "default_max_audit_entries")]
    pub max_audit_entries: usize,
    #[serde(default = "default_max_slow_queries")]
    pub max_slow_queries: usize,
    #[serde(default)]
    pub safety_rules: Vec<SafetyRule>,
    #[serde(default)]
    pub builtin_rule_overrides: Vec<BuiltinRuleOverride>,
    /// Whether sensitive-literal redaction is applied before persistence
    #[serde(default = "default_true")]
    pub redact_enabled: bool,
    /// Additional user-provided regex patterns whose matches are replaced by
    /// `[REDACTED]` on top of the driver-specific rules.
    #[serde(default)]
    pub redaction_patterns: Vec<String>,
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
            redact_enabled: true,
            redaction_patterns: Vec::new(),
        }
    }
}

/// Query context passed through the interceptor pipeline
#[derive(Debug, Clone)]
pub struct QueryContext {
    pub session_id: String,
    pub query: String,
    pub environment: Environment,
    pub driver_id: String,
    pub database: Option<String>,
    pub operation_type: QueryOperationType,
    pub is_mutation: bool,
    /// Whether the query is dangerous (DDL, etc.)
    pub is_dangerous: bool,
    /// Whether user has acknowledged dangerous query
    pub acknowledged: bool,
    pub read_only: bool,
}

/// Result of query execution for post-processing
#[derive(Debug, Clone)]
pub struct QueryExecutionResult {
    pub success: bool,
    pub error: Option<String>,
    /// Execution time in milliseconds
    pub execution_time_ms: f64,
    /// Number of rows affected/returned
    pub row_count: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::super::redaction::{set_redaction_enabled, test_lock};
    use super::*;

    #[test]
    fn audit_entry_redacts_mongodb_password() {
        let _guard = test_lock();
        set_redaction_enabled(true);
        let query = r#"{"operation":"insert","document":{"email":"a@b.c","password":"hunter2"}}"#;
        let entry = AuditLogEntry::new(
            "sess-1".into(),
            query.into(),
            Environment::Production,
            "mongodb".into(),
        );
        assert!(
            !entry.query.contains("hunter2"),
            "password should be redacted in stored query"
        );
        assert!(
            !entry.query_preview.contains("hunter2"),
            "preview should also be redacted"
        );
        assert!(entry.query.contains("[REDACTED]"));
    }

    #[test]
    fn audit_entry_redacts_redis_auth() {
        let _guard = test_lock();
        set_redaction_enabled(true);
        let entry = AuditLogEntry::new(
            "sess-1".into(),
            "AUTH hunter2".into(),
            Environment::Production,
            "redis".into(),
        );
        assert!(!entry.query.contains("hunter2"));
        assert!(entry.query.contains("***"));
    }

    #[test]
    fn audit_entry_redacts_sql_literal() {
        let _guard = test_lock();
        set_redaction_enabled(true);
        let entry = AuditLogEntry::new(
            "sess-1".into(),
            "INSERT INTO users (pwd) VALUES ('hunter2')".into(),
            Environment::Production,
            "postgres".into(),
        );
        assert!(!entry.query.contains("hunter2"));
    }
}

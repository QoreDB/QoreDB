// SPDX-License-Identifier: Apache-2.0

//! Interceptor Pipeline
//!
//! Orchestrates the query interception workflow:
//! 1. Pre-execution: Safety checks, audit logging setup
//! 2. Post-execution: Profiling, audit logging completion

use std::path::PathBuf;
use std::sync::Arc;

use tracing::{debug, info};

use super::audit::{AuditStats, AuditStore};
use super::profiling::ProfilingStore;
use super::safety::SafetyEngine;
use super::types::{
    AuditLogEntry, BuiltinRuleOverride, Environment, InterceptorConfig, ProfilingMetrics,
    QueryContext, QueryExecutionResult, QueryOperationType, SafetyCheckResult, SafetyRule,
    SlowQueryEntry,
};
use crate::engine::sql_safety::SqlSafetyAnalysis;

/// The main interceptor pipeline
pub struct InterceptorPipeline {
    /// Audit log store
    audit: Arc<AuditStore>,
    /// Profiling metrics store
    profiling: Arc<ProfilingStore>,
    /// Safety rules engine
    safety: Arc<SafetyEngine>,
    /// Configuration
    config: std::sync::RwLock<InterceptorConfig>,
    /// Data directory for persistence
    data_dir: PathBuf,
}

impl InterceptorPipeline {
    /// Creates a new interceptor pipeline
    pub fn new(data_dir: PathBuf) -> Self {
        let config = InterceptorConfig::default();

        let audit = Arc::new(AuditStore::new(
            data_dir.clone(),
            config.max_audit_entries,
        ));

        let profiling = Arc::new(ProfilingStore::new(
            config.slow_query_threshold_ms,
            config.max_slow_queries,
        ));

        let safety = Arc::new(SafetyEngine::new());

        info!("Interceptor pipeline initialized");

        Self {
            audit,
            profiling,
            safety,
            config: std::sync::RwLock::new(config),
            data_dir,
        }
    }

    /// Load configuration from file
    pub fn load_config(&self) -> Result<(), String> {
        let config_path = self.data_dir.join("interceptor.json");

        if !config_path.exists() {
            debug!("No interceptor config file found, using defaults");
            return Ok(());
        }

        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;

        let config: InterceptorConfig = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse config: {}", e))?;

        self.apply_config(config);

        info!("Loaded interceptor configuration from {:?}", config_path);
        Ok(())
    }

    /// Save configuration to file
    pub fn save_config(&self) -> Result<(), String> {
        let config_path = self.data_dir.join("interceptor.json");

        let config = self.config.read().unwrap().clone();
        let content = serde_json::to_string_pretty(&config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        std::fs::write(&config_path, content)
            .map_err(|e| format!("Failed to write config: {}", e))?;

        debug!("Saved interceptor configuration to {:?}", config_path);
        Ok(())
    }

    /// Apply configuration
    fn apply_config(&self, config: InterceptorConfig) {
        self.audit.set_enabled(config.audit_enabled);
        self.audit.set_max_entries(config.max_audit_entries);
        self.profiling.set_enabled(config.profiling_enabled);
        self.profiling.set_slow_threshold(config.slow_query_threshold_ms);
        self.profiling.set_max_slow_queries(config.max_slow_queries);
        self.safety.set_enabled(config.safety_enabled);
        self.safety.load_rules(config.safety_rules.clone());
        self.safety.apply_builtin_overrides(&config.builtin_rule_overrides);

        *self.config.write().unwrap() = config;
    }

    /// Get current configuration
    pub fn get_config(&self) -> InterceptorConfig {
        self.config.read().unwrap().clone()
    }

    /// Update configuration
    pub fn update_config(&self, config: InterceptorConfig) -> Result<(), String> {
        self.apply_config(config);
        self.save_config()
    }

    // ==================== Pre-execution ====================

    /// Pre-execution check: validates query against safety rules
    pub fn pre_execute(&self, context: &QueryContext) -> SafetyCheckResult {
        // Check safety rules
        self.safety.check(context)
    }

    /// Build query context from execution parameters
    pub fn build_context(
        &self,
        session_id: &str,
        query: &str,
        driver_id: &str,
        environment: Environment,
        read_only: bool,
        acknowledged: bool,
        database: Option<&str>,
        sql_analysis: Option<&SqlSafetyAnalysis>,
        is_mongo_mutation: bool,
    ) -> QueryContext {
        // Determine operation type from analysis
        let (operation_type, is_mutation, is_dangerous) = if let Some(analysis) = sql_analysis {
            let op = self.classify_sql_operation(query);
            (op, analysis.is_mutation, analysis.is_dangerous)
        } else {
            // MongoDB or unknown
            let op = self.classify_operation(query, driver_id);
            (op, is_mongo_mutation, false)
        };

        QueryContext {
            session_id: session_id.to_string(),
            query: query.to_string(),
            environment,
            driver_id: driver_id.to_string(),
            database: database.map(|s| s.to_string()),
            operation_type,
            is_mutation,
            is_dangerous,
            acknowledged,
            read_only,
        }
    }

    /// Classify SQL operation type from query
    fn classify_sql_operation(&self, query: &str) -> QueryOperationType {
        let query_upper = query.trim().to_uppercase();
        let first_word = query_upper.split_whitespace().next().unwrap_or("");

        match first_word {
            "SELECT" => QueryOperationType::Select,
            "INSERT" => QueryOperationType::Insert,
            "UPDATE" => QueryOperationType::Update,
            "DELETE" => QueryOperationType::Delete,
            "CREATE" => QueryOperationType::Create,
            "ALTER" => QueryOperationType::Alter,
            "DROP" => QueryOperationType::Drop,
            "TRUNCATE" => QueryOperationType::Truncate,
            "GRANT" => QueryOperationType::Grant,
            "REVOKE" => QueryOperationType::Revoke,
            "EXEC" | "EXECUTE" | "CALL" => QueryOperationType::Execute,
            _ => QueryOperationType::Other,
        }
    }

    /// Classify operation for non-SQL (MongoDB) queries
    fn classify_operation(&self, query: &str, driver_id: &str) -> QueryOperationType {
        if driver_id.eq_ignore_ascii_case("mongodb") {
            // MongoDB command detection
            let query_lower = query.to_lowercase();
            if query_lower.contains("find") || query_lower.contains("aggregate") {
                QueryOperationType::Select
            } else if query_lower.contains("insert") {
                QueryOperationType::Insert
            } else if query_lower.contains("update") {
                QueryOperationType::Update
            } else if query_lower.contains("delete") || query_lower.contains("remove") {
                QueryOperationType::Delete
            } else if query_lower.contains("drop") {
                QueryOperationType::Drop
            } else if query_lower.contains("create") {
                QueryOperationType::Create
            } else {
                QueryOperationType::Other
            }
        } else {
            self.classify_sql_operation(query)
        }
    }

    // ==================== Post-execution ====================

    /// Post-execution: record metrics and audit log
    pub fn post_execute(
        &self,
        context: &QueryContext,
        result: &QueryExecutionResult,
        blocked: bool,
        safety_rule: Option<&str>,
    ) {
        // Record profiling metrics
        self.profiling.record(
            result.execution_time_ms,
            result.success,
            blocked,
            context.operation_type,
            context.environment,
            Some(&context.query),
            context.database.as_deref(),
            result.row_count,
            &context.driver_id,
        );

        // Record audit log entry
        let mut entry = AuditLogEntry::new(
            context.session_id.clone(),
            context.query.clone(),
            context.environment,
            context.driver_id.clone(),
        );
        entry.operation_type = context.operation_type;
        entry.database = context.database.clone();
        entry.success = result.success;
        entry.error = result.error.clone();
        entry.execution_time_ms = result.execution_time_ms;
        entry.row_count = result.row_count;
        entry.blocked = blocked;
        entry.safety_rule = safety_rule.map(|s| s.to_string());

        self.audit.log(entry);
    }

    // ==================== Audit API ====================

    /// Get audit log entries
    pub fn get_audit_entries(
        &self,
        limit: usize,
        offset: usize,
        environment: Option<Environment>,
        operation: Option<QueryOperationType>,
        success: Option<bool>,
        search: Option<&str>,
    ) -> Vec<AuditLogEntry> {
        self.audit.get_entries(limit, offset, environment, operation, success, search, None, None)
    }

    /// Get audit statistics
    pub fn get_audit_stats(&self) -> AuditStats {
        self.audit.get_stats()
    }

    /// Clear audit log
    pub fn clear_audit(&self) {
        self.audit.clear();
    }

    /// Export audit log
    pub fn export_audit(&self) -> String {
        self.audit.export()
    }

    // ==================== Profiling API ====================

    /// Get profiling metrics
    pub fn get_profiling_metrics(&self) -> ProfilingMetrics {
        self.profiling.get_metrics()
    }

    /// Get slow queries
    pub fn get_slow_queries(&self, limit: usize, offset: usize) -> Vec<SlowQueryEntry> {
        self.profiling.get_slow_queries(limit, offset)
    }

    /// Clear slow queries
    pub fn clear_slow_queries(&self) {
        self.profiling.clear_slow_queries();
    }

    /// Reset profiling metrics
    pub fn reset_profiling(&self) {
        self.profiling.reset();
    }

    /// Export profiling data
    pub fn export_profiling(&self) -> String {
        self.profiling.export()
    }

    // ==================== Safety API ====================

    /// Get all safety rules
    pub fn get_safety_rules(&self) -> Vec<SafetyRule> {
        self.safety.get_rules()
    }

    /// Add a custom safety rule
    pub fn add_safety_rule(&self, rule: SafetyRule) -> Result<(), String> {
        self.safety.add_rule(rule.clone())?;

        // Update config and save
        let mut config = self.config.write().unwrap();
        config.safety_rules.push(rule);
        drop(config);

        self.save_config()
    }

    /// Update a safety rule
    pub fn update_safety_rule(&self, rule: SafetyRule) -> Result<(), String> {
        self.safety.update_rule(rule.clone())?;

        // Update config and save
        let mut config = self.config.write().unwrap();
        if rule.builtin {
            upsert_builtin_override(&mut config.builtin_rule_overrides, &rule.id, rule.enabled);
        } else if let Some(existing) = config.safety_rules.iter_mut().find(|r| r.id == rule.id) {
            *existing = rule;
        }
        drop(config);

        self.save_config()
    }

    /// Remove a safety rule
    pub fn remove_safety_rule(&self, rule_id: &str) -> Result<(), String> {
        self.safety.remove_rule(rule_id)?;

        // Update config and save
        let mut config = self.config.write().unwrap();
        config.safety_rules.retain(|r| r.id != rule_id);
        drop(config);

        self.save_config()
    }
}

fn upsert_builtin_override(
    overrides: &mut Vec<BuiltinRuleOverride>,
    rule_id: &str,
    enabled: bool,
) {
    if let Some(existing) = overrides.iter_mut().find(|r| r.id == rule_id) {
        existing.enabled = enabled;
    } else {
        overrides.push(BuiltinRuleOverride {
            id: rule_id.to_string(),
            enabled,
        });
    }
}

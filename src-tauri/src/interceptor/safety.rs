// SPDX-License-Identifier: Apache-2.0

//! Safety Rules Engine
//!
//! Enforces safety rules to prevent dangerous or unauthorized queries.
//! Supports both built-in and custom rules.

use std::sync::RwLock;

use regex::Regex;
use tracing::{debug, info, warn};

use super::types::{
    BuiltinRuleOverride, Environment, QueryContext, QueryOperationType, SafetyAction,
    SafetyCheckResult, SafetyRule,
};

/// Built-in safety rules
fn get_builtin_rules() -> Vec<SafetyRule> {
    vec![
        SafetyRule {
            id: "builtin-no-drop-production".to_string(),
            name: "Block DROP in Production".to_string(),
            description: "Prevents DROP statements in production environments".to_string(),
            enabled: true,
            environments: vec![Environment::Production],
            operations: vec![QueryOperationType::Drop],
            action: SafetyAction::Block,
            pattern: None,
            builtin: true,
        },
        SafetyRule {
            id: "builtin-no-truncate-production".to_string(),
            name: "Block TRUNCATE in Production".to_string(),
            description: "Prevents TRUNCATE statements in production environments".to_string(),
            enabled: true,
            environments: vec![Environment::Production],
            operations: vec![QueryOperationType::Truncate],
            action: SafetyAction::Block,
            pattern: None,
            builtin: true,
        },
        SafetyRule {
            id: "builtin-confirm-delete-production".to_string(),
            name: "Confirm DELETE in Production".to_string(),
            description: "Requires confirmation for DELETE statements in production".to_string(),
            enabled: true,
            environments: vec![Environment::Production],
            operations: vec![QueryOperationType::Delete],
            action: SafetyAction::RequireConfirmation,
            pattern: None,
            builtin: true,
        },
        SafetyRule {
            id: "builtin-confirm-update-no-where".to_string(),
            name: "Confirm UPDATE without WHERE".to_string(),
            description: "Requires confirmation for UPDATE statements without WHERE clause".to_string(),
            enabled: true,
            environments: vec![Environment::Production, Environment::Staging],
            operations: vec![QueryOperationType::Update],
            action: SafetyAction::RequireConfirmation,
            pattern: Some(r"^UPDATE\s+\S+\s+SET\s+[^;]+$".to_string()), // UPDATE without WHERE
            builtin: true,
        },
        SafetyRule {
            id: "builtin-confirm-delete-no-where".to_string(),
            name: "Confirm DELETE without WHERE".to_string(),
            description: "Requires confirmation for DELETE statements without WHERE clause".to_string(),
            enabled: true,
            environments: vec![Environment::Production, Environment::Staging],
            operations: vec![QueryOperationType::Delete],
            action: SafetyAction::RequireConfirmation,
            pattern: Some(r"^DELETE\s+FROM\s+\S+\s*$".to_string()), // DELETE without WHERE
            builtin: true,
        },
        SafetyRule {
            id: "builtin-warn-alter-production".to_string(),
            name: "Warn ALTER in Production".to_string(),
            description: "Warns before ALTER statements in production".to_string(),
            enabled: true,
            environments: vec![Environment::Production],
            operations: vec![QueryOperationType::Alter],
            action: SafetyAction::Warn,
            pattern: None,
            builtin: true,
        },
    ]
}

/// Safety rules engine
pub struct SafetyEngine {
    /// Built-in rules (always present, can be disabled)
    builtin_rules: RwLock<Vec<SafetyRule>>,
    /// Custom user-defined rules
    custom_rules: RwLock<Vec<SafetyRule>>,
    /// Whether safety checking is enabled
    enabled: RwLock<bool>,
    /// Compiled regex patterns cache
    pattern_cache: RwLock<std::collections::HashMap<String, Regex>>,
}

impl SafetyEngine {
    /// Creates a new safety engine
    pub fn new() -> Self {
        Self {
            builtin_rules: RwLock::new(get_builtin_rules()),
            custom_rules: RwLock::new(Vec::new()),
            enabled: RwLock::new(true),
            pattern_cache: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Load custom rules
    pub fn load_rules(&self, rules: Vec<SafetyRule>) {
        let mut custom = self.custom_rules.write().unwrap();
        *custom = rules.into_iter().filter(|r| !r.builtin).collect();

        // Clear pattern cache
        self.pattern_cache.write().unwrap().clear();

        info!("Loaded {} custom safety rules", custom.len());
    }

    /// Apply persisted enabled state for built-in rules
    pub fn apply_builtin_overrides(&self, overrides: &[BuiltinRuleOverride]) {
        let mut builtin = self.builtin_rules.write().unwrap();
        for override_entry in overrides {
            if let Some(rule) = builtin.iter_mut().find(|r| r.id == override_entry.id) {
                rule.enabled = override_entry.enabled;
            }
        }
    }

    /// Add a custom rule
    pub fn add_rule(&self, rule: SafetyRule) -> Result<(), String> {
        if rule.builtin {
            return Err("Cannot add built-in rules".to_string());
        }

        // Validate regex pattern if present
        if let Some(ref pattern) = rule.pattern {
            if let Err(e) = Regex::new(pattern) {
                return Err(format!("Invalid regex pattern: {}", e));
            }
        }

        let mut custom = self.custom_rules.write().unwrap();

        // Check for duplicate ID
        if custom.iter().any(|r| r.id == rule.id) {
            return Err(format!("Rule with ID '{}' already exists", rule.id));
        }

        custom.push(rule);

        // Clear pattern cache
        self.pattern_cache.write().unwrap().clear();

        Ok(())
    }

    /// Update a rule
    pub fn update_rule(&self, rule: SafetyRule) -> Result<(), String> {
        // Validate regex pattern if present
        if let Some(ref pattern) = rule.pattern {
            if let Err(e) = Regex::new(pattern) {
                return Err(format!("Invalid regex pattern: {}", e));
            }
        }

        if rule.builtin {
            // Update built-in rule enabled state only
            let mut builtin = self.builtin_rules.write().unwrap();
            if let Some(existing) = builtin.iter_mut().find(|r| r.id == rule.id) {
                existing.enabled = rule.enabled;
                return Ok(());
            }
            return Err(format!("Built-in rule with ID '{}' not found", rule.id));
        }

        let mut custom = self.custom_rules.write().unwrap();

        if let Some(existing) = custom.iter_mut().find(|r| r.id == rule.id) {
            *existing = rule;
            // Clear pattern cache
            self.pattern_cache.write().unwrap().clear();
            Ok(())
        } else {
            Err(format!("Rule with ID '{}' not found", rule.id))
        }
    }

    /// Remove a custom rule
    pub fn remove_rule(&self, rule_id: &str) -> Result<(), String> {
        let mut custom = self.custom_rules.write().unwrap();
        let initial_len = custom.len();
        custom.retain(|r| r.id != rule_id);

        if custom.len() == initial_len {
            // Check if it's a built-in rule
            if self.builtin_rules.read().unwrap().iter().any(|r| r.id == rule_id) {
                return Err("Cannot remove built-in rules".to_string());
            }
            return Err(format!("Rule with ID '{}' not found", rule_id));
        }

        // Clear pattern cache
        self.pattern_cache.write().unwrap().clear();

        Ok(())
    }

    /// Get all rules (built-in + custom)
    pub fn get_rules(&self) -> Vec<SafetyRule> {
        let custom = self.custom_rules.read().unwrap();
        let mut all_rules = self.builtin_rules.read().unwrap().clone();
        all_rules.extend(custom.iter().cloned());
        all_rules
    }

    /// Enable or disable safety checking
    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.write().unwrap() = enabled;
        info!("Safety checking {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Check if safety checking is enabled
    pub fn is_enabled(&self) -> bool {
        *self.enabled.read().unwrap()
    }

    /// Check a query against safety rules
    pub fn check(&self, context: &QueryContext) -> SafetyCheckResult {
        if !self.is_enabled() {
            return SafetyCheckResult::allowed();
        }

        // Get all applicable rules
        let builtin = self.builtin_rules.read().unwrap();
        let custom = self.custom_rules.read().unwrap();
        let all_rules: Vec<&SafetyRule> = builtin
            .iter()
            .chain(custom.iter())
            .filter(|r| r.enabled)
            .collect();

        // Check each rule in order (first match wins)
        for rule in all_rules {
            if let Some(result) = self.check_rule(rule, context) {
                debug!(
                    "Safety rule '{}' triggered for query",
                    rule.name
                );
                return result;
            }
        }

        SafetyCheckResult::allowed()
    }

    /// Check a single rule against the query context
    fn check_rule(&self, rule: &SafetyRule, context: &QueryContext) -> Option<SafetyCheckResult> {
        // Check environment
        if !rule.environments.contains(&context.environment) {
            return None;
        }

        // Check operation type (empty = match all)
        if !rule.operations.is_empty() && !rule.operations.contains(&context.operation_type) {
            return None;
        }

        // Check pattern if present
        if let Some(ref pattern_str) = rule.pattern {
            if !self.matches_pattern(pattern_str, &context.query) {
                return None;
            }
        }

        // Rule matches - return appropriate result
        let message = format!("{}: {}", rule.name, rule.description);

        Some(match rule.action {
            SafetyAction::Block => SafetyCheckResult::blocked(rule.id.clone(), message),
            SafetyAction::RequireConfirmation => {
                if context.acknowledged {
                    SafetyCheckResult::warning(rule.id.clone(), message)
                } else {
                    SafetyCheckResult::needs_confirmation(rule.id.clone(), message)
                }
            }
            SafetyAction::Warn => SafetyCheckResult::warning(rule.id.clone(), message),
        })
    }

    /// Check if query matches a regex pattern
    fn matches_pattern(&self, pattern: &str, query: &str) -> bool {
        // Check cache first
        {
            let cache = self.pattern_cache.read().unwrap();
            if let Some(regex) = cache.get(pattern) {
                return regex.is_match(query);
            }
        }

        // Compile and cache
        match Regex::new(&format!("(?i){}", pattern)) {
            Ok(regex) => {
                let matches = regex.is_match(query);
                self.pattern_cache.write().unwrap().insert(pattern.to_string(), regex);
                matches
            }
            Err(e) => {
                warn!("Invalid regex pattern '{}': {}", pattern, e);
                false
            }
        }
    }
}

impl Default for SafetyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context(env: Environment, op: QueryOperationType, query: &str) -> QueryContext {
        QueryContext {
            session_id: "test".to_string(),
            query: query.to_string(),
            environment: env,
            driver_id: "postgres".to_string(),
            database: None,
            operation_type: op,
            is_mutation: op.is_mutation(),
            is_dangerous: op.is_destructive(),
            acknowledged: false,
            read_only: false,
        }
    }

    #[test]
    fn test_block_drop_production() {
        let engine = SafetyEngine::new();
        let context = make_context(
            Environment::Production,
            QueryOperationType::Drop,
            "DROP TABLE users",
        );

        let result = engine.check(&context);
        assert!(!result.allowed);
        assert!(matches!(result.action, SafetyAction::Block));
    }

    #[test]
    fn test_allow_drop_development() {
        let engine = SafetyEngine::new();
        let context = make_context(
            Environment::Development,
            QueryOperationType::Drop,
            "DROP TABLE test",
        );

        let result = engine.check(&context);
        assert!(result.allowed);
    }

    #[test]
    fn test_confirm_delete_production() {
        let engine = SafetyEngine::new();
        let context = make_context(
            Environment::Production,
            QueryOperationType::Delete,
            "DELETE FROM users WHERE id = 1",
        );

        let result = engine.check(&context);
        assert!(!result.allowed);
        assert!(result.requires_confirmation);
    }
}

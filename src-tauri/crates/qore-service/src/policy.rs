// SPDX-License-Identifier: Apache-2.0

//! Backend safety policy configuration.
//!
//! Defaults are persisted to a per-user config file. Environment variables
//! override any stored values to allow managed deployments to enforce policy.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyPolicy {
    pub prod_require_confirmation: bool,
    pub prod_block_dangerous_sql: bool,
    /// Maximum query execution time in milliseconds (None = no limit)
    #[serde(default)]
    pub max_query_duration_ms: Option<u64>,
    /// Maximum number of rows returned per query (None = no limit)
    #[serde(default)]
    pub max_result_rows: Option<u64>,
    /// Maximum number of concurrent queries (None = no limit)
    #[serde(default)]
    pub max_concurrent_queries: Option<u32>,
    /// Anti-loop guardrail: cap the query rate per session (defaults to on).
    #[serde(default = "default_query_rate_limit")]
    pub query_rate_limit_enabled: bool,
}

fn default_query_rate_limit() -> bool {
    true
}

fn parse_env_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn config_path() -> PathBuf {
    // Delegates to the shared `paths` module so policy / logs / interceptor
    // all live under the same root (cf. audit B1-H4).
    crate::paths::safety_policy_file()
}

fn load_from_file(path: &PathBuf) -> Option<SafetyPolicy> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

impl SafetyPolicy {
    fn defaults() -> Self {
        Self {
            prod_require_confirmation: true,
            prod_block_dangerous_sql: false,
            max_query_duration_ms: None,
            max_result_rows: None,
            max_concurrent_queries: None,
            query_rate_limit_enabled: true,
        }
    }

    fn apply_env_overrides_from(&mut self, get: impl Fn(&str) -> Option<String>) {
        if let Some(value) = get("QOREDB_PROD_REQUIRE_CONFIRMATION") {
            let value = parse_env_bool(&value);
            self.prod_require_confirmation = value;
        }
        if let Some(value) = get("QOREDB_PROD_BLOCK_DANGEROUS") {
            let value = parse_env_bool(&value);
            self.prod_block_dangerous_sql = value;
        }
        if let Some(value) =
            get("QOREDB_MAX_QUERY_DURATION_MS").and_then(|value| value.trim().parse().ok())
        {
            self.max_query_duration_ms = Some(value);
        }
        if let Some(value) =
            get("QOREDB_MAX_RESULT_ROWS").and_then(|value| value.trim().parse().ok())
        {
            self.max_result_rows = Some(value);
        }
        if let Some(value) =
            get("QOREDB_MAX_CONCURRENT_QUERIES").and_then(|value| value.trim().parse().ok())
        {
            self.max_concurrent_queries = Some(value);
        }
        if let Some(value) = get("QOREDB_QUERY_RATE_LIMIT") {
            let value = parse_env_bool(&value);
            self.query_rate_limit_enabled = value;
        }
    }

    fn apply_env_overrides(&mut self) {
        self.apply_env_overrides_from(|key| std::env::var(key).ok());
    }

    pub fn load() -> Self {
        let path = config_path();
        let mut policy = load_from_file(&path).unwrap_or_else(Self::defaults);
        policy.apply_env_overrides();
        policy
    }

    pub fn save_to_file(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let payload =
            serde_json::to_string_pretty(self).map_err(|e| format!("Save failed: {}", e))?;
        fs::write(&path, payload).map_err(|e| format!("Save failed: {}", e))?;
        Ok(())
    }
}

impl Default for SafetyPolicy {
    fn default() -> Self {
        Self::load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_defaults() {
        let policy = SafetyPolicy::defaults();
        assert!(policy.prod_require_confirmation);
        assert!(!policy.prod_block_dangerous_sql);
        assert!(policy.max_query_duration_ms.is_none());
        assert!(policy.max_result_rows.is_none());
        assert!(policy.max_concurrent_queries.is_none());
        assert!(policy.query_rate_limit_enabled);
    }

    #[test]
    fn test_env_overrides() {
        let enabled = HashMap::from([
            ("QOREDB_PROD_REQUIRE_CONFIRMATION", "true"),
            ("QOREDB_PROD_BLOCK_DANGEROUS", "1"),
        ]);

        let mut policy = SafetyPolicy::defaults();
        policy.apply_env_overrides_from(|key| enabled.get(key).map(ToString::to_string));

        assert!(policy.prod_require_confirmation);
        assert!(policy.prod_block_dangerous_sql);

        let disabled = HashMap::from([
            ("QOREDB_PROD_REQUIRE_CONFIRMATION", "false"),
            ("QOREDB_PROD_BLOCK_DANGEROUS", "off"),
        ]);

        let mut policy = SafetyPolicy::defaults();
        policy.apply_env_overrides_from(|key| disabled.get(key).map(ToString::to_string));

        assert!(!policy.prod_require_confirmation);
        assert!(!policy.prod_block_dangerous_sql);
    }

    #[test]
    fn test_env_bool_parsing() {
        assert!(parse_env_bool("true"));
        assert!(parse_env_bool("1"));
        assert!(parse_env_bool(" YES "));
        assert!(!parse_env_bool("false"));
        assert!(!parse_env_bool("off"));
    }
}

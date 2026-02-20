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
}

fn env_bool_opt(key: &str) -> Option<bool> {
    std::env::var(key).ok().map(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn config_path() -> PathBuf {
    if cfg!(windows) {
        let appdata = std::env::var_os("APPDATA")
            .unwrap_or_else(|| std::env::var_os("USERPROFILE").unwrap_or_default());
        let mut path = PathBuf::from(appdata);
        path.push("QoreDB");
        path.push("config.json");
        path
    } else {
        let home = std::env::var_os("HOME").unwrap_or_default();
        let mut path = PathBuf::from(home);
        path.push(".qoredb");
        path.push("config.json");
        path
    }
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
        }
    }

    fn apply_env_overrides(&mut self) {
        if let Some(value) = env_bool_opt("QOREDB_PROD_REQUIRE_CONFIRMATION") {
            self.prod_require_confirmation = value;
        }
        if let Some(value) = env_bool_opt("QOREDB_PROD_BLOCK_DANGEROUS") {
            self.prod_block_dangerous_sql = value;
        }
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
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_defaults() {
        let policy = SafetyPolicy::defaults();
        assert!(policy.prod_require_confirmation);
        assert!(!policy.prod_block_dangerous_sql);
    }

    #[test]
    fn test_env_overrides() {
        let _guard = ENV_LOCK.lock().unwrap();

        // Helper to safely set/unset env
        let set_env = |key: &str, val: Option<&str>| {
            if let Some(v) = val {
                std::env::set_var(key, v);
            } else {
                std::env::remove_var(key);
            }
        };

        // Save original vars
        let orig_confirm = std::env::var("QOREDB_PROD_REQUIRE_CONFIRMATION").ok();
        let orig_block = std::env::var("QOREDB_PROD_BLOCK_DANGEROUS").ok();

        // Case 1: Override both to true
        set_env("QOREDB_PROD_REQUIRE_CONFIRMATION", Some("true"));
        set_env("QOREDB_PROD_BLOCK_DANGEROUS", Some("1"));

        let mut policy = SafetyPolicy::defaults();
        policy.apply_env_overrides();

        assert!(policy.prod_require_confirmation);
        assert!(policy.prod_block_dangerous_sql);

        // Case 2: Override both to false
        set_env("QOREDB_PROD_REQUIRE_CONFIRMATION", Some("false"));
        set_env("QOREDB_PROD_BLOCK_DANGEROUS", Some("off"));

        let mut policy = SafetyPolicy::defaults();
        policy.apply_env_overrides();

        assert!(!policy.prod_require_confirmation);
        assert!(!policy.prod_block_dangerous_sql);

        // Cleanup
        set_env("QOREDB_PROD_REQUIRE_CONFIRMATION", orig_confirm.as_deref());
        set_env("QOREDB_PROD_BLOCK_DANGEROUS", orig_block.as_deref());
    }

    #[test]
    fn test_env_bool_parsing() {
        let _guard = ENV_LOCK.lock().unwrap();

        std::env::set_var("TEST_BOOL_TRUE", "true");
        assert_eq!(env_bool_opt("TEST_BOOL_TRUE"), Some(true));

        std::env::set_var("TEST_BOOL_1", "1");
        assert_eq!(env_bool_opt("TEST_BOOL_1"), Some(true));

        std::env::set_var("TEST_BOOL_FALSE", "false");
        assert_eq!(env_bool_opt("TEST_BOOL_FALSE"), Some(false));

        std::env::remove_var("TEST_BOOL_TRUE");
        std::env::remove_var("TEST_BOOL_1");
        std::env::remove_var("TEST_BOOL_FALSE");

        assert_eq!(env_bool_opt("NON_EXISTENT"), None);
    }
}

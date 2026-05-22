// SPDX-License-Identifier: Apache-2.0

//! Query result cache (Core).
//!
//! Caches the materialised results of read-only table-browse calls
//! (`preview_table` / `query_table`) so repeated navigation is instant. The
//! cache is bounded (LRU), time-limited (TTL) and invalidated per session
//! whenever a mutation is executed through QoreDB. Mutations made outside
//! QoreDB are not observed — the TTL bounds that staleness.

mod store;

pub use store::{CacheHit, QueryCache};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// User-configurable cache behaviour, persisted under the app data directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheConfig {
    pub enabled: bool,
    /// Entry lifetime in seconds.
    pub ttl_secs: u64,
    /// Maximum number of cached entries (LRU eviction beyond this).
    pub max_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_secs: 60,
            max_entries: 100,
        }
    }
}

fn config_path() -> PathBuf {
    crate::paths::app_data_dir().join("cache_config.json")
}

impl CacheConfig {
    /// Loads the persisted config, falling back to defaults.
    pub fn load() -> Self {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    /// Persists the config to the app data directory.
    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, raw).map_err(|e| e.to_string())
    }
}

/// Runtime cache counters surfaced in Settings.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
}

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
    /// Clamps user-supplied values into safe ranges. Applied on load and
    /// before persisting so a hand-edited config file cannot, e.g., set an
    /// unbounded entry count or a near-infinite TTL.
    pub fn clamp(&mut self) {
        self.ttl_secs = self.ttl_secs.clamp(5, 3600);
        self.max_entries = self.max_entries.clamp(10, 1000);
    }

    /// Loads the persisted config (clamped), falling back to defaults.
    pub fn load() -> Self {
        let mut config: Self = std::fs::read_to_string(config_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        config.clamp();
        config
    }

    /// Persists the config to the app data directory.
    pub fn save(&self) -> Result<(), String> {
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        crate::paths::atomic_write(&config_path(), raw.as_bytes()).map_err(|e| e.to_string())
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

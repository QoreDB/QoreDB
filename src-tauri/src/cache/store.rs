// SPDX-License-Identifier: Apache-2.0

//! In-memory, bounded, time-limited query result cache.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{CacheConfig, CacheStats};

struct CacheEntry {
    value: serde_json::Value,
    /// Session that produced the result — used for per-session invalidation.
    session_id: String,
    inserted: Instant,
    last_access: Instant,
}

struct Inner {
    entries: HashMap<String, CacheEntry>,
    config: CacheConfig,
    hits: u64,
    misses: u64,
}

/// Thread-safe LRU cache of read-only query results.
pub struct QueryCache {
    inner: Mutex<Inner>,
}

impl QueryCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: HashMap::new(),
                config: CacheConfig::load(),
                hits: 0,
                misses: 0,
            }),
        }
    }

    pub fn config(&self) -> CacheConfig {
        self.inner.lock().unwrap().config.clone()
    }

    /// Replaces the active config, dropping entries that no longer fit.
    pub fn set_config(&self, config: CacheConfig) {
        let mut inner = self.inner.lock().unwrap();
        inner.config = config;
        if !inner.config.enabled {
            inner.entries.clear();
            return;
        }
        let max = inner.config.max_entries;
        while inner.entries.len() > max {
            evict_one(&mut inner);
        }
    }

    /// Returns a cached value when present and still fresh. A stale entry is
    /// dropped on access.
    pub fn get(&self, key: &str) -> Option<serde_json::Value> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.config.enabled {
            return None;
        }
        let ttl = Duration::from_secs(inner.config.ttl_secs);
        let now = Instant::now();
        let fresh = inner
            .entries
            .get(key)
            .is_some_and(|e| now.duration_since(e.inserted) < ttl);

        if fresh {
            let entry = inner.entries.get_mut(key).expect("checked above");
            entry.last_access = now;
            let value = entry.value.clone();
            inner.hits += 1;
            Some(value)
        } else {
            inner.entries.remove(key);
            inner.misses += 1;
            None
        }
    }

    /// Stores a value, evicting the least-recently-used entry past capacity.
    pub fn put(&self, key: String, session_id: String, value: serde_json::Value) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.config.enabled {
            return;
        }
        let now = Instant::now();
        inner.entries.insert(
            key,
            CacheEntry {
                value,
                session_id,
                inserted: now,
                last_access: now,
            },
        );
        let max = inner.config.max_entries;
        while inner.entries.len() > max {
            evict_one(&mut inner);
        }
    }

    /// Drops every entry produced by `session_id` — called after a mutation.
    pub fn invalidate_session(&self, session_id: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.entries.retain(|_, e| e.session_id != session_id);
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().entries.clear();
    }

    pub fn stats(&self) -> CacheStats {
        let inner = self.inner.lock().unwrap();
        CacheStats {
            entries: inner.entries.len(),
            hits: inner.hits,
            misses: inner.misses,
        }
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new()
    }
}

fn evict_one(inner: &mut Inner) {
    if let Some(key) = inner
        .entries
        .iter()
        .min_by_key(|(_, e)| e.last_access)
        .map(|(k, _)| k.clone())
    {
        inner.entries.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(ttl_secs: u64, max_entries: usize) -> QueryCache {
        let c = QueryCache::new();
        c.set_config(CacheConfig {
            enabled: true,
            ttl_secs,
            max_entries,
        });
        c
    }

    fn val(n: i64) -> serde_json::Value {
        serde_json::json!({ "n": n })
    }

    #[test]
    fn stores_and_serves_a_fresh_entry() {
        let c = cache(60, 10);
        c.put("k1".into(), "s1".into(), val(1));
        assert_eq!(c.get("k1"), Some(val(1)));
        assert_eq!(c.stats().hits, 1);
    }

    #[test]
    fn expired_entry_is_a_miss() {
        let c = cache(0, 10);
        c.put("k1".into(), "s1".into(), val(1));
        assert_eq!(c.get("k1"), None);
        assert_eq!(c.stats().misses, 1);
        assert_eq!(c.stats().entries, 0);
    }

    #[test]
    fn evicts_least_recently_used_past_capacity() {
        let c = cache(60, 2);
        c.put("k1".into(), "s1".into(), val(1));
        c.put("k2".into(), "s1".into(), val(2));
        let _ = c.get("k1"); // k1 becomes most-recently-used
        c.put("k3".into(), "s1".into(), val(3)); // evicts k2 (LRU)
        assert!(c.get("k1").is_some());
        assert!(c.get("k2").is_none());
        assert!(c.get("k3").is_some());
    }

    #[test]
    fn invalidate_session_drops_only_that_session() {
        let c = cache(60, 10);
        c.put("k1".into(), "s1".into(), val(1));
        c.put("k2".into(), "s2".into(), val(2));
        c.invalidate_session("s1");
        assert!(c.get("k1").is_none());
        assert!(c.get("k2").is_some());
    }

    #[test]
    fn disabling_clears_and_blocks_caching() {
        let c = cache(60, 10);
        c.put("k1".into(), "s1".into(), val(1));
        c.set_config(CacheConfig {
            enabled: false,
            ttl_secs: 60,
            max_entries: 10,
        });
        assert!(c.get("k1").is_none());
        c.put("k2".into(), "s1".into(), val(2));
        assert_eq!(c.stats().entries, 0);
    }
}

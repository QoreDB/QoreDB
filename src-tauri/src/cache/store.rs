// SPDX-License-Identifier: Apache-2.0

//! In-memory, bounded, time-limited query result cache.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{CacheConfig, CacheStats};

/// Hard ceiling on a single cached entry. A result larger than this is never
/// cached — it would dominate memory and evict everything else.
const MAX_ENTRY_BYTES: usize = 8 * 1024 * 1024;
/// Hard ceiling on the cache's total footprint, enforced on top of the
/// user-configurable entry count. The LRU evicts until both bounds hold.
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

struct CacheEntry {
    /// Serialised JSON of the cached result.
    value: String,
    /// Connection that produced the result — used for per-connection invalidation.
    connection_id: String,
    /// Byte size of `value`, kept for footprint accounting.
    bytes: usize,
    inserted: Instant,
    last_access: Instant,
}

struct Inner {
    entries: HashMap<String, CacheEntry>,
    /// Running sum of every entry's `bytes`.
    total_bytes: usize,
    config: CacheConfig,
    hits: u64,
    misses: u64,
}

/// A fresh cache hit: the stored value and how long it has been cached.
#[derive(Debug)]
pub struct CacheHit {
    pub value: String,
    pub age_ms: u64,
}

/// Thread-safe LRU cache of read-only query results.
pub struct QueryCache {
    inner: Mutex<Inner>,
    /// Largest result that may be cached at all.
    max_entry_bytes: usize,
    /// Largest total footprint before LRU eviction kicks in.
    max_total_bytes: usize,
}

impl QueryCache {
    pub fn new() -> Self {
        Self::with_limits(MAX_ENTRY_BYTES, MAX_TOTAL_BYTES)
    }

    fn with_limits(max_entry_bytes: usize, max_total_bytes: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: HashMap::new(),
                total_bytes: 0,
                config: CacheConfig::load(),
                hits: 0,
                misses: 0,
            }),
            max_entry_bytes,
            max_total_bytes,
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
            inner.total_bytes = 0;
            return;
        }
        self.enforce_bounds(&mut inner);
    }

    /// Returns a cached value when present and still fresh. A stale entry is
    /// dropped on access.
    pub fn get(&self, key: &str) -> Option<CacheHit> {
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
            let age_ms = now.duration_since(entry.inserted).as_millis() as u64;
            inner.hits += 1;
            Some(CacheHit { value, age_ms })
        } else {
            if let Some(entry) = inner.entries.remove(key) {
                inner.total_bytes = inner.total_bytes.saturating_sub(entry.bytes);
            }
            inner.misses += 1;
            None
        }
    }

    /// Stores a serialised result, evicting as needed to stay within the entry
    /// count and the memory ceilings. An oversized result is not cached.
    pub fn put(&self, key: String, connection_id: String, value: String) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.config.enabled {
            return;
        }
        let bytes = value.len();
        if bytes > self.max_entry_bytes {
            return;
        }
        // Replacing an existing key: drop the old footprint first.
        if let Some(old) = inner.entries.remove(&key) {
            inner.total_bytes = inner.total_bytes.saturating_sub(old.bytes);
        }
        let now = Instant::now();
        inner.entries.insert(
            key,
            CacheEntry {
                value,
                connection_id,
                bytes,
                inserted: now,
                last_access: now,
            },
        );
        inner.total_bytes += bytes;
        self.enforce_bounds(&mut inner);
    }

    /// Drops every entry produced by `connection_id` — called after a mutation
    /// so all sessions of that connection see fresh data.
    pub fn invalidate_connection(&self, connection_id: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.entries.retain(|_, e| e.connection_id != connection_id);
        inner.total_bytes = inner.entries.values().map(|e| e.bytes).sum();
    }

    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.entries.clear();
        inner.total_bytes = 0;
        inner.hits = 0;
        inner.misses = 0;
    }

    pub fn stats(&self) -> CacheStats {
        let inner = self.inner.lock().unwrap();
        CacheStats {
            entries: inner.entries.len(),
            hits: inner.hits,
            misses: inner.misses,
        }
    }

    /// Evicts least-recently-used entries until the cache satisfies both the
    /// configured entry count and the memory ceiling.
    fn enforce_bounds(&self, inner: &mut Inner) {
        while inner.entries.len() > inner.config.max_entries
            || inner.total_bytes > self.max_total_bytes
        {
            if !evict_one(inner) {
                break;
            }
        }
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Removes the single least-recently-used entry. Returns `false` when the
/// cache is already empty.
fn evict_one(inner: &mut Inner) -> bool {
    let Some(key) = inner
        .entries
        .iter()
        .min_by_key(|(_, e)| e.last_access)
        .map(|(k, _)| k.clone())
    else {
        return false;
    };
    if let Some(entry) = inner.entries.remove(&key) {
        inner.total_bytes = inner.total_bytes.saturating_sub(entry.bytes);
    }
    true
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

    fn val(n: i64) -> String {
        format!("{{\"n\":{n}}}")
    }

    #[test]
    fn stores_and_serves_a_fresh_entry() {
        let c = cache(60, 10);
        c.put("k1".into(), "c1".into(), val(1));
        assert_eq!(c.get("k1").map(|h| h.value), Some(val(1)));
        assert_eq!(c.stats().hits, 1);
    }

    #[test]
    fn expired_entry_is_a_miss() {
        let c = cache(0, 10);
        c.put("k1".into(), "c1".into(), val(1));
        assert!(c.get("k1").is_none());
        assert_eq!(c.stats().misses, 1);
        assert_eq!(c.stats().entries, 0);
    }

    #[test]
    fn evicts_least_recently_used_past_capacity() {
        let c = cache(60, 2);
        c.put("k1".into(), "c1".into(), val(1));
        c.put("k2".into(), "c1".into(), val(2));
        let _ = c.get("k1"); // k1 becomes most-recently-used
        c.put("k3".into(), "c1".into(), val(3)); // evicts k2 (LRU)
        assert!(c.get("k1").is_some());
        assert!(c.get("k2").is_none());
        assert!(c.get("k3").is_some());
    }

    #[test]
    fn invalidate_connection_drops_only_that_connection() {
        let c = cache(60, 10);
        c.put("k1".into(), "conn-a".into(), val(1));
        c.put("k2".into(), "conn-b".into(), val(2));
        c.invalidate_connection("conn-a");
        assert!(c.get("k1").is_none());
        assert!(c.get("k2").is_some());
    }

    #[test]
    fn disabling_clears_and_blocks_caching() {
        let c = cache(60, 10);
        c.put("k1".into(), "c1".into(), val(1));
        c.set_config(CacheConfig {
            enabled: false,
            ttl_secs: 60,
            max_entries: 10,
        });
        assert!(c.get("k1").is_none());
        c.put("k2".into(), "c1".into(), val(2));
        assert_eq!(c.stats().entries, 0);
    }

    #[test]
    fn clear_resets_entries_and_stats() {
        let c = cache(60, 10);
        c.put("k1".into(), "c1".into(), val(1));
        let _ = c.get("k1");
        c.clear();
        assert_eq!(c.stats().entries, 0);
        assert_eq!(c.stats().hits, 0);
        assert_eq!(c.stats().misses, 0);
    }

    #[test]
    fn oversized_entry_is_not_cached() {
        let c = QueryCache::with_limits(64, 1024);
        c.set_config(CacheConfig {
            enabled: true,
            ttl_secs: 60,
            max_entries: 100,
        });
        c.put("big".into(), "c1".into(), "x".repeat(128));
        assert!(c.get("big").is_none());
        assert_eq!(c.stats().entries, 0);
    }

    #[test]
    fn evicts_when_total_bytes_exceeded() {
        // Entry-count limit is generous; the byte ceiling is the binding bound.
        let c = QueryCache::with_limits(1024, 250);
        c.set_config(CacheConfig {
            enabled: true,
            ttl_secs: 60,
            max_entries: 100,
        });
        c.put("k1".into(), "c1".into(), "x".repeat(100));
        c.put("k2".into(), "c1".into(), "x".repeat(100));
        c.put("k3".into(), "c1".into(), "x".repeat(100)); // total 300 > 250 → evict LRU
        assert!(c.get("k1").is_none());
        assert!(c.get("k2").is_some());
        assert!(c.get("k3").is_some());
    }
}

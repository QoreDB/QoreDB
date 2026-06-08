// SPDX-License-Identifier: Apache-2.0

//! Per-plugin key-value store backing the `storage` capability.
//!
//! Each enabled plugin gets its own `storage.json` file inside its plugin
//! folder. Reads are served from an in-memory cache; writes are flushed
//! immediately with an atomic rename so a crash mid-write cannot truncate
//! the file. Hard caps on key length, value length and total size keep a
//! buggy plugin from filling the disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Generous-but-strict ceilings: any plugin that actually needs more than
/// this is doing something the `storage` capability was not designed for.
pub const MAX_KEY_LEN: usize = 256;
pub const MAX_VALUE_LEN: usize = 64 * 1024;
pub const MAX_TOTAL_ENTRIES: usize = 1024;
pub const MAX_TOTAL_BYTES: usize = 1024 * 1024;

/// File-backed KV store for a single plugin. Cheap to construct: deserialises
/// the JSON lazily on the first access.
pub struct PluginStorage {
    path: PathBuf,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    loaded: bool,
    entries: BTreeMap<String, String>,
    bytes: usize,
}

#[derive(Debug)]
pub enum StorageError {
    KeyTooLong,
    ValueTooLong,
    QuotaExceeded,
    Io(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KeyTooLong => write!(f, "storage key exceeds {MAX_KEY_LEN} bytes"),
            Self::ValueTooLong => write!(f, "storage value exceeds {MAX_VALUE_LEN} bytes"),
            Self::QuotaExceeded => write!(f, "storage quota exceeded"),
            Self::Io(m) => write!(f, "storage io error: {m}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl PluginStorage {
    /// Creates a handle to the storage file at `plugins/<id>/storage.json`.
    /// The file itself is read on demand.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            state: Mutex::new(State::default()),
        }
    }

    fn ensure_loaded(&self, state: &mut State) {
        if state.loaded {
            return;
        }
        state.loaded = true;
        let Ok(raw) = std::fs::read_to_string(&self.path) else {
            return;
        };
        if let Ok(entries) = serde_json::from_str::<BTreeMap<String, String>>(&raw) {
            state.bytes = entries.iter().map(|(k, v)| k.len() + v.len()).sum();
            state.entries = entries;
        }
    }

    fn persist(&self, state: &State) -> Result<(), StorageError> {
        let raw =
            serde_json::to_string(&state.entries).map_err(|e| StorageError::Io(e.to_string()))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StorageError::Io(e.to_string()))?;
        }
        crate::paths::atomic_write(&self.path, raw.as_bytes())
            .map_err(|e| StorageError::Io(e.to_string()))
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let mut state = self.state.lock().ok()?;
        self.ensure_loaded(&mut state);
        state.entries.get(key).cloned()
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        if key.len() > MAX_KEY_LEN {
            return Err(StorageError::KeyTooLong);
        }
        if value.len() > MAX_VALUE_LEN {
            return Err(StorageError::ValueTooLong);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| StorageError::Io("storage lock poisoned".into()))?;
        self.ensure_loaded(&mut state);

        let previous_len = state.entries.get(key).map(|v| v.len()).unwrap_or(0);
        let key_cost = if state.entries.contains_key(key) {
            0
        } else {
            key.len()
        };
        let new_bytes = state
            .bytes
            .saturating_sub(previous_len)
            .saturating_add(value.len())
            .saturating_add(key_cost);
        let new_entry_count = if state.entries.contains_key(key) {
            state.entries.len()
        } else {
            state.entries.len() + 1
        };
        if new_bytes > MAX_TOTAL_BYTES || new_entry_count > MAX_TOTAL_ENTRIES {
            return Err(StorageError::QuotaExceeded);
        }

        state.entries.insert(key.to_string(), value.to_string());
        state.bytes = new_bytes;
        self.persist(&state)
    }

    pub fn delete(&self, key: &str) -> Result<(), StorageError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StorageError::Io("storage lock poisoned".into()))?;
        self.ensure_loaded(&mut state);
        if let Some(previous) = state.entries.remove(key) {
            state.bytes = state
                .bytes
                .saturating_sub(previous.len())
                .saturating_sub(key.len());
            self.persist(&state)
        } else {
            Ok(())
        }
    }
}

/// Convenience: storage file for plugin `id` under the plugins directory.
pub fn storage_path(plugins_dir: &Path, plugin_id: &str) -> PathBuf {
    plugins_dir.join(plugin_id).join("storage.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "qoredb_storage_test_{}_{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base.join("storage.json")
    }

    #[test]
    fn round_trips_a_value() {
        let s = PluginStorage::new(tmp_path("rt"));
        s.set("k", "v").unwrap();
        assert_eq!(s.get("k").as_deref(), Some("v"));
    }

    #[test]
    fn delete_removes_a_key() {
        let s = PluginStorage::new(tmp_path("del"));
        s.set("k", "v").unwrap();
        s.delete("k").unwrap();
        assert_eq!(s.get("k"), None);
    }

    #[test]
    fn rejects_oversized_value() {
        let s = PluginStorage::new(tmp_path("size"));
        let big = "x".repeat(MAX_VALUE_LEN + 1);
        assert!(matches!(s.set("k", &big), Err(StorageError::ValueTooLong)));
    }

    #[test]
    fn enforces_quota() {
        let s = PluginStorage::new(tmp_path("quota"));
        let mid = "x".repeat(MAX_VALUE_LEN);
        // Fill until the byte quota is reached, then expect a quota error.
        let mut i = 0;
        loop {
            let key = format!("k{i}");
            match s.set(&key, &mid) {
                Ok(()) => i += 1,
                Err(StorageError::QuotaExceeded) => break,
                Err(e) => panic!("unexpected: {e}"),
            }
        }
        assert!(i >= 1);
    }

    #[test]
    fn persists_across_handles() {
        let path = tmp_path("persist");
        {
            let s = PluginStorage::new(path.clone());
            s.set("k", "v").unwrap();
        }
        let s2 = PluginStorage::new(path);
        assert_eq!(s2.get("k").as_deref(), Some("v"));
    }
}

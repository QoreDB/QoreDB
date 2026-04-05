// SPDX-License-Identifier: Apache-2.0

//! Write Registry
//!
//! Tracks files currently being written by QoreDB so the file watcher
//! can ignore self-triggered events and avoid feedback loops.

use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const UNREGISTER_DELAY: Duration = Duration::from_millis(300);

/// Shared registry of files currently being written by QoreDB.
#[derive(Clone, Default)]
pub struct WriteRegistry {
    inner: Arc<Mutex<HashSet<PathBuf>>>,
}

impl WriteRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a WriteRegistry that shares state with the given Arc.
    /// Used to bridge between the Arc<WriteRegistry> managed by Tauri
    /// and the WriteRegistry expected by the watcher.
    pub fn from_arc(arc: Arc<Mutex<HashSet<PathBuf>>>) -> Self {
        Self { inner: arc }
    }

    /// Returns a reference to the inner Arc for sharing.
    pub fn inner_arc(&self) -> &Arc<Mutex<HashSet<PathBuf>>> {
        &self.inner
    }

    /// Register a path as being written by QoreDB.
    pub fn register(&self, path: PathBuf) {
        self.inner.lock().insert(path);
    }

    /// Remove a path from the registry.
    pub fn unregister(&self, path: &PathBuf) {
        self.inner.lock().remove(path);
    }

    /// Check if a path is currently being written by QoreDB.
    pub fn is_self_write(&self, path: &PathBuf) -> bool {
        self.inner.lock().contains(path)
    }

    /// Register a path and schedule automatic unregistration after a delay.
    /// The delay accounts for the time between `fs::write` returning and
    /// `notify` delivering the event.
    pub fn register_with_auto_unregister(&self, path: PathBuf) {
        self.register(path.clone());
        let registry = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(UNREGISTER_DELAY).await;
            registry.unregister(&path);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_check() {
        let reg = WriteRegistry::new();
        let path = PathBuf::from("/tmp/test.json");

        assert!(!reg.is_self_write(&path));
        reg.register(path.clone());
        assert!(reg.is_self_write(&path));
        reg.unregister(&path);
        assert!(!reg.is_self_write(&path));
    }

    #[test]
    fn multiple_paths() {
        let reg = WriteRegistry::new();
        let a = PathBuf::from("/a.json");
        let b = PathBuf::from("/b.json");

        reg.register(a.clone());
        reg.register(b.clone());
        assert!(reg.is_self_write(&a));
        assert!(reg.is_self_write(&b));

        reg.unregister(&a);
        assert!(!reg.is_self_write(&a));
        assert!(reg.is_self_write(&b));
    }

    #[test]
    fn clone_shares_state() {
        let reg = WriteRegistry::new();
        let reg2 = reg.clone();
        let path = PathBuf::from("/shared.json");

        reg.register(path.clone());
        assert!(reg2.is_self_write(&path));
    }
}

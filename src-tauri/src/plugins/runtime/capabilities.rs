// SPDX-License-Identifier: Apache-2.0

//! Plugin capability model and per-plugin consent persistence.
//!
//! A plugin's manifest *requests* capabilities; the user *grants* a subset
//! through the consent dialog at install time (or later from the plugin
//! detail view). Host functions check the granted set at call time, so a
//! revoked capability stops working without an app restart.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::plugins::{PluginCapabilities, PluginManifest};

/// One of the Phase 2 capabilities a plugin may request. Phase 3 capabilities
/// (`http`, `fs`, `secrets`, `queryExec`) are still declared in the manifest
/// but not yet honoured at runtime — they validate, but never grant access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityKind {
    /// Append messages to the plugin's per-instance log.
    Log,
    /// Surface a toast notification to the user.
    Notify,
    /// Read and write entries in the plugin's bounded key-value store.
    Storage,
    /// Read row metadata and contents from the `postExecute` result.
    QueryRead,
}

impl CapabilityKind {
    /// Stable identifier used in the consent file and i18n keys.
    pub const fn as_str(self) -> &'static str {
        match self {
            CapabilityKind::Log => "log",
            CapabilityKind::Notify => "notify",
            CapabilityKind::Storage => "storage",
            CapabilityKind::QueryRead => "queryRead",
        }
    }

    /// Every Phase 2 capability in display order. Phase 3 capabilities are
    /// intentionally absent.
    pub const ALL: [CapabilityKind; 4] = [
        CapabilityKind::Log,
        CapabilityKind::Notify,
        CapabilityKind::Storage,
        CapabilityKind::QueryRead,
    ];
}

/// The set of Phase 2 capabilities a manifest *requests*. Order is stable so
/// the consent UI shows the same list every time.
pub fn requested(caps: &PluginCapabilities) -> Vec<CapabilityKind> {
    let mut out = Vec::new();
    if caps.log {
        out.push(CapabilityKind::Log);
    }
    if caps.notify {
        out.push(CapabilityKind::Notify);
    }
    if caps.storage {
        out.push(CapabilityKind::Storage);
    }
    if caps.query_read {
        out.push(CapabilityKind::QueryRead);
    }
    out
}

/// Pulls the requested-capabilities list straight from a manifest. An absent
/// `runtime` block (declarative-only plugin) requests nothing.
pub fn requested_from_manifest(manifest: &PluginManifest) -> Vec<CapabilityKind> {
    manifest
        .runtime
        .as_ref()
        .map(|r| requested(&r.capabilities))
        .unwrap_or_default()
}

/// On-disk consent record: `plugin_id → granted-capabilities`. A plugin not
/// in the map has granted nothing.
type ConsentIndex = BTreeMap<String, BTreeSet<CapabilityKind>>;

fn consent_file(dir: &Path) -> std::path::PathBuf {
    dir.join("consent.json")
}

fn read_index(dir: &Path) -> ConsentIndex {
    std::fs::read_to_string(consent_file(dir))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_index(dir: &Path, index: &ConsentIndex) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(index).map_err(|e| e.to_string())?;
    crate::paths::atomic_write(&consent_file(dir), raw.as_bytes())
        .map_err(|e| format!("Failed to write plugin consent: {e}"))
}

/// Reads the capabilities a user has granted to a plugin.
pub fn read_grants(dir: &Path, plugin_id: &str) -> BTreeSet<CapabilityKind> {
    read_index(dir).remove(plugin_id).unwrap_or_default()
}

/// Overwrites the capabilities granted to a plugin. Pass an empty set to
/// revoke everything.
pub fn write_grants(
    dir: &Path,
    plugin_id: &str,
    grants: BTreeSet<CapabilityKind>,
) -> Result<(), String> {
    let mut index = read_index(dir);
    if grants.is_empty() {
        index.remove(plugin_id);
    } else {
        index.insert(plugin_id.to_string(), grants);
    }
    write_index(dir, &index)
}

/// Drops a plugin's consent record entirely (called when the plugin is
/// removed). Silent no-op if no record exists.
pub fn forget(dir: &Path, plugin_id: &str) -> Result<(), String> {
    let mut index = read_index(dir);
    if index.remove(plugin_id).is_some() {
        write_index(dir, &index)?;
    }
    Ok(())
}

/// Loads every plugin's grants in one shot. Used by the runtime to snapshot
/// consent when (re)building plugin instances.
pub fn read_all(dir: &Path) -> ConsentIndex {
    read_index(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "qoredb_consent_test_{}_{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn capability_kinds_have_stable_ids() {
        assert_eq!(CapabilityKind::Log.as_str(), "log");
        assert_eq!(CapabilityKind::QueryRead.as_str(), "queryRead");
    }

    #[test]
    fn requested_reflects_manifest_flags() {
        let mut caps = PluginCapabilities::default();
        caps.log = true;
        caps.query_read = true;
        let r = requested(&caps);
        assert!(r.contains(&CapabilityKind::Log));
        assert!(r.contains(&CapabilityKind::QueryRead));
        assert!(!r.contains(&CapabilityKind::Notify));
    }

    #[test]
    fn round_trips_grants() {
        let dir = temp_dir("rt");
        let mut grants = BTreeSet::new();
        grants.insert(CapabilityKind::Log);
        grants.insert(CapabilityKind::Notify);
        write_grants(&dir, "acme.x", grants.clone()).unwrap();
        assert_eq!(read_grants(&dir, "acme.x"), grants);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_grants_removes_record() {
        let dir = temp_dir("empty");
        let mut grants = BTreeSet::new();
        grants.insert(CapabilityKind::Log);
        write_grants(&dir, "acme.y", grants).unwrap();
        write_grants(&dir, "acme.y", BTreeSet::new()).unwrap();
        assert!(read_grants(&dir, "acme.y").is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn forget_drops_record() {
        let dir = temp_dir("forget");
        let mut grants = BTreeSet::new();
        grants.insert(CapabilityKind::Storage);
        write_grants(&dir, "acme.z", grants).unwrap();
        forget(&dir, "acme.z").unwrap();
        assert!(read_grants(&dir, "acme.z").is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}

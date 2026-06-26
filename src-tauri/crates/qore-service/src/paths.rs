// SPDX-License-Identifier: Apache-2.0

//! Centralised filesystem layout for QoreDB.
//!
//! Before this module existed, three independent sites picked different
//! locations:
//!   - `lib.rs` → `dirs::data_local_dir() / "com.qoredb.app"`
//!   - `policy.rs` → `~/.qoredb/config.json` (Unix) /
//!                   `%APPDATA%/QoreDB/config.json` (Windows)
//!   - `observability.rs` → `~/.qoredb/logs/` (Unix) /
//!                          `%APPDATA%/QoreDB/logs/` (Windows)
//!
//! On Linux all three resolved to different directories. The duplication made
//! debugging painful ("where are my logs / settings?") and complicated any
//! future migration. This module consolidates them around the same root used
//! by `lib.rs` (cf. audit B1-H4).
//!
//! For each helper we fall back to the current working directory `"."` when
//! the OS query fails (no `$HOME`, headless CI, etc.) — same shape as the
//! pre-existing call sites, so the failure mode is unchanged.

use std::path::{Path, PathBuf};

/// Identifier embedded in every QoreDB path. Matches Tauri's bundle
/// identifier so the OS attributes data dirs to the same app.
const APP_BUNDLE_ID: &str = "com.qoredb.app";

/// Root directory for QoreDB persistent state (databases of preferences,
/// interceptor cache, snapshots, time-travel changelog, …). On a fresh
/// install nothing exists yet; callers are expected to `create_dir_all` the
/// specific subdirectory they need.
pub fn app_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_BUNDLE_ID)
}

/// Directory for log files. Sits under [`app_data_dir`] rather than under
/// `dirs::cache_dir()` because we want logs to survive cache wipes (they're
/// the only forensic record we keep client-side).
pub fn app_log_dir() -> PathBuf {
    app_data_dir().join("logs")
}

/// File holding the persisted [`SafetyPolicy`]. Stored under the data dir
/// alongside the interceptor / time-travel files for a single backup target.
pub fn safety_policy_file() -> PathBuf {
    app_data_dir().join("config.json")
}

/// Config directory for the headless entry points (CLI, MCP, server). Resolves
/// to the same location the desktop app stores its vault, so every front-end
/// shares one credential store. Honors `QOREDB_CONFIG_DIR` as an override
/// (tests, custom installs); otherwise the OS config dir under the Tauri bundle
/// identifier. Distinct from [`app_data_dir`], which holds policy/logs/cache.
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("QOREDB_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.rapha.qoredb")
}

/// Vault project id used by the headless entry points.
pub const PROJECT_ID: &str = "default";

/// Default per-query timeout (ms) for the headless entry points.
pub const QUERY_TIMEOUT_MS: u64 = 30_000;

/// Writes `contents` to `path` atomically: data is written to a sibling temp
/// file first, then a rename swaps it in. A crash mid-write therefore leaves
/// the previous file intact instead of a truncated, unparseable one.
pub fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_share_the_same_root() {
        let data = app_data_dir();
        assert!(app_log_dir().starts_with(&data));
        assert!(safety_policy_file().starts_with(&data));
    }

    #[test]
    fn paths_embed_bundle_id() {
        let data = app_data_dir();
        assert!(
            data.to_string_lossy().contains(APP_BUNDLE_ID),
            "app_data_dir must include the bundle identifier, got {}",
            data.display()
        );
    }
}

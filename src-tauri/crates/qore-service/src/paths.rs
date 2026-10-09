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

/// Backstop for an exact row count when the safety policy sets no duration.
/// Deliberately generous: a count the user asked for on purpose should be
/// allowed to finish, but never to hold a connection indefinitely.
pub const EXACT_COUNT_TIMEOUT_MS: u64 = 120_000;

/// Publish complete bytes using a private, unique sibling and a single rename.
/// The file is synced before publication; the parent directory is not synced,
/// so this is atomic replacement, not a power-loss durability guarantee.
/// Concurrent writers publish whole revisions (the last rename wins).
/// Unix outputs are private (0600); other platforms inherit directory ACLs.
pub fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let pending = PendingOutput::new(path)?;
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(pending.path())?;
        #[cfg(test)]
        fail_write_at(WriteFailure::PartialWrite, &mut file)?;
        file.write_all(contents)?;
        #[cfg(test)]
        fail_write_at(WriteFailure::Sync, &mut file)?;
        file.sync_all()?;
        #[cfg(test)]
        fail_write_at(WriteFailure::Publish, &mut file)?;
    }
    pending.commit()
}

// Per-thread, one-shot failures exercise the real callers without changing their API.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum WriteFailure {
    PartialWrite,
    Sync,
    Publish,
}

#[cfg(test)]
thread_local! {
    static WRITE_FAILURE: std::cell::Cell<Option<WriteFailure>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn fail_next_write(stage: WriteFailure) {
    WRITE_FAILURE.set(Some(stage));
}

#[cfg(test)]
fn fail_write_at(stage: WriteFailure, file: &mut std::fs::File) -> std::io::Result<()> {
    use std::io::Write;
    if WRITE_FAILURE.get() == Some(stage) {
        WRITE_FAILURE.set(None);
        if stage == WriteFailure::PartialWrite {
            file.write_all(b"partial")?;
        }
        return Err(std::io::Error::other("injected file write failure"));
    }
    Ok(())
}

/// A private sibling file, published only once its writer has closed successfully.
/// Dropping an unpublished output preserves the destination and removes the staging file.
pub struct PendingOutput {
    destination: PathBuf,
    staging: PathBuf,
}

impl PendingOutput {
    pub fn new(destination: &Path) -> std::io::Result<Self> {
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let staging = parent.join(format!(".qoredb-{}.partial", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&staging)?;
        Ok(Self {
            destination: destination.to_owned(),
            staging,
        })
    }

    pub fn path(&self) -> &Path {
        &self.staging
    }

    pub fn commit(self) -> std::io::Result<()> {
        // A sibling rename never exposes partially written contents. Do not delete
        // the destination first: a failed replacement must preserve its contents.
        std::fs::rename(&self.staging, &self.destination)
    }

    /// Publishes a new file without overwriting a concurrently created target.
    pub fn commit_new(self) -> std::io::Result<()> {
        // Linking the completed sibling is atomic and refuses existing targets.
        // Drop removes the staging name; unsupported filesystems return an error.
        std::fs::hard_link(&self.staging, &self.destination)
    }
}

impl Drop for PendingOutput {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.staging);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abandoned_write_child() {
        let Some(path) = std::env::var_os("QOREDB_TEST_ABANDONED_OUTPUT") else {
            return;
        };
        let pending = PendingOutput::new(Path::new(&path)).unwrap();
        std::fs::write(pending.path(), b"interrupted partial bytes").unwrap();
        // Exiting skips destructors, as does termination during an in-flight write.
        std::process::exit(0);
    }

    #[test]
    fn process_exit_leaves_previous_file_and_retry_ignores_abandoned_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("document.qnb");
        atomic_write(&path, b"previous").unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "paths::tests::abandoned_write_child"])
            .env("QOREDB_TEST_ABANDONED_OUTPUT", &path)
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
        atomic_write(&path, b"complete").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"complete");
        // Never guess that another writer's staging file is safe to remove.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn existing_temporary_names_are_never_reused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("connections.json");
        let collision = path.with_extension("tmp");
        std::fs::write(&collision, b"another writer").unwrap();
        atomic_write(&path, b"complete").unwrap();
        assert_eq!(std::fs::read(&collision).unwrap(), b"another writer");
        assert_eq!(std::fs::read(path).unwrap(), b"complete");
    }

    #[test]
    fn failed_writes_preserve_previous_bytes_clean_up_and_allow_retry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("document.qnb");
        for failure in [
            WriteFailure::PartialWrite,
            WriteFailure::Sync,
            WriteFailure::Publish,
        ] {
            atomic_write(&path, b"previous").unwrap();
            fail_next_write(failure);
            assert!(atomic_write(&path, b"replacement").is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"previous");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
            atomic_write(&path, b"replacement").unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn failed_first_write_leaves_no_document_or_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.json");
        fail_next_write(WriteFailure::PartialWrite);
        assert!(atomic_write(&path, b"new").is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        atomic_write(&path, b"new").unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"new");
    }

    #[test]
    fn simultaneous_publications_never_mix_or_remove_another_writers_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("document.json");
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for byte in 0..8 {
                let destination = &destination;
                let barrier = &barrier;
                scope.spawn(move || {
                    let output = PendingOutput::new(destination).unwrap();
                    std::fs::write(output.path(), vec![byte; 128_000]).unwrap();
                    barrier.wait();
                    output.commit().unwrap();
                    let bytes = std::fs::read(destination).unwrap();
                    assert_eq!(bytes.len(), 128_000);
                    assert!(bytes.iter().all(|b| *b == bytes[0]));
                });
            }
        });
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn publishing_new_output_never_replaces_an_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("new");
        let first = PendingOutput::new(&destination).unwrap();
        let second = PendingOutput::new(&destination).unwrap();
        std::fs::write(first.path(), "first complete file").unwrap();
        std::fs::write(second.path(), "second complete file").unwrap();
        first.commit_new().unwrap();
        assert!(second.commit_new().is_err());
        assert_eq!(
            std::fs::read_to_string(destination).unwrap(),
            "first complete file"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn pending_outputs_are_unique_and_discarded_without_touching_destination() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("export");
        std::fs::write(&destination, "previous").unwrap();
        let first = PendingOutput::new(&destination).unwrap();
        let second = PendingOutput::new(&destination).unwrap();
        assert_ne!(first.path(), second.path());
        std::fs::write(first.path(), "partial").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(first.path())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        drop(first);
        drop(second);
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "previous");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn pending_output_replaces_only_on_commit() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("export");
        std::fs::write(&destination, "previous").unwrap();
        let output = PendingOutput::new(&destination).unwrap();
        std::fs::write(output.path(), "complete").unwrap();
        assert_eq!(std::fs::read_to_string(&destination).unwrap(), "previous");
        output.commit().unwrap();
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "complete");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_commit_preserves_destination_and_cleans_staging_file() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("directory");
        std::fs::create_dir(&destination).unwrap();
        let output = PendingOutput::new(&destination).unwrap();
        let staging = output.path().to_owned();
        assert!(output.commit().is_err());
        assert!(destination.is_dir());
        assert!(!staging.exists());
    }

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

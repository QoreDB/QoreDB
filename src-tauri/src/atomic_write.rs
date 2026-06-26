// SPDX-License-Identifier: Apache-2.0

//! Crash-safe file writes.
//!
//! Direct `fs::write` truncates the target before writing, so a crash mid-write
//! leaves a truncated/corrupt file. [`write_atomic`] writes to a sibling temp
//! file, fsyncs it, then renames over the target — after a crash the target is
//! either the old content or the full new content, never partial.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Atomically writes `bytes` to `path` (temp file + fsync + rename).
///
/// The temp file is a sibling of `path` so the rename stays on one filesystem
/// (a cross-device rename would fail). Its `.tmp` extension also keeps the
/// workspace watcher — which only reacts to `.json` files — from firing on it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = tmp_path(path);
    {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_replaces_content() {
        let dir = std::env::temp_dir().join("qoredb_atomic_write_test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("state.json");

        write_atomic(&path, b"first").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"first");

        write_atomic(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");

        // No temp file left behind after a successful write.
        assert!(!dir.join("state.json.tmp").exists());

        let _ = fs::remove_dir_all(&dir);
    }
}

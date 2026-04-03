// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

const WORKSPACE_DIR: &str = ".qoredb";
const WORKSPACE_MANIFEST: &str = "workspace.json";
const MAX_PARENT_LEVELS: usize = 20;

/// Walk up from `start` looking for a `.qoredb/workspace.json` file.
/// Returns the path to the `.qoredb/` directory if found.
pub fn detect_workspace(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();

    for _ in 0..MAX_PARENT_LEVELS {
        let candidate = current.join(WORKSPACE_DIR).join(WORKSPACE_MANIFEST);
        if candidate.is_file() {
            return Some(current.join(WORKSPACE_DIR));
        }
        if !current.pop() {
            break;
        }
    }

    None
}

/// Detect a workspace from the process's current working directory.
pub fn detect_workspace_from_cwd() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    detect_workspace(&cwd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn detects_workspace_in_current_dir() {
        let tmp = TempDir::new().unwrap();
        let qoredb_dir = tmp.path().join(".qoredb");
        fs::create_dir_all(&qoredb_dir).unwrap();
        fs::write(
            qoredb_dir.join("workspace.json"),
            r#"{"version":1,"name":"test","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#,
        )
        .unwrap();

        let result = detect_workspace(tmp.path());
        assert_eq!(result, Some(qoredb_dir));
    }

    #[test]
    fn detects_workspace_in_parent_dir() {
        let tmp = TempDir::new().unwrap();
        let qoredb_dir = tmp.path().join(".qoredb");
        fs::create_dir_all(&qoredb_dir).unwrap();
        fs::write(
            qoredb_dir.join("workspace.json"),
            r#"{"version":1,"name":"test","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#,
        )
        .unwrap();

        let child = tmp.path().join("src").join("deep");
        fs::create_dir_all(&child).unwrap();

        let result = detect_workspace(&child);
        assert_eq!(result, Some(qoredb_dir));
    }

    #[test]
    fn returns_none_when_no_workspace() {
        let tmp = TempDir::new().unwrap();
        let result = detect_workspace(tmp.path());
        assert!(result.is_none());
    }
}

// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::engine::error::{EngineError, EngineResult};
use crate::workspace::discovery;
use crate::workspace::types::{
    RecentWorkspace, WorkspaceInfo, WorkspaceManifest, WorkspaceSource, WORKSPACE_SCHEMA_VERSION,
};

const WORKSPACE_DIR: &str = ".qoredb";
const WORKSPACE_MANIFEST_FILE: &str = "workspace.json";
const RECENT_WORKSPACES_FILE: &str = "recent_workspaces.json";
const MAX_RECENT_WORKSPACES: usize = 10;

const GITIGNORE_CONTENT: &str = "# Secrets are never stored in .qoredb, but just in case:\n*.key\n*.pem\n\n# Local cache\n.cache/\n";

/// FNV-1a 64-bit hash 
fn fnv1a_hash(data: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Manages workspace lifecycle: detection, creation, loading, switching.
pub struct WorkspaceManager {
    /// Path to the global app config directory (for recent workspaces, default workspace).
    app_config_dir: PathBuf,
    /// The currently active workspace.
    active: WorkspaceInfo,
}

impl WorkspaceManager {
    /// Creates a new manager, initializing with the default workspace.
    pub fn new(app_config_dir: PathBuf) -> Self {
        let active = Self::make_default_workspace(&app_config_dir);
        Self {
            app_config_dir,
            active,
        }
    }

    /// Returns the active workspace.
    pub fn active(&self) -> &WorkspaceInfo {
        &self.active
    }

    /// Returns the project ID derived from the active workspace.
    /// For the default workspace this is `"default"`.
    /// For file-based workspaces this is `"ws_<hash>"` using FNV-1a (stable across Rust versions).
    pub fn project_id(&self) -> String {
        match self.active.source {
            WorkspaceSource::Default => "default".to_string(),
            _ => {
                let hash = fnv1a_hash(self.active.path.to_string_lossy().as_bytes());
                format!("ws_{:016x}", hash)
            }
        }
    }

    /// Attempts to detect a workspace from the CWD and switch to it if found.
    /// Returns Some(info) if a workspace was detected.
    pub fn detect_and_activate(&mut self) -> Option<WorkspaceInfo> {
        let path = discovery::detect_workspace_from_cwd()?;
        match self.load_workspace_at(&path, WorkspaceSource::Detected) {
            Ok(info) => {
                self.active = info.clone();
                let _ = self.add_to_recent(&info);
                Some(info)
            }
            Err(_) => None,
        }
    }

    /// Loads and switches to the workspace at the given `.qoredb/` path.
    pub fn switch_to(&mut self, qoredb_path: &Path, source: WorkspaceSource) -> EngineResult<WorkspaceInfo> {
        let info = self.load_workspace_at(qoredb_path, source)?;
        self.active = info.clone();
        let _ = self.add_to_recent(&info);
        Ok(info)
    }

    /// Switches back to the default workspace.
    pub fn switch_to_default(&mut self) -> WorkspaceInfo {
        let info = Self::make_default_workspace(&self.app_config_dir);
        self.active = info.clone();
        info
    }

    /// Creates a new workspace at `project_dir/.qoredb/`.
    pub fn create_workspace(&mut self, project_dir: &Path, name: &str) -> EngineResult<WorkspaceInfo> {
        let qoredb_dir = project_dir.join(WORKSPACE_DIR);

        if qoredb_dir.join(WORKSPACE_MANIFEST_FILE).exists() {
            return Err(EngineError::internal(
                "A workspace already exists in this directory",
            ));
        }

        // Create the directory structure
        let dirs = [
            qoredb_dir.clone(),
            qoredb_dir.join("connections"),
            qoredb_dir.join("notebooks"),
            qoredb_dir.join("queries"),
            qoredb_dir.join("contracts"),
            qoredb_dir.join("context"),
        ];

        for dir in &dirs {
            fs::create_dir_all(dir).map_err(|e| {
                EngineError::internal(format!("Failed to create directory {}: {}", dir.display(), e))
            })?;
        }

        // Write workspace.json
        let now = Utc::now().to_rfc3339();
        let manifest = WorkspaceManifest {
            version: WORKSPACE_SCHEMA_VERSION,
            name: name.to_string(),
            created_at: now.clone(),
            updated_at: now,
        };

        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| EngineError::internal(format!("Failed to serialize manifest: {}", e)))?;

        fs::write(qoredb_dir.join(WORKSPACE_MANIFEST_FILE), manifest_json)
            .map_err(|e| EngineError::internal(format!("Failed to write workspace.json: {}", e)))?;

        // Write .gitignore
        fs::write(qoredb_dir.join(".gitignore"), GITIGNORE_CONTENT)
            .map_err(|e| EngineError::internal(format!("Failed to write .gitignore: {}", e)))?;

        // Write empty query library
        fs::write(
            qoredb_dir.join("queries").join("library.json"),
            r#"{"version":1,"folders":[],"items":[]}"#,
        )
        .map_err(|e| EngineError::internal(format!("Failed to write library.json: {}", e)))?;

        let info = WorkspaceInfo {
            path: qoredb_dir,
            manifest,
            source: WorkspaceSource::Manual,
        };

        self.active = info.clone();
        let _ = self.add_to_recent(&info);

        Ok(info)
    }

    /// Lists recently opened workspaces.
    pub fn list_recent(&self) -> Vec<RecentWorkspace> {
        let path = self.app_config_dir.join(RECENT_WORKSPACES_FILE);
        if !path.exists() {
            return Vec::new();
        }

        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    // ── Private ──────────────────────────────────────────────

    fn make_default_workspace(app_config_dir: &Path) -> WorkspaceInfo {
        WorkspaceInfo {
            path: app_config_dir.to_path_buf(),
            manifest: WorkspaceManifest {
                version: WORKSPACE_SCHEMA_VERSION,
                name: "Default".to_string(),
                created_at: String::new(),
                updated_at: String::new(),
            },
            source: WorkspaceSource::Default,
        }
    }

    fn load_workspace_at(
        &self,
        qoredb_path: &Path,
        source: WorkspaceSource,
    ) -> EngineResult<WorkspaceInfo> {
        let manifest_path = qoredb_path.join(WORKSPACE_MANIFEST_FILE);

        let content = fs::read_to_string(&manifest_path).map_err(|e| {
            EngineError::internal(format!(
                "Failed to read {}: {}",
                manifest_path.display(),
                e
            ))
        })?;

        let manifest: WorkspaceManifest = serde_json::from_str(&content).map_err(|e| {
            EngineError::internal(format!("Invalid workspace.json: {}", e))
        })?;

        if manifest.version == 0 {
            return Err(EngineError::internal(
                "Invalid workspace version: 0".to_string(),
            ));
        }
        if manifest.version > WORKSPACE_SCHEMA_VERSION {
            tracing::warn!(
                "Workspace {} uses version {} (current: {}). Some features may not work.",
                qoredb_path.display(),
                manifest.version,
                WORKSPACE_SCHEMA_VERSION
            );
        }

        Ok(WorkspaceInfo {
            path: qoredb_path.to_path_buf(),
            manifest,
            source,
        })
    }

    fn add_to_recent(&self, info: &WorkspaceInfo) -> EngineResult<()> {
        if info.source == WorkspaceSource::Default {
            return Ok(());
        }

        let mut recents = self.list_recent();

        // Remove existing entry for this path
        recents.retain(|r| r.path != info.path);

        // Prepend
        recents.insert(
            0,
            RecentWorkspace {
                path: info.path.clone(),
                name: info.manifest.name.clone(),
                last_opened: Utc::now().to_rfc3339(),
            },
        );

        // Trim
        recents.truncate(MAX_RECENT_WORKSPACES);

        let path = self.app_config_dir.join(RECENT_WORKSPACES_FILE);
        let content = serde_json::to_string_pretty(&recents)
            .map_err(|e| EngineError::internal(format!("Failed to serialize recents: {}", e)))?;

        fs::write(&path, content)
            .map_err(|e| EngineError::internal(format!("Failed to write recents: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_workspace_project_id() {
        let tmp = TempDir::new().unwrap();
        let mgr = WorkspaceManager::new(tmp.path().to_path_buf());
        assert_eq!(mgr.project_id(), "default");
        assert_eq!(mgr.active().source, WorkspaceSource::Default);
    }

    #[test]
    fn create_and_load_workspace() {
        let tmp = TempDir::new().unwrap();
        let app_config = TempDir::new().unwrap();
        let mut mgr = WorkspaceManager::new(app_config.path().to_path_buf());

        let info = mgr
            .create_workspace(tmp.path(), "My Project")
            .expect("create failed");

        assert_eq!(info.manifest.name, "My Project");
        assert_eq!(info.manifest.version, 1);
        assert!(info.path.join("workspace.json").exists());
        assert!(info.path.join(".gitignore").exists());
        assert!(info.path.join("connections").is_dir());
        assert!(info.path.join("notebooks").is_dir());
        assert!(info.path.join("queries/library.json").exists());

        // project_id should be ws_<hash>
        assert!(mgr.project_id().starts_with("ws_"));

        // Recent workspaces should include it
        let recents = mgr.list_recent();
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].name, "My Project");
    }

    #[test]
    fn create_workspace_rejects_duplicate() {
        let tmp = TempDir::new().unwrap();
        let app_config = TempDir::new().unwrap();
        let mut mgr = WorkspaceManager::new(app_config.path().to_path_buf());

        mgr.create_workspace(tmp.path(), "Project A").unwrap();
        let err = mgr.create_workspace(tmp.path(), "Project B");
        assert!(err.is_err());
    }

    #[test]
    fn switch_to_default() {
        let tmp = TempDir::new().unwrap();
        let app_config = TempDir::new().unwrap();
        let mut mgr = WorkspaceManager::new(app_config.path().to_path_buf());

        mgr.create_workspace(tmp.path(), "Test").unwrap();
        assert!(mgr.project_id().starts_with("ws_"));

        mgr.switch_to_default();
        assert_eq!(mgr.project_id(), "default");
    }

    /// Ensures FNV-1a produces deterministic, stable values.
    /// These golden values MUST NOT change — credentials are keyed by them.
    /// If this test fails, users will lose access to their saved credentials.
    #[test]
    fn fnv1a_hash_stability() {
        assert_eq!(fnv1a_hash(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a_hash(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a_hash(b"/Users/dev/project/.qoredb"), 0x1c089eff0e6e433e);
        assert_eq!(fnv1a_hash(b"/home/user/app/.qoredb"), 0x49f7a110a4ef9f9b);

        // Same input always gives same output
        let input = b"/some/path/.qoredb";
        assert_eq!(fnv1a_hash(input), fnv1a_hash(input));
    }

    /// The project_id for the same workspace path must be identical across calls.
    #[test]
    fn project_id_is_deterministic() {
        let tmp = TempDir::new().unwrap();
        let app_config = TempDir::new().unwrap();
        let mut mgr = WorkspaceManager::new(app_config.path().to_path_buf());

        mgr.create_workspace(tmp.path(), "Stable").unwrap();
        let id1 = mgr.project_id();
        let id2 = mgr.project_id();
        assert_eq!(id1, id2);
        assert!(id1.starts_with("ws_"));
        assert_eq!(id1.len(), 3 + 16); // "ws_" + 16 hex chars
    }
}

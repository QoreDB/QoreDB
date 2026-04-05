// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Schema version for workspace.json
pub const WORKSPACE_SCHEMA_VERSION: u32 = 1;

/// Manifest stored in `.qoredb/workspace.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub version: u32,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

/// How the workspace was discovered
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSource {
    /// Detected by walking up from the current working directory
    Detected,
    /// Opened manually by the user
    Manual,
    /// The built-in default workspace (app_config_dir)
    Default,
}

/// Full workspace descriptor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    /// Absolute path to the `.qoredb/` directory
    pub path: PathBuf,
    pub manifest: WorkspaceManifest,
    pub source: WorkspaceSource,
}

/// Entry in the recent workspaces list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentWorkspace {
    pub path: PathBuf,
    pub name: String,
    pub last_opened: String,
}

// SPDX-License-Identifier: Apache-2.0

//! Shared substrate for local semantic schema search: Ollama embeddings and
//! the DuckDB vector store, consumed by the desktop app (read-write) and the
//! MCP server (read-only).

pub mod ollama;
pub mod store;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_MODEL: &str = "nomic-embed-text";
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

impl Default for SemanticConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: None,
            model: default_model(),
        }
    }
}

impl SemanticConfig {
    pub fn effective_base_url(&self) -> &str {
        self.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL)
    }

    /// Read the persisted config, falling back to defaults (disabled) when
    /// the file is missing or unreadable.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }
}

/// Directory holding the semantic config and per-workspace index files,
/// shared by every surface so the MCP server reads the index the app built.
pub fn semantic_dir() -> PathBuf {
    crate::paths::app_data_dir().join("semantic")
}

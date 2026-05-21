// SPDX-License-Identifier: Apache-2.0

//! Plugin System Foundation — declarative plugins (Core).
//!
//! v0.1.29 ships *declarative* plugins only: a plugin is a folder with a
//! `plugin.json` manifest that contributes static data — SQL snippet packs,
//! connection templates and color themes. No code is executed, so no sandbox
//! is required. The manifest / registry / lifecycle defined here are the
//! foundation a future executable-plugin runtime (WASM) will plug into.

mod manifest;
mod registry;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use manifest::{is_compatible, parse_manifest, validate_manifest};
pub use registry::{
    get_contributions, install_plugin, list_plugins, remove_plugin, set_plugin_enabled,
};

/// Directory holding installed plugins (`<app-data>/plugins/`).
pub fn plugins_dir() -> PathBuf {
    crate::paths::app_data_dir().join("plugins")
}

/// A parsed `plugin.json` manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Optional QoreDB version requirement, e.g. `">=0.1.29"`.
    #[serde(default)]
    pub qoredb: Option<String>,
    #[serde(default)]
    pub contributes: PluginContributions,
}

/// The three declarative contribution kinds a plugin may provide.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributions {
    #[serde(default)]
    pub snippets: Vec<SnippetContribution>,
    #[serde(default)]
    pub connection_templates: Vec<ConnectionTemplateContribution>,
    #[serde(default)]
    pub themes: Vec<ThemeContribution>,
}

/// A reusable SQL snippet contributed by a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetContribution {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    pub template: String,
}

/// A pre-filled connection preset contributed by a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTemplateContribution {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub driver: String,
    /// Pre-filled connection fields (host, port, database, …).
    #[serde(default)]
    pub defaults: BTreeMap<String, serde_json::Value>,
}

/// A color theme contributed by a plugin — maps QoreDB design tokens
/// (`--q-*` CSS custom properties) to values, per light / dark mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeContribution {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub light: BTreeMap<String, String>,
    #[serde(default)]
    pub dark: BTreeMap<String, String>,
}

/// A plugin discovered on disk, with its runtime state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPlugin {
    pub manifest: PluginManifest,
    /// Folder name under the plugins directory.
    pub dir_name: String,
    /// Whether the plugin is enabled (its contributions are active).
    pub enabled: bool,
    /// Whether the plugin's `qoredb` requirement matches this build.
    pub compatible: bool,
}

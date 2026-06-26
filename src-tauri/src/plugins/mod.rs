// SPDX-License-Identifier: Apache-2.0

//! Plugin System Foundation — declarative plugins (Core).
//!
//! v0.1.29 ships *declarative* plugins only: a plugin is a folder with a
//! `plugin.json` manifest that contributes static data — SQL snippet packs,
//! connection templates and color themes. No code is executed, so no sandbox
//! is required. The manifest / registry / lifecycle defined here are the
//! foundation a future executable-plugin runtime (WASM) will plug into.

mod manifest;
mod marketplace;
mod registry;
pub mod runtime;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use manifest::{is_compatible, parse_manifest, validate_manifest};
pub use marketplace::{fetch_index, install_from_archive_url};
pub use registry::{
    get_contributions, install_plugin, list_plugins, remove_plugin, set_plugin_enabled,
};

/// Directory holding installed plugins (`<app-data>/plugins/`).
pub fn plugins_dir() -> PathBuf {
    crate::paths::app_data_dir().join("plugins")
}

pub(crate) fn read_json_index<T: Default + serde::de::DeserializeOwned>(
    path: &std::path::Path,
) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub(crate) fn write_json_index<T: serde::Serialize>(
    path: &std::path::Path,
    index: &T,
    what: &str,
) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(index).map_err(|e| e.to_string())?;
    crate::paths::atomic_write(path, raw.as_bytes())
        .map_err(|e| format!("Failed to write {what}: {e}"))
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
    /// Executable-runtime descriptor. Absent for declarative-only plugins.
    #[serde(default)]
    pub runtime: Option<RuntimeSpec>,
}

/// Declarative contribution kinds a plugin may provide. Snippets,
/// connection templates and themes are pure data. `result_viewers` is also
/// pure data but it tells the result grid which built-in renderer to use for
/// matching columns. `commands` ride the executable runtime: clicking a
/// command invokes the plugin's WASM `command` hook.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributions {
    #[serde(default)]
    pub snippets: Vec<SnippetContribution>,
    #[serde(default)]
    pub connection_templates: Vec<ConnectionTemplateContribution>,
    #[serde(default)]
    pub themes: Vec<ThemeContribution>,
    #[serde(default)]
    pub result_viewers: Vec<ResultViewerContribution>,
    #[serde(default)]
    pub commands: Vec<CommandContribution>,
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

/// How a `resultViewers` contribution opts into rendering a column. At least
/// one of the fields must be set, or the contribution is rejected.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerMatch {
    /// Matches the column's declared type (case-insensitive, e.g. `"jsonb"`).
    #[serde(default)]
    pub column_type: Option<String>,
    /// Glob-like pattern against the column's name — `*` is the only wildcard.
    #[serde(default)]
    pub name_pattern: Option<String>,
}

/// The built-in renderers a viewer contribution may select. Hosted by
/// QoreDB; the plugin only picks one and supplies static options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ViewerRenderer {
    JsonTree,
    Image,
    Map,
    Chart,
    Color,
    Boolean,
    Bytes,
}

/// A declarative cell renderer a plugin contributes. The plugin runs no code
/// at render time — QoreDB looks up the matching contribution and hands the
/// cell value to the picked renderer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultViewerContribution {
    pub id: String,
    #[serde(rename = "match")]
    pub match_on: ViewerMatch,
    pub renderer: ViewerRenderer,
    /// Renderer-specific options, passed through to the frontend as-is.
    #[serde(default)]
    pub options: serde_json::Value,
}

/// A user-invocable action a plugin contributes. Clicking the action calls
/// the plugin's WASM `command` hook with the command id and the JSON args.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandContribution {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Executable-runtime descriptor. A plugin carrying this block ships a
/// sandboxed WASM module; without it the plugin is purely declarative.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSpec {
    /// Host ABI version the plugin was built against.
    pub abi_version: u32,
    /// WASM module filename, a bare name relative to the plugin folder.
    pub entry: String,
    /// Lifecycle hooks the plugin subscribes to.
    #[serde(default)]
    pub hooks: Vec<HookKind>,
    /// Capabilities the plugin requests. Default: none.
    #[serde(default)]
    pub capabilities: PluginCapabilities,
    /// Expected SHA-256 of the WASM module, in the form `sha256-<64 hex>`.
    /// When present, the runtime refuses to load a module whose actual hash
    /// differs — a tampered or swapped binary fails fast. Optional: plugins
    /// without an integrity hash are surfaced as "Unsigned" in the UI.
    #[serde(default)]
    pub integrity: Option<String>,
}

/// A lifecycle hook a plugin's WASM module can subscribe to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HookKind {
    /// Runs before a query executes; may allow, warn or block it.
    PreExecute,
    /// Runs after a query completes; observes the outcome.
    PostExecute,
}

/// Capabilities a plugin requests. Every field defaults to the safe value
/// (off / empty), so an omitted `capabilities` block grants nothing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCapabilities {
    /// Write to the plugin's log.
    #[serde(default)]
    pub log: bool,
    /// Show a toast notification in QoreDB.
    #[serde(default)]
    pub notify: bool,
    /// Use the host-managed key-value store.
    #[serde(default)]
    pub storage: bool,
    /// Read the rows and metadata of the current query result.
    #[serde(default)]
    pub query_read: bool,
    /// Make outbound HTTP requests to an explicit host allow-list.
    #[serde(default)]
    pub http: Option<HttpCapability>,
    /// Read and write within the plugin's own data directory.
    #[serde(default)]
    pub fs: Option<FsCapability>,
    /// Read named secrets the user has provisioned for this plugin.
    #[serde(default)]
    pub secrets: Vec<String>,
}

/// Outbound-HTTP capability: the hosts a plugin may reach.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpCapability {
    /// Hosts the plugin may contact. An empty list is rejected at validation.
    pub allowed_hosts: Vec<String>,
    /// Opt-in escape hatch: when true, the plugin may resolve `allowed_hosts`
    /// to private / loopback / link-local / cloud-metadata addresses. False
    /// by default so an attacker who tricks DNS into pointing
    /// `api.example.com` at `169.254.169.254` (or `10.0.0.1`) cannot use the
    /// plugin as an SSRF jump host into the user's internal network.
    #[serde(default)]
    pub allow_private_networks: bool,
}

/// Filesystem capability, scoped so a plugin can never escape its own folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsCapability {
    pub scope: FsScope,
}

/// Filesystem scopes a plugin may request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FsScope {
    /// The plugin's own data directory only.
    PluginData,
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

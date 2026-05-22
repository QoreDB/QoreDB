// SPDX-License-Identifier: Apache-2.0

//! Plugin system Tauri commands.

use crate::plugins::{self, InstalledPlugin, PluginContributions};

/// Runs a blocking filesystem operation off the async runtime so plugin I/O
/// (folder scans, recursive copies) never stalls the Tauri event loop.
async fn blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("Plugin task failed: {e}"))
}

/// Lists every installed plugin with its runtime state.
#[tauri::command]
pub async fn list_plugins() -> Result<Vec<InstalledPlugin>, String> {
    blocking(|| plugins::list_plugins(&plugins::plugins_dir())).await
}

/// Installs (or updates) a plugin from a local folder containing a
/// `plugin.json` manifest.
#[tauri::command]
pub async fn install_plugin(source_path: String) -> Result<InstalledPlugin, String> {
    blocking(move || plugins::install_plugin(&plugins::plugins_dir(), &source_path)).await?
}

/// Removes an installed plugin.
#[tauri::command]
pub async fn remove_plugin(plugin_id: String) -> Result<(), String> {
    blocking(move || plugins::remove_plugin(&plugins::plugins_dir(), &plugin_id)).await?
}

/// Enables or disables an installed plugin.
#[tauri::command]
pub async fn set_plugin_enabled(plugin_id: String, enabled: bool) -> Result<(), String> {
    blocking(move || plugins::set_plugin_enabled(&plugins::plugins_dir(), &plugin_id, enabled))
        .await?
}

/// Returns the aggregated contributions of all enabled, compatible plugins.
#[tauri::command]
pub async fn get_plugin_contributions() -> Result<PluginContributions, String> {
    blocking(|| plugins::get_contributions(&plugins::plugins_dir())).await
}

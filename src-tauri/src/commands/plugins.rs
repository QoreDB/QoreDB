// SPDX-License-Identifier: Apache-2.0

//! Plugin system Tauri commands.

use crate::plugins::{self, InstalledPlugin, PluginContributions};

/// Lists every installed plugin with its runtime state.
#[tauri::command]
pub async fn list_plugins() -> Result<Vec<InstalledPlugin>, String> {
    Ok(plugins::list_plugins(&plugins::plugins_dir()))
}

/// Installs (or updates) a plugin from a local folder containing a
/// `plugin.json` manifest.
#[tauri::command]
pub async fn install_plugin(source_path: String) -> Result<InstalledPlugin, String> {
    plugins::install_plugin(&plugins::plugins_dir(), &source_path)
}

/// Removes an installed plugin.
#[tauri::command]
pub async fn remove_plugin(plugin_id: String) -> Result<(), String> {
    plugins::remove_plugin(&plugins::plugins_dir(), &plugin_id)
}

/// Enables or disables an installed plugin.
#[tauri::command]
pub async fn set_plugin_enabled(plugin_id: String, enabled: bool) -> Result<(), String> {
    plugins::set_plugin_enabled(&plugins::plugins_dir(), &plugin_id, enabled)
}

/// Returns the aggregated contributions of all enabled, compatible plugins.
#[tauri::command]
pub async fn get_plugin_contributions() -> Result<PluginContributions, String> {
    Ok(plugins::get_contributions(&plugins::plugins_dir()))
}

// SPDX-License-Identifier: Apache-2.0

//! Plugin system Tauri commands.

use std::sync::Arc;

use tauri::State;

use crate::plugins::runtime::PluginHost;
use crate::plugins::{self, InstalledPlugin, PluginContributions};
use crate::SharedState;

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

/// Captures the executable-plugin host so a hot-reload picks up the change
/// without holding the `AppState` lock across the blocking reload itself.
async fn plugin_host(state: &State<'_, SharedState>) -> Arc<PluginHost> {
    Arc::clone(&state.lock().await.plugin_host)
}

/// Lists every installed plugin with its runtime state.
#[tauri::command]
pub async fn list_plugins() -> Result<Vec<InstalledPlugin>, String> {
    blocking(|| plugins::list_plugins(&plugins::plugins_dir())).await
}

/// Installs (or updates) a plugin from a local folder containing a
/// `plugin.json` manifest. Reloads the executable runtime so a newly-installed
/// WASM plugin's hooks take effect immediately.
#[tauri::command]
pub async fn install_plugin(
    source_path: String,
    state: State<'_, SharedState>,
) -> Result<InstalledPlugin, String> {
    let plugin = blocking(move || plugins::install_plugin(&plugins::plugins_dir(), &source_path))
        .await??;
    let host = plugin_host(&state).await;
    blocking(move || host.reload()).await?;
    Ok(plugin)
}

/// Removes an installed plugin. Reloads the executable runtime so its hooks
/// stop firing right away.
#[tauri::command]
pub async fn remove_plugin(
    plugin_id: String,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    blocking(move || plugins::remove_plugin(&plugins::plugins_dir(), &plugin_id)).await??;
    let host = plugin_host(&state).await;
    blocking(move || host.reload()).await?;
    Ok(())
}

/// Enables or disables an installed plugin. Reloads the executable runtime so
/// the change takes effect on the next query.
#[tauri::command]
pub async fn set_plugin_enabled(
    plugin_id: String,
    enabled: bool,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    blocking(move || plugins::set_plugin_enabled(&plugins::plugins_dir(), &plugin_id, enabled))
        .await??;
    let host = plugin_host(&state).await;
    blocking(move || host.reload()).await?;
    Ok(())
}

/// Returns the aggregated contributions of all enabled, compatible plugins.
#[tauri::command]
pub async fn get_plugin_contributions() -> Result<PluginContributions, String> {
    blocking(|| plugins::get_contributions(&plugins::plugins_dir())).await
}

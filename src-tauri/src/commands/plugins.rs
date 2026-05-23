// SPDX-License-Identifier: Apache-2.0

//! Plugin system Tauri commands.

use std::collections::BTreeSet;
use std::sync::Arc;

use tauri::State;

use crate::plugins::runtime::{capabilities, CapabilityKind, PluginHost};
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

/// Removes an installed plugin and forgets any consent it had. Reloads the
/// executable runtime so its hooks stop firing right away.
#[tauri::command]
pub async fn remove_plugin(
    plugin_id: String,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    let id_for_consent = plugin_id.clone();
    blocking(move || plugins::remove_plugin(&plugins::plugins_dir(), &plugin_id)).await??;
    blocking(move || capabilities::forget(&plugins::plugins_dir(), &id_for_consent)).await??;
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

/// Returns the capabilities the user has granted to a plugin. Capabilities
/// the manifest did not request are filtered out so a tampered consent file
/// can never escalate access.
#[tauri::command]
pub async fn get_plugin_consent(plugin_id: String) -> Result<Vec<CapabilityKind>, String> {
    blocking(move || {
        let dir = plugins::plugins_dir();
        let grants = capabilities::read_grants(&dir, &plugin_id);
        let requested: BTreeSet<CapabilityKind> = plugins::list_plugins(&dir)
            .into_iter()
            .find(|p| p.manifest.id == plugin_id)
            .map(|p| capabilities::requested_from_manifest(&p.manifest))
            .unwrap_or_default()
            .into_iter()
            .collect();
        Ok(grants
            .into_iter()
            .filter(|c| requested.contains(c))
            .collect())
    })
    .await?
}

/// Overwrites the capabilities granted to a plugin. The runtime is reloaded
/// so the new consent set takes effect on the next query.
#[tauri::command]
pub async fn set_plugin_consent(
    plugin_id: String,
    grants: Vec<CapabilityKind>,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    let id = plugin_id.clone();
    blocking(move || {
        let dir = plugins::plugins_dir();
        // Filter to capabilities the manifest actually requests — granting
        // more than was asked for would be a no-op anyway, but persisting
        // junk muddies the consent record.
        let requested: BTreeSet<CapabilityKind> = plugins::list_plugins(&dir)
            .into_iter()
            .find(|p| p.manifest.id == id)
            .map(|p| capabilities::requested_from_manifest(&p.manifest))
            .unwrap_or_default()
            .into_iter()
            .collect();
        let filtered: BTreeSet<CapabilityKind> =
            grants.into_iter().filter(|c| requested.contains(c)).collect();
        capabilities::write_grants(&dir, &id, filtered)
    })
    .await??;
    let host = plugin_host(&state).await;
    blocking(move || host.reload()).await?;
    Ok(())
}

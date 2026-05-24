// SPDX-License-Identifier: Apache-2.0

//! Plugin system Tauri commands.

use std::collections::BTreeSet;
use std::sync::Arc;

use tauri::State;

use crate::plugins::runtime::{capabilities, secrets, CapabilityKind, PluginHost};
use crate::plugins::{self, InstalledPlugin, PluginContributions};
use crate::SharedState;

/// Wraps blocking plugin I/O so it never stalls the Tauri event loop.
async fn blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("Plugin task failed: {e}"))
}

/// Snapshots the host Arc so reload runs without holding the `AppState` lock.
async fn plugin_host(state: &State<'_, SharedState>) -> Arc<PluginHost> {
    Arc::clone(&state.lock().await.plugin_host)
}

/// Lists every installed plugin with its runtime state.
#[tauri::command]
pub async fn list_plugins() -> Result<Vec<InstalledPlugin>, String> {
    blocking(|| plugins::list_plugins(&plugins::plugins_dir())).await
}

/// Installs (or updates) a plugin from a local folder and reloads the
/// runtime so its hooks fire on the next query.
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

/// Installs a plugin from a remote archive URL — the marketplace path.
///
/// The flow mirrors the local-folder install but adds two checks up front:
///
/// 1. The downloaded bytes' sha256 must match `expected_sha256`. The expected
///    digest comes from the marketplace registry's `index.json`; the host
///    refuses to extract anything before this check passes, so a hostile mirror
///    can't smuggle a different archive past the manifest validator.
/// 2. The archive must be a flat zip with `plugin.json` at the root and no
///    path traversal. The existing `install_plugin` flow then re-validates the
///    manifest like for any other source folder.
///
/// `expected_sha256` may be passed as either the raw lowercase hex digest or
/// the `sha256-<hex>` form the registry uses — both shapes are accepted.
#[tauri::command]
pub async fn install_plugin_from_url(
    url: String,
    expected_sha256: String,
    state: State<'_, SharedState>,
) -> Result<InstalledPlugin, String> {
    let plugin = blocking(move || {
        crate::plugins::install_from_archive_url(&plugins::plugins_dir(), &url, &expected_sha256)
    })
    .await??;
    let host = plugin_host(&state).await;
    blocking(move || host.reload()).await?;
    Ok(plugin)
}

/// Removes a plugin and forgets its consent + secrets.
#[tauri::command]
pub async fn remove_plugin(
    plugin_id: String,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    let dir = plugins::plugins_dir();
    // Snapshot the secret names before the manifest folder is wiped — the
    // keyring cleanup below needs them.
    let id_for_secrets = plugin_id.clone();
    let secret_names: Vec<String> = blocking(move || {
        plugins::list_plugins(&dir)
            .into_iter()
            .find(|p| p.manifest.id == id_for_secrets)
            .and_then(|p| p.manifest.runtime.map(|r| r.capabilities.secrets))
            .unwrap_or_default()
    })
    .await?;

    let id_for_consent = plugin_id.clone();
    let id_for_secret_cleanup = plugin_id.clone();
    blocking(move || plugins::remove_plugin(&plugins::plugins_dir(), &plugin_id)).await??;
    blocking(move || capabilities::forget(&plugins::plugins_dir(), &id_for_consent)).await??;
    blocking(move || secrets::forget_all(&id_for_secret_cleanup, &secret_names)).await?;
    let host = plugin_host(&state).await;
    blocking(move || host.reload()).await?;
    Ok(())
}

/// Enables or disables a plugin; the next query sees the change.
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

/// Aggregated contributions of every enabled, compatible plugin. Served
/// from the host's in-memory cache; safe to poll.
#[tauri::command]
pub async fn get_plugin_contributions(
    state: State<'_, SharedState>,
) -> Result<PluginContributions, String> {
    let host = plugin_host(&state).await;
    Ok((*host.contributions()).clone())
}

/// Capabilities the user has granted to a plugin, intersected with what the
/// manifest actually requests so a tampered consent file can't escalate.
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

/// Names of secrets that have a value in the keyring. Values stay backend-side.
#[tauri::command]
pub async fn list_provisioned_secrets(plugin_id: String) -> Result<Vec<String>, String> {
    blocking(move || {
        let dir = plugins::plugins_dir();
        let names: Vec<String> = plugins::list_plugins(&dir)
            .into_iter()
            .find(|p| p.manifest.id == plugin_id)
            .and_then(|p| p.manifest.runtime.map(|r| r.capabilities.secrets))
            .unwrap_or_default();
        names
            .into_iter()
            .filter(|n| secrets::read(&plugin_id, n).is_some())
            .collect()
    })
    .await
}

/// Stores a secret in the keyring. The name must be declared in the manifest.
#[tauri::command]
pub async fn set_plugin_secret(
    plugin_id: String,
    name: String,
    value: String,
) -> Result<(), String> {
    blocking(move || {
        let dir = plugins::plugins_dir();
        let names: Vec<String> = plugins::list_plugins(&dir)
            .into_iter()
            .find(|p| p.manifest.id == plugin_id)
            .and_then(|p| p.manifest.runtime.map(|r| r.capabilities.secrets))
            .unwrap_or_default();
        if !names.iter().any(|n| n == &name) {
            return Err(format!(
                "Plugin '{plugin_id}' did not declare a secret named '{name}'"
            ));
        }
        secrets::write(&plugin_id, &name, &value)
    })
    .await?
}

/// Deletes a single provisioned secret for a plugin.
#[tauri::command]
pub async fn delete_plugin_secret(plugin_id: String, name: String) -> Result<(), String> {
    blocking(move || secrets::delete(&plugin_id, &name)).await?
}

/// Invokes a contributed command. `plugin_id` and `command_id` come from
/// `get_plugin_contributions` (e.g. `acme.linter` + `lint-current`).
#[tauri::command]
pub async fn run_plugin_command(
    plugin_id: String,
    command_id: String,
    args: Option<serde_json::Value>,
    state: State<'_, SharedState>,
) -> Result<serde_json::Value, String> {
    let host = plugin_host(&state).await;
    let args = args.unwrap_or(serde_json::Value::Null);
    host.run_command(&plugin_id, &command_id, args).await
}

/// Overwrites the granted capabilities; the next query sees the change.
#[tauri::command]
pub async fn set_plugin_consent(
    plugin_id: String,
    grants: Vec<CapabilityKind>,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    let id = plugin_id.clone();
    blocking(move || {
        let dir = plugins::plugins_dir();
        // Filter against the manifest so the persisted consent record only
        // ever contains capabilities the plugin actually asked for.
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

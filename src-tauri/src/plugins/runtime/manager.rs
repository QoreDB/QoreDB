// SPDX-License-Identifier: Apache-2.0

//! Plugin host: loads executable plugins and dispatches hooks to them.
//!
//! Owns the WASM runtime, the set of loaded plugin instances, and the
//! per-plugin host services (storage handle, notify sender, consent
//! snapshot). A hook runs against every enabled executable plugin; a plugin
//! that errors is logged and skipped — it can never block a query by failing.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use super::{
    capabilities, storage, Budget, CapabilityKind, Decision, HookContext, InvocationServices,
    NotifyEvent, NotifyLevel, NotifySender, PluginInstance, PluginRuntime, PluginStorage,
    PostExecuteResult, QueryReadPayload, WasmiRuntime,
};
use crate::plugins::{plugins_dir, registry};

/// Loads executable plugins and runs their hooks.
pub struct PluginHost {
    runtime: Arc<dyn PluginRuntime>,
    /// Sender end of the notification channel — the app drains it and emits
    /// Tauri events. Cloned into each plugin instance's services.
    notify: Mutex<Option<NotifySender>>,
    /// Loaded instances, keyed by plugin id.
    instances: Mutex<HashMap<String, Box<dyn PluginInstance>>>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(WasmiRuntime::new()),
            notify: Mutex::new(None),
            instances: Mutex::new(HashMap::new()),
        }
    }

    /// Installs the notification sender the runtime hands toast events to.
    /// Called once at app startup, before the first reload that should be
    /// able to surface notifications.
    pub fn set_notify_sender(&self, sender: NotifySender) {
        *self.notify.lock().unwrap() = Some(sender);
    }

    /// Rescans the plugins directory and (re)loads every enabled, compatible
    /// executable plugin. Called at startup and whenever a plugin is added,
    /// removed, enabled, disabled, or its consent changes.
    pub fn reload(&self) {
        let dir = plugins_dir();
        let notify = self.notify.lock().unwrap().clone();
        let mut instances = self.instances.lock().unwrap();
        instances.clear();

        for plugin in registry::list_plugins(&dir) {
            if !plugin.enabled || !plugin.compatible {
                continue;
            }
            let Some(runtime_spec) = &plugin.manifest.runtime else {
                continue;
            };
            let plugin_dir = dir.join(&plugin.dir_name);
            let wasm_path = plugin_dir.join(&runtime_spec.entry);
            let wasm = match std::fs::read(&wasm_path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin.manifest.id,
                        error = %e,
                        "could not read plugin WASM module"
                    );
                    continue;
                }
            };

            // Build the host services for this plugin: snapshot consent,
            // hand it the storage file under its own folder, share the
            // notify sender (if any).
            let consent = capabilities::read_grants(&dir, &plugin.manifest.id);
            // A plugin can never see a capability it did not request, even
            // if the consent file got tampered with.
            let requested: BTreeSet<CapabilityKind> = capabilities::requested(
                &runtime_spec.capabilities,
            )
            .into_iter()
            .collect();
            let effective: BTreeSet<CapabilityKind> =
                consent.intersection(&requested).copied().collect();

            let storage_path = storage::storage_path(&dir, &plugin.dir_name);
            // Phase 3 capability inputs — pulled from the manifest so host
            // fns get a re-validated copy that consent tampering can't widen.
            let http_allowed_hosts: Vec<String> = runtime_spec
                .capabilities
                .http
                .as_ref()
                .map(|h| h.allowed_hosts.clone())
                .unwrap_or_default();
            let fs_root = runtime_spec
                .capabilities
                .fs
                .as_ref()
                .map(|_| dir.join(&plugin.dir_name).join("data"));
            let secret_names: Vec<String> = runtime_spec.capabilities.secrets.clone();
            let services = InvocationServices {
                plugin_id: plugin.manifest.id.clone(),
                consent: Arc::new(effective),
                storage: Arc::new(PluginStorage::new(storage_path)),
                notify: notify.clone(),
                query_result: None,
                http_allowed_hosts: Arc::new(http_allowed_hosts),
                fs_root,
                secret_names: Arc::new(secret_names),
            };

            match self.runtime.load(
                plugin.manifest.id.clone(),
                &wasm,
                Budget::default(),
                services,
            ) {
                Ok(instance) => {
                    instances.insert(plugin.manifest.id.clone(), instance);
                }
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin.manifest.id,
                        error = %e,
                        "could not load plugin"
                    );
                }
            }
        }
    }

    /// Runs the `pre_execute` hook of every loaded plugin and aggregates the
    /// verdicts: any `Block` wins; otherwise the first `Warn`; else `Allow`.
    /// A `Warn` is also surfaced to the user as a toast through the same
    /// `plugin-notify` channel the `notify` capability uses.
    pub fn run_pre_execute(&self, context: &HookContext) -> Decision {
        let mut instances = self.instances.lock().unwrap();
        let mut warning: Option<(String, String)> = None;
        for (id, instance) in instances.iter_mut() {
            match instance.pre_execute(context) {
                Ok(Decision::Allow) => {}
                Ok(Decision::Warn { message }) => {
                    warning.get_or_insert_with(|| (id.clone(), message));
                }
                Ok(Decision::Block { reason }) => return Decision::Block { reason },
                Err(e) => {
                    tracing::warn!(plugin = %id, error = %e, "plugin pre_execute hook failed");
                }
            }
        }
        drop(instances);
        match warning {
            Some((plugin_id, message)) => {
                self.emit_notify(NotifyEvent {
                    plugin_id,
                    level: NotifyLevel::Warning,
                    message: message.clone(),
                });
                Decision::Warn { message }
            }
            None => Decision::Allow,
        }
    }

    /// Sends a notification through the `plugin-notify` channel. Silent no-op
    /// when the bridge is not wired (early startup, headless tests).
    fn emit_notify(&self, event: NotifyEvent) {
        let sender = self.notify.lock().unwrap().clone();
        if let Some(sender) = sender {
            let _ = sender.send(event);
        }
    }

    /// Runs the `post_execute` hook of every loaded plugin. `query_payload`
    /// carries row data; it's only handed to plugins that have been granted
    /// `queryRead`. Failures are logged; the host never propagates them.
    pub fn run_post_execute(
        &self,
        context: &HookContext,
        result: &PostExecuteResult,
        query_payload: Option<Arc<QueryReadPayload>>,
    ) {
        let mut instances = self.instances.lock().unwrap();
        for (id, instance) in instances.iter_mut() {
            if let Err(e) = instance.post_execute(context, result, query_payload.clone()) {
                tracing::warn!(plugin = %id, error = %e, "plugin post_execute hook failed");
            }
        }
    }

    /// Invokes a contributed command on the matching plugin. Returns the
    /// JSON value the plugin produced. Errors are surfaced to the caller —
    /// commands are explicit user actions, so swallowing a failure would
    /// leave the user wondering whether anything happened.
    pub fn run_command(
        &self,
        plugin_id: &str,
        command_id: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let mut instances = self.instances.lock().unwrap();
        let instance = instances
            .get_mut(plugin_id)
            .ok_or_else(|| format!("Plugin '{plugin_id}' is not loaded"))?;
        instance.command(command_id, args).map_err(|e| e.to_string())
    }

    /// Number of currently loaded executable plugins.
    pub fn loaded_count(&self) -> usize {
        self.instances.lock().unwrap().len()
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

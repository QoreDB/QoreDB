// SPDX-License-Identifier: Apache-2.0

//! Plugin host: loads executable plugins and dispatches hooks to them.
//!
//! Owns the WASM runtime, the set of loaded plugin instances, and the
//! per-plugin host services (storage handle, notify sender, consent
//! snapshot). A hook runs against every enabled executable plugin; a plugin
//! that errors is logged and skipped — it can never block a query by failing.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

use super::{
    capabilities, storage, Budget, CapabilityKind, Decision, HookContext, InvocationServices,
    NotifyEvent, NotifyLevel, NotifySender, PluginInstance, PluginRuntime, PluginStorage,
    PostExecuteResult, QueryReadPayload, WasmiRuntime,
};
use crate::plugins::{plugins_dir, registry};

/// Locks a `Mutex` and recovers from poisoning so a single panicked hook can
/// never lock the whole `PluginHost` out. Poisoning is logged so it isn't
/// silently swept under the rug.
fn lock_recover<'a, T>(mutex: &'a Mutex<T>, what: &'static str) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::error!(target: "plugins", lock = what, "plugin host mutex was poisoned; recovering");
            poisoned.into_inner()
        }
    }
}

/// Consecutive hook failures after which a misbehaving plugin is unloaded
/// for the rest of the session. The threshold is small on purpose: a plugin
/// that traps once is unlucky, three times in a row is broken.
const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;

/// Loads executable plugins and runs their hooks.
pub struct PluginHost {
    runtime: Arc<dyn PluginRuntime>,
    /// Sender end of the notification channel — the app drains it and emits
    /// Tauri events. Cloned into each plugin instance's services.
    notify: Mutex<Option<NotifySender>>,
    /// Loaded instances, keyed by plugin id.
    instances: Mutex<HashMap<String, Box<dyn PluginInstance>>>,
    /// Consecutive hook-failure count per plugin id. A plugin that crosses
    /// [`CIRCUIT_BREAKER_THRESHOLD`] is unloaded; a success resets the count.
    failures: Mutex<HashMap<String, u32>>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(WasmiRuntime::new()),
            notify: Mutex::new(None),
            instances: Mutex::new(HashMap::new()),
            failures: Mutex::new(HashMap::new()),
        }
    }

    /// Installs the notification sender the runtime hands toast events to.
    /// Called once at app startup, before the first reload that should be
    /// able to surface notifications.
    pub fn set_notify_sender(&self, sender: NotifySender) {
        *lock_recover(&self.notify, "notify") = Some(sender);
    }

    /// Rescans the plugins directory and (re)loads every enabled, compatible
    /// executable plugin. Called at startup and whenever a plugin is added,
    /// removed, enabled, disabled, or its consent changes.
    pub fn reload(&self) {
        let dir = plugins_dir();
        let notify = lock_recover(&self.notify, "notify").clone();
        let mut instances = lock_recover(&self.instances, "instances");
        instances.clear();
        lock_recover(&self.failures, "failures").clear();

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
        let mut instances = lock_recover(&self.instances, "instances");
        let mut warning: Option<(String, String)> = None;
        let mut tripped: Vec<String> = Vec::new();
        for (id, instance) in instances.iter_mut() {
            match instance.pre_execute(context) {
                Ok(Decision::Allow) => {
                    self.record_success(id);
                }
                Ok(Decision::Warn { message }) => {
                    self.record_success(id);
                    warning.get_or_insert_with(|| (id.clone(), message));
                }
                Ok(Decision::Block { reason }) => {
                    self.record_success(id);
                    return Decision::Block { reason };
                }
                Err(e) => {
                    tracing::warn!(plugin = %id, error = %e, "plugin pre_execute hook failed");
                    if self.record_failure(id) {
                        tripped.push(id.clone());
                    }
                }
            }
        }
        for id in &tripped {
            instances.remove(id);
        }
        drop(instances);
        for id in tripped {
            self.notify_disabled(&id);
        }
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
        let sender = lock_recover(&self.notify, "notify").clone();
        if let Some(sender) = sender {
            let _ = sender.send(event);
        }
    }

    /// Increments the consecutive-failure counter for `plugin_id` and returns
    /// `true` if the circuit breaker just tripped (caller must unload the
    /// plugin and notify the user).
    fn record_failure(&self, plugin_id: &str) -> bool {
        let mut failures = lock_recover(&self.failures, "failures");
        let count = failures.entry(plugin_id.to_string()).or_insert(0);
        *count += 1;
        *count >= CIRCUIT_BREAKER_THRESHOLD
    }

    /// Resets the consecutive-failure counter — a successful hook means the
    /// plugin is well-behaved again.
    fn record_success(&self, plugin_id: &str) {
        let mut failures = lock_recover(&self.failures, "failures");
        failures.remove(plugin_id);
    }

    /// Surfaces the circuit-breaker trip to the user. Goes out as a Warning
    /// notification so the toast pipeline picks it up like a plugin's own
    /// `notify`.
    fn notify_disabled(&self, plugin_id: &str) {
        tracing::warn!(
            plugin = plugin_id,
            threshold = CIRCUIT_BREAKER_THRESHOLD,
            "plugin unloaded after repeated hook failures"
        );
        self.emit_notify(NotifyEvent {
            plugin_id: plugin_id.to_string(),
            level: NotifyLevel::Warning,
            message: format!(
                "Plugin disabled after {CIRCUIT_BREAKER_THRESHOLD} consecutive errors"
            ),
        });
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
        let mut instances = lock_recover(&self.instances, "instances");
        let mut tripped: Vec<String> = Vec::new();
        for (id, instance) in instances.iter_mut() {
            match instance.post_execute(context, result, query_payload.clone()) {
                Ok(()) => self.record_success(id),
                Err(e) => {
                    tracing::warn!(plugin = %id, error = %e, "plugin post_execute hook failed");
                    if self.record_failure(id) {
                        tripped.push(id.clone());
                    }
                }
            }
        }
        for id in &tripped {
            instances.remove(id);
        }
        drop(instances);
        for id in tripped {
            self.notify_disabled(&id);
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
        let mut instances = lock_recover(&self.instances, "instances");
        let instance = instances
            .get_mut(plugin_id)
            .ok_or_else(|| format!("Plugin '{plugin_id}' is not loaded"))?;
        instance.command(command_id, args).map_err(|e| e.to_string())
    }

    /// Number of currently loaded executable plugins.
    pub fn loaded_count(&self) -> usize {
        lock_recover(&self.instances, "instances").len()
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::runtime::PluginError;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc as StdArc;
    use tokio::sync::mpsc;

    /// Stub plugin that returns the queued decisions / results in order. The
    /// shared counter lets a test observe how many times each hook fired.
    struct StubPlugin {
        pre: Vec<Result<Decision, PluginError>>,
        post: Vec<Result<(), PluginError>>,
        pre_calls: StdArc<AtomicU32>,
        post_calls: StdArc<AtomicU32>,
    }

    impl PluginInstance for StubPlugin {
        fn pre_execute(&mut self, _context: &HookContext) -> Result<Decision, PluginError> {
            self.pre_calls.fetch_add(1, Ordering::SeqCst);
            if self.pre.is_empty() {
                Ok(Decision::Allow)
            } else {
                self.pre.remove(0)
            }
        }

        fn post_execute(
            &mut self,
            _context: &HookContext,
            _result: &PostExecuteResult,
            _payload: Option<Arc<QueryReadPayload>>,
        ) -> Result<(), PluginError> {
            self.post_calls.fetch_add(1, Ordering::SeqCst);
            if self.post.is_empty() {
                Ok(())
            } else {
                self.post.remove(0)
            }
        }

        fn command(
            &mut self,
            _command_id: &str,
            _args: &serde_json::Value,
        ) -> Result<serde_json::Value, PluginError> {
            Ok(serde_json::Value::Null)
        }
    }

    fn ctx() -> HookContext {
        HookContext {
            query: String::new(),
            driver_id: "postgres".into(),
            environment: "test".into(),
            operation_type: "select".into(),
            is_mutation: false,
            is_dangerous: false,
            read_only: true,
        }
    }

    fn post_ok() -> PostExecuteResult {
        PostExecuteResult {
            success: true,
            execution_time_ms: 1,
            row_count: None,
            error: None,
        }
    }

    fn insert(host: &PluginHost, id: &str, stub: StubPlugin) {
        let mut instances = host.instances.lock().unwrap();
        instances.insert(id.to_string(), Box::new(stub));
    }

    #[test]
    fn circuit_breaker_unloads_plugin_after_repeated_pre_execute_failures() {
        let host = PluginHost::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        host.set_notify_sender(tx);

        let calls = StdArc::new(AtomicU32::new(0));
        let stub = StubPlugin {
            pre: (0..CIRCUIT_BREAKER_THRESHOLD as usize)
                .map(|_| Err(PluginError::Trap("boom".into())))
                .collect(),
            post: vec![],
            pre_calls: calls.clone(),
            post_calls: StdArc::new(AtomicU32::new(0)),
        };
        insert(&host, "acme.bad", stub);

        for _ in 0..CIRCUIT_BREAKER_THRESHOLD {
            assert_eq!(host.run_pre_execute(&ctx()), Decision::Allow);
        }
        assert_eq!(host.loaded_count(), 0, "plugin should be unloaded");
        assert_eq!(calls.load(Ordering::SeqCst), CIRCUIT_BREAKER_THRESHOLD);

        // The trip emitted a Warning toast.
        let event = rx.try_recv().expect("notify event was sent");
        assert_eq!(event.plugin_id, "acme.bad");
        assert_eq!(event.level, NotifyLevel::Warning);
    }

    #[test]
    fn successful_hook_resets_the_failure_counter() {
        let host = PluginHost::new();
        let stub = StubPlugin {
            pre: vec![
                Err(PluginError::Trap("once".into())),
                Err(PluginError::Trap("twice".into())),
                Ok(Decision::Allow),
                Err(PluginError::Trap("again".into())),
                Err(PluginError::Trap("more".into())),
            ],
            post: vec![],
            pre_calls: StdArc::new(AtomicU32::new(0)),
            post_calls: StdArc::new(AtomicU32::new(0)),
        };
        insert(&host, "acme.flaky", stub);

        // Two failures + a success — the success resets the counter.
        host.run_pre_execute(&ctx());
        host.run_pre_execute(&ctx());
        host.run_pre_execute(&ctx());
        assert_eq!(host.loaded_count(), 1);

        // Two more failures alone are still below the threshold.
        host.run_pre_execute(&ctx());
        host.run_pre_execute(&ctx());
        assert_eq!(host.loaded_count(), 1);
    }

    #[test]
    fn circuit_breaker_unloads_plugin_after_repeated_post_execute_failures() {
        let host = PluginHost::new();
        let stub = StubPlugin {
            pre: vec![],
            post: (0..CIRCUIT_BREAKER_THRESHOLD as usize)
                .map(|_| Err(PluginError::Trap("boom".into())))
                .collect(),
            pre_calls: StdArc::new(AtomicU32::new(0)),
            post_calls: StdArc::new(AtomicU32::new(0)),
        };
        insert(&host, "acme.bad", stub);

        for _ in 0..CIRCUIT_BREAKER_THRESHOLD {
            host.run_post_execute(&ctx(), &post_ok(), None);
        }
        assert_eq!(host.loaded_count(), 0);
    }
}

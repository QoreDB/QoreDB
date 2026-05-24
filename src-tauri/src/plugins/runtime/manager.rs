// SPDX-License-Identifier: Apache-2.0

//! Plugin host: loads executable plugins and dispatches hooks to them.
//!
//! Owns the WASM runtime, the set of loaded plugin instances, and the
//! per-plugin host services (storage handle, notify sender, consent
//! snapshot). A hook runs against every enabled executable plugin; a plugin
//! that errors is logged and skipped — it can never block a query by failing.
//!
//! ## Concurrency model
//!
//! * Each plugin's `PluginInstance` lives behind its *own* `Mutex`, wrapped
//!   in an `Arc`. The outer `instances` map is only locked long enough to
//!   snapshot the `(id, Arc<Mutex<…>>)` pairs; once dropped, hooks for
//!   *different* plugins can run concurrently from different queries.
//! * Every hook invocation runs through `spawn_blocking` because `wasmi` is
//!   synchronous — this keeps the async runtime free for unrelated work.
//! * `tokio::time::timeout` wraps each hook so a plugin that wedges in a
//!   blocking host fn cannot stall the query path forever.
//! * `post_execute` can also be *scheduled* via [`PluginHost::schedule_post_execute`]:
//!   the call returns immediately, the actual hooks run on a background task
//!   under a bounded semaphore so a slow plugin never adds latency to the
//!   query response.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::Semaphore;

use super::{
    capabilities, storage, Budget, CapabilityKind, Decision, HookContext, InvocationServices,
    NotifyEvent, NotifyLevel, NotifySender, PluginInstance, PluginRuntime, PluginStorage,
    PostExecuteResult, QueryReadPayload, WasmiRuntime,
};
use crate::plugins::{plugins_dir, registry, PluginContributions};

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

/// Wall-clock budget for a `pre_execute` hook. Short on purpose: the hook is
/// on the query critical path, and a typical linter completes in a fraction
/// of a millisecond. Anything longer than half a second is almost certainly
/// a wedged host fn — we'd rather treat the verdict as Allow than stall the
/// user's query.
const PRE_EXECUTE_TIMEOUT: Duration = Duration::from_millis(500);
/// Wall-clock budget for a `post_execute` hook. More generous because it
/// runs off the critical path (via [`PluginHost::schedule_post_execute`])
/// and may legitimately do bookkeeping like POSTing to an audit endpoint.
const POST_EXECUTE_TIMEOUT: Duration = Duration::from_secs(5);
/// Maximum number of `post_execute` invocations that may be in flight at
/// once. Once full, new schedules drop with a log line so a runaway plugin
/// can't grow an unbounded task queue.
const POST_EXECUTE_QUEUE_DEPTH: usize = 64;

type SharedInstance = Arc<Mutex<Box<dyn PluginInstance>>>;

/// Loads executable plugins and runs their hooks.
pub struct PluginHost {
    runtime: Arc<dyn PluginRuntime>,
    /// Sender end of the notification channel — the app drains it and emits
    /// Tauri events. Cloned into each plugin instance's services.
    notify: Mutex<Option<NotifySender>>,
    /// Loaded instances, each behind its own per-plugin mutex so distinct
    /// plugins can run their hooks concurrently.
    instances: Mutex<HashMap<String, SharedInstance>>,
    /// Consecutive hook-failure count per plugin id. A plugin that crosses
    /// [`CIRCUIT_BREAKER_THRESHOLD`] is unloaded; a success resets the count.
    failures: Mutex<HashMap<String, u32>>,
    /// Caps the number of background `post_execute` dispatches in flight.
    post_queue: Arc<Semaphore>,
    /// Memoised aggregated contributions of every enabled, compatible
    /// plugin. `None` means "rescan on next read"; `reload()` clears it.
    contributions_cache: Mutex<Option<Arc<PluginContributions>>>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(WasmiRuntime::new()),
            notify: Mutex::new(None),
            instances: Mutex::new(HashMap::new()),
            failures: Mutex::new(HashMap::new()),
            post_queue: Arc::new(Semaphore::new(POST_EXECUTE_QUEUE_DEPTH)),
            contributions_cache: Mutex::new(None),
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
        // Drop the cached contribution snapshot — install / enable / disable
        // / consent change all go through reload(), so this is the single
        // invalidation point we need.
        *lock_recover(&self.contributions_cache, "contributions_cache") = None;

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

            // Integrity gate: if the manifest pinned a sha256 digest, the
            // bytes we just read must match it. A mismatch means the .wasm
            // was swapped or tampered with after publication — refuse to
            // load it (a malicious plugin could still ship without an
            // integrity field, but then the UI marks it "Unsigned").
            if let Some(expected) = runtime_spec.integrity.as_deref() {
                if let Err(e) = verify_integrity(&wasm, expected) {
                    tracing::warn!(
                        plugin = %plugin.manifest.id,
                        error = %e,
                        "plugin integrity check failed; refusing to load"
                    );
                    continue;
                }
            }

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
            let http_allow_private_networks = runtime_spec
                .capabilities
                .http
                .as_ref()
                .map(|h| h.allow_private_networks)
                .unwrap_or(false);
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
                http_allow_private_networks,
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
                    instances.insert(
                        plugin.manifest.id.clone(),
                        Arc::new(Mutex::new(instance)),
                    );
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
    ///
    /// Each plugin's hook runs in `spawn_blocking` with a timeout, so a
    /// wedged host fn cannot stall the caller forever — the verdict simply
    /// falls back to "no opinion from this plugin" and the rest carry on.
    pub async fn run_pre_execute(&self, context: HookContext) -> Decision {
        let snapshot = self.snapshot_instances();
        let mut warning: Option<(String, String)> = None;
        let mut tripped: Vec<String> = Vec::new();

        for (id, instance) in snapshot {
            let outcome = run_with_timeout(instance, PRE_EXECUTE_TIMEOUT, {
                let context = context.clone();
                move |guard| guard.pre_execute(&context)
            })
            .await;

            match outcome {
                HookOutcome::Ok(Decision::Allow) => self.record_success(&id),
                HookOutcome::Ok(Decision::Warn { message }) => {
                    self.record_success(&id);
                    warning.get_or_insert_with(|| (id.clone(), message));
                }
                HookOutcome::Ok(Decision::Block { reason }) => {
                    self.record_success(&id);
                    self.unload_tripped(tripped);
                    return Decision::Block { reason };
                }
                HookOutcome::Failed(reason) => {
                    tracing::warn!(plugin = %id, reason = %reason, "plugin pre_execute hook failed");
                    if self.record_failure(&id) {
                        tripped.push(id.clone());
                    }
                }
            }
        }

        self.unload_tripped(tripped);
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

    /// Runs the `post_execute` hook of every loaded plugin. `query_payload`
    /// carries row data; it's only handed to plugins that have been granted
    /// `queryRead`. Failures are logged; the host never propagates them.
    pub async fn run_post_execute(
        &self,
        context: HookContext,
        result: PostExecuteResult,
        query_payload: Option<Arc<QueryReadPayload>>,
    ) {
        let snapshot = self.snapshot_instances();
        let mut tripped: Vec<String> = Vec::new();

        for (id, instance) in snapshot {
            let outcome = run_with_timeout(instance, POST_EXECUTE_TIMEOUT, {
                let context = context.clone();
                let result = result.clone();
                let payload = query_payload.clone();
                move |guard| guard.post_execute(&context, &result, payload)
            })
            .await;

            match outcome {
                HookOutcome::Ok(()) => self.record_success(&id),
                HookOutcome::Failed(reason) => {
                    tracing::warn!(plugin = %id, reason = %reason, "plugin post_execute hook failed");
                    if self.record_failure(&id) {
                        tripped.push(id.clone());
                    }
                }
            }
        }

        self.unload_tripped(tripped);
    }

    /// Fires-and-forgets a `post_execute` dispatch on a background task. The
    /// query path can return its response immediately; the plugin hooks run
    /// off the critical path. A bounded semaphore caps the queue depth so a
    /// slow plugin can't accumulate work indefinitely.
    pub fn schedule_post_execute(
        self: &Arc<Self>,
        context: HookContext,
        result: PostExecuteResult,
        payload: Option<Arc<QueryReadPayload>>,
    ) {
        let permit = match Arc::clone(&self.post_queue).try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!(
                    target: "plugins",
                    "post_execute queue full ({POST_EXECUTE_QUEUE_DEPTH} dispatches in flight); dropping plugin hook batch"
                );
                return;
            }
        };
        let host = Arc::clone(self);
        tokio::spawn(async move {
            let _permit = permit;
            host.run_post_execute(context, result, payload).await;
        });
    }

    /// Invokes a contributed command on the matching plugin. Returns the
    /// JSON value the plugin produced. Errors are surfaced to the caller —
    /// commands are explicit user actions, so swallowing a failure would
    /// leave the user wondering whether anything happened.
    pub async fn run_command(
        &self,
        plugin_id: &str,
        command_id: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let instance = {
            let instances = lock_recover(&self.instances, "instances");
            instances.get(plugin_id).cloned()
        };
        let instance =
            instance.ok_or_else(|| format!("Plugin '{plugin_id}' is not loaded"))?;
        let command_id = command_id.to_string();
        let task = tokio::task::spawn_blocking(move || {
            let mut guard = match instance.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            guard.command(&command_id, &args)
        });
        match task.await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => Err(e.to_string()),
            Err(join_err) => Err(format!("plugin command task failed: {join_err}")),
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

    /// Removes the given plugins from the loaded set and emits the
    /// circuit-breaker-tripped notification for each. Cheap when `tripped`
    /// is empty (no lock taken).
    fn unload_tripped(&self, tripped: Vec<String>) {
        if tripped.is_empty() {
            return;
        }
        {
            let mut instances = lock_recover(&self.instances, "instances");
            for id in &tripped {
                instances.remove(id);
            }
        }
        for id in tripped {
            self.notify_disabled(&id);
        }
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

    /// Atomically copies the (id, instance) pairs out of the outer mutex so
    /// the rest of a hook batch can run without holding it. Hooks then
    /// contend only on each plugin's own inner mutex — different plugins
    /// proceed in parallel.
    fn snapshot_instances(&self) -> Vec<(String, SharedInstance)> {
        lock_recover(&self.instances, "instances")
            .iter()
            .map(|(k, v)| (k.clone(), Arc::clone(v)))
            .collect()
    }

    /// Number of currently loaded executable plugins.
    pub fn loaded_count(&self) -> usize {
        lock_recover(&self.instances, "instances").len()
    }

    /// Returns the aggregated contributions of every enabled, compatible
    /// plugin. The result is memoised; the disk scan only runs after the
    /// next `reload()`. Callers receive a cheap `Arc` clone, so frequent
    /// frontend polls cost almost nothing.
    pub fn contributions(&self) -> Arc<PluginContributions> {
        {
            let cache = lock_recover(&self.contributions_cache, "contributions_cache");
            if let Some(existing) = cache.as_ref() {
                return Arc::clone(existing);
            }
        }
        // Cache miss: compute outside the lock so a slow disk doesn't pin
        // the mutex, then store.
        let fresh = Arc::new(registry::get_contributions(&plugins_dir()));
        let mut cache = lock_recover(&self.contributions_cache, "contributions_cache");
        // Another thread may have populated the cache while we computed; keep
        // whichever value landed first — both reflect the same on-disk state.
        let result = cache.get_or_insert_with(|| Arc::clone(&fresh));
        Arc::clone(result)
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a single per-plugin hook attempt. `Ok(T)` carries the hook's
/// own return; `Failed(reason)` covers timeout, join error, and the
/// plugin's own `PluginError` — the caller treats all three the same way.
enum HookOutcome<T> {
    Ok(T),
    Failed(String),
}

/// Compares a WASM module's actual SHA-256 against the manifest-declared
/// `sha256-<hex>` digest. The format has already been validated at manifest
/// parse time, so this only deals with the cryptographic comparison.
fn verify_integrity(wasm: &[u8], expected: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let Some(expected_hex) = expected.strip_prefix("sha256-") else {
        return Err(format!("malformed integrity '{expected}'"));
    };
    let mut hasher = Sha256::new();
    hasher.update(wasm);
    let actual = hasher.finalize();
    let actual_hex = hex_encode(&actual);
    // Constant-time compare isn't required (digest is public, the only thing
    // the comparison protects is determinism); plain `==` is fine.
    if actual_hex.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(format!(
            "integrity mismatch: expected sha256-{expected_hex}, got sha256-{actual_hex}"
        ))
    }
}

/// Lowercase-hex encoding for a 32-byte SHA-256 digest. Avoids pulling in a
/// `hex` crate for a single allocation site.
fn hex_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(TABLE[(b >> 4) as usize] as char);
        out.push(TABLE[(b & 0x0f) as usize] as char);
    }
    out
}

/// Runs `f` on `instance` through `spawn_blocking`, bounded by `duration`.
/// Folds timeouts, panics and `PluginError`s into a single `HookOutcome` so
/// the caller has a flat shape to match on.
async fn run_with_timeout<T, F>(
    instance: SharedInstance,
    duration: Duration,
    f: F,
) -> HookOutcome<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Box<dyn PluginInstance>) -> Result<T, super::PluginError> + Send + 'static,
{
    let task = tokio::task::spawn_blocking(move || {
        let mut guard = match instance.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        f(&mut guard)
    });
    match tokio::time::timeout(duration, task).await {
        Ok(Ok(Ok(value))) => HookOutcome::Ok(value),
        Ok(Ok(Err(err))) => HookOutcome::Failed(err.to_string()),
        Ok(Err(join_err)) => HookOutcome::Failed(format!("task join failed: {join_err}")),
        Err(_) => HookOutcome::Failed(format!("hook timed out after {duration:?}")),
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
        sleep_pre: Option<Duration>,
    }

    impl PluginInstance for StubPlugin {
        fn pre_execute(&mut self, _context: &HookContext) -> Result<Decision, PluginError> {
            self.pre_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(d) = self.sleep_pre {
                std::thread::sleep(d);
            }
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
        let mut instances = lock_recover(&host.instances, "instances");
        instances.insert(
            id.to_string(),
            Arc::new(Mutex::new(Box::new(stub) as Box<dyn PluginInstance>)),
        );
    }

    #[tokio::test]
    async fn circuit_breaker_unloads_plugin_after_repeated_pre_execute_failures() {
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
            sleep_pre: None,
        };
        insert(&host, "acme.bad", stub);

        for _ in 0..CIRCUIT_BREAKER_THRESHOLD {
            assert_eq!(host.run_pre_execute(ctx()).await, Decision::Allow);
        }
        assert_eq!(host.loaded_count(), 0, "plugin should be unloaded");
        assert_eq!(calls.load(Ordering::SeqCst), CIRCUIT_BREAKER_THRESHOLD);

        // The trip emitted a Warning toast.
        let event = rx.try_recv().expect("notify event was sent");
        assert_eq!(event.plugin_id, "acme.bad");
        assert_eq!(event.level, NotifyLevel::Warning);
    }

    #[tokio::test]
    async fn successful_hook_resets_the_failure_counter() {
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
            sleep_pre: None,
        };
        insert(&host, "acme.flaky", stub);

        // Two failures + a success — the success resets the counter.
        host.run_pre_execute(ctx()).await;
        host.run_pre_execute(ctx()).await;
        host.run_pre_execute(ctx()).await;
        assert_eq!(host.loaded_count(), 1);

        // Two more failures alone are still below the threshold.
        host.run_pre_execute(ctx()).await;
        host.run_pre_execute(ctx()).await;
        assert_eq!(host.loaded_count(), 1);
    }

    #[tokio::test]
    async fn circuit_breaker_unloads_plugin_after_repeated_post_execute_failures() {
        let host = PluginHost::new();
        let stub = StubPlugin {
            pre: vec![],
            post: (0..CIRCUIT_BREAKER_THRESHOLD as usize)
                .map(|_| Err(PluginError::Trap("boom".into())))
                .collect(),
            pre_calls: StdArc::new(AtomicU32::new(0)),
            post_calls: StdArc::new(AtomicU32::new(0)),
            sleep_pre: None,
        };
        insert(&host, "acme.bad", stub);

        for _ in 0..CIRCUIT_BREAKER_THRESHOLD {
            host.run_post_execute(ctx(), post_ok(), None).await;
        }
        assert_eq!(host.loaded_count(), 0);
    }

    #[tokio::test]
    async fn pre_execute_timeout_treats_plugin_as_failed_without_stalling_the_caller() {
        // A plugin that sleeps for longer than PRE_EXECUTE_TIMEOUT must not
        // delay the caller past that budget. The hook is treated as failed
        // and the next call increments the circuit-breaker counter as usual.
        let host = PluginHost::new();
        let stub = StubPlugin {
            pre: vec![],
            post: vec![],
            pre_calls: StdArc::new(AtomicU32::new(0)),
            post_calls: StdArc::new(AtomicU32::new(0)),
            sleep_pre: Some(PRE_EXECUTE_TIMEOUT + Duration::from_secs(2)),
        };
        insert(&host, "acme.slow", stub);

        let start = std::time::Instant::now();
        let verdict = host.run_pre_execute(ctx()).await;
        let elapsed = start.elapsed();

        assert_eq!(verdict, Decision::Allow);
        assert!(
            elapsed < PRE_EXECUTE_TIMEOUT + Duration::from_millis(500),
            "run_pre_execute returned in {elapsed:?}, should have honoured the timeout"
        );
    }

    #[test]
    fn verify_integrity_matches_a_known_digest() {
        // sha256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        let expected = "sha256-2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert!(verify_integrity(b"hello", expected).is_ok());
    }

    #[test]
    fn verify_integrity_rejects_a_tampered_payload() {
        let expected = "sha256-2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let err = verify_integrity(b"hello world", expected).unwrap_err();
        assert!(err.contains("mismatch"), "unexpected error: {err}");
    }

    #[test]
    fn contributions_cache_is_shared_across_calls_and_cleared_on_reload() {
        let host = PluginHost::new();
        // Two consecutive reads return Arcs that point at the same allocation:
        // the second call hit the cache, no rescan happened.
        let first = host.contributions();
        let second = host.contributions();
        assert!(Arc::ptr_eq(&first, &second));

        // reload() invalidates the cache; the next read produces a fresh Arc.
        host.reload();
        let third = host.contributions();
        assert!(!Arc::ptr_eq(&first, &third));
    }

    #[tokio::test]
    async fn schedule_post_execute_returns_immediately_and_eventually_runs_the_hook() {
        let host = Arc::new(PluginHost::new());
        let calls = StdArc::new(AtomicU32::new(0));
        let stub = StubPlugin {
            pre: vec![],
            post: vec![],
            pre_calls: StdArc::new(AtomicU32::new(0)),
            post_calls: calls.clone(),
            sleep_pre: None,
        };
        insert(&host, "acme.post", stub);

        let start = std::time::Instant::now();
        host.schedule_post_execute(ctx(), post_ok(), None);
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(50),
            "schedule must be fire-and-forget; took {elapsed:?}"
        );

        // Give the background task a window to run the hook.
        for _ in 0..50 {
            if calls.load(Ordering::SeqCst) >= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

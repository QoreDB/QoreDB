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
    LogEvent, LogSender, NotifyEvent, NotifyLevel, NotifySender, PluginInstance, PluginRuntime,
    PluginStorage, PostExecuteResult, QueryReadPayload, WasmiRuntime,
};
use crate::plugins::{plugins_dir, registry, PluginContributions};

/// Locks a `Mutex`, recovering from poisoning. A panicked hook must not
/// lock the host out for the rest of the session.
fn lock_recover<'a, T>(mutex: &'a Mutex<T>, what: &'static str) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::error!(target: "plugins", lock = what, "plugin host mutex was poisoned; recovering");
            poisoned.into_inner()
        }
    }
}

const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;
/// `pre_execute` runs on the query critical path; treat anything past this
/// as a wedged host fn and let the query proceed.
const PRE_EXECUTE_TIMEOUT: Duration = Duration::from_millis(500);
const POST_EXECUTE_TIMEOUT: Duration = Duration::from_secs(5);
/// A command is an explicit user action, so it gets a wider budget than the
/// query-path hooks — but still bounded so a wedged module can't pin a
/// blocking thread forever.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const POST_EXECUTE_QUEUE_DEPTH: usize = 64;

type SharedInstance = Arc<Mutex<Box<dyn PluginInstance>>>;

/// Loads executable plugins and runs their hooks.
pub struct PluginHost {
    runtime: Arc<dyn PluginRuntime>,
    notify: Mutex<Option<NotifySender>>,
    log: Mutex<Option<LogSender>>,
    /// Per-plugin mutex so distinct plugins run their hooks concurrently.
    instances: Mutex<HashMap<String, SharedInstance>>,
    /// Consecutive hook-failure count per plugin id; reset on success,
    /// triggers unload at [`CIRCUIT_BREAKER_THRESHOLD`].
    failures: Mutex<HashMap<String, u32>>,
    post_queue: Arc<Semaphore>,
    /// `None` means "rescan on next read"; cleared by [`reload`].
    contributions_cache: Mutex<Option<Arc<PluginContributions>>>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(WasmiRuntime::new()),
            notify: Mutex::new(None),
            log: Mutex::new(None),
            instances: Mutex::new(HashMap::new()),
            failures: Mutex::new(HashMap::new()),
            post_queue: Arc::new(Semaphore::new(POST_EXECUTE_QUEUE_DEPTH)),
            contributions_cache: Mutex::new(None),
        }
    }

    /// Wires the sender the runtime pushes toast events to. Must be set
    /// before the reload that should be able to surface notifications.
    pub fn set_notify_sender(&self, sender: NotifySender) {
        *lock_recover(&self.notify, "notify") = Some(sender);
    }

    /// Wires the sender lifecycle and plugin log lines are pushed to. Must be
    /// set before the reload whose loads should appear in the log.
    pub fn set_log_sender(&self, sender: LogSender) {
        *lock_recover(&self.log, "log") = Some(sender);
    }

    /// Rescans the plugins directory and (re)loads every enabled, compatible
    /// executable plugin. Single invalidation point for the contributions
    /// cache: install / remove / enable / disable / consent all funnel here.
    pub fn reload(&self) {
        let dir = plugins_dir();
        let notify = lock_recover(&self.notify, "notify").clone();
        let log = lock_recover(&self.log, "log").clone();
        let mut instances = lock_recover(&self.instances, "instances");
        instances.clear();
        lock_recover(&self.failures, "failures").clear();
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
                    emit_log(
                        &log,
                        &plugin.manifest.id,
                        NotifyLevel::Error,
                        format!("could not read module: {e}"),
                    );
                    continue;
                }
            };

            if let Some(expected) = runtime_spec.integrity.as_deref() {
                if let Err(e) = verify_integrity(&wasm, expected) {
                    tracing::warn!(
                        plugin = %plugin.manifest.id,
                        error = %e,
                        "plugin integrity check failed; refusing to load"
                    );
                    emit_log(
                        &log,
                        &plugin.manifest.id,
                        NotifyLevel::Error,
                        format!("integrity check failed: {e}"),
                    );
                    continue;
                }
            }

            let consent = capabilities::read_grants(&dir, &plugin.manifest.id);
            // A plugin never sees a capability it did not request, even when
            // the on-disk consent file has been tampered with.
            let requested: BTreeSet<CapabilityKind> = capabilities::requested(
                &runtime_spec.capabilities,
            )
            .into_iter()
            .collect();
            let effective: BTreeSet<CapabilityKind> =
                consent.intersection(&requested).copied().collect();
            let granted_count = effective.len();
            let requested_count = requested.len();

            let storage_path = storage::storage_path(&dir, &plugin.dir_name);
            // Read from the manifest, never from the consent file: a tampered
            // consent record can't widen the network surface.
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
                log: log.clone(),
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
                    emit_log(
                        &log,
                        &plugin.manifest.id,
                        NotifyLevel::Info,
                        format!("loaded — {granted_count}/{requested_count} capabilities granted"),
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin.manifest.id,
                        error = %e,
                        "could not load plugin"
                    );
                    emit_log(
                        &log,
                        &plugin.manifest.id,
                        NotifyLevel::Error,
                        format!("failed to load: {e}"),
                    );
                }
            }
        }
    }

    /// Aggregates the `pre_execute` verdicts: any `Block` wins, otherwise
    /// the first `Warn`, else `Allow`. A `Warn` also fires a toast on the
    /// `plugin-notify` channel.
    pub async fn run_pre_execute(&self, context: HookContext) -> Decision {
        let snapshot = self.snapshot_instances();
        let mut warning: Option<(String, String)> = None;
        let mut tripped: Vec<(String, String)> = Vec::new();

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
                        tripped.push((id.clone(), reason));
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
                    code: None,
                });
                Decision::Warn { message }
            }
            None => Decision::Allow,
        }
    }

    /// Runs `post_execute` on every loaded plugin. `query_payload` is only
    /// passed to plugins granted `queryRead`.
    pub async fn run_post_execute(
        &self,
        context: HookContext,
        result: PostExecuteResult,
        query_payload: Option<Arc<QueryReadPayload>>,
    ) {
        let snapshot = self.snapshot_instances();
        let mut tripped: Vec<(String, String)> = Vec::new();

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
                        tripped.push((id.clone(), reason));
                    }
                }
            }
        }

        self.unload_tripped(tripped);
    }

    /// Fires `post_execute` on a background task and returns immediately.
    /// The queue is bounded — overflow is dropped (and logged).
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

    /// Invokes a contributed command on the matching plugin. Errors surface
    /// to the caller — a command is an explicit user action. Bounded by
    /// [`COMMAND_TIMEOUT`] and fed into the same circuit breaker as the hooks,
    /// so a command that traps or wedges repeatedly unloads the plugin instead
    /// of hanging a blocking thread on every click.
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
        let outcome = run_with_timeout(instance, COMMAND_TIMEOUT, move |guard| {
            guard.command(&command_id, &args)
        })
        .await;

        match outcome {
            HookOutcome::Ok(value) => {
                self.record_success(plugin_id);
                Ok(value)
            }
            HookOutcome::Failed(reason) => {
                tracing::warn!(plugin = %plugin_id, reason = %reason, "plugin command failed");
                if self.record_failure(plugin_id) {
                    self.unload_tripped(vec![(plugin_id.to_string(), reason.clone())]);
                }
                Err(reason)
            }
        }
    }

    /// No-op when the bridge is not wired (early startup, headless tests).
    fn emit_notify(&self, event: NotifyEvent) {
        let sender = lock_recover(&self.notify, "notify").clone();
        if let Some(sender) = sender {
            let _ = sender.send(event);
        }
    }

    /// Returns `true` when the circuit breaker just tripped.
    fn record_failure(&self, plugin_id: &str) -> bool {
        let mut failures = lock_recover(&self.failures, "failures");
        let count = failures.entry(plugin_id.to_string()).or_insert(0);
        *count += 1;
        *count >= CIRCUIT_BREAKER_THRESHOLD
    }

    fn record_success(&self, plugin_id: &str) {
        let mut failures = lock_recover(&self.failures, "failures");
        failures.remove(plugin_id);
    }

    fn unload_tripped(&self, tripped: Vec<(String, String)>) {
        if tripped.is_empty() {
            return;
        }
        {
            let mut instances = lock_recover(&self.instances, "instances");
            for (id, _) in &tripped {
                instances.remove(id);
            }
        }
        for (id, reason) in tripped {
            self.notify_disabled(&id, &reason);
        }
    }

    /// Emits the lifecycle `"disabled"` notification. `message` carries the
    /// raw failure reason; the UI localizes the headline from the `code` and
    /// the plugin name and shows the reason as the toast description.
    fn notify_disabled(&self, plugin_id: &str, reason: &str) {
        tracing::warn!(
            plugin = plugin_id,
            threshold = CIRCUIT_BREAKER_THRESHOLD,
            reason = reason,
            "plugin unloaded after repeated hook failures"
        );
        emit_log(
            &lock_recover(&self.log, "log").clone(),
            plugin_id,
            NotifyLevel::Error,
            format!(
                "unloaded after {CIRCUIT_BREAKER_THRESHOLD} consecutive failures: {reason}"
            ),
        );
        self.emit_notify(NotifyEvent {
            plugin_id: plugin_id.to_string(),
            level: NotifyLevel::Warning,
            message: reason.to_string(),
            code: Some("disabled".to_string()),
        });
    }

    /// Snapshots the (id, instance) pairs so hooks contend only on their
    /// own inner mutex — different plugins run in parallel.
    fn snapshot_instances(&self) -> Vec<(String, SharedInstance)> {
        lock_recover(&self.instances, "instances")
            .iter()
            .map(|(k, v)| (k.clone(), Arc::clone(v)))
            .collect()
    }

    pub fn loaded_count(&self) -> usize {
        lock_recover(&self.instances, "instances").len()
    }

    /// Whether the plugin currently has a live instance. `false` once the
    /// circuit breaker has unloaded it (until the next [`reload`]).
    pub fn is_loaded(&self, plugin_id: &str) -> bool {
        lock_recover(&self.instances, "instances").contains_key(plugin_id)
    }

    /// Consecutive hook/command failures recorded for a plugin since its last
    /// success. Reaches [`CIRCUIT_BREAKER_THRESHOLD`] right before an unload.
    pub fn failure_count(&self, plugin_id: &str) -> u32 {
        lock_recover(&self.failures, "failures")
            .get(plugin_id)
            .copied()
            .unwrap_or(0)
    }

    /// Memoised; the disk rescan only runs after the next [`reload`].
    pub fn contributions(&self) -> Arc<PluginContributions> {
        {
            let cache = lock_recover(&self.contributions_cache, "contributions_cache");
            if let Some(existing) = cache.as_ref() {
                return Arc::clone(existing);
            }
        }
        // Compute outside the lock; a slow disk would otherwise pin it.
        let fresh = Arc::new(registry::get_contributions(&plugins_dir()));
        let mut cache = lock_recover(&self.contributions_cache, "contributions_cache");
        // Lost the race: keep whichever Arc landed first, both reflect the
        // same on-disk state.
        let result = cache.get_or_insert_with(|| Arc::clone(&fresh));
        Arc::clone(result)
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

/// `Failed` covers timeout, join error, and the plugin's own `PluginError` —
/// callers handle them identically.
enum HookOutcome<T> {
    Ok(T),
    Failed(String),
}

/// Sends a log line if the channel is wired. Shared by `reload` (which holds a
/// cloned sender) and the lifecycle paths. A no-op in headless tests.
fn emit_log(sender: &Option<LogSender>, plugin_id: &str, level: NotifyLevel, message: String) {
    if let Some(sender) = sender {
        let _ = sender.send(LogEvent {
            plugin_id: plugin_id.to_string(),
            level,
            message,
        });
    }
}

fn verify_integrity(wasm: &[u8], expected: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let Some(expected_hex) = expected.strip_prefix("sha256-") else {
        return Err(format!("malformed integrity '{expected}'"));
    };
    let mut hasher = Sha256::new();
    hasher.update(wasm);
    let actual = hasher.finalize();
    let actual_hex = hex_encode(&actual);
    if actual_hex.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(format!(
            "integrity mismatch: expected sha256-{expected_hex}, got sha256-{actual_hex}"
        ))
    }
}

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

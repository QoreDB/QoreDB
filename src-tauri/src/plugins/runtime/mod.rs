// SPDX-License-Identifier: Apache-2.0

//! Executable plugin runtime — the sandbox that runs plugin WASM code.
//!
//! This module defines the runtime *abstraction*. The `wasmi` implementation
//! lives behind the [`PluginRuntime`] trait so a faster JIT (`wasmtime`) can
//! be swapped in later without touching callers. Plugin code is always fuel-
//! and memory-bounded: a plugin can never block or crash QoreDB.

pub mod capabilities;
mod host_fns;
mod manager;
pub mod secrets;
pub mod storage;
mod wasmi_host;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

pub use capabilities::CapabilityKind;
pub use manager::PluginHost;
pub use storage::PluginStorage;
pub use wasmi_host::WasmiRuntime;

/// Decision a `preExecute` hook returns for a query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Decision {
    /// Let the query run unchanged.
    Allow,
    /// Let the query run, but surface a warning to the user.
    Warn { message: String },
    /// Stop the query before it runs.
    Block { reason: String },
}

/// Read-only query context handed to a hook. Kept independent of the
/// interceptor's own `QueryContext` so the runtime has no dependency on it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookContext {
    pub query: String,
    pub driver_id: String,
    pub environment: String,
    pub operation_type: String,
    pub is_mutation: bool,
    pub is_dangerous: bool,
    pub read_only: bool,
}

/// Metadata of a completed query handed to the `postExecute` hook.
///
/// Row contents are intentionally *not* part of this struct — they're only
/// available when the `queryRead` capability is granted, via a dedicated
/// host call from within the hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostExecuteResult {
    pub success: bool,
    pub execution_time_ms: u64,
    /// `None` if the driver did not report a row count.
    pub row_count: Option<u64>,
    /// `None` on success.
    pub error: Option<String>,
}

/// A toast notification a plugin asked the host to surface to the user. The
/// runtime hands these off to a Tauri-driven dispatcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyEvent {
    pub plugin_id: String,
    pub level: NotifyLevel,
    pub message: String,
    /// Lifecycle marker for host-generated notifications (e.g. `"disabled"`).
    /// Lets the UI localize the toast instead of surfacing the raw English
    /// string. Absent for plugin-issued `notify` calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Severity of a plugin-issued toast. Mirrors `sonner`'s four standard levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifyLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// Sender end of the runtime → app notification channel. The app spawns a
/// task that drains this channel and emits Tauri events to the webview.
pub type NotifySender = UnboundedSender<NotifyEvent>;

/// Per-invocation services snapshotted into the `Store::data` so host
/// functions can read them via `Caller::data()`.
#[derive(Clone)]
pub struct InvocationServices {
    pub plugin_id: String,
    pub consent: Arc<BTreeSet<CapabilityKind>>,
    pub storage: Arc<PluginStorage>,
    pub notify: Option<NotifySender>,
    /// `None` outside `postExecute` or when `queryRead` isn't granted.
    pub query_result: Option<Arc<QueryReadPayload>>,
    pub http_allowed_hosts: Arc<Vec<String>>,
    /// SSRF escape hatch: when false (default), private / loopback /
    /// link-local / cloud-metadata addresses are refused even if the name
    /// is on the allowlist.
    pub http_allow_private_networks: bool,
    pub fs_root: Option<PathBuf>,
    /// Read from the manifest; a tampered consent file can't widen this set.
    pub secret_names: Arc<Vec<String>>,
}

/// JSON-serialised so the host doesn't re-serialise on every `queryRead` call.
pub struct QueryReadPayload {
    pub json: String,
}

/// Per-invocation budget. No wall-clock here: `wasmi` has no cheap
/// interruption primitive, fuel bounds invocation cost. Wall-clock timeouts
/// live one level up, in [`PluginHost`].
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub fuel: u64,
    /// 64 KiB per page.
    pub memory_pages: u32,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            fuel: 50_000_000,
            memory_pages: 256,
        }
    }
}

/// None of these abort the host operation — the caller logs and carries on.
#[derive(Debug)]
pub enum PluginError {
    Load(String),
    /// Panic, out-of-bounds access, `unreachable`, …
    Trap(String),
    BudgetExceeded,
    /// Marshalling failure across the host/guest ABI boundary.
    Abi(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load(m) => write!(f, "plugin load failed: {m}"),
            Self::Trap(m) => write!(f, "plugin trapped: {m}"),
            Self::BudgetExceeded => write!(f, "plugin exceeded its resource budget"),
            Self::Abi(m) => write!(f, "plugin ABI error: {m}"),
        }
    }
}

impl std::error::Error for PluginError {}

/// Abstraction over the WASM runtime. Today: `wasmi`.
pub trait PluginRuntime: Send + Sync {
    fn load(
        &self,
        plugin_id: String,
        wasm: &[u8],
        budget: Budget,
        services: InvocationServices,
    ) -> Result<Box<dyn PluginInstance>, PluginError>;
}

/// Not `Sync`; callers serialise access through a per-plugin mutex.
pub trait PluginInstance: Send {
    /// A module that doesn't export `pre_execute` yields [`Decision::Allow`].
    fn pre_execute(&mut self, context: &HookContext) -> Result<Decision, PluginError>;

    /// `payload` is only present when `queryRead` was granted.
    fn post_execute(
        &mut self,
        context: &HookContext,
        result: &PostExecuteResult,
        payload: Option<Arc<QueryReadPayload>>,
    ) -> Result<(), PluginError>;

    /// A module that doesn't export `command` returns [`PluginError::Abi`] —
    /// commands are explicit user actions, so the host surfaces the failure
    /// instead of silently no-oping.
    fn command(
        &mut self,
        command_id: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, PluginError>;
}

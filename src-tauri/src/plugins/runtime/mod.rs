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

/// Per-invocation services exposed to host functions. Snapshotted at instance
/// build time (consent, storage, notify, plugin id) and stashed inside the
/// `wasmi::Store::data` so host functions can read them via `Caller::data()`.
#[derive(Clone)]
pub struct InvocationServices {
    pub plugin_id: String,
    pub consent: Arc<BTreeSet<CapabilityKind>>,
    pub storage: Arc<PluginStorage>,
    pub notify: Option<NotifySender>,
    /// Row data exposed to a hook when `queryRead` is granted. `None` outside
    /// `postExecute` invocations or when the capability is not granted.
    pub query_result: Option<Arc<QueryReadPayload>>,
    /// Hosts the plugin is allowed to contact. Re-checked by the `http`
    /// host fn against the URL the plugin passes — defence in depth.
    pub http_allowed_hosts: Arc<Vec<String>>,
    /// When false (default), the `http` host fn refuses any outbound request
    /// that resolves to a loopback / private / link-local / cloud-metadata
    /// address. Plugins that legitimately need internal-network access (an
    /// on-premise data catalogue, a sidecar) flip this on in their manifest.
    pub http_allow_private_networks: bool,
    /// Directory the plugin's `fs` host fns are scoped to. Every requested
    /// path is joined here and rejected if it escapes the directory.
    pub fs_root: Option<PathBuf>,
    /// Names of secrets the manifest requested. The `secrets` host fn rejects
    /// reads for any name not in this list, so a tampered consent file can't
    /// pull arbitrary secrets from the keyring.
    pub secret_names: Arc<Vec<String>>,
}

/// The bundle a `queryRead`-capable hook can pull through the
/// `qoredb_query_read` host call. Kept as a JSON string so the host doesn't
/// have to re-serialise on every fetch.
pub struct QueryReadPayload {
    pub json: String,
}

/// Resource budget enforced on every single plugin invocation.
///
/// Fuel covers runaway execution (an infinite loop traps once exhausted).
/// `memory_pages` caps how far a plugin can grow its linear memory, so a
/// hostile `memory.grow` cannot push the host into swap or OOM. A wall-clock
/// timeout is intentionally absent: `wasmi` has no cheap interruption
/// primitive, and fuel already bounds invocation cost.
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    /// Maximum WASM instructions (fuel) per invocation.
    pub fuel: u64,
    /// Maximum linear memory, in WASM pages of 64 KiB.
    pub memory_pages: u32,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            fuel: 50_000_000,
            memory_pages: 256, // 16 MiB
        }
    }
}

/// Why a plugin invocation failed. None of these abort the host operation:
/// the caller logs, disables the plugin and carries on.
#[derive(Debug)]
pub enum PluginError {
    /// The WASM module could not be loaded or instantiated.
    Load(String),
    /// The module trapped (panic, out-of-bounds access, `unreachable`).
    Trap(String),
    /// The fuel, memory or time budget was exhausted.
    BudgetExceeded,
    /// Data could not be marshalled across the host/guest ABI boundary.
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

/// A sandboxed plugin runtime. One implementation today (`wasmi`); the trait
/// keeps the door open to a JIT backend without changing callers.
pub trait PluginRuntime: Send + Sync {
    /// Loads a WASM module from raw bytes, ready to run hooks.
    fn load(
        &self,
        plugin_id: String,
        wasm: &[u8],
        budget: Budget,
        services: InvocationServices,
    ) -> Result<Box<dyn PluginInstance>, PluginError>;
}

/// A loaded, runnable plugin instance. Not `Sync`; callers serialise access.
pub trait PluginInstance: Send {
    /// Runs the `pre_execute` hook. A module that does not export it yields
    /// [`Decision::Allow`].
    fn pre_execute(&mut self, context: &HookContext) -> Result<Decision, PluginError>;

    /// Runs the `post_execute` hook. A module that does not export it is a
    /// no-op. `payload` carries row data; only handed in when the plugin has
    /// been granted `queryRead`.
    fn post_execute(
        &mut self,
        context: &HookContext,
        result: &PostExecuteResult,
        payload: Option<Arc<QueryReadPayload>>,
    ) -> Result<(), PluginError>;

    /// Runs the `command` hook with the contributed command id and a JSON
    /// arg payload. Returns the JSON value the plugin produced. A module
    /// that does not export `command` returns [`PluginError::Abi`] with a
    /// "not exported" message — the host turns that into a user-facing
    /// error rather than silently doing nothing (commands are explicit
    /// user actions).
    fn command(
        &mut self,
        command_id: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, PluginError>;
}

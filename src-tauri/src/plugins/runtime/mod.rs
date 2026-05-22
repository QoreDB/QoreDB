// SPDX-License-Identifier: Apache-2.0

//! Executable plugin runtime — the sandbox that runs plugin WASM code.
//!
//! This module defines the runtime *abstraction*. The `wasmi` implementation
//! lives behind the [`PluginRuntime`] trait so a faster JIT (`wasmtime`) can
//! be swapped in later without touching callers. Plugin code is always fuel-
//! and time-bounded: a plugin can never block or crash QoreDB.

mod manager;
mod wasmi_host;

pub use manager::PluginHost;
pub use wasmi_host::WasmiRuntime;

use std::time::Duration;

use serde::{Deserialize, Serialize};

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

/// Resource budget enforced on every single plugin invocation.
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    /// Maximum WASM instructions (fuel) per invocation.
    pub fuel: u64,
    /// Maximum linear memory, in WASM pages of 64 KiB.
    pub memory_pages: u32,
    /// Wall-clock ceiling per invocation.
    pub timeout: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            fuel: 50_000_000,
            memory_pages: 256, // 16 MiB
            timeout: Duration::from_millis(100),
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
    fn load(&self, wasm: &[u8], budget: Budget) -> Result<Box<dyn PluginInstance>, PluginError>;
}

/// A loaded, runnable plugin instance. Not `Sync`; callers serialise access.
pub trait PluginInstance: Send {
    /// Runs the `pre_execute` hook. A module that does not export it yields
    /// [`Decision::Allow`].
    fn pre_execute(&mut self, context: &HookContext) -> Result<Decision, PluginError>;
}

// SPDX-License-Identifier: Apache-2.0

//! SDK for writing QoreDB executable plugins.
//!
//! A plugin is a `cdylib` compiled to `wasm32-unknown-unknown`. It implements
//! a hook as a typed `fn(HookContext) -> Decision`; this SDK hides the host
//! ABI (linear-memory marshalling) behind [`export_pre_execute!`].
//!
//! ```ignore
//! use qoredb_plugin_sdk::{export_pre_execute, Decision, HookContext};
//!
//! fn check(ctx: HookContext) -> Decision {
//!     if ctx.is_mutation && !ctx.query.to_uppercase().contains("WHERE") {
//!         return Decision::block("mutation without WHERE");
//!     }
//!     Decision::allow()
//! }
//! export_pre_execute!(check);
//! ```

use serde::{Deserialize, Serialize};

/// Read-only query context passed to a hook. Mirrors the QoreDB host type.
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

/// Verdict a `pre_execute` hook returns for a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Decision {
    /// Let the query run unchanged.
    Allow,
    /// Let the query run, but surface a warning.
    Warn { message: String },
    /// Stop the query.
    Block { reason: String },
}

impl Decision {
    pub fn allow() -> Self {
        Decision::Allow
    }
    pub fn warn(message: impl Into<String>) -> Self {
        Decision::Warn {
            message: message.into(),
        }
    }
    pub fn block(reason: impl Into<String>) -> Self {
        Decision::Block {
            reason: reason.into(),
        }
    }
}

/// Reserves `len` bytes in the plugin's linear memory and returns the offset.
/// The host writes hook input there before calling the hook. The buffer is
/// intentionally leaked: the host re-instantiates the module per call, so it
/// is reclaimed wholesale rather than freed here.
pub fn alloc(len: i32) -> i32 {
    let mut buf: Vec<u8> = Vec::with_capacity(len.max(0) as usize);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr as i32
}

/// Decodes the `HookContext` the host placed at `[ptr, ptr + len)`.
///
/// # Safety
/// `ptr`/`len` must come from the host ABI and describe initialised bytes.
pub unsafe fn read_context(ptr: i32, len: i32) -> Option<HookContext> {
    let bytes = std::slice::from_raw_parts(ptr as *const u8, len.max(0) as usize);
    serde_json::from_slice(bytes).ok()
}

/// Serialises `decision`, leaks the bytes and packs `(ptr << 32 | len)` for the
/// host to read back.
pub fn pack_decision(decision: &Decision) -> i64 {
    let json =
        serde_json::to_vec(decision).unwrap_or_else(|_| br#"{"kind":"allow"}"#.to_vec());
    let len = json.len() as i64;
    let bytes = json.into_boxed_slice();
    let ptr = bytes.as_ptr() as i64;
    std::mem::forget(bytes);
    (ptr << 32) | (len & 0xFFFF_FFFF)
}

/// Exports the host ABI for a typed `fn(HookContext) -> Decision`. Invoke once
/// at the crate root of a plugin.
#[macro_export]
macro_rules! export_pre_execute {
    ($handler:path) => {
        /// Host ABI: reserve `len` bytes for hook input.
        #[no_mangle]
        pub extern "C" fn qoredb_alloc(len: i32) -> i32 {
            $crate::alloc(len)
        }

        /// Host ABI: run the `pre_execute` hook.
        ///
        /// # Safety
        /// Exported for the QoreDB host; `ptr`/`len` come from the host ABI.
        #[no_mangle]
        pub unsafe extern "C" fn pre_execute(ptr: i32, len: i32) -> i64 {
            let decision = match $crate::read_context(ptr, len) {
                Some(context) => $handler(context),
                None => $crate::Decision::Allow,
            };
            $crate::pack_decision(&decision)
        }
    };
}

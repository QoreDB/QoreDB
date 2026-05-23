// SPDX-License-Identifier: Apache-2.0

//! SDK for writing QoreDB executable plugins.
//!
//! A plugin is a `cdylib` compiled to `wasm32-unknown-unknown`. It implements
//! one or both lifecycle hooks as typed Rust functions; this SDK hides the
//! host ABI (linear-memory marshalling) behind the [`export_pre_execute!`]
//! and [`export_post_execute!`] macros. Phase 2 also exposes helpers for the
//! `log`, `notify`, `storage` and `queryRead` capabilities — every helper
//! is a no-op when the corresponding capability hasn't been granted, so a
//! plugin can be written defensively.
//!
//! ```ignore
//! use qoredb_plugin_sdk::{export_pre_execute, Decision, HookContext, log, LogLevel};
//!
//! fn check(ctx: HookContext) -> Decision {
//!     if ctx.is_mutation && !ctx.query.to_uppercase().contains("WHERE") {
//!         log(LogLevel::Warn, "blocking unsafe mutation");
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

/// Metadata of a completed query handed to a `post_execute` hook. Row data
/// is *not* in here — fetch it via [`query_read`] when `queryRead` is
/// granted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostExecuteResult {
    pub success: bool,
    pub execution_time_ms: u64,
    pub row_count: Option<u64>,
    pub error: Option<String>,
}

/// Envelope a `post_execute` hook receives: the original query context plus
/// the execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostExecuteEnvelope {
    pub context: HookContext,
    pub result: PostExecuteResult,
}

/// Envelope a `command` hook receives: the contributed command id and the
/// JSON args the host forwarded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandEnvelope {
    pub id: String,
    #[serde(default)]
    pub args: serde_json::Value,
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

/// Severity of a log line written through the `log` capability.
#[derive(Debug, Clone, Copy)]
#[repr(i32)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

/// Severity of a toast issued through the `notify` capability.
#[derive(Debug, Clone, Copy)]
#[repr(i32)]
pub enum NotifyLevel {
    Info = 0,
    Success = 1,
    Warning = 2,
    Error = 3,
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

/// Decodes the `post_execute` envelope (`{ context, result }`).
///
/// # Safety
/// `ptr`/`len` must come from the host ABI and describe initialised bytes.
pub unsafe fn read_post_envelope(ptr: i32, len: i32) -> Option<PostExecuteEnvelope> {
    let bytes = std::slice::from_raw_parts(ptr as *const u8, len.max(0) as usize);
    serde_json::from_slice(bytes).ok()
}

/// Decodes the `command` envelope (`{ id, args }`).
///
/// # Safety
/// `ptr`/`len` must come from the host ABI and describe initialised bytes.
pub unsafe fn read_command_envelope(ptr: i32, len: i32) -> Option<CommandEnvelope> {
    let bytes = std::slice::from_raw_parts(ptr as *const u8, len.max(0) as usize);
    serde_json::from_slice(bytes).ok()
}

/// Serialises a JSON value, leaks the bytes and packs `(ptr << 32 | len)`
/// for the host to read back. Returns 0 for `null` to signal "no result".
pub fn pack_value(value: &serde_json::Value) -> i64 {
    if value.is_null() {
        return 0;
    }
    let json = serde_json::to_vec(value).unwrap_or_default();
    if json.is_empty() {
        return 0;
    }
    let len = json.len() as i64;
    let bytes = json.into_boxed_slice();
    let ptr = bytes.as_ptr() as i64;
    std::mem::forget(bytes);
    (ptr << 32) | (len & 0xFFFF_FFFF)
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

// ─── Capability helpers ─────────────────────────────────────────────────────
//
// Each capability exposes a `qoredb_*` host import. The helpers below wrap
// the unsafe FFI behind a typed surface that gracefully no-ops when the
// capability is not granted (the host returns an error code).

mod ffi {
    extern "C" {
        pub fn qoredb_log(level: i32, ptr: i32, len: i32) -> i32;
        pub fn qoredb_notify(level: i32, ptr: i32, len: i32) -> i32;
        pub fn qoredb_kv_get(key_ptr: i32, key_len: i32) -> i64;
        pub fn qoredb_kv_set(key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32) -> i32;
        pub fn qoredb_kv_del(key_ptr: i32, key_len: i32) -> i32;
        pub fn qoredb_query_read() -> i64;
    }
}

fn slice_parts(s: &str) -> (i32, i32) {
    (s.as_ptr() as i32, s.len() as i32)
}

/// Writes a log line through the host. No-op if `log` was not granted.
pub fn log(level: LogLevel, message: &str) {
    let (ptr, len) = slice_parts(message);
    unsafe {
        let _ = ffi::qoredb_log(level as i32, ptr, len);
    }
}

/// Surfaces a toast to the user. No-op if `notify` was not granted.
pub fn notify(level: NotifyLevel, message: &str) {
    let (ptr, len) = slice_parts(message);
    unsafe {
        let _ = ffi::qoredb_notify(level as i32, ptr, len);
    }
}

/// Reads a value from the plugin's KV store. `None` if absent or if
/// `storage` was not granted.
pub fn storage_get(key: &str) -> Option<String> {
    let (kp, kl) = slice_parts(key);
    let packed = unsafe { ffi::qoredb_kv_get(kp, kl) };
    if packed == 0 {
        return None;
    }
    let (ptr, len) = unpack(packed);
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
    String::from_utf8(bytes.to_vec()).ok()
}

/// Sets a value in the plugin's KV store. Returns `true` if the host
/// accepted the write (capability granted, value within quota).
pub fn storage_set(key: &str, value: &str) -> bool {
    let (kp, kl) = slice_parts(key);
    let (vp, vl) = slice_parts(value);
    unsafe { ffi::qoredb_kv_set(kp, kl, vp, vl) == 0 }
}

/// Deletes a key from the plugin's KV store.
pub fn storage_delete(key: &str) -> bool {
    let (kp, kl) = slice_parts(key);
    unsafe { ffi::qoredb_kv_del(kp, kl) == 0 }
}

/// Returns the JSON of the current query result. Only meaningful inside
/// `post_execute`, and only when `queryRead` was granted. Returns `None`
/// otherwise.
pub fn query_read_json() -> Option<String> {
    let packed = unsafe { ffi::qoredb_query_read() };
    if packed == 0 {
        return None;
    }
    let (ptr, len) = unpack(packed);
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
    String::from_utf8(bytes.to_vec()).ok()
}

fn unpack(packed: i64) -> (usize, usize) {
    let ptr = (packed >> 32) as u32;
    let len = (packed & 0xFFFF_FFFF) as u32;
    (ptr as usize, len as usize)
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

/// Exports the host ABI for a typed `fn(PostExecuteEnvelope)`. The
/// `qoredb_alloc` export is reused when both pre and post hooks are exported.
#[macro_export]
macro_rules! export_post_execute {
    ($handler:path) => {
        /// Host ABI: run the `post_execute` hook.
        ///
        /// # Safety
        /// Exported for the QoreDB host; `ptr`/`len` come from the host ABI.
        #[no_mangle]
        pub unsafe extern "C" fn post_execute(ptr: i32, len: i32) {
            if let Some(envelope) = $crate::read_post_envelope(ptr, len) {
                $handler(envelope);
            }
        }
    };
}

/// Exports the host ABI for a typed
/// `fn(CommandEnvelope) -> serde_json::Value` command handler.
#[macro_export]
macro_rules! export_command {
    ($handler:path) => {
        /// Host ABI: run the `command` hook.
        ///
        /// # Safety
        /// Exported for the QoreDB host; `ptr`/`len` come from the host ABI.
        #[no_mangle]
        pub unsafe extern "C" fn command(ptr: i32, len: i32) -> i64 {
            let result = match $crate::read_command_envelope(ptr, len) {
                Some(envelope) => $handler(envelope),
                None => serde_json::Value::Null,
            };
            $crate::pack_value(&result)
        }
    };
}

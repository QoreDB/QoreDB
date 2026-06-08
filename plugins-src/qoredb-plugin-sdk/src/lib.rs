// SPDX-License-Identifier: Apache-2.0

//! Minimal authoring SDK for QoreDB WASM plugins (ABI v1).
//!
//! It declares the host imports, owns the `qoredb_alloc` body the host calls
//! to pass bytes into the guest, and wraps memory marshalling and the
//! capability host functions behind safe helpers. A plugin re-exports the
//! allocator from its own cdylib root via [`export_alloc!`].

use serde::Serialize;
use std::alloc::{alloc, Layout};

#[link(wasm_import_module = "env")]
extern "C" {
    fn qoredb_log(level: i32, ptr: i32, len: i32) -> i32;
    fn qoredb_notify(level: i32, ptr: i32, len: i32) -> i32;
    fn qoredb_kv_get(key_ptr: i32, key_len: i32) -> i64;
    fn qoredb_kv_set(key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32) -> i32;
    fn qoredb_kv_del(key_ptr: i32, key_len: i32) -> i32;
}

/// The allocator body the host calls to reserve `len` bytes in guest memory.
/// Memory is re-initialised on every invocation (the host runs each hook in a
/// fresh store), so the leaked allocation never outlives the call.
pub fn alloc_impl(len: i32) -> i32 {
    let size = (len.max(0) as usize).max(1);
    let layout = Layout::from_size_align(size, 1).expect("alloc layout");
    unsafe { alloc(layout) as i32 }
}

fn pack(ptr: i32, len: i32) -> i64 {
    ((ptr as i64) << 32) | (len as i64 & 0xFFFF_FFFF)
}

fn unpack(packed: i64) -> (i32, usize) {
    (((packed >> 32) as u32) as i32, (packed & 0xFFFF_FFFF) as usize)
}

/// Copies the host-provided input slice `[ptr, ptr + len)` out of guest memory.
pub fn input(ptr: i32, len: i32) -> Vec<u8> {
    if len <= 0 {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) }.to_vec()
}

/// Serialises `value` into guest memory and returns the packed `(ptr << 32 |
/// len)` the host reads back. Returns 0 (JSON null to the host) on failure.
pub fn respond<T: Serialize>(value: &T) -> i64 {
    let Ok(bytes) = serde_json::to_vec(value) else {
        return 0;
    };
    let len = bytes.len() as i32;
    let ptr = alloc_impl(len);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
    }
    pack(ptr, len)
}

/// Log severity, matching the host's `qoredb_log` level codes.
pub enum Level {
    Info = 1,
    Warning = 2,
    Error = 3,
}

pub fn log(level: Level, msg: &str) {
    unsafe {
        qoredb_log(level as i32, msg.as_ptr() as i32, msg.len() as i32);
    }
}

pub fn notify(level: Level, msg: &str) {
    unsafe {
        qoredb_notify(level as i32, msg.as_ptr() as i32, msg.len() as i32);
    }
}

pub fn kv_get(key: &str) -> Option<String> {
    let packed = unsafe { qoredb_kv_get(key.as_ptr() as i32, key.len() as i32) };
    if packed == 0 {
        return None;
    }
    let (ptr, len) = unpack(packed);
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) }.to_vec();
    String::from_utf8(bytes).ok()
}

pub fn kv_set(key: &str, val: &str) -> bool {
    let code = unsafe {
        qoredb_kv_set(
            key.as_ptr() as i32,
            key.len() as i32,
            val.as_ptr() as i32,
            val.len() as i32,
        )
    };
    code == 0
}

pub fn kv_del(key: &str) {
    unsafe {
        qoredb_kv_del(key.as_ptr() as i32, key.len() as i32);
    }
}

/// Emits the `#[no_mangle] qoredb_alloc` export from the plugin's cdylib root,
/// where the linker is guaranteed to keep it as an exported symbol.
#[macro_export]
macro_rules! export_alloc {
    () => {
        #[no_mangle]
        pub extern "C" fn qoredb_alloc(len: i32) -> i32 {
            $crate::alloc_impl(len)
        }
    };
}

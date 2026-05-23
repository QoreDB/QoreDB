// SPDX-License-Identifier: Apache-2.0

//! Host-function catalogue: the surface a WASM plugin sees.
//!
//! Every Phase 2 capability has its host function unconditionally registered,
//! so any plugin can *import* it. The function then checks the per-invocation
//! consent set (snapshotted into the `Store`) and either does the work or
//! returns an error code. A revoked capability becomes a no-op; instantiation
//! never fails because of a missing import.

use wasmi::{Caller, Linker};

use super::wasmi_host::StoreData;
use super::{CapabilityKind, NotifyEvent, NotifyLevel};

/// ABI status codes. `Ok` = 0 is the no-news return; the negative codes are
/// the only ones a plugin needs to branch on.
pub const OK: i32 = 0;
pub const ERR_DENIED: i32 = -1;
pub const ERR_INVALID: i32 = -2;
pub const ERR_QUOTA: i32 = -3;

/// Memory budget for a single host call's string arguments — keeps a buggy
/// plugin from asking the host to read its entire linear memory as a key.
const MAX_STRING_ARG: usize = 64 * 1024;

/// Registers every Phase 2 host function on the linker.
pub fn register(linker: &mut Linker<StoreData>) -> Result<(), wasmi::errors::LinkerError> {
    register_log(linker)?;
    register_notify(linker)?;
    register_storage(linker)?;
    register_query_read(linker)?;
    Ok(())
}

fn register_log(linker: &mut Linker<StoreData>) -> Result<(), wasmi::errors::LinkerError> {
    linker
        .func_wrap(
            "env",
            "qoredb_log",
            |mut caller: Caller<'_, StoreData>, level: i32, ptr: i32, len: i32| -> i32 {
                if !caller
                    .data()
                    .services
                    .consent
                    .contains(&CapabilityKind::Log)
                {
                    return ERR_DENIED;
                }
                let Some(msg) = read_string(&mut caller, ptr, len) else {
                    return ERR_INVALID;
                };
                let plugin_id = caller.data().services.plugin_id.clone();
                match level {
                    0 => tracing::debug!(plugin = %plugin_id, "plugin: {msg}"),
                    1 => tracing::info!(plugin = %plugin_id, "plugin: {msg}"),
                    2 => tracing::warn!(plugin = %plugin_id, "plugin: {msg}"),
                    3 => tracing::error!(plugin = %plugin_id, "plugin: {msg}"),
                    _ => tracing::info!(plugin = %plugin_id, "plugin: {msg}"),
                }
                OK
            },
        )
        .map(|_| ())
}

fn register_notify(linker: &mut Linker<StoreData>) -> Result<(), wasmi::errors::LinkerError> {
    linker
        .func_wrap(
            "env",
            "qoredb_notify",
            |mut caller: Caller<'_, StoreData>, level: i32, ptr: i32, len: i32| -> i32 {
                if !caller
                    .data()
                    .services
                    .consent
                    .contains(&CapabilityKind::Notify)
                {
                    return ERR_DENIED;
                }
                let Some(msg) = read_string(&mut caller, ptr, len) else {
                    return ERR_INVALID;
                };
                let Some(sender) = caller.data().services.notify.clone() else {
                    return OK;
                };
                let event = NotifyEvent {
                    plugin_id: caller.data().services.plugin_id.clone(),
                    level: notify_level(level),
                    message: msg,
                };
                let _ = sender.send(event);
                OK
            },
        )
        .map(|_| ())
}

fn register_storage(linker: &mut Linker<StoreData>) -> Result<(), wasmi::errors::LinkerError> {
    linker.func_wrap(
        "env",
        "qoredb_kv_get",
        |mut caller: Caller<'_, StoreData>, key_ptr: i32, key_len: i32| -> i64 {
            if !caller
                .data()
                .services
                .consent
                .contains(&CapabilityKind::Storage)
            {
                return 0;
            }
            let Some(key) = read_string(&mut caller, key_ptr, key_len) else {
                return 0;
            };
            let Some(value) = caller.data().services.storage.get(&key) else {
                return 0;
            };
            match write_into_guest(&mut caller, value.as_bytes()) {
                Some((ptr, len)) => pack(ptr, len),
                None => 0,
            }
        },
    )?;

    linker.func_wrap(
        "env",
        "qoredb_kv_set",
        |mut caller: Caller<'_, StoreData>,
         key_ptr: i32,
         key_len: i32,
         val_ptr: i32,
         val_len: i32|
         -> i32 {
            if !caller
                .data()
                .services
                .consent
                .contains(&CapabilityKind::Storage)
            {
                return ERR_DENIED;
            }
            let Some(key) = read_string(&mut caller, key_ptr, key_len) else {
                return ERR_INVALID;
            };
            let Some(value) = read_string(&mut caller, val_ptr, val_len) else {
                return ERR_INVALID;
            };
            match caller.data().services.storage.set(&key, &value) {
                Ok(()) => OK,
                Err(super::storage::StorageError::QuotaExceeded) => ERR_QUOTA,
                Err(_) => ERR_INVALID,
            }
        },
    )?;

    linker
        .func_wrap(
            "env",
            "qoredb_kv_del",
            |mut caller: Caller<'_, StoreData>, key_ptr: i32, key_len: i32| -> i32 {
                if !caller
                    .data()
                    .services
                    .consent
                    .contains(&CapabilityKind::Storage)
                {
                    return ERR_DENIED;
                }
                let Some(key) = read_string(&mut caller, key_ptr, key_len) else {
                    return ERR_INVALID;
                };
                match caller.data().services.storage.delete(&key) {
                    Ok(()) => OK,
                    Err(_) => ERR_INVALID,
                }
            },
        )
        .map(|_| ())
}

/// `qoredb_query_read() -> i64`: returns a packed `(ptr, len)` pointing at
/// the JSON payload of the current query result. 0 if the capability is not
/// granted, the hook is not `postExecute`, or no payload is available.
fn register_query_read(linker: &mut Linker<StoreData>) -> Result<(), wasmi::errors::LinkerError> {
    linker
        .func_wrap(
            "env",
            "qoredb_query_read",
            |mut caller: Caller<'_, StoreData>| -> i64 {
                if !caller
                    .data()
                    .services
                    .consent
                    .contains(&CapabilityKind::QueryRead)
                {
                    return 0;
                }
                let Some(payload) = caller.data().services.query_result.clone() else {
                    return 0;
                };
                match write_into_guest(&mut caller, payload.json.as_bytes()) {
                    Some((ptr, len)) => pack(ptr, len),
                    None => 0,
                }
            },
        )
        .map(|_| ())
}

fn notify_level(raw: i32) -> NotifyLevel {
    match raw {
        0 => NotifyLevel::Info,
        1 => NotifyLevel::Success,
        2 => NotifyLevel::Warning,
        3 => NotifyLevel::Error,
        _ => NotifyLevel::Info,
    }
}

/// Reads `len` bytes at `ptr` from the guest's `memory` export, decoded as
/// UTF-8. Returns `None` on a bounds error, oversized input, or invalid UTF-8.
fn read_string(caller: &mut Caller<'_, StoreData>, ptr: i32, len: i32) -> Option<String> {
    if len < 0 || ptr < 0 {
        return None;
    }
    let len = len as usize;
    if len > MAX_STRING_ARG {
        return None;
    }
    let memory = caller.get_export("memory").and_then(|e| e.into_memory())?;
    let mut buf = vec![0u8; len];
    memory.read(&*caller, ptr as usize, &mut buf).ok()?;
    String::from_utf8(buf).ok()
}

/// Allocates `bytes.len()` bytes in the guest via its `qoredb_alloc` export
/// and writes `bytes` there. Returns the `(ptr, len)` pair on success.
fn write_into_guest(caller: &mut Caller<'_, StoreData>, bytes: &[u8]) -> Option<(i32, i32)> {
    let len = i32::try_from(bytes.len()).ok()?;
    let alloc = caller
        .get_export("qoredb_alloc")
        .and_then(|e| e.into_func())?;
    let alloc = alloc.typed::<i32, i32>(&*caller).ok()?;
    let ptr = alloc.call(&mut *caller, len).ok()?;
    let memory = caller.get_export("memory").and_then(|e| e.into_memory())?;
    memory.write(&mut *caller, ptr as usize, bytes).ok()?;
    Some((ptr, len))
}

fn pack(ptr: i32, len: i32) -> i64 {
    ((ptr as i64) << 32) | (len as i64 & 0xFFFF_FFFF)
}

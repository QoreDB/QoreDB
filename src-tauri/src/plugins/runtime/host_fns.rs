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

/// Registers every host function on the linker. Phase 2 plus the Phase 3
/// `http` / `fs` / `secrets` surfaces; every function self-checks its
/// capability at call time, so a plugin that didn't request a capability
/// gets a denied error code instead of an instantiation failure.
pub fn register(linker: &mut Linker<StoreData>) -> Result<(), wasmi::errors::LinkerError> {
    register_log(linker)?;
    register_notify(linker)?;
    register_storage(linker)?;
    register_query_read(linker)?;
    register_http(linker)?;
    register_fs(linker)?;
    register_secrets(linker)?;
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

/// HTTP timeout for plugin-issued requests — long enough for a real API
/// call, short enough that a hanging server doesn't stall the hook.
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Maximum response body a plugin can read back. Beyond this the request
/// errors so a tiny plugin can't be tricked into reading a 1 GB blob.
const HTTP_MAX_BODY_BYTES: usize = 1024 * 1024;
/// Maximum size of a file the plugin can read or write through the `fs`
/// capability.
const FS_MAX_FILE_BYTES: usize = 4 * 1024 * 1024;

fn register_http(linker: &mut Linker<StoreData>) -> Result<(), wasmi::errors::LinkerError> {
    linker
        .func_wrap(
            "env",
            "qoredb_http_request",
            |mut caller: Caller<'_, StoreData>,
             method_ptr: i32,
             method_len: i32,
             url_ptr: i32,
             url_len: i32,
             body_ptr: i32,
             body_len: i32|
             -> i64 {
                if !caller
                    .data()
                    .services
                    .consent
                    .contains(&CapabilityKind::Http)
                {
                    return 0;
                }
                let Some(method) = read_string(&mut caller, method_ptr, method_len) else {
                    return 0;
                };
                let Some(url) = read_string(&mut caller, url_ptr, url_len) else {
                    return 0;
                };
                let body = if body_len > 0 {
                    read_string(&mut caller, body_ptr, body_len)
                } else {
                    Some(String::new())
                };
                let Some(body) = body else {
                    return 0;
                };

                // Re-validate the URL against the manifest's allow-list,
                // *not* against whatever the consent record says — the
                // allow-list is the contract the user saw at install time.
                let parsed = match url::Url::parse(&url) {
                    Ok(u) => u,
                    Err(_) => return 0,
                };
                if !matches!(parsed.scheme(), "http" | "https") {
                    return 0;
                }
                let Some(host) = parsed.host_str() else {
                    return 0;
                };
                let allowed = caller
                    .data()
                    .services
                    .http_allowed_hosts
                    .iter()
                    .any(|h| h.eq_ignore_ascii_case(host));
                if !allowed {
                    return 0;
                }

                // Synchronous blocking client — wasmi is sync and lives on
                // `spawn_blocking` already, so an internal runtime is fine
                // for the modest call frequency a plugin sees.
                let client = match reqwest::blocking::Client::builder()
                    .timeout(HTTP_TIMEOUT)
                    .build()
                {
                    Ok(c) => c,
                    Err(_) => return 0,
                };
                let method_parsed = match reqwest::Method::from_bytes(method.as_bytes()) {
                    Ok(m) => m,
                    Err(_) => return 0,
                };
                let req = client
                    .request(method_parsed, parsed)
                    .body(body)
                    .send();
                let resp = match req {
                    Ok(r) => r,
                    Err(_) => return 0,
                };

                let status = resp.status().as_u16();
                let body_bytes = match resp.bytes() {
                    Ok(b) => b,
                    Err(_) => return 0,
                };
                if body_bytes.len() > HTTP_MAX_BODY_BYTES {
                    return 0;
                }
                let body_str = String::from_utf8_lossy(&body_bytes).into_owned();

                let payload = serde_json::json!({
                    "status": status,
                    "body": body_str,
                });
                let bytes = match serde_json::to_vec(&payload) {
                    Ok(b) => b,
                    Err(_) => return 0,
                };
                match write_into_guest(&mut caller, &bytes) {
                    Some((ptr, len)) => pack(ptr, len),
                    None => 0,
                }
            },
        )
        .map(|_| ())
}

fn register_fs(linker: &mut Linker<StoreData>) -> Result<(), wasmi::errors::LinkerError> {
    linker.func_wrap(
        "env",
        "qoredb_fs_read",
        |mut caller: Caller<'_, StoreData>, path_ptr: i32, path_len: i32| -> i64 {
            if !caller
                .data()
                .services
                .consent
                .contains(&CapabilityKind::Fs)
            {
                return 0;
            }
            let Some(path) = read_string(&mut caller, path_ptr, path_len) else {
                return 0;
            };
            let Some(full) = scoped_fs_path(caller.data(), &path) else {
                return 0;
            };
            let bytes = match std::fs::read(&full) {
                Ok(b) => b,
                Err(_) => return 0,
            };
            if bytes.len() > FS_MAX_FILE_BYTES {
                return 0;
            }
            match write_into_guest(&mut caller, &bytes) {
                Some((ptr, len)) => pack(ptr, len),
                None => 0,
            }
        },
    )?;

    linker.func_wrap(
        "env",
        "qoredb_fs_write",
        |mut caller: Caller<'_, StoreData>,
         path_ptr: i32,
         path_len: i32,
         data_ptr: i32,
         data_len: i32|
         -> i32 {
            if !caller
                .data()
                .services
                .consent
                .contains(&CapabilityKind::Fs)
            {
                return ERR_DENIED;
            }
            if data_len < 0 || (data_len as usize) > FS_MAX_FILE_BYTES {
                return ERR_QUOTA;
            }
            let Some(path) = read_string(&mut caller, path_ptr, path_len) else {
                return ERR_INVALID;
            };
            let Some(full) = scoped_fs_path(caller.data(), &path) else {
                return ERR_INVALID;
            };
            // Re-read the body now we know the path validates so we don't
            // shuffle bytes across memory if the path was bogus.
            let Some(data) = read_bytes(&mut caller, data_ptr, data_len) else {
                return ERR_INVALID;
            };
            if let Some(parent) = full.parent() {
                if std::fs::create_dir_all(parent).is_err() {
                    return ERR_INVALID;
                }
            }
            match std::fs::write(&full, data) {
                Ok(()) => OK,
                Err(_) => ERR_INVALID,
            }
        },
    )?;

    linker
        .func_wrap(
            "env",
            "qoredb_fs_delete",
            |mut caller: Caller<'_, StoreData>, path_ptr: i32, path_len: i32| -> i32 {
                if !caller
                    .data()
                    .services
                    .consent
                    .contains(&CapabilityKind::Fs)
                {
                    return ERR_DENIED;
                }
                let Some(path) = read_string(&mut caller, path_ptr, path_len) else {
                    return ERR_INVALID;
                };
                let Some(full) = scoped_fs_path(caller.data(), &path) else {
                    return ERR_INVALID;
                };
                if !full.exists() {
                    return OK;
                }
                match std::fs::remove_file(&full) {
                    Ok(()) => OK,
                    Err(_) => ERR_INVALID,
                }
            },
        )
        .map(|_| ())
}

fn register_secrets(linker: &mut Linker<StoreData>) -> Result<(), wasmi::errors::LinkerError> {
    linker
        .func_wrap(
            "env",
            "qoredb_secret_get",
            |mut caller: Caller<'_, StoreData>, name_ptr: i32, name_len: i32| -> i64 {
                if !caller
                    .data()
                    .services
                    .consent
                    .contains(&CapabilityKind::Secrets)
                {
                    return 0;
                }
                let Some(name) = read_string(&mut caller, name_ptr, name_len) else {
                    return 0;
                };
                // The plugin can only ask for secret names the manifest
                // declared. Anything else is rejected even if the consent
                // checkbox is on.
                if !caller
                    .data()
                    .services
                    .secret_names
                    .iter()
                    .any(|n| n == &name)
                {
                    return 0;
                }
                let plugin_id = caller.data().services.plugin_id.clone();
                let value = match crate::plugins::runtime::secrets::read(&plugin_id, &name) {
                    Some(v) => v,
                    None => return 0,
                };
                match write_into_guest(&mut caller, value.as_bytes()) {
                    Some((ptr, len)) => pack(ptr, len),
                    None => 0,
                }
            },
        )
        .map(|_| ())
}

/// Joins `requested` to the plugin's `fs_root` and rejects any path that
/// escapes the root via `..` segments or absolute components.
fn scoped_fs_path(data: &StoreData, requested: &str) -> Option<std::path::PathBuf> {
    let root = data.services.fs_root.as_ref()?;
    let requested_path = std::path::Path::new(requested);
    if requested_path.is_absolute() {
        return None;
    }
    if requested_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(root.join(requested_path))
}

/// Reads `len` bytes at `ptr` from the guest's memory. Unlike `read_string`
/// this does not validate UTF-8 — used for `fs_write` where the body may be
/// any byte payload.
fn read_bytes(caller: &mut Caller<'_, StoreData>, ptr: i32, len: i32) -> Option<Vec<u8>> {
    if len < 0 || ptr < 0 {
        return None;
    }
    let len = len as usize;
    if len > FS_MAX_FILE_BYTES {
        return None;
    }
    let memory = caller.get_export("memory").and_then(|e| e.into_memory())?;
    let mut buf = vec![0u8; len];
    memory.read(&*caller, ptr as usize, &mut buf).ok()?;
    Some(buf)
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

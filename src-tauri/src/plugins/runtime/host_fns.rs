// SPDX-License-Identifier: Apache-2.0

//! Host-function catalogue: the surface a WASM plugin sees.
//!
//! Every host function is unconditionally registered so a plugin can import
//! it regardless of its manifest — capability enforcement happens at call
//! time, not at instantiation. A revoked capability returns an error code;
//! the plugin keeps running.

use wasmi::{Caller, Linker};

use super::wasmi_host::StoreData;
use super::{CapabilityKind, LogEvent, NotifyEvent, NotifyLevel};

pub const OK: i32 = 0;
pub const ERR_DENIED: i32 = -1;
pub const ERR_INVALID: i32 = -2;
pub const ERR_QUOTA: i32 = -3;

const MAX_STRING_ARG: usize = 64 * 1024;

/// Capability check that doubles as audit trail: a refusal is logged so the
/// operator sees what the plugin tried.
fn has_capability(caller: &Caller<'_, StoreData>, kind: CapabilityKind) -> bool {
    if caller.data().services.consent.contains(&kind) {
        return true;
    }
    tracing::warn!(
        target: "plugins",
        plugin = %caller.data().services.plugin_id,
        capability = ?kind,
        "plugin attempted to use a capability it was not granted"
    );
    // Surface the refusal in the plugin's log too: a silently-denied
    // capability is the usual reason an enabled plugin appears to "do nothing".
    if let Some(log) = caller.data().services.log.clone() {
        let _ = log.send(LogEvent {
            plugin_id: caller.data().services.plugin_id.clone(),
            level: NotifyLevel::Warning,
            message: format!("capability '{}' denied — not granted", kind.as_str()),
        });
    }
    false
}

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
                if !has_capability(&caller, CapabilityKind::Log) {
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
                if let Some(sender) = caller.data().services.log.clone() {
                    let _ = sender.send(LogEvent {
                        plugin_id,
                        level: log_level(level),
                        message: msg,
                    });
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
                if !has_capability(&caller, CapabilityKind::Notify) {
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
                    code: None,
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
            if !has_capability(&caller, CapabilityKind::Storage) {
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
            if !has_capability(&caller, CapabilityKind::Storage) {
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
                if !has_capability(&caller, CapabilityKind::Storage) {
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

const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const HTTP_MAX_BODY_BYTES: usize = 1024 * 1024;
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
                if !has_capability(&caller, CapabilityKind::Http) {
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

                // Allow-list comes from the manifest, not the consent record
                // — that's the contract the user saw at install time.
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

                // SSRF guard: a name-based allowlist alone lets an attacker
                // who controls DNS for an allowed host pivot through the
                // plugin into the user's internal network.
                if !caller.data().services.http_allow_private_networks {
                    let port = parsed.port_or_known_default().unwrap_or(0);
                    let resolved: Vec<std::net::SocketAddr> =
                        match std::net::ToSocketAddrs::to_socket_addrs(&(host, port)) {
                            Ok(iter) => iter.collect(),
                            Err(_) => {
                                tracing::warn!(
                                    target: "plugins",
                                    plugin = %caller.data().services.plugin_id,
                                    host = %host,
                                    "DNS resolution failed for plugin HTTP request"
                                );
                                return 0;
                            }
                        };
                    if let Some(blocked) =
                        resolved.iter().map(|a| a.ip()).find(is_private_destination)
                    {
                        tracing::warn!(
                            target: "plugins",
                            plugin = %caller.data().services.plugin_id,
                            host = %host,
                            address = %blocked,
                            "blocked plugin HTTP request to private / loopback / metadata address"
                        );
                        return 0;
                    }
                }

                // Blocking client is fine: wasmi is sync and already runs
                // through `spawn_blocking`.
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
                let req = client.request(method_parsed, parsed).body(body).send();
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
            if !has_capability(&caller, CapabilityKind::Fs) {
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
            if !has_capability(&caller, CapabilityKind::Fs) {
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
            // Read the body only after the path validates: a bogus path
            // shouldn't trigger a large guest-memory copy.
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
                if !has_capability(&caller, CapabilityKind::Fs) {
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
                if !has_capability(&caller, CapabilityKind::Secrets) {
                    return 0;
                }
                let Some(name) = read_string(&mut caller, name_ptr, name_len) else {
                    return 0;
                };
                // Manifest-declared list, not consent: a tampered consent
                // file can't widen the set of readable secret names.
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

/// SSRF-target ranges: IPv4 loopback / RFC1918 / link-local / unspecified
/// / broadcast / multicast / CGNAT, and IPv6 loopback / ULA / link-local /
/// IPv4-mapped (inheriting the v4 verdict).
fn is_private_destination(ip: &std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                // 100.64.0.0/10 — CGNAT.
                || {
                    let o = v4.octets();
                    o[0] == 100 && (o[1] & 0b1100_0000) == 0b0100_0000
                }
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // fc00::/7 ULA
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // fe80::/10 link-local
                || v6.to_ipv4_mapped().is_some_and(|v4| is_private_destination(&IpAddr::V4(v4)))
        }
    }
}

/// Rejects absolute paths and any `..` components — the plugin must stay
/// under its `fs_root`.
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

/// Like `read_string` but without UTF-8 validation — for binary payloads
/// (`fs_write`).
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

/// Returns a packed `(ptr, len)` for the current query-result JSON, or 0
/// when the capability isn't granted, the hook isn't `postExecute`, or no
/// payload is available.
fn register_query_read(linker: &mut Linker<StoreData>) -> Result<(), wasmi::errors::LinkerError> {
    linker
        .func_wrap(
            "env",
            "qoredb_query_read",
            |mut caller: Caller<'_, StoreData>| -> i64 {
                if !has_capability(&caller, CapabilityKind::QueryRead) {
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

/// `qoredb_log` levels (0 debug, 1 info, 2 warn, 3 error) folded onto the
/// three severities the log view renders.
fn log_level(raw: i32) -> NotifyLevel {
    match raw {
        2 => NotifyLevel::Warning,
        3 => NotifyLevel::Error,
        _ => NotifyLevel::Info,
    }
}

/// Returns `None` on a bounds error, oversized input, or invalid UTF-8.
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

/// Allocates `bytes.len()` bytes in the guest via `qoredb_alloc`, writes
/// the payload there, and returns its `(ptr, len)`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn private_destinations_are_recognised() {
        assert!(is_private_destination(&v4(127, 0, 0, 1)));
        assert!(is_private_destination(&v4(10, 0, 0, 1)));
        assert!(is_private_destination(&v4(172, 16, 0, 1)));
        assert!(is_private_destination(&v4(192, 168, 1, 1)));
        assert!(is_private_destination(&v4(169, 254, 169, 254)));
        assert!(is_private_destination(&v4(100, 64, 0, 1)));
        assert!(is_private_destination(&v4(0, 0, 0, 0)));
        assert!(is_private_destination(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(is_private_destination(&IpAddr::V6(Ipv6Addr::new(
            0xfc00, 0, 0, 0, 0, 0, 0, 1
        ))));
        assert!(is_private_destination(&IpAddr::V6(Ipv6Addr::new(
            0xfe80, 0, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn public_destinations_are_allowed() {
        assert!(!is_private_destination(&v4(8, 8, 8, 8)));
        assert!(!is_private_destination(&v4(1, 1, 1, 1)));
        assert!(!is_private_destination(&v4(172, 32, 0, 1)));
        assert!(!is_private_destination(&v4(100, 128, 0, 1)));
        assert!(!is_private_destination(&IpAddr::V6(Ipv6Addr::new(
            0x2606, 0x4700, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn ipv4_mapped_ipv6_addresses_inherit_the_ipv4_verdict() {
        let mapped_loop = IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped());
        assert!(is_private_destination(&mapped_loop));
        let mapped_public = IpAddr::V6(Ipv4Addr::new(8, 8, 8, 8).to_ipv6_mapped());
        assert!(!is_private_destination(&mapped_public));
    }
}

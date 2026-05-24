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

/// Returns `true` if the plugin has been granted `kind`. A refusal is logged
/// at warn level so an attempt to use a non-granted capability leaves an
/// audit trail — a plugin can still receive `ERR_DENIED` silently from its
/// own perspective, but the host operator sees what happened.
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
    false
}

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

                // SSRF guard: refuse if any resolved IP is private / loopback
                // / link-local / cloud-metadata. The allowlist by name isn't
                // enough — an attacker who can influence DNS for
                // `api.example.com` could otherwise point it at
                // `169.254.169.254` and pivot through the plugin. Plugins
                // that legitimately need internal-network access flip
                // `allowPrivateNetworks: true` in their manifest.
                if !caller.data().services.http_allow_private_networks {
                    let port = parsed.port_or_known_default().unwrap_or(0);
                    let resolved: Vec<std::net::SocketAddr> = match std::net::ToSocketAddrs::to_socket_addrs(&(host, port)) {
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
                    if let Some(blocked) = resolved
                        .iter()
                        .map(|a| a.ip())
                        .find(is_private_destination)
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

/// Returns `true` if `ip` falls inside an address range a plugin's
/// outbound HTTP must not reach by default. Covers IPv4 loopback / private
/// RFC1918 / link-local / unspecified / broadcast / multicast, and the IPv6
/// equivalents plus ULA `fc00::/7` and link-local `fe80::/10`. These are
/// the ranges an SSRF pivot would target — host metadata services
/// (`169.254.169.254`), internal databases on `10.x`, the host itself on
/// `127.0.0.1`.
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
                // CGNAT 100.64.0.0/10 — assigned to ISP-internal NAT, not
                // user-facing; an external name should not resolve here.
                || {
                    let o = v4.octets();
                    o[0] == 100 && (o[1] & 0b1100_0000) == 0b0100_0000
                }
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // fc00::/7 — unique local addresses (the IPv6 equivalent of
                // RFC1918).
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // fe80::/10 — link-local.
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // IPv4-mapped IPv6 — apply the IPv4 rules to the embedded v4.
                || v6.to_ipv4_mapped().is_some_and(|v4| is_private_destination(&IpAddr::V4(v4)))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn private_destinations_are_recognised() {
        // Loopback, RFC1918, link-local, cloud metadata, CGNAT, IPv6 ULA,
        // IPv6 link-local, IPv6 loopback — all the ranges an SSRF pivot
        // would aim for.
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
        // Pinned literals for stability — these public ranges must not be
        // mistaken for private ones.
        assert!(!is_private_destination(&v4(8, 8, 8, 8)));
        assert!(!is_private_destination(&v4(1, 1, 1, 1)));
        assert!(!is_private_destination(&v4(172, 32, 0, 1))); // outside 172.16/12
        assert!(!is_private_destination(&v4(100, 128, 0, 1))); // first address after 100.64.0.0/10 CGNAT
        assert!(!is_private_destination(&IpAddr::V6(Ipv6Addr::new(
            0x2606, 0x4700, 0, 0, 0, 0, 0, 1
        )))); // Cloudflare IPv6
    }

    #[test]
    fn ipv4_mapped_ipv6_addresses_inherit_the_ipv4_verdict() {
        // ::ffff:127.0.0.1 must be treated like the v4 loopback it embeds.
        let mapped_loop = IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped());
        assert!(is_private_destination(&mapped_loop));
        let mapped_public = IpAddr::V6(Ipv4Addr::new(8, 8, 8, 8).to_ipv6_mapped());
        assert!(!is_private_destination(&mapped_public));
    }
}

// SPDX-License-Identifier: Apache-2.0

//! Per-plugin secrets, backed by the OS keyring.
//!
//! Each secret lives under a single service id, keyed by
//! `<plugin-id>::<secret-name>`. Writes ignore "no backend"-class errors so a
//! dev machine without a keyring still surfaces a clean error message rather
//! than a panic.

use keyring::Entry;

/// Keyring service id under which every plugin secret is stored. Distinct
/// from the main app vault's service id so a keyring browser shows them as
/// a separate group.
const SERVICE: &str = "qoredb-plugin-secrets";

fn key(plugin_id: &str, name: &str) -> String {
    format!("{plugin_id}::{name}")
}

/// Returns the secret's current value, or `None` if it isn't provisioned or
/// the keyring is unavailable.
pub fn read(plugin_id: &str, name: &str) -> Option<String> {
    let entry = Entry::new(SERVICE, &key(plugin_id, name)).ok()?;
    entry.get_password().ok()
}

/// Stores a secret value under the plugin's namespace. Overwrites any
/// existing value.
pub fn write(plugin_id: &str, name: &str, value: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, &key(plugin_id, name))
        .map_err(|e| format!("Could not open keyring entry: {e}"))?;
    entry
        .set_password(value)
        .map_err(|e| format!("Could not write secret: {e}"))
}

/// Drops a single secret. Silent if the entry doesn't exist.
pub fn delete(plugin_id: &str, name: &str) -> Result<(), String> {
    let entry = match Entry::new(SERVICE, &key(plugin_id, name)) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        // Don't propagate "no entry" — the caller's intent is already met.
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Could not delete secret: {e}")),
    }
}

/// Drops every secret a plugin holds. Best-effort: failure on one name
/// doesn't stop the rest.
pub fn forget_all(plugin_id: &str, names: &[String]) {
    for name in names {
        let _ = delete(plugin_id, name);
    }
}

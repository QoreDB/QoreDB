// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use keyring::Entry;
use parking_lot::Mutex;

use crate::engine::error::{EngineError, EngineResult};

/// Typed credential-storage error. Used by `has_credential` and
/// `delete_credential` so callers can distinguish "entry missing" from a real
/// failure without parsing error message substrings — the original code did
/// the latter and would silently mis-classify a future keyring wording change
/// as "no master password set" (cf. audit B5-C1 / B5-C2).
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("credential not found")]
    NotFound,
    #[error("access denied: {0}")]
    AccessDenied(String),
    #[error("credential backend error: {0}")]
    Other(String),
}

impl From<CredentialError> for EngineError {
    fn from(err: CredentialError) -> EngineError {
        match err {
            CredentialError::NotFound => EngineError::internal("Credentials not found"),
            CredentialError::AccessDenied(msg) => {
                EngineError::auth_failed(format!("Keyring access denied: {msg}"))
            }
            CredentialError::Other(msg) => {
                EngineError::internal(format!("Keyring error: {msg}"))
            }
        }
    }
}

/// Trait for credential storage backend
pub trait CredentialProvider: Send + Sync {
    fn set_password(&self, service: &str, username: &str, password: &str) -> EngineResult<()>;
    fn get_password(&self, service: &str, username: &str) -> EngineResult<String>;
    fn delete_password(&self, service: &str, username: &str) -> EngineResult<()>;

    /// Returns `true` iff an entry exists for `(service, username)`. Used by
    /// the vault lock to decide whether to prompt for a master password,
    /// without relying on substring matching of error messages.
    fn has_credential(&self, service: &str, username: &str) -> Result<bool, CredentialError>;

    /// Deletes an entry. Idempotent: `NotFound` is treated as success because
    /// the caller's intent ("ensure absence") is already satisfied. Any other
    /// failure is surfaced so we don't silently leave secrets in the keychain
    /// after `delete_connection` / `remove_master_password` (cf. B5-C2).
    fn delete_credential(&self, service: &str, username: &str) -> Result<(), CredentialError>;
}

/// Production implementation using OS Keyring
pub struct KeyringProvider;

impl KeyringProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for KeyringProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a `keyring::Error` into our typed [`CredentialError`]. Centralised so a
/// future wording change in the crate cannot silently degrade the classifier.
fn map_keyring_err(err: keyring::Error) -> CredentialError {
    match err {
        keyring::Error::NoEntry => CredentialError::NotFound,
        keyring::Error::PlatformFailure(e) => CredentialError::Other(e.to_string()),
        // `NoStorageAccess` is the typical macOS error when the user denies
        // Keychain access. We surface it distinctly so the UI can prompt.
        keyring::Error::NoStorageAccess(e) => CredentialError::AccessDenied(e.to_string()),
        other => CredentialError::Other(other.to_string()),
    }
}

impl CredentialProvider for KeyringProvider {
    fn set_password(&self, service: &str, username: &str, password: &str) -> EngineResult<()> {
        let entry = Entry::new(service, username)
            .map_err(|e| EngineError::internal(format!("Keyring error: {}", e)))?;
        entry
            .set_password(password)
            .map_err(|e| EngineError::internal(format!("Failed to set password: {}", e)))
    }

    fn get_password(&self, service: &str, username: &str) -> EngineResult<String> {
        let entry = Entry::new(service, username).map_err(map_keyring_err)?;
        entry.get_password().map_err(|e| {
            let err = map_keyring_err(e);
            // Preserve the historical message wording for callers that grep
            // logs, while still surfacing the typed error to programmatic
            // consumers via `has_credential`.
            if matches!(err, CredentialError::NotFound) {
                EngineError::internal("Credentials not found")
            } else {
                err.into()
            }
        })
    }

    fn delete_password(&self, service: &str, username: &str) -> EngineResult<()> {
        match self.delete_credential(service, username) {
            Ok(()) => Ok(()),
            Err(CredentialError::NotFound) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn has_credential(&self, service: &str, username: &str) -> Result<bool, CredentialError> {
        let entry = Entry::new(service, username).map_err(map_keyring_err)?;
        match entry.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(map_keyring_err(e)),
        }
    }

    fn delete_credential(&self, service: &str, username: &str) -> Result<(), CredentialError> {
        let entry = Entry::new(service, username).map_err(map_keyring_err)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Err(CredentialError::NotFound),
            Err(e) => Err(map_keyring_err(e)),
        }
    }
}

/// Mock implementation for testing
#[derive(Clone)]
pub struct MockProvider {
    storage: Arc<Mutex<HashMap<String, String>>>,
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn key(service: &str, username: &str) -> String {
        format!("{}::{}", service, username)
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialProvider for MockProvider {
    fn set_password(&self, service: &str, username: &str, password: &str) -> EngineResult<()> {
        let mut map = self.storage.lock();
        map.insert(Self::key(service, username), password.to_string());
        Ok(())
    }

    fn get_password(&self, service: &str, username: &str) -> EngineResult<String> {
        let map = self.storage.lock();
        map.get(&Self::key(service, username))
            .cloned()
            .ok_or_else(|| EngineError::internal("Credentials not found"))
    }

    fn delete_password(&self, service: &str, username: &str) -> EngineResult<()> {
        let mut map = self.storage.lock();
        map.remove(&Self::key(service, username));
        Ok(())
    }

    fn has_credential(&self, service: &str, username: &str) -> Result<bool, CredentialError> {
        Ok(self
            .storage
            .lock()
            .contains_key(&Self::key(service, username)))
    }

    fn delete_credential(&self, service: &str, username: &str) -> Result<(), CredentialError> {
        let mut map = self.storage.lock();
        if map.remove(&Self::key(service, username)).is_some() {
            Ok(())
        } else {
            Err(CredentialError::NotFound)
        }
    }
}

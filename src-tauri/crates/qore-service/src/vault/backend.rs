// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use keyring::Entry;
use parking_lot::Mutex;

use qore_core::error::{EngineError, EngineResult};

/// Typed credential-storage error. Used by `has_credential` and
/// `delete_credential` so callers can distinguish "entry missing" from a real
/// failure without parsing error message substrings — the original code did
/// the latter and would silently mis-classify a future keyring wording change
/// as "no master password set" (cf. audit B5-C1 / B5-C2).
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("credential not found")]
    NotFound,
    #[error("credential storage is temporarily unavailable: {0}")]
    TemporarilyUnavailable(String),
    #[error("access denied: {0}")]
    AccessDenied(String),
    #[error("credential backend error: {0}")]
    Other(String),
}

impl From<CredentialError> for EngineError {
    fn from(err: CredentialError) -> EngineError {
        match err {
            CredentialError::NotFound => EngineError::internal("Credentials not found"),
            CredentialError::TemporarilyUnavailable(msg) => EngineError::auth_failed(msg),
            CredentialError::AccessDenied(msg) => {
                EngineError::auth_failed(format!("Keyring access denied: {msg}"))
            }
            CredentialError::Other(msg) => EngineError::internal(format!("Keyring error: {msg}")),
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

/// Builds the credential provider for the current deployment. Returns the OS
/// keyring by default; when `QORE_VAULT_KEY` is set (headless/containerised
/// deployments where no keyring exists), returns the encrypted-file provider
/// instead. The file path is `QORE_VAULT_FILE` when set, otherwise
/// `app_data_dir()/vault.enc`, so every call site resolves to the same file.
pub fn default_provider() -> Box<dyn CredentialProvider> {
    let Ok(passphrase) = std::env::var("QORE_VAULT_KEY") else {
        return Box::new(KeyringProvider::new());
    };
    let path = std::env::var("QORE_VAULT_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| crate::paths::app_data_dir().join("vault.enc"));
    match crate::vault::encrypted_file::EncryptedFileProvider::new(path, &passphrase) {
        Ok(provider) => Box::new(provider),
        Err(e) => {
            tracing::error!(
                "Encrypted vault init failed ({e}); falling back to OS keyring. \
                 Set QORE_VAULT_KEY / QORE_VAULT_FILE correctly for headless use."
            );
            Box::new(KeyringProvider::new())
        }
    }
}

pub struct KeyringProvider;

type CredentialCache = HashMap<(String, String), String>;

/// Secrets that the user has already authorized during this process. Besides
/// avoiding repeated Keychain prompts, this insulates active sessions from a
/// macOS Keychain authentication state that can become stale after sleep or a
/// long idle period. The cache is process-local and disappears on exit.
fn credential_cache() -> &'static Mutex<CredentialCache> {
    static CACHE: OnceLock<Mutex<CredentialCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_key(service: &str, username: &str) -> (String, String) {
    (service.to_owned(), username.to_owned())
}

fn cached_password(service: &str, username: &str) -> Option<String> {
    credential_cache()
        .lock()
        .get(&(service.to_owned(), username.to_owned()))
        .cloned()
}

fn cache_password(service: &str, username: &str, password: &str) {
    credential_cache()
        .lock()
        .insert(cache_key(service, username), password.to_owned());
}

fn evict_password(service: &str, username: &str) {
    credential_cache()
        .lock()
        .remove(&cache_key(service, username));
}

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
        keyring::Error::PlatformFailure(e) => {
            #[cfg(target_os = "macos")]
            if e.downcast_ref::<security_framework::base::Error>()
                .is_some_and(|error| error.code() == -25293)
            {
                return CredentialError::TemporarilyUnavailable(
                    "macOS Keychain authentication is temporarily unavailable. Unlock your Mac or restart QoreDB, then try again. Your saved database credentials have not been rejected."
                        .to_string(),
                );
            }
            CredentialError::Other(e.to_string())
        }
        // `NoStorageAccess` is the typical macOS error when the user denies
        // Keychain access. We surface it distinctly so the UI can prompt.
        keyring::Error::NoStorageAccess(e) => CredentialError::AccessDenied(e.to_string()),
        other => CredentialError::Other(other.to_string()),
    }
}

impl CredentialProvider for KeyringProvider {
    fn set_password(&self, service: &str, username: &str, password: &str) -> EngineResult<()> {
        let entry = Entry::new(service, username).map_err(map_keyring_err)?;
        entry.set_password(password).map_err(map_keyring_err)?;
        cache_password(service, username, password);
        Ok(())
    }

    fn get_password(&self, service: &str, username: &str) -> EngineResult<String> {
        if let Some(password) = cached_password(service, username) {
            return Ok(password);
        }

        let entry = Entry::new(service, username).map_err(map_keyring_err)?;
        let password = entry.get_password().map_err(|e| {
            let err = map_keyring_err(e);
            // Preserve the historical message wording for callers that grep
            // logs, while still surfacing the typed error to programmatic
            // consumers via `has_credential`.
            if matches!(err, CredentialError::NotFound) {
                EngineError::internal("Credentials not found")
            } else {
                err.into()
            }
        })?;
        cache_password(service, username, &password);
        Ok(password)
    }

    fn delete_password(&self, service: &str, username: &str) -> EngineResult<()> {
        match self.delete_credential(service, username) {
            Ok(()) => Ok(()),
            Err(CredentialError::NotFound) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn has_credential(&self, service: &str, username: &str) -> Result<bool, CredentialError> {
        if cached_password(service, username).is_some() {
            return Ok(true);
        }

        let entry = Entry::new(service, username).map_err(map_keyring_err)?;
        match entry.get_password() {
            Ok(password) => {
                cache_password(service, username, &password);
                Ok(true)
            }
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(map_keyring_err(e)),
        }
    }

    fn delete_credential(&self, service: &str, username: &str) -> Result<(), CredentialError> {
        let entry = Entry::new(service, username).map_err(map_keyring_err)?;
        match entry.delete_credential() {
            Ok(()) => {
                evict_password(service, username);
                Ok(())
            }
            Err(keyring::Error::NoEntry) => {
                evict_password(service, username);
                Err(CredentialError::NotFound)
            }
            Err(e) => Err(map_keyring_err(e)),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyring_cache_is_shared_between_provider_instances() {
        let suffix = uuid::Uuid::new_v4().to_string();
        let service = format!("qoredb-test-{suffix}");
        let username = "credentials";

        cache_password(&service, username, "secret");

        let provider = KeyringProvider::new();
        assert_eq!(provider.get_password(&service, username).unwrap(), "secret");

        evict_password(&service, username);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_auth_failure_is_reported_as_temporarily_unavailable() {
        let platform_error = security_framework::base::Error::from_code(-25293);
        let error = map_keyring_err(keyring::Error::PlatformFailure(Box::new(platform_error)));

        assert!(matches!(error, CredentialError::TemporarilyUnavailable(_)));
    }
}

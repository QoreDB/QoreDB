use crate::engine::error::{EngineError, EngineResult};
use keyring::Entry;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Trait for credential storage backend
pub trait CredentialProvider: Send + Sync {
    fn set_password(&self, service: &str, username: &str, password: &str) -> EngineResult<()>;
    fn get_password(&self, service: &str, username: &str) -> EngineResult<String>;
    fn delete_password(&self, service: &str, username: &str) -> EngineResult<()>;
}

/// Production implementation using OS Keyring
pub struct KeyringProvider;

impl KeyringProvider {
    pub fn new() -> Self {
        Self
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
        let entry = Entry::new(service, username)
            .map_err(|e| EngineError::internal(format!("Keyring error: {}", e)))?;
        match entry.get_password() {
            Ok(pwd) => Ok(pwd),
            Err(keyring::Error::NoEntry) => Err(EngineError::internal("Credentials not found")),
            Err(e) => Err(EngineError::internal(format!("Failed to get password: {}", e))),
        }
    }

    fn delete_password(&self, service: &str, username: &str) -> EngineResult<()> {
        // If entry doesn't exist, it's fine
        if let Ok(entry) = Entry::new(service, username) {
            let _ = entry.delete_credential();
        }
        Ok(())
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

impl CredentialProvider for MockProvider {
    fn set_password(&self, service: &str, username: &str, password: &str) -> EngineResult<()> {
        let mut map = self.storage.lock().unwrap();
        map.insert(Self::key(service, username), password.to_string());
        Ok(())
    }

    fn get_password(&self, service: &str, username: &str) -> EngineResult<String> {
        let map = self.storage.lock().unwrap();
        map.get(&Self::key(service, username))
            .cloned()
            .ok_or_else(|| EngineError::internal("Credentials not found"))
    }

    fn delete_password(&self, service: &str, username: &str) -> EngineResult<()> {
        let mut map = self.storage.lock().unwrap();
        map.remove(&Self::key(service, username));
        Ok(())
    }
}

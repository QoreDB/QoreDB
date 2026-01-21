//! Vault Lock
//!
//! Master password protection for the vault at startup.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

use crate::engine::error::{EngineError, EngineResult};
use crate::vault::backend::CredentialProvider;

const SERVICE_NAME: &str = "qoredb";
const MASTER_PASSWORD_KEY: &str = "__master_password_hash__";

/// Manages vault locking with master password
pub struct VaultLock {
    is_unlocked: bool,
    provider: Box<dyn CredentialProvider>,
}

impl VaultLock {
    pub fn new(provider: Box<dyn CredentialProvider>) -> Self {
        Self { 
            is_unlocked: false,
            provider,
        }
    }

    fn master_key_params(&self) -> (String, String) {
        let service = std::env::var("QOREDB_VAULT_SERVICE").unwrap_or_else(|_| SERVICE_NAME.to_string());
        let key = std::env::var("QOREDB_VAULT_MASTER_KEY").unwrap_or_else(|_| MASTER_PASSWORD_KEY.to_string());
        (service, key)
    }

    /// Checks if a master password has been set
    pub fn has_master_password(&self) -> EngineResult<bool> {
        let (service, key) = self.master_key_params();
        
        match self.provider.get_password(&service, &key) {
            Ok(_) => Ok(true),
            Err(e) if e.to_string().contains("not found") => Ok(false),
            Err(e) if e.to_string().contains("NoEntry") => Ok(false),
            Err(e) if e.to_string().contains("internal") => {
                 // Check if it's "Credentials not found" which is what MockProvider/Backend returns
                 if e.to_string().contains("Credentials not found") {
                     return Ok(false);
                 }
                 Err(e)
            },
            Err(e) => Err(e),
        }
    }

    /// Sets up a new master password
    pub fn setup_master_password(&mut self, password: &str) -> EngineResult<()> {
        // Hash the password with Argon2
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        
        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| EngineError::internal(format!("Hashing error: {}", e)))?
            .to_string();

        // Store the hash in keyring
        let (service, key) = self.master_key_params();

        self.provider.set_password(&service, &key, &hash)
             .map_err(|e| EngineError::internal(format!("Failed to store master password: {}", e)))?;

        self.is_unlocked = true;
        Ok(())
    }

    /// Attempts to unlock the vault with the given password
    pub fn unlock(&mut self, password: &str) -> EngineResult<bool> {
        let (service, key) = self.master_key_params();

        let stored_hash = self.provider.get_password(&service, &key)
             .map_err(|e| EngineError::internal(format!("No master password set: {}", e)))?;

        let parsed_hash = PasswordHash::new(&stored_hash)
            .map_err(|e| EngineError::internal(format!("Invalid stored hash: {}", e)))?;

        let argon2 = Argon2::default();
        
        if argon2.verify_password(password.as_bytes(), &parsed_hash).is_ok() {
            self.is_unlocked = true;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Locks the vault
    pub fn lock(&mut self) {
        self.is_unlocked = false;
    }

    /// Checks if the vault is currently unlocked
    pub fn is_locked(&self) -> bool {
        !self.is_unlocked
    }

    /// Checks if the vault is currently unlocked
    pub fn is_unlocked(&self) -> bool {
        self.is_unlocked
    }

    /// Removes the master password (requires current password)
    pub fn remove_master_password(&mut self, password: &str) -> EngineResult<()> {
        // Verify current password first
        if !self.unlock(password)? {
            return Err(EngineError::auth_failed("Invalid password"));
        }

        let (service, key) = self.master_key_params();

        self.provider.delete_password(&service, &key)
            .map_err(|e| EngineError::internal(format!("Failed to delete: {}", e)))?;

        self.is_unlocked = true; // No password = always unlocked
        Ok(())
    }

    /// Auto-unlocks if no master password is set
    pub fn auto_unlock_if_no_password(&mut self) -> EngineResult<()> {
        if !self.has_master_password()? {
            self.is_unlocked = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use crate::vault::backend::MockProvider;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn master_password_roundtrip() -> EngineResult<()> {
        let _guard = env_lock().lock().expect("env lock poisoned");
        // Use MockProvider for testing
        let mut lock = VaultLock::new(Box::new(MockProvider::new()));
        
        assert!(!lock.has_master_password()?);

        lock.setup_master_password("secret")?;
        assert!(lock.has_master_password()?);

        lock.lock();
        assert!(lock.is_locked());
        assert!(!lock.unlock("wrong")?);
        assert!(lock.is_locked());
        assert!(lock.unlock("secret")?);
        assert!(lock.is_unlocked());

        lock.remove_master_password("secret")?;
        assert!(!lock.has_master_password()?);

        lock.lock();
        lock.auto_unlock_if_no_password()?;
        assert!(lock.is_unlocked());

        Ok(())
    }
}

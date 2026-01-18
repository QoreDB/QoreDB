//! Vault Lock
//!
//! Master password protection for the vault at startup.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use keyring::Entry;

use crate::engine::error::{EngineError, EngineResult};

const SERVICE_NAME: &str = "qoredb";
const MASTER_PASSWORD_KEY: &str = "__master_password_hash__";

fn master_entry() -> EngineResult<Entry> {
    let service = std::env::var("QOREDB_VAULT_SERVICE").unwrap_or_else(|_| SERVICE_NAME.to_string());
    let key =
        std::env::var("QOREDB_VAULT_MASTER_KEY").unwrap_or_else(|_| MASTER_PASSWORD_KEY.to_string());
    Entry::new(&service, &key)
        .map_err(|e| EngineError::internal(format!("Keyring error: {}", e)))
}

/// Manages vault locking with master password
pub struct VaultLock {
    is_unlocked: bool,
}

impl VaultLock {
    pub fn new() -> Self {
        Self { is_unlocked: false }
    }

    /// Checks if a master password has been set
    pub fn has_master_password() -> EngineResult<bool> {
        let entry = master_entry()?;

        match entry.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(EngineError::internal(format!("Keyring error: {}", e))),
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
        let entry = master_entry()?;

        entry
            .set_password(&hash)
            .map_err(|e| EngineError::internal(format!("Failed to store master password: {}", e)))?;

        self.is_unlocked = true;
        Ok(())
    }

    /// Attempts to unlock the vault with the given password
    pub fn unlock(&mut self, password: &str) -> EngineResult<bool> {
        let entry = master_entry()?;

        let stored_hash = entry
            .get_password()
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

        let entry = master_entry()?;

        entry
            .delete_credential()
            .map_err(|e| EngineError::internal(format!("Failed to delete: {}", e)))?;

        self.is_unlocked = true; // No password = always unlocked
        Ok(())
    }

    /// Auto-unlocks if no master password is set
    pub fn auto_unlock_if_no_password(&mut self) -> EngineResult<()> {
        if !Self::has_master_password()? {
            self.is_unlocked = true;
        }
        Ok(())
    }
}

impl Default for VaultLock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use uuid::Uuid;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn cleanup(service: &str, key: &str) {
        if let Ok(entry) = Entry::new(service, key) {
            let _ = entry.delete_credential();
        }
    }

    #[test]
    fn master_password_roundtrip() -> EngineResult<()> {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let service = format!("qoredb_test_{}", Uuid::new_v4().simple());
        let key = format!("__master_password_hash__{}", Uuid::new_v4().simple());

        std::env::set_var("QOREDB_VAULT_SERVICE", &service);
        std::env::set_var("QOREDB_VAULT_MASTER_KEY", &key);
        cleanup(&service, &key);

        let mut lock = VaultLock::new();
        assert!(!VaultLock::has_master_password()?);

        lock.setup_master_password("secret")?;
        assert!(VaultLock::has_master_password()?);

        lock.lock();
        assert!(lock.is_locked());
        assert!(!lock.unlock("wrong")?);
        assert!(lock.is_locked());
        assert!(lock.unlock("secret")?);
        assert!(lock.is_unlocked());

        lock.remove_master_password("secret")?;
        assert!(!VaultLock::has_master_password()?);

        lock.lock();
        lock.auto_unlock_if_no_password()?;
        assert!(lock.is_unlocked());

        cleanup(&service, &key);
        std::env::remove_var("QOREDB_VAULT_SERVICE");
        std::env::remove_var("QOREDB_VAULT_MASTER_KEY");

        Ok(())
    }
}

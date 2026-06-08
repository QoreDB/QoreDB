// SPDX-License-Identifier: Apache-2.0

//! Vault Lock
//!
//! Master password protection for the vault at startup.

use std::time::{Duration, Instant};

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};

use crate::vault::backend::CredentialProvider;
use qore_core::error::{EngineError, EngineResult};

const SERVICE_NAME: &str = "qoredb";
const MASTER_PASSWORD_KEY: &str = "__master_password_hash__";

/// Minimum acceptable master password length. Below this, brute-force on a
/// stolen Argon2 hash is realistic even with hardened params. Picked to align
/// with NIST 800-63B "memorized secret" guidance (min 8) with a small headroom.
const MIN_PASSWORD_LEN: usize = 12;

/// Maximum number of consecutive failed unlocks tracked before each new
/// attempt sleeps. The intent is to slow brute-force from the IPC surface
/// (`unlock_vault` Tauri command), not to permanently lock out a forgetful
/// user — the sleep resets on successful unlock.
const MAX_TRACKED_FAILURES: u32 = 8;

/// Idle window after a successful unlock during which "sensitive" reads
/// (currently `get_connection_credentials`) are allowed without a fresh
/// unlock prompt. Beyond this, callers must re-authenticate (B6-H3).
const REAUTH_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// OWASP 2024 baseline Argon2id parameters (m=64 MiB, t=3, p=1). The previous
/// `Argon2::default()` exposed m=19 MiB / t=2 which is below the modern
/// threshold (cf. audit B5-H1). Stored hashes encode the params they were
/// computed with, so verifying an old hash still works — only new
/// `setup_master_password` calls get the harder profile.
fn hardened_argon2() -> Argon2<'static> {
    let params = Params::new(
        64 * 1024, // m_cost in KiB → 64 MiB
        3,         // t_cost (iterations)
        1,         // p_cost (parallelism)
        None,
    )
    .expect("hardened Argon2 params are valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Manages vault locking with master password
pub struct VaultLock {
    is_unlocked: bool,
    provider: Box<dyn CredentialProvider>,
    /// Last time the vault transitioned to unlocked. Sensitive IPC paths
    /// (e.g. `get_connection_credentials`) consult this for re-auth (B6-H3).
    last_unlocked_at: Option<Instant>,
    /// Count of consecutive failed unlocks since the last success. Used to
    /// rate-limit brute force on the IPC `unlock_vault` command (B5-H3).
    consecutive_failures: u32,
    /// Instant of the last failed unlock; combined with `consecutive_failures`
    /// to compute how long the next attempt must wait.
    last_failure_at: Option<Instant>,
}

impl VaultLock {
    pub fn new(provider: Box<dyn CredentialProvider>) -> Self {
        Self {
            is_unlocked: false,
            provider,
            last_unlocked_at: None,
            consecutive_failures: 0,
            last_failure_at: None,
        }
    }

    fn master_key_params(&self) -> (String, String) {
        let service =
            std::env::var("QOREDB_VAULT_SERVICE").unwrap_or_else(|_| SERVICE_NAME.to_string());
        let key = std::env::var("QOREDB_VAULT_MASTER_KEY")
            .unwrap_or_else(|_| MASTER_PASSWORD_KEY.to_string());
        (service, key)
    }

    /// Checks if a master password has been set. Uses the typed
    /// `has_credential` API so a future wording change in the keyring crate
    /// can't silently flip the answer (cf. audit B5-C1).
    pub fn has_master_password(&self) -> EngineResult<bool> {
        let (service, key) = self.master_key_params();
        self.provider
            .has_credential(&service, &key)
            .map_err(EngineError::from)
    }

    /// Sets up a new master password. Enforces a minimum length so callers
    /// cannot store a one-char or empty master password (B6-H2). The hash is
    /// computed with the OWASP-2024 Argon2id profile (B5-H1).
    pub fn setup_master_password(&mut self, password: &str) -> EngineResult<()> {
        validate_password_strength(password)?;

        let salt = SaltString::generate(&mut OsRng);
        let argon2 = hardened_argon2();

        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| EngineError::internal(format!("Hashing error: {}", e)))?
            .to_string();

        // Store the hash
        let (service, key) = self.master_key_params();

        self.provider
            .set_password(&service, &key, &hash)
            .map_err(|e| {
                EngineError::internal(format!("Failed to store master password: {}", e))
            })?;

        self.mark_unlocked();
        Ok(())
    }

    /// Attempts to unlock the vault with the given password.
    ///
    /// Rate-limited via exponential sleep based on the recent failure count
    /// (B5-H3 / B6-H1). The verifier itself uses the params encoded in the
    /// stored PHC string, so legacy hashes still validate.
    pub async fn unlock(&mut self, password: &str) -> EngineResult<bool> {
        // Sleep first if we're inside the back-off window.
        if let Some(delay) = self.current_backoff() {
            tokio::time::sleep(delay).await;
        }

        let (service, key) = self.master_key_params();

        let stored_hash = self
            .provider
            .get_password(&service, &key)
            .map_err(|e| EngineError::internal(format!("No master password set: {}", e)))?;

        let parsed_hash = PasswordHash::new(&stored_hash)
            .map_err(|e| EngineError::internal(format!("Invalid stored hash: {}", e)))?;

        // `verify_password` uses the params stored in the PHC string, not the
        // current hardened profile, so old hashes still verify.
        let argon2 = Argon2::default();

        if argon2
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
        {
            self.mark_unlocked();
            self.consecutive_failures = 0;
            self.last_failure_at = None;
            Ok(true)
        } else {
            self.record_failure();
            Ok(false)
        }
    }

    /// Locks the vault
    pub fn lock(&mut self) {
        self.is_unlocked = false;
        self.last_unlocked_at = None;
    }

    /// Checks if the vault is currently unlocked
    pub fn is_locked(&self) -> bool {
        !self.is_unlocked
    }

    /// Checks if the vault is currently unlocked
    pub fn is_unlocked(&self) -> bool {
        self.is_unlocked
    }

    /// `true` while the vault is unlocked **and** the last successful unlock
    /// is within [`REAUTH_IDLE_TIMEOUT`]. Used by sensitive IPC commands to
    /// require a fresh unlock after a stale session (B6-H3).
    pub fn is_fresh_authentication(&self) -> bool {
        if !self.is_unlocked {
            return false;
        }
        match self.last_unlocked_at {
            Some(t) => t.elapsed() < REAUTH_IDLE_TIMEOUT,
            None => false,
        }
    }

    /// Removes the master password (requires current password)
    pub async fn remove_master_password(&mut self, password: &str) -> EngineResult<()> {
        // Verify current password first
        if !self.unlock(password).await? {
            return Err(EngineError::auth_failed("Invalid password"));
        }

        let (service, key) = self.master_key_params();

        self.provider
            .delete_password(&service, &key)
            .map_err(|e| EngineError::internal(format!("Failed to delete: {}", e)))?;

        // No password = always unlocked; reset the freshness window so the
        // re-auth check doesn't immediately reject the next sensitive call.
        self.mark_unlocked();
        Ok(())
    }

    /// Auto-unlocks the vault when no master password is set. This is a
    /// deliberate UX trade-off (no prompt on first launch) but it means a
    /// fresh install ships with credentials decryptable by anyone with
    /// session-level access to the OS account. We `tracing::warn!` so the
    /// state is at least observable in logs — the proper fix is an
    /// onboarding flow forcing master-password setup (cf. audit B5-C3,
    /// tracked separately).
    pub fn auto_unlock_if_no_password(&mut self) -> EngineResult<()> {
        if !self.has_master_password()? {
            tracing::warn!(
                "Vault auto-unlocked because no master password is configured. \
                 Credentials are stored in the OS keyring but the in-memory \
                 vault is open by default — set a master password to require \
                 unlock at startup."
            );
            self.mark_unlocked();
        }
        Ok(())
    }

    fn mark_unlocked(&mut self) {
        self.is_unlocked = true;
        self.last_unlocked_at = Some(Instant::now());
    }

    fn record_failure(&mut self) {
        self.consecutive_failures = self
            .consecutive_failures
            .saturating_add(1)
            .min(MAX_TRACKED_FAILURES);
        self.last_failure_at = Some(Instant::now());
    }

    /// Exponential back-off computed from the recent failure count. Returns
    /// `None` when the caller can proceed immediately. The schedule starts at
    /// 250 ms after the first failure, doubles on each retry, and caps at
    /// 30 s so a forgetful user isn't permanently locked out.
    fn current_backoff(&self) -> Option<Duration> {
        if self.consecutive_failures == 0 {
            return None;
        }
        let exp = (self.consecutive_failures - 1).min(7); // 250ms .. 32s
        let delay = Duration::from_millis(250u64.saturating_mul(1u64 << exp));
        let capped = delay.min(Duration::from_secs(30));

        match self.last_failure_at {
            Some(t) if t.elapsed() < capped => Some(capped - t.elapsed()),
            _ => None,
        }
    }
}

fn validate_password_strength(password: &str) -> EngineResult<()> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(EngineError::validation(format!(
            "Master password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    // At least one digit and one non-alphanumeric character so a long string
    // of lowercase letters ("aaaaaaaaaaaa") doesn't pass — cheap entropy
    // guardrail without enforcing a full strength meter (B6-H2).
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_symbol = password.chars().any(|c| !c.is_alphanumeric());
    if !(has_digit && has_symbol) {
        return Err(EngineError::validation(
            "Master password must include at least one digit and one symbol".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::backend::MockProvider;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    const STRONG_PASSWORD: &str = "Hunter2-master!";

    #[tokio::test]
    async fn master_password_roundtrip() -> EngineResult<()> {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let mut lock = VaultLock::new(Box::new(MockProvider::new()));

        assert!(!lock.has_master_password()?);

        lock.setup_master_password(STRONG_PASSWORD)?;
        assert!(lock.has_master_password()?);

        lock.lock();
        assert!(lock.is_locked());
        assert!(!lock.unlock("wrong-but-long-1!").await?);
        assert!(lock.is_locked());
        assert!(lock.unlock(STRONG_PASSWORD).await?);
        assert!(lock.is_unlocked());

        lock.remove_master_password(STRONG_PASSWORD).await?;
        assert!(!lock.has_master_password()?);

        lock.lock();
        lock.auto_unlock_if_no_password()?;
        assert!(lock.is_unlocked());

        Ok(())
    }

    #[test]
    fn setup_rejects_short_password() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let mut lock = VaultLock::new(Box::new(MockProvider::new()));
        let err = lock.setup_master_password("Aa1!").unwrap_err();
        assert!(err.to_string().contains("at least"));
    }

    #[test]
    fn setup_rejects_letters_only_password() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let mut lock = VaultLock::new(Box::new(MockProvider::new()));
        let err = lock
            .setup_master_password("aaaaaaaaaaaa") // 12 chars, all letters
            .unwrap_err();
        assert!(err.to_string().contains("digit"));
    }

    #[tokio::test]
    async fn unlock_back_off_increases_with_failures() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let mut lock = VaultLock::new(Box::new(MockProvider::new()));
        lock.setup_master_password(STRONG_PASSWORD).unwrap();
        lock.lock();

        // First two failed unlocks should be observable as back-off > 0.
        assert!(!lock.unlock("wrong-attempt-1!").await.unwrap());
        assert!(!lock.unlock("wrong-attempt-2!").await.unwrap());
        let delay = lock.current_backoff();
        assert!(delay.is_some(), "expected back-off after failures");

        // A successful unlock clears the back-off window.
        assert!(lock.unlock(STRONG_PASSWORD).await.unwrap());
        assert_eq!(lock.consecutive_failures, 0);
        assert!(lock.current_backoff().is_none());
    }

    #[tokio::test]
    async fn fresh_authentication_window() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let mut lock = VaultLock::new(Box::new(MockProvider::new()));
        lock.setup_master_password(STRONG_PASSWORD).unwrap();
        assert!(lock.is_fresh_authentication());
        lock.lock();
        assert!(!lock.is_fresh_authentication());
    }
}

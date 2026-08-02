// SPDX-License-Identifier: Apache-2.0

pub mod key;
pub mod status;

use crate::vault::backend::CredentialProvider;
use key::{LicenseError, decode_license, verify_license};
use status::{LicenseStatus, LicenseTier};

const LICENSE_SERVICE: &str = "com.qoredb.license";
const LICENSE_USERNAME: &str = "license_key";

pub struct LicenseManager {
    provider: Box<dyn CredentialProvider>,
    cached_status: LicenseStatus,
    storage_loaded: bool,
    /// Dev-only: override the effective tier without a real license key.
    /// Only compiled in debug builds — cannot exist in release binaries.
    #[cfg(debug_assertions)]
    dev_override_tier: Option<LicenseTier>,
}

impl LicenseManager {
    pub fn new(provider: Box<dyn CredentialProvider>) -> Self {
        let mut manager = Self {
            provider,
            cached_status: LicenseStatus::default(),
            storage_loaded: false,
            #[cfg(debug_assertions)]
            dev_override_tier: None,
        };
        if let Err(error) = manager.load_status() {
            tracing::warn!("Failed to load the stored license: {error}");
        }
        manager
    }

    /// Returns the effective license status.
    /// In debug builds, the dev override tier takes precedence if set.
    pub fn status(&self) -> &LicenseStatus {
        &self.cached_status
    }

    /// Returns the effective status, applying dev override if set (debug builds only).
    pub fn effective_status(&self) -> LicenseStatus {
        #[cfg(debug_assertions)]
        if let Some(tier) = self.dev_override_tier {
            return LicenseStatus {
                tier,
                email: Some("dev@qoredb.local".to_string()),
                payment_id: None,
                issued_at: None,
                expires_at: None,
                is_expired: false,
                seats: None,
                is_founder: false,
            };
        }
        self.cached_status.clone()
    }

    pub fn load_status(&mut self) -> Result<LicenseStatus, LicenseError> {
        if !self.storage_loaded {
            self.refresh_status()?;
        }
        Ok(self.effective_status())
    }

    /// Dev-only: set a tier override. Pass None to clear.
    #[cfg(debug_assertions)]
    pub fn set_dev_override(&mut self, tier: Option<LicenseTier>) {
        self.dev_override_tier = tier;
    }

    /// Dev-only: get current override tier.
    #[cfg(debug_assertions)]
    pub fn dev_override(&self) -> Option<LicenseTier> {
        self.dev_override_tier
    }

    /// Validates the key, persists it in the keyring, and updates the cached status.
    pub fn activate(&mut self, license_key: &str) -> Result<LicenseStatus, LicenseError> {
        let payload = verify_license(license_key)?;

        self.provider
            .set_password(LICENSE_SERVICE, LICENSE_USERNAME, license_key)
            .map_err(|e| LicenseError::Storage(e.to_string()))?;

        let status = LicenseStatus {
            tier: payload.tier,
            email: Some(payload.email),
            payment_id: Some(payload.payment_id),
            issued_at: Some(payload.issued_at),
            expires_at: payload.expires_at,
            is_expired: false,
            seats: payload.seats,
            is_founder: payload.is_founder,
        };
        self.cached_status = status.clone();
        self.storage_loaded = true;
        Ok(status)
    }

    /// Removes the stored key and resets to Core tier.
    pub fn deactivate(&mut self) -> Result<(), LicenseError> {
        self.provider
            .delete_password(LICENSE_SERVICE, LICENSE_USERNAME)
            .map_err(|e| LicenseError::Storage(e.to_string()))?;
        self.cached_status = LicenseStatus::default();
        self.storage_loaded = true;
        Ok(())
    }

    fn refresh_status(&mut self) -> Result<(), LicenseError> {
        let has_stored_key = self
            .provider
            .has_credential(LICENSE_SERVICE, LICENSE_USERNAME)
            .map_err(|error| LicenseError::Storage(error.to_string()))?;

        if !has_stored_key {
            self.cached_status = LicenseStatus::default();
            self.storage_loaded = true;
            return Ok(());
        }

        let stored_key = self
            .provider
            .get_password(LICENSE_SERVICE, LICENSE_USERNAME)
            .map_err(|error| LicenseError::Storage(error.to_string()))?;

        match verify_license(&stored_key) {
            Ok(payload) => {
                self.cached_status = LicenseStatus {
                    tier: payload.tier,
                    email: Some(payload.email),
                    payment_id: Some(payload.payment_id),
                    issued_at: Some(payload.issued_at),
                    expires_at: payload.expires_at,
                    is_expired: false,
                    seats: payload.seats,
                    is_founder: payload.is_founder,
                };
                self.storage_loaded = true;
            }
            Err(LicenseError::Expired) => {
                // Expose payload metadata for the UI while forcing the tier
                // back to Core so gated features remain locked.
                if let Ok(payload) = decode_license(&stored_key) {
                    self.cached_status = LicenseStatus {
                        tier: LicenseTier::Core,
                        email: Some(payload.email),
                        payment_id: Some(payload.payment_id),
                        issued_at: Some(payload.issued_at),
                        expires_at: payload.expires_at,
                        is_expired: true,
                        seats: payload.seats,
                        is_founder: payload.is_founder,
                    };
                    self.storage_loaded = true;
                }
            }
            Err(_) => {
                let _ = self
                    .provider
                    .delete_password(LICENSE_SERVICE, LICENSE_USERNAME);
                self.cached_status = LicenseStatus::default();
                self.storage_loaded = true;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::backend::{CredentialError, MockProvider};
    use qore_core::error::{EngineError, EngineResult};

    fn make_manager() -> LicenseManager {
        LicenseManager::new(Box::new(MockProvider::new()))
    }

    #[test]
    fn default_status_is_core() {
        let mgr = make_manager();
        assert_eq!(mgr.status().tier, LicenseTier::Core);
        assert!(!mgr.status().is_expired);
        assert!(mgr.status().email.is_none());
    }

    #[test]
    fn activate_with_invalid_key_fails() {
        let mut mgr = make_manager();
        let result = mgr.activate("garbage-key");
        assert!(result.is_err());
        assert_eq!(mgr.status().tier, LicenseTier::Core);
    }

    #[test]
    fn deactivate_resets_to_core() {
        let mut mgr = make_manager();
        assert!(mgr.deactivate().is_ok());
        assert_eq!(mgr.status().tier, LicenseTier::Core);
    }

    struct UnavailableProvider;

    impl CredentialProvider for UnavailableProvider {
        fn set_password(&self, _: &str, _: &str, _: &str) -> EngineResult<()> {
            Err(EngineError::internal("unavailable"))
        }

        fn get_password(&self, _: &str, _: &str) -> EngineResult<String> {
            Err(EngineError::internal("unavailable"))
        }

        fn delete_password(&self, _: &str, _: &str) -> EngineResult<()> {
            Err(EngineError::internal("unavailable"))
        }

        fn has_credential(&self, _: &str, _: &str) -> Result<bool, CredentialError> {
            Err(CredentialError::TemporarilyUnavailable(
                "test keyring outage".to_string(),
            ))
        }

        fn delete_credential(&self, _: &str, _: &str) -> Result<(), CredentialError> {
            Err(CredentialError::TemporarilyUnavailable(
                "test keyring outage".to_string(),
            ))
        }
    }

    #[test]
    fn storage_failure_is_not_reported_as_core_license() {
        let mut manager = LicenseManager::new(Box::new(UnavailableProvider));

        let result = manager.load_status();

        assert!(matches!(result, Err(LicenseError::Storage(_))));
        assert!(!manager.storage_loaded);
    }

    // NOTE: Full activate/deactivate roundtrip tests require the production
    // public key to be set, or use verify_license_with_key directly.
    // See key.rs tests for signature verification coverage.
}

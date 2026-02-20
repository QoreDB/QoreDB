// SPDX-License-Identifier: Apache-2.0

pub mod key;
pub mod status;

use crate::vault::backend::CredentialProvider;
use key::{decode_license, verify_license, LicenseError};
use status::{LicenseStatus, LicenseTier};

const LICENSE_SERVICE: &str = "com.qoredb.license";
const LICENSE_USERNAME: &str = "license_key";

pub struct LicenseManager {
    provider: Box<dyn CredentialProvider>,
    cached_status: LicenseStatus,
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
            #[cfg(debug_assertions)]
            dev_override_tier: None,
        };
        manager.refresh_status();
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
            };
        }
        self.cached_status.clone()
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
        };
        self.cached_status = status.clone();
        Ok(status)
    }

    /// Removes the stored key and resets to Core tier.
    pub fn deactivate(&mut self) -> Result<(), LicenseError> {
        self.provider
            .delete_password(LICENSE_SERVICE, LICENSE_USERNAME)
            .map_err(|e| LicenseError::Storage(e.to_string()))?;
        self.cached_status = LicenseStatus::default();
        Ok(())
    }

    /// Loads the stored key from keyring and refreshes the cached status.
    fn refresh_status(&mut self) {
        let stored_key = match self.provider.get_password(LICENSE_SERVICE, LICENSE_USERNAME) {
            Ok(key) => key,
            Err(_) => return, // No stored key → keep default Core status
        };

        match verify_license(&stored_key) {
            Ok(payload) => {
                self.cached_status = LicenseStatus {
                    tier: payload.tier,
                    email: Some(payload.email),
                    payment_id: Some(payload.payment_id),
                    issued_at: Some(payload.issued_at),
                    expires_at: payload.expires_at,
                    is_expired: false,
                };
            }
            Err(LicenseError::Expired) => {
                // Show info but mark as expired (tier falls back to Core)
                if let Ok(payload) = decode_license(&stored_key) {
                    self.cached_status = LicenseStatus {
                        tier: LicenseTier::Core,
                        email: Some(payload.email),
                        payment_id: Some(payload.payment_id),
                        issued_at: Some(payload.issued_at),
                        expires_at: payload.expires_at,
                        is_expired: true,
                    };
                }
            }
            Err(_) => {
                // Corrupt key — remove it silently
                let _ = self.provider.delete_password(LICENSE_SERVICE, LICENSE_USERNAME);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::backend::MockProvider;

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
        // Even without activation, deactivate should succeed
        assert!(mgr.deactivate().is_ok());
        assert_eq!(mgr.status().tier, LicenseTier::Core);
    }

    // NOTE: Full activate/deactivate roundtrip tests require the production
    // public key to be set, or use verify_license_with_key directly.
    // See key.rs tests for signature verification coverage.
}

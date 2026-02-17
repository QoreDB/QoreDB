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
}

impl LicenseManager {
    pub fn new(provider: Box<dyn CredentialProvider>) -> Self {
        let mut manager = Self {
            provider,
            cached_status: LicenseStatus::default(),
        };
        manager.refresh_status();
        manager
    }

    pub fn status(&self) -> &LicenseStatus {
        &self.cached_status
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

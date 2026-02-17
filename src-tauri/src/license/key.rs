// SPDX-License-Identifier: Apache-2.0

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::status::LicenseTier;

/// Ed25519 public key for license verification (embedded in binary).
/// Replace with production public key before release.
/// Generate a new keypair with: `cargo test -p qoredb generate_dev_keypair -- --nocapture`
const PUBLIC_KEY_BYTES: [u8; 32] = [
    1, 113, 141, 7, 16, 243, 72, 191, 94, 203, 142, 178, 11, 110, 99, 138, 1, 104, 110, 132,
    222, 221, 231, 246, 206, 72, 216, 110, 19, 248, 61, 112,
];

/// The payload signed by the license server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePayload {
    pub email: String,
    pub tier: LicenseTier,
    pub issued_at: i64,
    pub expires_at: i64,
    #[serde(default)]
    pub machine_id: Option<String>,
}

/// Wire format: base64-encoded JSON with data (base64 payload) + sig (base64 signature).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignedLicense {
    /// Base64-encoded JSON payload bytes
    data: String,
    /// Base64-encoded Ed25519 signature bytes
    sig: String,
}

/// Errors from license operations.
#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    #[error("Invalid license format: {0}")]
    InvalidFormat(String),
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("License expired")]
    Expired,
    #[error("Storage error: {0}")]
    Storage(String),
}

/// Decodes and verifies signature only (no expiration check).
/// Uses the embedded production public key.
pub fn decode_license(license_key: &str) -> Result<LicensePayload, LicenseError> {
    decode_license_with_key(license_key, &PUBLIC_KEY_BYTES)
}

/// Full verification: signature + expiration check.
/// Uses the embedded production public key.
pub fn verify_license(license_key: &str) -> Result<LicensePayload, LicenseError> {
    let payload = decode_license(license_key)?;
    check_expiration(&payload)?;
    Ok(payload)
}

/// Decodes and verifies signature with a specific public key.
pub fn decode_license_with_key(
    license_key: &str,
    public_key_bytes: &[u8; 32],
) -> Result<LicensePayload, LicenseError> {
    // 1. Decode outer base64 → JSON
    let outer_json = BASE64
        .decode(license_key.trim())
        .map_err(|e| LicenseError::InvalidFormat(format!("base64: {e}")))?;

    // 2. Parse signed license envelope
    let signed: SignedLicense = serde_json::from_slice(&outer_json)
        .map_err(|e| LicenseError::InvalidFormat(format!("json: {e}")))?;

    // 3. Decode inner payload and signature
    let payload_bytes = BASE64
        .decode(&signed.data)
        .map_err(|e| LicenseError::InvalidFormat(format!("payload: {e}")))?;
    let sig_bytes = BASE64
        .decode(&signed.sig)
        .map_err(|e| LicenseError::InvalidFormat(format!("sig: {e}")))?;

    // 4. Verify Ed25519 signature
    let verifying_key = VerifyingKey::from_bytes(public_key_bytes)
        .map_err(|_| LicenseError::InvalidFormat("invalid public key".into()))?;
    let signature = Signature::from_slice(&sig_bytes)
        .map_err(|_| LicenseError::InvalidFormat("invalid signature bytes".into()))?;
    verifying_key
        .verify(&payload_bytes, &signature)
        .map_err(|_| LicenseError::InvalidSignature)?;

    // 5. Deserialize payload
    serde_json::from_slice(&payload_bytes)
        .map_err(|e| LicenseError::InvalidFormat(format!("payload json: {e}")))
}

/// Full verification with a specific public key.
pub fn verify_license_with_key(
    license_key: &str,
    public_key_bytes: &[u8; 32],
) -> Result<LicensePayload, LicenseError> {
    let payload = decode_license_with_key(license_key, public_key_bytes)?;
    check_expiration(&payload)?;
    Ok(payload)
}

fn check_expiration(payload: &LicensePayload) -> Result<(), LicenseError> {
    if payload.expires_at > 0 {
        let now = Utc::now().timestamp();
        if now > payload.expires_at {
            return Err(LicenseError::Expired);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Test helpers (available in tests for creating signed license keys)
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod test_helpers {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// Creates a signed license key string for testing.
    pub fn create_test_license(signing_key: &SigningKey, payload: &LicensePayload) -> String {
        let payload_json = serde_json::to_vec(payload).unwrap();
        let payload_b64 = BASE64.encode(&payload_json);

        let signature = signing_key.sign(&payload_json);
        let sig_b64 = BASE64.encode(signature.to_bytes());

        let signed = SignedLicense {
            data: payload_b64,
            sig: sig_b64,
        };

        let signed_json = serde_json::to_vec(&signed).unwrap();
        BASE64.encode(&signed_json)
    }

    /// Returns a deterministic dev keypair (signing key, public key bytes).
    pub fn dev_keypair() -> (SigningKey, [u8; 32]) {
        let seed: [u8; 32] = *b"qoredb-dev-license-key-seed!1234";
        let signing_key = SigningKey::from_bytes(&seed);
        let public_key = signing_key.verifying_key();
        (signing_key, public_key.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use super::*;
    use chrono::Utc;

    #[test]
    fn generate_dev_keypair() {
        let (signing_key, pub_bytes) = dev_keypair();
        println!("\n=== Dev Ed25519 Keypair ===");
        println!("Private (signing) key bytes: {:?}", signing_key.to_bytes());
        println!("Public (verifying) key bytes: {:?}", pub_bytes);
        println!(
            "Public key (Rust const): [{}]",
            pub_bytes
                .iter()
                .map(|b| format!("{b}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    #[test]
    fn valid_license_roundtrip() {
        let (signing_key, pub_bytes) = dev_keypair();

        let payload = LicensePayload {
            email: "test@example.com".into(),
            tier: LicenseTier::Pro,
            issued_at: Utc::now().timestamp(),
            expires_at: Utc::now().timestamp() + 365 * 24 * 3600, // 1 year
            machine_id: None,
        };

        let key_str = create_test_license(&signing_key, &payload);
        let result = verify_license_with_key(&key_str, &pub_bytes).unwrap();

        assert_eq!(result.email, "test@example.com");
        assert_eq!(result.tier, LicenseTier::Pro);
    }

    #[test]
    fn expired_license_rejected() {
        let (signing_key, pub_bytes) = dev_keypair();

        let payload = LicensePayload {
            email: "expired@example.com".into(),
            tier: LicenseTier::Pro,
            issued_at: 1_000_000,
            expires_at: 1_000_001, // long expired
            machine_id: None,
        };

        let key_str = create_test_license(&signing_key, &payload);
        let err = verify_license_with_key(&key_str, &pub_bytes).unwrap_err();
        assert!(matches!(err, LicenseError::Expired));

        // But decode_license_with_key should succeed (no expiration check)
        let result = decode_license_with_key(&key_str, &pub_bytes).unwrap();
        assert_eq!(result.email, "expired@example.com");
    }

    #[test]
    fn invalid_signature_rejected() {
        let (signing_key, _) = dev_keypair();

        let payload = LicensePayload {
            email: "test@example.com".into(),
            tier: LicenseTier::Pro,
            issued_at: Utc::now().timestamp(),
            expires_at: Utc::now().timestamp() + 365 * 24 * 3600,
            machine_id: None,
        };

        let key_str = create_test_license(&signing_key, &payload);

        // Use a different public key → signature mismatch
        let wrong_pub = [42u8; 32];
        // from_bytes might fail for invalid point, so we check both cases
        match decode_license_with_key(&key_str, &wrong_pub) {
            Err(LicenseError::InvalidSignature) | Err(LicenseError::InvalidFormat(_)) => {}
            other => panic!("Expected signature/format error, got: {:?}", other),
        }
    }

    #[test]
    fn invalid_format_rejected() {
        let (_, pub_bytes) = dev_keypair();

        assert!(matches!(
            verify_license_with_key("not-base64!!!", &pub_bytes),
            Err(LicenseError::InvalidFormat(_))
        ));

        let bad_json = BASE64.encode(b"not json");
        assert!(matches!(
            verify_license_with_key(&bad_json, &pub_bytes),
            Err(LicenseError::InvalidFormat(_))
        ));
    }

    #[test]
    fn no_expiration_means_perpetual() {
        let (signing_key, pub_bytes) = dev_keypair();

        let payload = LicensePayload {
            email: "perpetual@example.com".into(),
            tier: LicenseTier::Enterprise,
            issued_at: Utc::now().timestamp(),
            expires_at: 0, // 0 = no expiration
            machine_id: None,
        };

        let key_str = create_test_license(&signing_key, &payload);
        let result = verify_license_with_key(&key_str, &pub_bytes).unwrap();
        assert_eq!(result.tier, LicenseTier::Enterprise);
    }
}

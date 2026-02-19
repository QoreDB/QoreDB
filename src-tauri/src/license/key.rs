// SPDX-License-Identifier: Apache-2.0

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::status::LicenseTier;

fn public_key_bytes() -> [u8; 32] {
    let b64 = env!("PUBLIC_KEY_BASE64");
    let bytes = BASE64.decode(b64).expect("PUBLIC_KEY_BASE64 is not valid base64");
    bytes
        .try_into()
        .expect("PUBLIC_KEY_BASE64 must decode to exactly 32 bytes")
}

/// Wire format envelope matching the showcase backend.
/// `payload` is captured as raw JSON to preserve exact bytes for signature verification.
/// `signature` is the base64-encoded Ed25519 signature over UTF-8(JSON.stringify(payload)).
#[derive(Debug, Deserialize)]
struct LicenseEnvelope<'a> {
    #[serde(borrow)]
    payload: &'a serde_json::value::RawValue,
    signature: String,
}

/// The license payload fields.
/// Matches the showcase backend's `lib/license/generate.ts` output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePayload {
    pub email: String,
    pub tier: LicenseTier,
    pub issued_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    pub payment_id: String,
}

/// Errors from license operations.
#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    #[error("INVALID_BASE64")]
    InvalidBase64,
    #[error("INVALID_JSON")]
    InvalidJson,
    #[error("INVALID_FORMAT: {0}")]
    InvalidFormat(String),
    #[error("INVALID_SIGNATURE")]
    InvalidSignature,
    #[error("EXPIRED_LICENSE")]
    Expired,
    #[error("UNSUPPORTED_TIER")]
    UnsupportedTier,
    #[error("Storage error: {0}")]
    Storage(String),
}

/// Decodes and verifies signature only (no expiration check).
/// Uses the embedded production public key.
pub fn decode_license(license_key: &str) -> Result<LicensePayload, LicenseError> {
    decode_license_with_key(license_key, &public_key_bytes())
}

/// Full verification: signature + expiration + tier check.
/// Uses the embedded production public key.
pub fn verify_license(license_key: &str) -> Result<LicensePayload, LicenseError> {
    let payload = decode_license(license_key)?;
    check_tier(&payload)?;
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
        .map_err(|_| LicenseError::InvalidBase64)?;

    // 2. Parse envelope, capturing payload as raw JSON bytes
    let envelope: LicenseEnvelope = serde_json::from_slice(&outer_json)
        .map_err(|_| LicenseError::InvalidJson)?;

    // 3. The signature is over the exact JSON bytes of the payload field
    let payload_json_bytes = envelope.payload.get().as_bytes();

    // 4. Decode the base64 signature
    let sig_bytes = BASE64
        .decode(&envelope.signature)
        .map_err(|_| LicenseError::InvalidFormat("invalid signature encoding".into()))?;

    // 5. Verify Ed25519 signature
    let verifying_key = VerifyingKey::from_bytes(public_key_bytes)
        .map_err(|_| LicenseError::InvalidFormat("invalid public key".into()))?;
    let signature = Signature::from_slice(&sig_bytes)
        .map_err(|_| LicenseError::InvalidFormat("invalid signature bytes".into()))?;
    verifying_key
        .verify(payload_json_bytes, &signature)
        .map_err(|_| LicenseError::InvalidSignature)?;

    // 6. Deserialize the payload
    let payload: LicensePayload = serde_json::from_str(envelope.payload.get())
        .map_err(|_| LicenseError::InvalidFormat("invalid payload schema".into()))?;

    // 7. Validate required fields
    if payload.email.is_empty() {
        return Err(LicenseError::InvalidFormat("email is empty".into()));
    }
    if payload.payment_id.is_empty() {
        return Err(LicenseError::InvalidFormat("payment_id is empty".into()));
    }
    // Validate issued_at is a parseable ISO date
    payload.issued_at.parse::<DateTime<Utc>>()
        .map_err(|_| LicenseError::InvalidFormat("invalid issued_at date".into()))?;
    // Validate expires_at if present
    if let Some(ref exp) = payload.expires_at {
        exp.parse::<DateTime<Utc>>()
            .map_err(|_| LicenseError::InvalidFormat("invalid expires_at date".into()))?;
    }

    Ok(payload)
}

/// Full verification with a specific public key.
pub fn verify_license_with_key(
    license_key: &str,
    public_key_bytes: &[u8; 32],
) -> Result<LicensePayload, LicenseError> {
    let payload = decode_license_with_key(license_key, public_key_bytes)?;
    check_tier(&payload)?;
    check_expiration(&payload)?;
    Ok(payload)
}

fn check_tier(payload: &LicensePayload) -> Result<(), LicenseError> {
    match payload.tier {
        LicenseTier::Pro | LicenseTier::Team | LicenseTier::Enterprise => Ok(()),
        LicenseTier::Core => Err(LicenseError::UnsupportedTier),
    }
}

fn check_expiration(payload: &LicensePayload) -> Result<(), LicenseError> {
    if let Some(ref expires_at) = payload.expires_at {
        if let Ok(exp) = expires_at.parse::<DateTime<Utc>>() {
            if Utc::now() > exp {
                return Err(LicenseError::Expired);
            }
        }
    }
    // No expires_at (null) means perpetual license
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
    /// Mimics the showcase backend's wire format:
    /// base64({ payload: { ... }, signature: "base64_ed25519_sig" })
    pub fn create_test_license(signing_key: &SigningKey, payload: &LicensePayload) -> String {
        // JSON.stringify(payload) — this is what the signature covers
        let payload_json = serde_json::to_string(payload).unwrap();
        let payload_json_bytes = payload_json.as_bytes();

        // Sign the exact JSON bytes
        let signature = signing_key.sign(payload_json_bytes);
        let sig_b64 = BASE64.encode(signature.to_bytes());

        // Build the envelope with inline payload (not double-base64)
        let envelope = format!(
            r#"{{"payload":{},"signature":"{}"}}"#,
            payload_json, sig_b64
        );

        BASE64.encode(envelope.as_bytes())
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
            issued_at: "2026-02-17T13:00:00.000Z".into(),
            expires_at: Some("2027-02-17T13:00:00.000Z".into()),
            payment_id: "pi_test_123".into(),
        };

        let key_str = create_test_license(&signing_key, &payload);
        let result = verify_license_with_key(&key_str, &pub_bytes).unwrap();

        assert_eq!(result.email, "test@example.com");
        assert_eq!(result.tier, LicenseTier::Pro);
        assert_eq!(result.payment_id, "pi_test_123");
        assert_eq!(result.issued_at, "2026-02-17T13:00:00.000Z");
        assert_eq!(
            result.expires_at.as_deref(),
            Some("2027-02-17T13:00:00.000Z")
        );
    }

    #[test]
    fn perpetual_license_no_expiration() {
        let (signing_key, pub_bytes) = dev_keypair();

        let payload = LicensePayload {
            email: "perpetual@example.com".into(),
            tier: LicenseTier::Enterprise,
            issued_at: "2026-02-17T13:00:00.000Z".into(),
            expires_at: None,
            payment_id: "pi_perpetual".into(),
        };

        let key_str = create_test_license(&signing_key, &payload);
        let result = verify_license_with_key(&key_str, &pub_bytes).unwrap();
        assert_eq!(result.tier, LicenseTier::Enterprise);
        assert!(result.expires_at.is_none());
    }

    #[test]
    fn expired_license_rejected() {
        let (signing_key, pub_bytes) = dev_keypair();

        let payload = LicensePayload {
            email: "expired@example.com".into(),
            tier: LicenseTier::Pro,
            issued_at: "2020-01-01T00:00:00.000Z".into(),
            expires_at: Some("2020-01-02T00:00:00.000Z".into()),
            payment_id: "pi_expired".into(),
        };

        let key_str = create_test_license(&signing_key, &payload);
        let err = verify_license_with_key(&key_str, &pub_bytes).unwrap_err();
        assert!(matches!(err, LicenseError::Expired));

        // But decode_license_with_key should succeed (no expiration check)
        let result = decode_license_with_key(&key_str, &pub_bytes).unwrap();
        assert_eq!(result.email, "expired@example.com");
    }

    #[test]
    fn unsupported_tier_rejected() {
        let (signing_key, pub_bytes) = dev_keypair();

        let payload = LicensePayload {
            email: "core@example.com".into(),
            tier: LicenseTier::Core,
            issued_at: "2026-02-17T13:00:00.000Z".into(),
            expires_at: None,
            payment_id: "pi_core".into(),
        };

        let key_str = create_test_license(&signing_key, &payload);
        let err = verify_license_with_key(&key_str, &pub_bytes).unwrap_err();
        assert!(matches!(err, LicenseError::UnsupportedTier));
    }

    #[test]
    fn invalid_signature_rejected() {
        let (signing_key, _) = dev_keypair();

        let payload = LicensePayload {
            email: "test@example.com".into(),
            tier: LicenseTier::Pro,
            issued_at: "2026-02-17T13:00:00.000Z".into(),
            expires_at: Some("2027-02-17T13:00:00.000Z".into()),
            payment_id: "pi_test".into(),
        };

        let key_str = create_test_license(&signing_key, &payload);

        // Use a different public key → signature mismatch
        let wrong_pub = [42u8; 32];
        match decode_license_with_key(&key_str, &wrong_pub) {
            Err(LicenseError::InvalidSignature) | Err(LicenseError::InvalidFormat(_)) => {}
            other => panic!("Expected signature/format error, got: {:?}", other),
        }
    }

    #[test]
    fn invalid_base64_rejected() {
        let (_, pub_bytes) = dev_keypair();
        let err = verify_license_with_key("not-base64!!!", &pub_bytes).unwrap_err();
        assert!(matches!(err, LicenseError::InvalidBase64));
    }

    #[test]
    fn invalid_json_rejected() {
        let (_, pub_bytes) = dev_keypair();
        let bad_json = BASE64.encode(b"not json");
        let err = verify_license_with_key(&bad_json, &pub_bytes).unwrap_err();
        assert!(matches!(err, LicenseError::InvalidJson));
    }

    #[test]
    fn invalid_format_missing_fields() {
        let (_, pub_bytes) = dev_keypair();
        // Valid JSON but missing required fields
        let bad_envelope = BASE64.encode(br#"{"payload":{"email":"a"},"signature":"AAAA"}"#);
        let err = decode_license_with_key(&bad_envelope, &pub_bytes).unwrap_err();
        assert!(
            matches!(err, LicenseError::InvalidFormat(_) | LicenseError::InvalidSignature),
            "Expected format or signature error, got: {:?}",
            err
        );
    }
}

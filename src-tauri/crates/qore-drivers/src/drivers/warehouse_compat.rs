// SPDX-License-Identifier: Apache-2.0

//! What the cloud warehouses share: an RSA key pair turned into a signed
//! JWT, and an HTTPS client that refuses to run without TLS. Snowflake uses
//! the JWT directly as a bearer; BigQuery trades it for an access token.

use std::time::Duration;

use base64::Engine as _;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use qore_core::error::{EngineError, EngineResult};
use reqwest::Client as HttpClient;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue};
use rsa::RsaPrivateKey;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::{DecodePrivateKey, EncodePublicKey};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub struct KeyPair {
    signing_key: EncodingKey,
    /// `SHA256:<base64>` of the DER-encoded public key, the form Snowflake
    /// expects in the JWT issuer and shows in `DESC USER`.
    pub fingerprint: String,
}

impl KeyPair {
    pub fn from_pem(pem: &str) -> EngineResult<Self> {
        let pem = pem.trim();
        if pem.is_empty() {
            return Err(EngineError::auth_failed(
                "A private key (PEM) is required for key-pair authentication",
            ));
        }
        if pem.contains("ENCRYPTED PRIVATE KEY") {
            return Err(EngineError::auth_failed(
                "Encrypted private keys are not supported; decrypt it first with \
                 `openssl pkcs8 -topk8 -nocrypt -in key.p8 -out key.pem`",
            ));
        }
        let private = RsaPrivateKey::from_pkcs8_pem(pem)
            .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem))
            .map_err(|e| EngineError::auth_failed(format!("Cannot read the private key: {e}")))?;
        let public_der = private
            .to_public_key()
            .to_public_key_der()
            .map_err(|e| EngineError::internal(format!("Cannot encode the public key: {e}")))?;
        let digest = Sha256::digest(public_der.as_bytes());
        let fingerprint = format!(
            "SHA256:{}",
            base64::engine::general_purpose::STANDARD.encode(digest)
        );
        let signing_key = EncodingKey::from_rsa_pem(pem.as_bytes())
            .map_err(|e| EngineError::auth_failed(format!("Cannot use the private key: {e}")))?;
        Ok(Self {
            signing_key,
            fingerprint,
        })
    }

    pub fn sign<C: Serialize>(&self, claims: &C) -> EngineResult<String> {
        jsonwebtoken::encode(&Header::new(Algorithm::RS256), claims, &self.signing_key)
            .map_err(|e| EngineError::auth_failed(format!("Cannot sign the JWT: {e}")))
    }
}

/// An HTTPS client for a JSON API. Cleartext is never an option: every
/// request carries a credential.
pub fn build_https_client(
    timeout: Duration,
    ca_cert_path: Option<&str>,
    extra_headers: HeaderMap,
) -> EngineResult<HttpClient> {
    let mut headers = extra_headers;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    let mut builder = HttpClient::builder()
        .default_headers(headers)
        .https_only(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .pool_idle_timeout(Duration::from_secs(90));

    if let Some(path) = ca_cert_path.map(str::trim).filter(|p| !p.is_empty()) {
        let pem = std::fs::read(path).map_err(|e| {
            EngineError::connection_failed(format!("Cannot read CA certificate '{path}': {e}"))
        })?;
        let cert = reqwest::Certificate::from_pem(&pem)
            .map_err(|e| EngineError::connection_failed(format!("Invalid CA certificate: {e}")))?;
        builder = builder.add_root_certificate(cert);
    }

    builder
        .build()
        .map_err(|e| EngineError::connection_failed(format!("HTTP client build failed: {e}")))
}

#[cfg(test)]
pub(crate) const TEST_PRIVATE_KEY: &str = include_str!("../../tests/fixtures/test_rsa_pkcs8.pem");
#[cfg(test)]
pub(crate) const TEST_FINGERPRINT: &str = "SHA256:kOq/SDnAgQQvcIWV6MpWk2h0LjN9i7ws3zOKUDe8k40=";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_matches_openssl() {
        // openssl rsa -pubout -outform DER | openssl dgst -sha256 -binary | base64
        let key = KeyPair::from_pem(TEST_PRIVATE_KEY).unwrap();
        assert_eq!(key.fingerprint, TEST_FINGERPRINT);
    }

    #[test]
    fn a_signed_token_has_three_parts_and_an_rs256_header() {
        let key = KeyPair::from_pem(TEST_PRIVATE_KEY).unwrap();
        let token = key
            .sign(&serde_json::json!({ "sub": "x", "exp": 4102444800u64 }))
            .unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[0])
            .unwrap();
        assert!(String::from_utf8(header).unwrap().contains("RS256"));
    }

    #[test]
    fn unusable_keys_are_refused_with_a_reason() {
        assert!(KeyPair::from_pem("").is_err());
        assert!(KeyPair::from_pem("not a key").is_err());
        let Err(err) = KeyPair::from_pem(
            "-----BEGIN ENCRYPTED PRIVATE KEY-----\nabc\n-----END ENCRYPTED PRIVATE KEY-----",
        ) else {
            panic!("an encrypted key must be refused");
        };
        assert!(err.to_string().contains("nocrypt"), "{err}");
    }
}

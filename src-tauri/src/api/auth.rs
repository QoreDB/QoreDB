// SPDX-License-Identifier: BUSL-1.1

//! Bearer token authentication for Instant Data API endpoints.
//!
//! - Tokens are generated with 32 cryptographically-random bytes (`OsRng`)
//!   and prefixed with `api-` for visibility ("this is a QoreDB API token").
//! - At rest, only the Argon2id hash is stored alongside the endpoint.
//! - Constant-time verification via `argon2::PasswordVerifier`.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::Engine as _;
use rand::RngCore;
use thiserror::Error;

const TOKEN_PREFIX: &str = "api-";
const TOKEN_BYTES: usize = 32;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("token generation failed: {0}")]
    Hash(String),
    #[error("token verification failed")]
    Invalid,
}

/// Newly-issued token. The raw `value` is shown to the user **once** at
/// endpoint creation; only `hash` is persisted.
pub struct IssuedToken {
    pub value: String,
    pub hash: String,
}

/// Generates a fresh `api-<base64url-32-bytes>` token and its Argon2id hash.
pub fn issue_token() -> Result<IssuedToken, AuthError> {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let value = format!("{TOKEN_PREFIX}{raw}");
    let hash = hash_token(&value)?;
    Ok(IssuedToken { value, hash })
}

fn hash_token(token: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(token.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AuthError::Hash(e.to_string()))
}

/// Constant-time verification of a raw token against a stored Argon2 hash.
pub fn verify_token(raw: &str, stored_hash: &str) -> Result<(), AuthError> {
    let parsed = PasswordHash::new(stored_hash).map_err(|_| AuthError::Invalid)?;
    Argon2::default()
        .verify_password(raw.as_bytes(), &parsed)
        .map_err(|_| AuthError::Invalid)
}

/// Extracts the bearer token from an HTTP `Authorization` header value.
/// Returns `None` if the header is missing, malformed, or uses a non-Bearer
/// scheme.
pub fn parse_bearer(header_value: &str) -> Option<&str> {
    let trimmed = header_value.trim();
    let (scheme, rest) = trimmed.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = rest.trim();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_token_has_expected_shape() {
        let t = issue_token().unwrap();
        assert!(t.value.starts_with(TOKEN_PREFIX));
        // 32 bytes base64url ≈ 43 chars
        assert!(t.value.len() > TOKEN_PREFIX.len() + 40);
    }

    #[test]
    fn verify_accepts_matching_token() {
        let t = issue_token().unwrap();
        verify_token(&t.value, &t.hash).expect("should verify");
    }

    #[test]
    fn verify_rejects_other_token() {
        let a = issue_token().unwrap();
        let b = issue_token().unwrap();
        assert!(verify_token(&b.value, &a.hash).is_err());
    }

    #[test]
    fn verify_rejects_malformed_hash() {
        assert!(verify_token("api-whatever", "not-a-valid-hash").is_err());
    }

    #[test]
    fn parse_bearer_extracts_token() {
        assert_eq!(parse_bearer("Bearer abc123"), Some("abc123"));
        assert_eq!(parse_bearer("bearer  trimmed "), Some("trimmed"));
    }

    #[test]
    fn parse_bearer_rejects_other_schemes() {
        assert_eq!(parse_bearer("Basic dXNlcjpwYXNz"), None);
        assert_eq!(parse_bearer(""), None);
        assert_eq!(parse_bearer("Bearer "), None);
    }
}

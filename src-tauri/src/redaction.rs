// SPDX-License-Identifier: Apache-2.0

//! Canonical PII / secret column detection.
//!
//! Single source of truth for "does this column name look sensitive?", shared
//! by the AI schema redactor ([`crate::ai::context`]) and the Time-Travel
//! changelog redactor ([`crate::time_travel`]). Keeping the token list here
//! stops the two redactors from drifting — a token added once protects both.

/// Tokens (normalised snake_case) that mark a column as holding PII or secrets.
/// Matching is separator/case-insensitive, so a single token covers its
/// variants: `token` catches `access_token`/`refreshToken`, `password` catches
/// `password_hash`, `api_key` catches `apiKey`/`api-key`.
const SENSITIVE_COLUMN_TOKENS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "secret",
    "api_key",
    "access_token",
    "refresh_token",
    "auth_token",
    "token",
    "ssn",
    "social_security",
    "tax_id",
    "cc_number",
    "card_number",
    "credit_card",
    "cvv",
    "cvc",
    "iban",
    "bic",
    "swift",
    "email",
    "phone",
    "mobile",
    "address",
    "postal_code",
    "zip",
    "birth_date",
    "date_of_birth",
    "dob",
    "salary",
    "income",
];

/// Strips case and word separators so `apiKey`, `api_key` and `api-key` all
/// reduce to `apikey`. Over-matching only ever redacts more, which is the safe
/// direction for a leak guard.
fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Returns `true` if the column name looks like it holds PII or a secret.
pub fn is_sensitive_column(name: &str) -> bool {
    let normalized = normalize(name);
    SENSITIVE_COLUMN_TOKENS
        .iter()
        .any(|token| normalized.contains(&normalize(token)))
}

/// The default sensitive-column list for the user-configurable Time-Travel
/// config. Same canonical set as [`is_sensitive_column`].
pub fn default_sensitive_columns() -> Vec<String> {
    SENSITIVE_COLUMN_TOKENS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_common_variants() {
        for name in [
            "password",
            "user_password",
            "password_hash",
            "passwd",
            "pwd",
            "api_key",
            "apiKey",
            "access_token",
            "refresh_token",
            "auth_token",
            "credit_card",
            "card_number",
            "ssn",
            "social_security",
            "tax_id",
            "cvv",
            "iban",
            "email",
            "user_email",
            "phone",
            "phone_number",
            "address",
            "postal_code",
            "zip_code",
            "birth_date",
            "date_of_birth",
            "salary",
        ] {
            assert!(is_sensitive_column(name), "expected to match: {name}");
        }
    }

    #[test]
    fn ignores_benign_columns() {
        for benign in ["id", "name", "created_at", "username", "first_name"] {
            assert!(!is_sensitive_column(benign), "should not match: {benign}");
        }
    }
}

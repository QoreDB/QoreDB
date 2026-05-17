// SPDX-License-Identifier: Apache-2.0

//! One-shot confirmation tokens for destructive IPC commands.
//!
//! A random JS call (drive-by from the webview or DevTools) can otherwise wipe
//! the audit log or the time-travel changelog. We require callers to first
//! request a token for a named action, then submit it within a short TTL.
//! Tokens are single-use and bound to the action name.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::State;
use uuid::Uuid;

const TOKEN_TTL_SECS: u64 = 60;

#[derive(Debug)]
struct TokenEntry {
    action: String,
    expires_at: Instant,
}

#[derive(Default)]
pub struct ConfirmationTokenStore {
    tokens: Mutex<HashMap<String, TokenEntry>>,
}

impl ConfirmationTokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Issues a fresh token for the given action, garbage-collecting expired
    /// entries on the fly.
    pub fn issue(&self, action: impl Into<String>) -> (String, u64) {
        let action = action.into();
        let mut map = self.tokens.lock().expect("confirmation token mutex poisoned");
        let now = Instant::now();
        map.retain(|_, e| e.expires_at > now);

        let token = format!("ctok-{}", Uuid::new_v4());
        map.insert(
            token.clone(),
            TokenEntry {
                action,
                expires_at: now + Duration::from_secs(TOKEN_TTL_SECS),
            },
        );
        (token, TOKEN_TTL_SECS)
    }

    /// Validates and consumes a token. Returns `Err` if the token is unknown,
    /// expired, or bound to a different action.
    pub fn consume(&self, action: &str, token: &str) -> Result<(), String> {
        let mut map = self.tokens.lock().expect("confirmation token mutex poisoned");
        let entry = map
            .remove(token)
            .ok_or_else(|| "Invalid or expired confirmation token".to_string())?;
        if entry.expires_at <= Instant::now() {
            return Err("Confirmation token has expired".to_string());
        }
        if entry.action != action {
            return Err("Confirmation token does not match this action".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct ConfirmationTokenResponse {
    pub token: String,
    pub expires_in_secs: u64,
}

/// Issues a short-lived confirmation token bound to `action`. The token must be
/// passed back to the matching destructive command within `expires_in_secs`.
#[tauri::command]
pub async fn request_confirmation_token(
    state: State<'_, crate::SharedState>,
    action: String,
) -> Result<ConfirmationTokenResponse, String> {
    let store = {
        let state = state.lock().await;
        std::sync::Arc::clone(&state.confirmation_tokens)
    };
    let (token, expires_in_secs) = store.issue(action);
    Ok(ConfirmationTokenResponse {
        token,
        expires_in_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_token_consumed_once() {
        let store = ConfirmationTokenStore::new();
        let (token, ttl) = store.issue("clear_audit_log");
        assert!(ttl > 0);
        assert!(store.consume("clear_audit_log", &token).is_ok());
        // second consume must fail (single-use)
        assert!(store.consume("clear_audit_log", &token).is_err());
    }

    #[test]
    fn rejects_token_for_wrong_action() {
        let store = ConfirmationTokenStore::new();
        let (token, _) = store.issue("clear_audit_log");
        assert!(store.consume("clear_all_changelog", &token).is_err());
    }

    #[test]
    fn rejects_unknown_token() {
        let store = ConfirmationTokenStore::new();
        assert!(store.consume("clear_audit_log", "nope").is_err());
    }
}

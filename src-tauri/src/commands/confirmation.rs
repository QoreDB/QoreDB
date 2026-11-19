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

/// Acknowledges a dangerous query or mutation (production confirmation,
/// destructive statement) on one session.
pub const DANGEROUS_OPERATION_ACTION: &str = "acknowledge_dangerous_query";

#[derive(Debug)]
struct TokenEntry {
    action: String,
    scope: Option<String>,
    expires_at: Instant,
}

fn workspace_scope(workspace_id: &str) -> String {
    format!("workspace:{workspace_id}")
}

fn session_scope(session_id: &str) -> String {
    format!("session:{session_id}")
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
        self.issue_scoped(action.into(), None)
    }

    pub fn issue_for_workspace(&self, action: &str, workspace_id: &str) -> (String, u64) {
        self.issue_scoped(action.to_owned(), Some(workspace_scope(workspace_id)))
    }

    pub fn issue_for_session(&self, action: &str, session_id: &str) -> (String, u64) {
        self.issue_scoped(action.to_owned(), Some(session_scope(session_id)))
    }

    fn issue_scoped(&self, action: String, scope: Option<String>) -> (String, u64) {
        let mut map = self
            .tokens
            .lock()
            .expect("confirmation token mutex poisoned");
        let now = Instant::now();
        map.retain(|_, e| e.expires_at > now);

        let token = format!("ctok-{}", Uuid::new_v4());
        map.insert(
            token.clone(),
            TokenEntry {
                action,
                scope,
                expires_at: now + Duration::from_secs(TOKEN_TTL_SECS),
            },
        );
        (token, TOKEN_TTL_SECS)
    }

    /// Validates and consumes a token. Returns `Err` if the token is unknown,
    /// expired, or bound to a different action.
    pub fn consume(&self, action: &str, token: &str) -> Result<(), String> {
        self.consume_scoped(action, None, token)
    }

    pub fn consume_for_workspace(
        &self,
        action: &str,
        workspace_id: &str,
        token: &str,
    ) -> Result<(), String> {
        self.consume_scoped(action, Some(&workspace_scope(workspace_id)), token)
    }

    pub fn consume_for_session(
        &self,
        action: &str,
        session_id: &str,
        token: &str,
    ) -> Result<(), String> {
        self.consume_scoped(action, Some(&session_scope(session_id)), token)
    }

    fn consume_scoped(&self, action: &str, scope: Option<&str>, token: &str) -> Result<(), String> {
        let mut map = self
            .tokens
            .lock()
            .expect("confirmation token mutex poisoned");
        let entry = map
            .remove(token)
            .ok_or_else(|| "Invalid or expired confirmation token".to_string())?;
        if entry.expires_at <= Instant::now() {
            return Err("Confirmation token has expired".to_string());
        }
        if entry.action != action || entry.scope.as_deref() != scope {
            return Err("Confirmation token does not match this action".to_string());
        }
        Ok(())
    }
}

/// Turns the renderer's `acknowledged_dangerous` flag into a backend decision:
/// the flag alone is refused, and the token must have been issued for this
/// session. The token is single-use and expires after [`TOKEN_TTL_SECS`].
pub fn resolve_dangerous_acknowledgement(
    store: &ConfirmationTokenStore,
    session_id: &str,
    acknowledged_dangerous: Option<bool>,
    confirmation_token: Option<&str>,
) -> Result<bool, String> {
    if let Some(token) = confirmation_token {
        store.consume_for_session(DANGEROUS_OPERATION_ACTION, session_id, token)?;
        return Ok(true);
    }
    if acknowledged_dangerous.unwrap_or(false) {
        return Err("Dangerous operation requires a confirmation token".to_string());
    }
    Ok(false)
}

pub async fn acknowledge_dangerous(
    state: &State<'_, crate::SharedState>,
    session_id: &str,
    acknowledged_dangerous: Option<bool>,
    confirmation_token: Option<&str>,
) -> Result<bool, String> {
    let store = {
        let state = state.lock().await;
        std::sync::Arc::clone(&state.confirmation_tokens)
    };
    resolve_dangerous_acknowledgement(
        &store,
        session_id,
        acknowledged_dangerous,
        confirmation_token,
    )
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
    ws_manager: State<'_, crate::commands::workspace::SharedWorkspaceManager>,
    action: String,
    session_id: Option<String>,
) -> Result<ConfirmationTokenResponse, String> {
    let store = {
        let state = state.lock().await;
        std::sync::Arc::clone(&state.confirmation_tokens)
    };
    let (token, expires_in_secs) = if matches!(
        action.as_str(),
        "clear_table_changelog" | "clear_all_changelog"
    ) {
        let workspace_id = ws_manager.lock().await.project_id();
        store.issue_for_workspace(&action, &workspace_id)
    } else if action == DANGEROUS_OPERATION_ACTION {
        let session_id = session_id
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| "A session is required for this confirmation".to_string())?;
        store.issue_for_session(&action, &session_id)
    } else {
        store.issue(action)
    };
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

    #[test]
    fn workspace_confirmation_cannot_be_reused_after_switching_workspaces() {
        let store = ConfirmationTokenStore::new();
        let (token, _) = store.issue_for_workspace("clear_all_changelog", "workspace-a");
        assert!(
            store
                .consume_for_workspace("clear_all_changelog", "workspace-b", &token)
                .is_err()
        );
        assert!(
            store
                .consume_for_workspace("clear_all_changelog", "workspace-a", &token)
                .is_err()
        );
        let (token, _) = store.issue_for_workspace("clear_all_changelog", "workspace-a");
        assert!(
            store
                .consume_for_workspace("clear_all_changelog", "workspace-a", &token)
                .is_ok()
        );
        assert!(
            store
                .consume_for_workspace("clear_all_changelog", "workspace-a", &token)
                .is_err()
        );
    }

    #[test]
    fn workspace_confirmation_requires_both_action_and_origin() {
        let store = ConfirmationTokenStore::new();
        let (global, _) = store.issue("clear_all_changelog");
        assert!(
            store
                .consume_for_workspace("clear_all_changelog", "workspace-a", &global)
                .is_err()
        );
        let (scoped, _) = store.issue_for_workspace("clear_all_changelog", "workspace-a");
        assert!(store.consume("clear_all_changelog", &scoped).is_err());
        let (scoped, _) = store.issue_for_workspace("clear_table_changelog", "workspace-a");
        assert!(
            store
                .consume_for_workspace("clear_all_changelog", "workspace-a", &scoped)
                .is_err()
        );
    }

    #[test]
    fn dangerous_acknowledgement_requires_a_token_bound_to_the_session() {
        let store = ConfirmationTokenStore::new();
        assert!(!resolve_dangerous_acknowledgement(&store, "s1", None, None).unwrap());
        assert!(!resolve_dangerous_acknowledgement(&store, "s1", Some(false), None).unwrap());
        assert!(resolve_dangerous_acknowledgement(&store, "s1", Some(true), None).is_err());

        let (token, _) = store.issue_for_session(DANGEROUS_OPERATION_ACTION, "s1");
        assert!(resolve_dangerous_acknowledgement(&store, "s2", Some(true), Some(&token)).is_err());

        let (token, _) = store.issue_for_session(DANGEROUS_OPERATION_ACTION, "s1");
        assert!(resolve_dangerous_acknowledgement(&store, "s1", Some(true), Some(&token)).unwrap());
        assert!(resolve_dangerous_acknowledgement(&store, "s1", Some(true), Some(&token)).is_err());

        let (unscoped, _) = store.issue(DANGEROUS_OPERATION_ACTION);
        assert!(
            resolve_dangerous_acknowledgement(&store, "s1", Some(true), Some(&unscoped)).is_err()
        );
        let (other_action, _) = store.issue_for_session("apply_migration", "s1");
        assert!(
            resolve_dangerous_acknowledgement(&store, "s1", Some(true), Some(&other_action))
                .is_err()
        );
    }

    #[test]
    fn session_and_workspace_scopes_never_collide() {
        let store = ConfirmationTokenStore::new();
        let (token, _) = store.issue_for_workspace(DANGEROUS_OPERATION_ACTION, "same-id");
        assert!(
            store
                .consume_for_session(DANGEROUS_OPERATION_ACTION, "same-id", &token)
                .is_err()
        );
    }
}

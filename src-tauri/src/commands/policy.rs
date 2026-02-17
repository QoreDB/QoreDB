// SPDX-License-Identifier: Apache-2.0

//! Safety policy commands.

use serde::Serialize;
use tauri::State;

use crate::policy::SafetyPolicy;
use crate::SharedState;

#[derive(Debug, Serialize)]
pub struct SafetyPolicyResponse {
    pub success: bool,
    pub policy: Option<SafetyPolicy>,
    pub error: Option<String>,
}

/// Returns the effective safety policy (env overrides applied).
#[tauri::command]
pub async fn get_safety_policy(
    state: State<'_, SharedState>,
) -> Result<SafetyPolicyResponse, String> {
    let state = state.lock().await;
    Ok(SafetyPolicyResponse {
        success: true,
        policy: Some(state.policy.clone()),
        error: None,
    })
}

/// Updates the stored safety policy.
#[tauri::command]
pub async fn set_safety_policy(
    state: State<'_, SharedState>,
    policy: SafetyPolicy,
) -> Result<SafetyPolicyResponse, String> {
    if let Err(err) = policy.save_to_file() {
        return Ok(SafetyPolicyResponse {
            success: false,
            policy: None,
            error: Some(err),
        });
    }

    let effective = SafetyPolicy::load();
    let mut state = state.lock().await;
    state.policy = effective.clone();

    Ok(SafetyPolicyResponse {
        success: true,
        policy: Some(effective),
        error: None,
    })
}

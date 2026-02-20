// SPDX-License-Identifier: Apache-2.0

use tauri::State;

use crate::license::status::{LicenseStatus, LicenseTier};
use crate::SharedState;

#[tauri::command]
pub async fn activate_license(
    state: State<'_, SharedState>,
    key: String,
) -> Result<LicenseStatus, String> {
    let mut state = state.lock().await;
    state
        .license_manager
        .activate(&key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_license_status(
    state: State<'_, SharedState>,
) -> Result<LicenseStatus, String> {
    let state = state.lock().await;
    Ok(state.license_manager.effective_status())
}

#[tauri::command]
pub async fn deactivate_license(
    state: State<'_, SharedState>,
) -> Result<(), String> {
    let mut state = state.lock().await;
    state
        .license_manager
        .deactivate()
        .map_err(|e| e.to_string())
}

/// Dev-only: override the license tier without a real key.
/// This command is stripped from release builds entirely.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn dev_set_license_tier(
    state: State<'_, SharedState>,
    tier: Option<LicenseTier>,
) -> Result<LicenseStatus, String> {
    let mut state = state.lock().await;
    state.license_manager.set_dev_override(tier);
    Ok(state.license_manager.effective_status())
}

/// Stub for release builds — always returns an error.
#[cfg(not(debug_assertions))]
#[tauri::command]
pub async fn dev_set_license_tier(
    _state: State<'_, SharedState>,
    _tier: Option<LicenseTier>,
) -> Result<LicenseStatus, String> {
    Err("Dev license override is not available in release builds".to_string())
}

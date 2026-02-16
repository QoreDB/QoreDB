use tauri::State;

use crate::license::status::LicenseStatus;
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
    Ok(state.license_manager.status().clone())
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

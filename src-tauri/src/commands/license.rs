// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use tauri::State;

use crate::license::status::{LicenseStatus, LicenseTier};
use crate::SharedState;

/// Marketing site base URL. The webview cannot reach it directly (CSP
/// `connect-src` is locked down), so license refresh and billing portal
/// calls are proxied through these Rust commands.
const SITE_BASE_URL: &str = "https://www.qoredb.com";

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
pub async fn get_license_status(state: State<'_, SharedState>) -> Result<LicenseStatus, String> {
    let state = state.lock().await;
    Ok(state.license_manager.effective_status())
}

#[tauri::command]
pub async fn deactivate_license(state: State<'_, SharedState>) -> Result<(), String> {
    let mut state = state.lock().await;
    state
        .license_manager
        .deactivate()
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct CurrentLicenseResponse {
    #[serde(rename = "licenseKey")]
    license_key: String,
}

/// Fetches the up-to-date license key from the site for the active license's
/// email and re-activates it. Used after a Team renewal or seat change so the
/// user never has to copy-paste a fresh key.
#[tauri::command]
pub async fn refresh_license(state: State<'_, SharedState>) -> Result<LicenseStatus, String> {
    let email = {
        let state = state.lock().await;
        state.license_manager.status().email.clone()
    };
    let Some(email) = email else {
        return Err("NO_ACTIVE_LICENSE".to_string());
    };

    let resp = reqwest::Client::new()
        .post(format!("{SITE_BASE_URL}/api/license/current"))
        .json(&serde_json::json!({ "email": email }))
        .send()
        .await
        .map_err(|e| format!("REFRESH_REQUEST_FAILED: {e}"))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("NO_ACTIVE_SUBSCRIPTION".to_string());
    }
    if !resp.status().is_success() {
        return Err(format!("REFRESH_FAILED: {}", resp.status()));
    }

    let body: CurrentLicenseResponse = resp
        .json()
        .await
        .map_err(|e| format!("REFRESH_INVALID_RESPONSE: {e}"))?;

    let mut state = state.lock().await;
    state
        .license_manager
        .activate(&body.license_key)
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct BillingPortalResponse {
    url: String,
}

/// Returns a Stripe billing portal URL for the active license's email so the
/// admin can manage seats, invoices and cancellation. The frontend opens the
/// returned URL externally.
#[tauri::command]
pub async fn get_billing_portal_url(state: State<'_, SharedState>) -> Result<String, String> {
    let email = {
        let state = state.lock().await;
        state.license_manager.status().email.clone()
    };
    let Some(email) = email else {
        return Err("NO_ACTIVE_LICENSE".to_string());
    };

    let resp = reqwest::Client::new()
        .post(format!("{SITE_BASE_URL}/api/billing/portal"))
        .json(&serde_json::json!({ "email": email }))
        .send()
        .await
        .map_err(|e| format!("PORTAL_REQUEST_FAILED: {e}"))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("NO_ACTIVE_SUBSCRIPTION".to_string());
    }
    if !resp.status().is_success() {
        return Err(format!("PORTAL_FAILED: {}", resp.status()));
    }

    let body: BillingPortalResponse = resp
        .json()
        .await
        .map_err(|e| format!("PORTAL_INVALID_RESPONSE: {e}"))?;
    Ok(body.url)
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

// SPDX-License-Identifier: Apache-2.0

//! Driver metadata Tauri commands
//!
//! Exposes driver capabilities to the frontend so it can adapt features safely.

use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use crate::engine::types::DriverInfo;

/// Response wrapper for a single driver lookup.
#[derive(Debug, Serialize)]
pub struct DriverInfoResponse {
    pub success: bool,
    pub driver: Option<DriverInfo>,
    pub error: Option<String>,
}

/// Response wrapper for listing all drivers.
#[derive(Debug, Serialize)]
pub struct DriverListResponse {
    pub success: bool,
    pub drivers: Vec<DriverInfo>,
    pub error: Option<String>,
}

/// Returns the driver info for a given session.
#[tauri::command]
pub async fn get_driver_info(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<DriverInfoResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };

    let session_uuid = match uuid::Uuid::parse_str(&session_id) {
        Ok(uuid) => uuid,
        Err(e) => {
            return Ok(DriverInfoResponse {
                success: false,
                driver: None,
                error: Some(format!("Invalid session ID: {}", e)),
            });
        }
    };
    let session = crate::engine::types::SessionId(session_uuid);

    match session_manager.get_driver(session).await {
        Ok(driver) => Ok(DriverInfoResponse {
            success: true,
            driver: Some(DriverInfo {
                id: driver.driver_id().to_string(),
                name: driver.driver_name().to_string(),
                capabilities: driver.capabilities(),
            }),
            error: None,
        }),
        Err(e) => Ok(DriverInfoResponse {
            success: false,
            driver: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Lists all registered drivers with their capabilities.
#[tauri::command]
pub async fn list_drivers(
    state: State<'_, crate::SharedState>,
) -> Result<DriverListResponse, String> {
    let registry = {
        let state = state.lock().await;
        Arc::clone(&state.registry)
    };

    Ok(DriverListResponse {
        success: true,
        drivers: registry.list_infos(),
        error: None,
    })
}

// SPDX-License-Identifier: Apache-2.0

//! Maintenance Tauri Commands
//!
//! Commands for running table maintenance operations (vacuum, analyze, optimize, etc.)

use serde::Serialize;
use tauri::State;
use uuid::Uuid;
use std::sync::Arc;
use tracing::instrument;

use crate::engine::types::{
    MaintenanceOperationInfo, MaintenanceRequest, MaintenanceResult,
    Namespace, SessionId,
};
use crate::interceptor::{Environment, QueryExecutionResult, SafetyAction};

const READ_ONLY_BLOCKED: &str = "Operation blocked: read-only mode";
const MAINTENANCE_NOT_SUPPORTED: &str = "Maintenance operations are not supported by this driver";
const DANGEROUS_BLOCKED: &str = "Dangerous query blocked: confirmation required";
const SAFETY_RULE_BLOCKED: &str = "Query blocked by safety rule";

fn map_environment(env: &str) -> Environment {
    match env {
        "production" => Environment::Production,
        "staging" => Environment::Staging,
        _ => Environment::Development,
    }
}

fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

/// Response wrapper for listing maintenance operations
#[derive(Debug, Serialize)]
pub struct MaintenanceListResponse {
    pub success: bool,
    pub operations: Vec<MaintenanceOperationInfo>,
    pub error: Option<String>,
}

/// Response wrapper for running a maintenance operation
#[derive(Debug, Serialize)]
pub struct MaintenanceRunResponse {
    pub success: bool,
    pub result: Option<MaintenanceResult>,
    pub error: Option<String>,
}

/// Lists available maintenance operations for a table
#[tauri::command]
#[instrument(
    skip(state),
    fields(session_id = %session_id, database = %database, schema = ?schema, table = %table)
)]
pub async fn list_maintenance_operations(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    table: String,
) -> Result<MaintenanceListResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.capabilities().maintenance {
        return Ok(MaintenanceListResponse {
            success: true,
            operations: Vec::new(),
            error: None,
        });
    }

    let namespace = Namespace { database, schema };

    match driver.list_maintenance_operations(session, &namespace, &table).await {
        Ok(operations) => Ok(MaintenanceListResponse {
            success: true,
            operations,
            error: None,
        }),
        Err(e) => Ok(MaintenanceListResponse {
            success: false,
            operations: Vec::new(),
            error: Some(e.to_string()),
        }),
    }
}

/// Runs a maintenance operation on a table
#[tauri::command]
#[instrument(
    skip(state, request),
    fields(session_id = %session_id, database = %database, schema = ?schema, table = %table)
)]
pub async fn run_maintenance(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    table: String,
    request: MaintenanceRequest,
    acknowledged_dangerous: Option<bool>,
) -> Result<MaintenanceRunResponse, String> {
    let (session_manager, interceptor) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.interceptor),
        )
    };
    let session = parse_session_id(&session_id)?;

    let read_only = session_manager
        .is_read_only(session)
        .await
        .map_err(|e| e.to_string())?;
    if read_only {
        return Ok(MaintenanceRunResponse {
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
        });
    }

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.capabilities().maintenance {
        return Ok(MaintenanceRunResponse {
            success: false,
            result: None,
            error: Some(MAINTENANCE_NOT_SUPPORTED.to_string()),
        });
    }

    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let interceptor_env = map_environment(&environment);

    let query_preview = format!(
        "MAINTENANCE {:?} ON {}.{}",
        request.operation, database, table
    );

    let acknowledged = acknowledged_dangerous.unwrap_or(false);
    let interceptor_context = interceptor.build_context(
        &session_id,
        &query_preview,
        driver.driver_id(),
        interceptor_env,
        read_only,
        acknowledged,
        Some(&database),
        None,
        true,
    );

    let safety_result = interceptor.pre_execute(&interceptor_context);
    if !safety_result.allowed {
        interceptor.post_execute(
            &interceptor_context,
            &QueryExecutionResult {
                success: false,
                error: safety_result.message.clone(),
                execution_time_ms: 0.0,
                row_count: None,
            },
            true,
            safety_result.triggered_rule.as_deref(),
        );

        let error_msg = match safety_result.action {
            SafetyAction::Block => {
                format!(
                    "{}: {}",
                    SAFETY_RULE_BLOCKED,
                    safety_result.message.unwrap_or_default()
                )
            }
            SafetyAction::RequireConfirmation => {
                format!(
                    "{}: {}",
                    DANGEROUS_BLOCKED,
                    safety_result.message.unwrap_or_default()
                )
            }
            SafetyAction::Warn => "Warning triggered".to_string(),
        };

        return Ok(MaintenanceRunResponse {
            success: false,
            result: None,
            error: Some(error_msg),
        });
    }

    let safety_warning = if matches!(safety_result.action, SafetyAction::Warn) {
        safety_result.triggered_rule.clone()
    } else {
        None
    };

    let namespace = Namespace { database, schema };

    match driver.run_maintenance(session, &namespace, &table, &request).await {
        Ok(result) => {
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: result.success,
                    error: None,
                    execution_time_ms: result.execution_time_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(MaintenanceRunResponse {
                success: true,
                result: Some(result),
                error: None,
            })
        }
        Err(e) => {
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: false,
                    error: Some(e.to_string()),
                    execution_time_ms: 0.0,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(MaintenanceRunResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
            })
        }
    }
}

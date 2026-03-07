// SPDX-License-Identifier: Apache-2.0

//! Routine Management Tauri Commands
//!
//! Commands for viewing definitions and dropping database routines (functions/procedures).

use serde::Serialize;
use std::sync::Arc;
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::engine::types::{
    Namespace, RoutineDefinition, RoutineOperationResult, RoutineType, SessionId,
};
use crate::interceptor::{Environment, QueryExecutionResult, SafetyAction};

const READ_ONLY_BLOCKED: &str = "Operation blocked: read-only mode";
const ROUTINES_NOT_SUPPORTED: &str = "Routine operations are not supported by this driver";
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

fn parse_routine_type(s: &str) -> Result<RoutineType, String> {
    match s {
        "Function" => Ok(RoutineType::Function),
        "Procedure" => Ok(RoutineType::Procedure),
        _ => Err(format!("Invalid routine type: {}", s)),
    }
}

/// Response wrapper for getting a routine definition
#[derive(Debug, Serialize)]
pub struct RoutineDefinitionResponse {
    pub success: bool,
    pub definition: Option<RoutineDefinition>,
    pub error: Option<String>,
}

/// Response wrapper for dropping a routine
#[derive(Debug, Serialize)]
pub struct RoutineDropResponse {
    pub success: bool,
    pub result: Option<RoutineOperationResult>,
    pub error: Option<String>,
}

/// Gets the full definition (CREATE statement) of a routine
#[tauri::command]
#[instrument(
    skip(state),
    fields(session_id = %session_id, database = %database, schema = ?schema, routine_name = %routine_name)
)]
pub async fn get_routine_definition(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    routine_name: String,
    routine_type: String,
    arguments: Option<String>,
) -> Result<RoutineDefinitionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.supports_routines() {
        return Ok(RoutineDefinitionResponse {
            success: false,
            definition: None,
            error: Some(ROUTINES_NOT_SUPPORTED.to_string()),
        });
    }

    let rt = parse_routine_type(&routine_type)?;
    let namespace = Namespace { database, schema };
    let args = arguments.as_deref();

    match driver
        .get_routine_definition(session, &namespace, &routine_name, rt, args)
        .await
    {
        Ok(def) => Ok(RoutineDefinitionResponse {
            success: true,
            definition: Some(def),
            error: None,
        }),
        Err(e) => Ok(RoutineDefinitionResponse {
            success: false,
            definition: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Drops a routine (function or procedure) with safety interceptor integration
#[tauri::command]
#[instrument(
    skip(state),
    fields(session_id = %session_id, database = %database, schema = ?schema, routine_name = %routine_name)
)]
pub async fn drop_routine(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    routine_name: String,
    routine_type: String,
    arguments: Option<String>,
    acknowledged_dangerous: Option<bool>,
) -> Result<RoutineDropResponse, String> {
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
        return Ok(RoutineDropResponse {
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
        });
    }

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.supports_routines() {
        return Ok(RoutineDropResponse {
            success: false,
            result: None,
            error: Some(ROUTINES_NOT_SUPPORTED.to_string()),
        });
    }

    let rt = parse_routine_type(&routine_type)?;

    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let interceptor_env = map_environment(&environment);

    let type_keyword = match rt {
        RoutineType::Function => "FUNCTION",
        RoutineType::Procedure => "PROCEDURE",
    };
    let query_preview = format!(
        "DROP {} {}.{}",
        type_keyword, database, routine_name
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
        false,
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

        return Ok(RoutineDropResponse {
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
    let args = arguments.as_deref();

    match driver
        .drop_routine(session, &namespace, &routine_name, rt, args)
        .await
    {
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
            Ok(RoutineDropResponse {
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
            Ok(RoutineDropResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
            })
        }
    }
}

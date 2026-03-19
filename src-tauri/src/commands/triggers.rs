// SPDX-License-Identifier: Apache-2.0

//! Trigger & Event Management Tauri Commands
//!
//! Commands for viewing definitions, dropping, and toggling database triggers and events.

use serde::Serialize;
use std::sync::Arc;
use tauri::State;
use tracing::instrument;
use uuid::Uuid;

use crate::engine::types::{
    EventDefinition, EventOperationResult, Namespace, SessionId, TriggerDefinition,
    TriggerOperationResult,
};
use crate::interceptor::{Environment, QueryExecutionResult, SafetyAction};

const READ_ONLY_BLOCKED: &str = "Operation blocked: read-only mode";
const TRIGGERS_NOT_SUPPORTED: &str = "Trigger operations are not supported by this driver";
const EVENTS_NOT_SUPPORTED: &str = "Event operations are not supported by this driver";
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

// ==================== Trigger Responses ====================

#[derive(Debug, Serialize)]
pub struct TriggerDefinitionResponse {
    pub success: bool,
    pub definition: Option<TriggerDefinition>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TriggerDropResponse {
    pub success: bool,
    pub result: Option<TriggerOperationResult>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TriggerToggleResponse {
    pub success: bool,
    pub result: Option<TriggerOperationResult>,
    pub error: Option<String>,
}

// ==================== Event Responses ====================

#[derive(Debug, Serialize)]
pub struct EventDefinitionResponse {
    pub success: bool,
    pub definition: Option<EventDefinition>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EventDropResponse {
    pub success: bool,
    pub result: Option<EventOperationResult>,
    pub error: Option<String>,
}

// ==================== Trigger Commands ====================

/// Gets the full definition (CREATE statement) of a trigger
#[tauri::command]
#[instrument(
    skip(state),
    fields(session_id = %session_id, database = %database, schema = ?schema, trigger_name = %trigger_name)
)]
pub async fn get_trigger_definition(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    trigger_name: String,
) -> Result<TriggerDefinitionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.supports_triggers() {
        return Ok(TriggerDefinitionResponse {
            success: false,
            definition: None,
            error: Some(TRIGGERS_NOT_SUPPORTED.to_string()),
        });
    }

    let namespace = Namespace { database, schema };

    match driver
        .get_trigger_definition(session, &namespace, &trigger_name)
        .await
    {
        Ok(def) => Ok(TriggerDefinitionResponse {
            success: true,
            definition: Some(def),
            error: None,
        }),
        Err(e) => Ok(TriggerDefinitionResponse {
            success: false,
            definition: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Drops a trigger with safety interceptor integration
#[tauri::command]
#[instrument(
    skip(state),
    fields(session_id = %session_id, database = %database, schema = ?schema, trigger_name = %trigger_name)
)]
pub async fn drop_trigger(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    trigger_name: String,
    table_name: String,
    acknowledged_dangerous: Option<bool>,
) -> Result<TriggerDropResponse, String> {
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
        return Ok(TriggerDropResponse {
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
        });
    }

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.supports_triggers() {
        return Ok(TriggerDropResponse {
            success: false,
            result: None,
            error: Some(TRIGGERS_NOT_SUPPORTED.to_string()),
        });
    }

    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let interceptor_env = map_environment(&environment);

    let query_preview = format!("DROP TRIGGER {}.{}", database, trigger_name);

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

        return Ok(TriggerDropResponse {
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

    match driver
        .drop_trigger(session, &namespace, &trigger_name, &table_name)
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
            Ok(TriggerDropResponse {
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
            Ok(TriggerDropResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// Enables or disables a trigger
#[tauri::command]
#[instrument(
    skip(state),
    fields(session_id = %session_id, database = %database, schema = ?schema, trigger_name = %trigger_name, enable = %enable)
)]
pub async fn toggle_trigger(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    trigger_name: String,
    table_name: String,
    enable: bool,
) -> Result<TriggerToggleResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let read_only = session_manager
        .is_read_only(session)
        .await
        .map_err(|e| e.to_string())?;
    if read_only {
        return Ok(TriggerToggleResponse {
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
        });
    }

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.supports_triggers() {
        return Ok(TriggerToggleResponse {
            success: false,
            result: None,
            error: Some(TRIGGERS_NOT_SUPPORTED.to_string()),
        });
    }

    let namespace = Namespace { database, schema };

    match driver
        .toggle_trigger(session, &namespace, &trigger_name, &table_name, enable)
        .await
    {
        Ok(result) => Ok(TriggerToggleResponse {
            success: true,
            result: Some(result),
            error: None,
        }),
        Err(e) => Ok(TriggerToggleResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
        }),
    }
}

// ==================== Event Commands ====================

/// Gets the full definition (CREATE statement) of a scheduled event
#[tauri::command]
#[instrument(
    skip(state),
    fields(session_id = %session_id, database = %database, schema = ?schema, event_name = %event_name)
)]
pub async fn get_event_definition(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    event_name: String,
) -> Result<EventDefinitionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.supports_events() {
        return Ok(EventDefinitionResponse {
            success: false,
            definition: None,
            error: Some(EVENTS_NOT_SUPPORTED.to_string()),
        });
    }

    let namespace = Namespace { database, schema };

    match driver
        .get_event_definition(session, &namespace, &event_name)
        .await
    {
        Ok(def) => Ok(EventDefinitionResponse {
            success: true,
            definition: Some(def),
            error: None,
        }),
        Err(e) => Ok(EventDefinitionResponse {
            success: false,
            definition: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Drops a scheduled event with safety interceptor integration
#[tauri::command]
#[instrument(
    skip(state),
    fields(session_id = %session_id, database = %database, schema = ?schema, event_name = %event_name)
)]
pub async fn drop_event(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    event_name: String,
    acknowledged_dangerous: Option<bool>,
) -> Result<EventDropResponse, String> {
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
        return Ok(EventDropResponse {
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
        });
    }

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.to_string())?;

    if !driver.supports_events() {
        return Ok(EventDropResponse {
            success: false,
            result: None,
            error: Some(EVENTS_NOT_SUPPORTED.to_string()),
        });
    }

    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let interceptor_env = map_environment(&environment);

    let query_preview = format!("DROP EVENT {}.{}", database, event_name);

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

        return Ok(EventDropResponse {
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

    match driver.drop_event(session, &namespace, &event_name).await {
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
            Ok(EventDropResponse {
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
            Ok(EventDropResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
            })
        }
    }
}

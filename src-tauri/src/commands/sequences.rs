// SPDX-License-Identifier: Apache-2.0

//! Commands for viewing definitions and dropping database sequences (MariaDB 10.3+).

use serde::Serialize;
use std::sync::Arc;
use tauri::State;
use tracing::instrument;

use super::parse_session_id;
use crate::engine::types::{Namespace, SequenceDefinition, SequenceOperationResult};
use crate::interceptor::{map_environment, QueryExecutionResult, SafetyAction};

const READ_ONLY_BLOCKED: &str = "Operation blocked: read-only mode";
const SEQUENCES_NOT_SUPPORTED: &str = "Sequence operations are not supported by this driver";
const DANGEROUS_BLOCKED: &str = "Dangerous query blocked: confirmation required";
const SAFETY_RULE_BLOCKED: &str = "Query blocked by safety rule";

#[derive(Debug, Serialize)]
pub struct SequenceDefinitionResponse {
    pub success: bool,
    pub definition: Option<SequenceDefinition>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SequenceDropResponse {
    pub success: bool,
    pub result: Option<SequenceOperationResult>,
    pub error: Option<String>,
}

#[tauri::command]
#[instrument(
    skip(state),
    fields(session_id = %session_id, database = %database, schema = ?schema, sequence_name = %sequence_name)
)]
pub async fn get_sequence_definition(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    sequence_name: String,
) -> Result<SequenceDefinitionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;

    if !driver.supports_sequences() {
        return Ok(SequenceDefinitionResponse {
            success: false,
            definition: None,
            error: Some(SEQUENCES_NOT_SUPPORTED.to_string()),
        });
    }

    let namespace = Namespace { database, schema };

    match driver
        .get_sequence_definition(session, &namespace, &sequence_name)
        .await
    {
        Ok(def) => Ok(SequenceDefinitionResponse {
            success: true,
            definition: Some(def),
            error: None,
        }),
        Err(e) => Ok(SequenceDefinitionResponse {
            success: false,
            definition: None,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Drops a sequence with safety interceptor integration
#[tauri::command]
#[instrument(
    skip(state),
    fields(session_id = %session_id, database = %database, schema = ?schema, sequence_name = %sequence_name)
)]
pub async fn drop_sequence(
    state: State<'_, crate::SharedState>,
    session_id: String,
    database: String,
    schema: Option<String>,
    sequence_name: String,
    acknowledged_dangerous: Option<bool>,
) -> Result<SequenceDropResponse, String> {
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
        .map_err(|e| e.sanitized_message())?;
    if read_only {
        return Ok(SequenceDropResponse {
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
        });
    }

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;

    if !driver.supports_sequences() {
        return Ok(SequenceDropResponse {
            success: false,
            result: None,
            error: Some(SEQUENCES_NOT_SUPPORTED.to_string()),
        });
    }

    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let interceptor_env = map_environment(&environment);

    let query_preview = format!("DROP SEQUENCE {}.{}", database, sequence_name);

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

        return Ok(SequenceDropResponse {
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
        .drop_sequence(session, &namespace, &sequence_name)
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
            Ok(SequenceDropResponse {
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
                    error: Some(e.sanitized_message()),
                    execution_time_ms: 0.0,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(SequenceDropResponse {
                success: false,
                result: None,
                error: Some(e.sanitized_message()),
            })
        }
    }
}

// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::engine::types::SessionId;
use crate::export::types::{ExportCancelResponse, ExportConfig, ExportStartResponse};

fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

fn parse_export_id(id: &str) -> Result<String, String> {
    Uuid::parse_str(id).map_err(|e| format!("Invalid export ID: {}", e))?;
    Ok(id.to_string())
}

#[tauri::command]
pub async fn start_export(
    state: State<'_, crate::SharedState>,
    window: tauri::Window,
    session_id: String,
    config: ExportConfig,
    export_id: Option<String>,
) -> Result<ExportStartResponse, String> {
    let (session_manager, export_pipeline, query_rate_limiter, interceptor, policy) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.export_pipeline),
            Arc::clone(&state.query_rate_limiter),
            Arc::clone(&state.interceptor),
            state.policy.clone(),
        )
    };

    let session = parse_session_id(&session_id)?;

    // Route the user-supplied export query through the same safety preflight as
    // execute_query: read-only mode, production guards, dangerous-query and
    // safety-rule checks. Without this, an export could run an arbitrary
    // `DELETE … RETURNING *` on a read-only or production connection.
    qore_service::query::preflight(
        &session_manager,
        &query_rate_limiter,
        &interceptor,
        &policy,
        session,
        &session_id,
        &config.query,
        config.namespace.as_ref(),
        false,
    )
    .await?;

    let export_id = match export_id {
        Some(id) => parse_export_id(&id)?,
        None => Uuid::new_v4().to_string(),
    };
    let export_id = export_pipeline
        .clone()
        .start_export(session_manager, session, export_id, config, window)
        .await?;

    Ok(ExportStartResponse { export_id })
}

#[tauri::command]
pub async fn cancel_export(
    state: State<'_, crate::SharedState>,
    export_id: String,
) -> Result<ExportCancelResponse, String> {
    let export_pipeline = {
        let state = state.lock().await;
        Arc::clone(&state.export_pipeline)
    };

    match export_pipeline.cancel_export(&export_id).await {
        Ok(()) => Ok(ExportCancelResponse {
            success: true,
            export_id,
            error: None,
        }),
        Err(err) => Ok(ExportCancelResponse {
            success: false,
            export_id,
            error: Some(err),
        }),
    }
}

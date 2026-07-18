// SPDX-License-Identifier: BUSL-1.1

//! Commands for the agentic chat (Database Agent). Pro feature.

use tauri::State;

use crate::SharedState;

#[cfg(not(feature = "pro"))]
const PRO_REQUIRED: &str = "AI Assistant requires a Pro license.";

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn agent_send_message(
    _state: State<'_, SharedState>,
    _window: tauri::Window,
    _request: serde_json::Value,
) -> Result<(), String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn agent_respond_permission(
    _state: State<'_, SharedState>,
    _permission_id: String,
    _approved: bool,
    _remember: bool,
) -> Result<(), String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn agent_cancel(
    _state: State<'_, SharedState>,
    _request_id: String,
) -> Result<(), String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(feature = "pro")]
use std::sync::Arc;

#[cfg(feature = "pro")]
use qore_service::agent_tools::AgentToolContext;

#[cfg(feature = "pro")]
use super::parse_session_id;
#[cfg(feature = "pro")]
use crate::ai::agent::orchestrator::{self, AgentChatRequest, PermissionDecision};

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn agent_send_message(
    state: State<'_, SharedState>,
    window: tauri::Window,
    request: AgentChatRequest,
) -> Result<(), String> {
    crate::ai::context::validate_user_prompt(&request.prompt)?;

    let (runtime, ai_manager, tool_ctx) = {
        let s = state.lock().await;
        (
            Arc::clone(&s.agent_runtime),
            Arc::clone(&s.ai_manager),
            AgentToolContext::from_service(&s.service),
        )
    };
    let session = parse_session_id(&request.session_id)?;

    tokio::spawn(orchestrator::run(
        window, runtime, ai_manager, tool_ctx, session, request,
    ));
    Ok(())
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn agent_respond_permission(
    state: State<'_, SharedState>,
    permission_id: String,
    approved: bool,
    remember: bool,
) -> Result<(), String> {
    let runtime = {
        let s = state.lock().await;
        Arc::clone(&s.agent_runtime)
    };
    runtime.respond_permission(&permission_id, PermissionDecision { approved, remember })
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn agent_cancel(state: State<'_, SharedState>, request_id: String) -> Result<(), String> {
    let runtime = {
        let s = state.lock().await;
        Arc::clone(&s.agent_runtime)
    };
    runtime.cancel(&request_id);
    Ok(())
}

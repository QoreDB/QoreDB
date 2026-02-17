// SPDX-License-Identifier: BUSL-1.1

//! AI Assistant Tauri Commands
//!
//! Commands for AI-powered query generation, explanation, and schema summarization.
//! AI is a Pro feature — Core builds return an explicit error.

use tauri::State;

use crate::SharedState;

// ─── Core stubs (compiled when pro feature is disabled) ──────

#[cfg(not(feature = "pro"))]
const PRO_REQUIRED: &str = "AI Assistant requires a Pro license.";

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn ai_generate_query(
    _state: State<'_, SharedState>,
    _window: tauri::Window,
    _request: serde_json::Value,
) -> Result<(), String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn ai_explain_result(
    _state: State<'_, SharedState>,
    _session_id: String,
    _query: String,
    _result_summary: String,
    _config: serde_json::Value,
    _namespace: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn ai_summarize_schema(
    _state: State<'_, SharedState>,
    _session_id: String,
    _config: serde_json::Value,
    _namespace: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn ai_fix_error(
    _state: State<'_, SharedState>,
    _window: tauri::Window,
    _request: serde_json::Value,
) -> Result<(), String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn ai_save_api_key(
    _state: State<'_, SharedState>,
    _provider: String,
    _key: String,
) -> Result<(), String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn ai_delete_api_key(
    _state: State<'_, SharedState>,
    _provider: String,
) -> Result<(), String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn ai_get_provider_status(
    _state: State<'_, SharedState>,
) -> Result<Vec<serde_json::Value>, String> {
    Err(PRO_REQUIRED.to_string())
}

// ─── Pro implementation ──────────────────────────────────────

#[cfg(feature = "pro")]
use std::sync::Arc;

#[cfg(feature = "pro")]
use tauri::Emitter;

#[cfg(feature = "pro")]
use uuid::Uuid;

#[cfg(feature = "pro")]
use crate::ai::context;
#[cfg(feature = "pro")]
use crate::ai::provider::extract_query_from_response;
#[cfg(feature = "pro")]
use crate::ai::safety::validate_generated_query;
#[cfg(feature = "pro")]
use crate::ai::types::{
    AiAction, AiConfig, AiProvider, AiRequest, AiResponse, AiStreamChunk,
};
#[cfg(feature = "pro")]
use crate::engine::types::{Namespace, SessionId};

#[cfg(feature = "pro")]
fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

/// Streaming: generate a query from a natural language prompt
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn ai_generate_query(
    state: State<'_, SharedState>,
    window: tauri::Window,
    request: AiRequest,
) -> Result<(), String> {
    stream_ai_request(state, window, request).await
}

/// Streaming: fix a SQL/MQL error
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn ai_fix_error(
    state: State<'_, SharedState>,
    window: tauri::Window,
    request: AiRequest,
) -> Result<(), String> {
    stream_ai_request(state, window, request).await
}

/// Non-streaming: explain a query result
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn ai_explain_result(
    state: State<'_, SharedState>,
    session_id: String,
    query: String,
    result_summary: String,
    config: AiConfig,
    namespace: Option<Namespace>,
) -> Result<AiResponse, String> {
    let (session_manager, ai_manager, virtual_relations) = {
        let s = state.lock().await;
        (
            Arc::clone(&s.session_manager),
            Arc::clone(&s.ai_manager),
            Arc::clone(&s.virtual_relations),
        )
    };

    let sid = parse_session_id(&session_id)?;
    let driver = session_manager
        .get_driver(sid)
        .await
        .map_err(|e| e.to_string())?;
    let driver_id = driver.driver_id().to_string();

    let ns = namespace.unwrap_or_else(|| Namespace::new("default"));

    let schema_ctx = context::build_context(
        &session_manager,
        sid,
        &ns,
        &driver_id,
        &virtual_relations,
        None,
        &query,
    )
    .await?;

    let user_prompt = format!(
        "Explain the following query and its results:\n\nQuery:\n```\n{}\n```\n\nResult summary:\n{}\n\nProvide a concise explanation of what this query does and what the results mean.",
        query, result_summary
    );

    let content = collect_streamed_response(
        &ai_manager,
        &config,
        &schema_ctx.system_prompt,
        &user_prompt,
    )
    .await?;

    Ok(AiResponse {
        request_id: Uuid::new_v4().to_string(),
        content,
        generated_query: None,
        safety_analysis: None,
        provider_used: config.provider,
        tokens_used: None,
    })
}

/// Non-streaming: summarize the schema of the active connection
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn ai_summarize_schema(
    state: State<'_, SharedState>,
    session_id: String,
    config: AiConfig,
    namespace: Option<Namespace>,
) -> Result<AiResponse, String> {
    let (session_manager, ai_manager, virtual_relations) = {
        let s = state.lock().await;
        (
            Arc::clone(&s.session_manager),
            Arc::clone(&s.ai_manager),
            Arc::clone(&s.virtual_relations),
        )
    };

    let sid = parse_session_id(&session_id)?;
    let driver = session_manager
        .get_driver(sid)
        .await
        .map_err(|e| e.to_string())?;
    let driver_id = driver.driver_id().to_string();

    let ns = namespace.unwrap_or_else(|| Namespace::new("default"));

    let schema_ctx = context::build_context(
        &session_manager,
        sid,
        &ns,
        &driver_id,
        &virtual_relations,
        None,
        "",
    )
    .await?;

    let user_prompt = "Summarize this database schema in a clear and concise way. Describe the main tables, their purposes, and the relationships between them.";

    let content = collect_streamed_response(
        &ai_manager,
        &config,
        &schema_ctx.system_prompt,
        user_prompt,
    )
    .await?;

    Ok(AiResponse {
        request_id: Uuid::new_v4().to_string(),
        content,
        generated_query: None,
        safety_analysis: None,
        provider_used: config.provider,
        tokens_used: None,
    })
}

/// Store an API key for a provider
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn ai_save_api_key(
    state: State<'_, SharedState>,
    provider: AiProvider,
    key: String,
) -> Result<(), String> {
    let ai_manager = {
        let s = state.lock().await;
        Arc::clone(&s.ai_manager)
    };
    ai_manager.save_api_key(&provider, &key)
}

/// Delete an API key for a provider
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn ai_delete_api_key(
    state: State<'_, SharedState>,
    provider: AiProvider,
) -> Result<(), String> {
    let ai_manager = {
        let s = state.lock().await;
        Arc::clone(&s.ai_manager)
    };
    ai_manager.delete_api_key(&provider)
}

/// List all providers with their configuration status
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn ai_get_provider_status(
    state: State<'_, SharedState>,
) -> Result<Vec<crate::ai::types::AiProviderStatus>, String> {
    let ai_manager = {
        let s = state.lock().await;
        Arc::clone(&s.ai_manager)
    };
    Ok(ai_manager.list_configured_providers())
}

// ─── Internal helpers (Pro only) ─────────────────────────────

/// Collect the full response from a streamed AI request (used for non-streaming commands)
#[cfg(feature = "pro")]
async fn collect_streamed_response(
    ai_manager: &Arc<crate::ai::manager::AiManager>,
    config: &AiConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    let provider = ai_manager
        .get_provider(&config.provider)
        .ok_or_else(|| format!("Provider {:?} not available", config.provider))?;

    let api_key = if config.provider.requires_api_key() {
        ai_manager.get_api_key(&config.provider)?
    } else {
        String::new()
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<AiStreamChunk>(64);
    let request_id = Uuid::new_v4().to_string();

    let system_prompt = system_prompt.to_string();
    let user_prompt = user_prompt.to_string();
    let config_clone = config.clone();
    let rid = request_id.clone();

    tokio::spawn(async move {
        if let Err(e) = provider
            .stream(
                &api_key,
                &system_prompt,
                &user_prompt,
                &config_clone,
                tx.clone(),
                rid.clone(),
            )
            .await
        {
            let _ = tx
                .send(AiStreamChunk {
                    request_id: rid,
                    delta: String::new(),
                    done: true,
                    error: Some(e),
                    generated_query: None,
                    safety_analysis: None,
                })
                .await;
        }
    });

    let mut content = String::new();
    while let Some(chunk) = rx.recv().await {
        if let Some(e) = chunk.error {
            return Err(e);
        }
        content.push_str(&chunk.delta);
    }

    Ok(content)
}

/// Stream an AI request and emit chunks to the frontend via window events
#[cfg(feature = "pro")]
async fn stream_ai_request(
    state: State<'_, SharedState>,
    window: tauri::Window,
    request: AiRequest,
) -> Result<(), String> {
    let (session_manager, ai_manager, virtual_relations) = {
        let s = state.lock().await;
        (
            Arc::clone(&s.session_manager),
            Arc::clone(&s.ai_manager),
            Arc::clone(&s.virtual_relations),
        )
    };

    let sid = parse_session_id(&request.session_id)?;
    let driver = session_manager
        .get_driver(sid)
        .await
        .map_err(|e| e.to_string())?;
    let driver_id = driver.driver_id().to_string();

    let ns = request
        .namespace
        .clone()
        .unwrap_or_else(|| Namespace::new("default"));

    let schema_ctx = context::build_context(
        &session_manager,
        sid,
        &ns,
        &driver_id,
        &virtual_relations,
        request.connection_id.as_deref(),
        &request.prompt,
    )
    .await?;

    let user_prompt = build_user_prompt(&request);

    let provider = ai_manager
        .get_provider(&request.config.provider)
        .ok_or_else(|| format!("Provider {:?} not available", request.config.provider))?;

    let api_key = if request.config.provider.requires_api_key() {
        ai_manager.get_api_key(&request.config.provider)?
    } else {
        String::new()
    };

    let request_id = request.request_id.clone();
    let config = request.config.clone();
    let system_prompt = schema_ctx.system_prompt;
    let event_name = format!("ai_stream:{}", request_id);

    // Spawn the streaming task
    tokio::spawn(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<AiStreamChunk>(64);
        let rid = request_id.clone();
        let event = event_name.clone();

        // Spawn the provider stream
        let provider_handle = tokio::spawn(async move {
            provider
                .stream(&api_key, &system_prompt, &user_prompt, &config, tx, rid)
                .await
        });

        // Forward chunks to the frontend
        let mut full_response = String::new();
        while let Some(chunk) = rx.recv().await {
            full_response.push_str(&chunk.delta);
            let _ = window.emit(&event, &chunk);
        }

        // Wait for the provider to finish
        let stream_result = provider_handle.await;
        let error = match stream_result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e),
            Err(e) => Some(format!("Stream task panicked: {}", e)),
        };

        // Extract query and run safety analysis
        let generated_query = extract_query_from_response(&full_response);
        let safety_analysis = generated_query
            .as_ref()
            .map(|q| validate_generated_query(&driver_id, q));

        // Send final chunk with done=true
        let final_chunk = AiStreamChunk {
            request_id: request_id.clone(),
            delta: String::new(),
            done: true,
            error,
            generated_query,
            safety_analysis,
        };
        let _ = window.emit(&event_name, &final_chunk);
    });

    Ok(())
}

/// Build the user-facing prompt based on the action type
#[cfg(feature = "pro")]
fn build_user_prompt(request: &AiRequest) -> String {
    match &request.action {
        AiAction::GenerateQuery => {
            format!(
                "Generate a query for the following request:\n\n{}",
                request.prompt
            )
        }
        AiAction::FixError => {
            let original = request
                .original_query
                .as_deref()
                .unwrap_or("(no query provided)");
            let error = request
                .error_context
                .as_deref()
                .unwrap_or("(no error message)");
            format!(
                "Fix the following query that produced an error:\n\nQuery:\n```\n{}\n```\n\nError:\n{}\n\n{}\n\nProvide the corrected query.",
                original, error, request.prompt
            )
        }
        AiAction::ExplainResult => {
            let result = request
                .result_context
                .as_deref()
                .unwrap_or("(no result provided)");
            format!(
                "Explain this query result:\n\n{}\n\n{}",
                result, request.prompt
            )
        }
        AiAction::SummarizeSchema => request.prompt.clone(),
    }
}

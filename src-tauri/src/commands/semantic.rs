// SPDX-License-Identifier: BUSL-1.1

//! Commands for local semantic schema search. Pro feature — Core builds
//! return an explicit error.

use tauri::State;

use crate::SharedState;

#[cfg(not(feature = "pro"))]
const PRO_REQUIRED: &str = "Semantic search requires a Pro license.";

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn semantic_status(
    _state: State<'_, SharedState>,
    _session_id: Option<String>,
) -> Result<serde_json::Value, String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn semantic_set_config(
    _state: State<'_, SharedState>,
    _config: serde_json::Value,
) -> Result<serde_json::Value, String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn semantic_reindex(
    _state: State<'_, SharedState>,
    _session_id: String,
) -> Result<serde_json::Value, String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn semantic_search(
    _state: State<'_, SharedState>,
    _session_id: String,
    _query: String,
    _limit: Option<u32>,
) -> Result<serde_json::Value, String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(feature = "pro")]
use std::sync::Arc;

#[cfg(feature = "pro")]
use serde::Serialize;

#[cfg(feature = "pro")]
use super::parse_session_id;
#[cfg(feature = "pro")]
use super::workspace::SharedWorkspaceManager;
#[cfg(feature = "pro")]
use crate::engine::SessionManager;
#[cfg(feature = "pro")]
use crate::semantic::ollama::OllamaEmbedder;
#[cfg(feature = "pro")]
use crate::semantic::store::SemanticHit;
#[cfg(feature = "pro")]
use crate::semantic::{IndexSummary, SemanticConfig};

#[cfg(feature = "pro")]
const DEFAULT_SEARCH_LIMIT: u32 = 8;

#[cfg(feature = "pro")]
#[derive(Serialize)]
pub struct IndexInfo {
    pub objects: u64,
    pub building: bool,
}

#[cfg(feature = "pro")]
#[derive(Serialize)]
pub struct SemanticStatusResponse {
    pub enabled: bool,
    pub model: String,
    pub base_url: String,
    pub ollama_running: bool,
    pub model_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<IndexInfo>,
}

#[cfg(feature = "pro")]
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticSearchStatus {
    Ready,
    Disabled,
    OllamaMissing,
    ModelMissing,
    IndexEmpty,
    Building,
}

#[cfg(feature = "pro")]
#[derive(Serialize)]
pub struct SemanticSearchResponse {
    pub status: SemanticSearchStatus,
    pub results: Vec<SemanticHit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(feature = "pro")]
async fn resolve_connection_key(
    session_manager: &Arc<SessionManager>,
    session_id: &str,
) -> Result<(crate::engine::types::SessionId, String), String> {
    let sid = parse_session_id(session_id)?;
    let key = session_manager
        .connection_key(sid)
        .await
        .ok_or_else(|| "Unknown session".to_string())?;
    Ok((sid, key))
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn semantic_status(
    state: State<'_, SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    session_id: Option<String>,
) -> Result<SemanticStatusResponse, String> {
    let (service, session_manager) = {
        let s = state.lock().await;
        (Arc::clone(&s.semantic), Arc::clone(&s.session_manager))
    };
    let config = service.config();
    let status = OllamaEmbedder::new(&config).detect().await;

    let index = match session_id {
        Some(ref sid) => {
            let (_, key) = resolve_connection_key(&session_manager, sid).await?;
            let project_id = ws_manager.lock().await.project_id();
            let objects = service
                .store_for(&project_id)?
                .count(&key, &config.model)
                .await?;
            Some(IndexInfo {
                objects,
                building: service.is_building(&key),
            })
        }
        None => None,
    };

    Ok(SemanticStatusResponse {
        enabled: config.enabled,
        model: config.model.clone(),
        base_url: config.effective_base_url().to_string(),
        ollama_running: status.running,
        model_available: status.model_available,
        index,
    })
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn semantic_set_config(
    state: State<'_, SharedState>,
    config: SemanticConfig,
) -> Result<SemanticStatusResponse, String> {
    let service = {
        let s = state.lock().await;
        Arc::clone(&s.semantic)
    };
    service.set_config(config);
    let config = service.config();
    let status = OllamaEmbedder::new(&config).detect().await;
    Ok(SemanticStatusResponse {
        enabled: config.enabled,
        model: config.model.clone(),
        base_url: config.effective_base_url().to_string(),
        ollama_running: status.running,
        model_available: status.model_available,
        index: None,
    })
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn semantic_reindex(
    state: State<'_, SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    session_id: String,
) -> Result<IndexSummary, String> {
    let (service, session_manager) = {
        let s = state.lock().await;
        (Arc::clone(&s.semantic), Arc::clone(&s.session_manager))
    };
    let (sid, key) = resolve_connection_key(&session_manager, &session_id).await?;
    let project_id = ws_manager.lock().await.project_id();
    service
        .refresh(&session_manager, sid, &key, &project_id)
        .await
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn semantic_search(
    state: State<'_, SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    session_id: String,
    query: String,
    limit: Option<u32>,
) -> Result<SemanticSearchResponse, String> {
    let (service, session_manager) = {
        let s = state.lock().await;
        (Arc::clone(&s.semantic), Arc::clone(&s.session_manager))
    };
    let config = service.config();

    let empty = |status: SemanticSearchStatus| SemanticSearchResponse {
        status,
        results: Vec::new(),
        error: None,
    };

    if !config.enabled {
        return Ok(empty(SemanticSearchStatus::Disabled));
    }
    let (_, key) = resolve_connection_key(&session_manager, &session_id).await?;
    if service.is_building(&key) {
        return Ok(empty(SemanticSearchStatus::Building));
    }
    let project_id = ws_manager.lock().await.project_id();
    let store = service.store_for(&project_id)?;
    if store.count(&key, &config.model).await? == 0 {
        return Ok(empty(SemanticSearchStatus::IndexEmpty));
    }
    let ollama = OllamaEmbedder::new(&config).detect().await;
    if !ollama.running {
        return Ok(empty(SemanticSearchStatus::OllamaMissing));
    }
    if !ollama.model_available {
        return Ok(empty(SemanticSearchStatus::ModelMissing));
    }

    let limit = limit.unwrap_or(DEFAULT_SEARCH_LIMIT).min(50);
    match service.search(&key, &project_id, &query, limit).await {
        Ok(results) => Ok(SemanticSearchResponse {
            status: SemanticSearchStatus::Ready,
            results,
            error: None,
        }),
        Err(e) => Ok(SemanticSearchResponse {
            status: SemanticSearchStatus::Ready,
            results: Vec::new(),
            error: Some(e),
        }),
    }
}

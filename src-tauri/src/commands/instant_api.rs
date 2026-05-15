// SPDX-License-Identifier: BUSL-1.1

//! Tauri commands for the Instant Data API (Pro).
//!
//! Frontend usage:
//! ```ts
//! await invoke('start_instant_api', { port: 4787 });
//! const status = await invoke<InstantApiStatus>('get_instant_api_status');
//! const list = await invoke<EndpointMeta[]>('list_endpoints');
//! const created = await invoke<{ endpoint: EndpointMeta; token: string }>(
//!   'create_endpoint',
//!   { name, connectionId, querySource, params, shape, pageSize },
//! );
//! await invoke('delete_endpoint', { id });
//! await invoke('stop_instant_api');
//! ```

#![cfg(feature = "pro")]

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex as TokioMutex;

use crate::api::auth::issue_token;
use crate::api::endpoints::{EndpointStore, StoreError};
use crate::api::server::{ApiServer, ServerError};
use crate::api::types::{
    Endpoint, EndpointMeta, EndpointParam, InstantApiStatus, QueryShape,
};
use crate::commands::workspace::SharedWorkspaceManager;

/// Pro-only shared state: the API server is created lazily on first
/// `start_instant_api` and dropped on `stop_instant_api` (so a workspace
/// switch surfaces with fresh credentials/storage paths).
pub type SharedInstantApi = Arc<TokioMutex<InstantApiState>>;

pub struct InstantApiState {
    pub store: Arc<EndpointStore>,
    pub server: Option<Arc<ApiServer>>,
}

impl InstantApiState {
    pub fn new(data_dir: PathBuf) -> Result<Self, StoreError> {
        let store = Arc::new(EndpointStore::new(data_dir)?);
        Ok(Self {
            store,
            server: None,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateEndpointResponse {
    pub endpoint: EndpointMeta,
    /// One-shot raw token. The client must capture this — it is never
    /// returned again.
    pub token: String,
}

/// Starts the local HTTP server on the requested port (default 4787).
#[tauri::command]
pub async fn start_instant_api(
    app: AppHandle,
    api_state: State<'_, SharedInstantApi>,
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    port: Option<u16>,
) -> Result<InstantApiStatus, String> {
    let project_id = {
        let mgr = ws_manager.lock().await;
        mgr.project_id()
    };
    let storage_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let session_manager = {
        let st = state.lock().await;
        Arc::clone(&st.session_manager)
    };

    let mut guard = api_state.lock().await;
    if guard.server.is_some() {
        return build_status(&guard).await;
    }

    let server = Arc::new(ApiServer::new(
        Arc::clone(&guard.store),
        session_manager,
        project_id,
        storage_dir,
    ));
    server
        .start(port)
        .await
        .map_err(|e: ServerError| e.to_string())?;
    guard.server = Some(server);
    build_status(&guard).await
}

/// Stops the server and drains cached upstream sessions. Idempotent: returns
/// `Ok` even when the server was already stopped.
#[tauri::command]
pub async fn stop_instant_api(
    api_state: State<'_, SharedInstantApi>,
) -> Result<InstantApiStatus, String> {
    let mut guard = api_state.lock().await;
    if let Some(server) = guard.server.take() {
        server.stop().await.map_err(|e| e.to_string())?;
    }
    build_status(&guard).await
}

#[tauri::command]
pub async fn get_instant_api_status(
    api_state: State<'_, SharedInstantApi>,
) -> Result<InstantApiStatus, String> {
    let guard = api_state.lock().await;
    build_status(&guard).await
}

#[tauri::command]
pub async fn list_endpoints(
    api_state: State<'_, SharedInstantApi>,
) -> Result<Vec<EndpointMeta>, String> {
    let guard = api_state.lock().await;
    Ok(guard.store.list())
}

/// Returns the OpenAPI 3.1 document generated from the current registry.
/// Pretty-printed so the preview UI stays readable when piped to the user.
#[tauri::command]
pub async fn get_openapi_document(
    api_state: State<'_, SharedInstantApi>,
) -> Result<String, String> {
    let guard = api_state.lock().await;
    let doc = crate::api::openapi::build_document(&guard.store);
    serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_endpoint(
    api_state: State<'_, SharedInstantApi>,
    name: String,
    connection_id: String,
    query_source: String,
    #[allow(non_snake_case)] params: Option<Vec<EndpointParam>>,
    shape: Option<QueryShape>,
    page_size: Option<u32>,
) -> Result<CreateEndpointResponse, String> {
    let token = issue_token().map_err(|e| e.to_string())?;
    let guard = api_state.lock().await;
    let endpoint: Endpoint = guard
        .store
        .create(
            name,
            connection_id,
            query_source,
            params.unwrap_or_default(),
            shape.unwrap_or(QueryShape::Rows),
            page_size.unwrap_or(100),
            token.hash,
        )
        .map_err(|e| e.to_string())?;

    Ok(CreateEndpointResponse {
        endpoint: EndpointMeta::from(&endpoint),
        token: token.value,
    })
}

/// Replaces the bearer token for an existing endpoint. The new raw token is
/// returned **once** to the caller, exactly like `create_endpoint`. The old
/// token stops being accepted as soon as the store has been flushed.
#[tauri::command]
pub async fn regenerate_endpoint_token(
    api_state: State<'_, SharedInstantApi>,
    id: String,
) -> Result<CreateEndpointResponse, String> {
    let token = issue_token().map_err(|e| e.to_string())?;
    let guard = api_state.lock().await;
    let endpoint = guard
        .store
        .regenerate_token(&id, token.hash)
        .map_err(|e| e.to_string())?;
    Ok(CreateEndpointResponse {
        endpoint: EndpointMeta::from(&endpoint),
        token: token.value,
    })
}

#[tauri::command]
pub async fn delete_endpoint(
    api_state: State<'_, SharedInstantApi>,
    id: String,
) -> Result<(), String> {
    let guard = api_state.lock().await;
    // Look up the connection_id before deleting so we can invalidate the
    // session cache afterwards. Missing endpoints are surfaced as a normal
    // NotFound error by the store.
    let connection_id = guard
        .store
        .list()
        .into_iter()
        .find(|e| e.id == id)
        .map(|e| e.connection_id);

    guard.store.delete(&id).map_err(|e| e.to_string())?;

    if let (Some(server), Some(conn_id)) = (guard.server.as_ref(), connection_id) {
        server.on_endpoint_deleted(&id, &conn_id).await;
    }
    Ok(())
}

async fn build_status(state: &InstantApiState) -> Result<InstantApiStatus, String> {
    let endpoints_count = state.store.count();
    match state.server.as_ref() {
        Some(server) => {
            let addr = server.current_addr().await;
            let uptime = server.uptime_secs().await;
            Ok(InstantApiStatus {
                running: addr.is_some(),
                port: addr.map(|a| a.port()),
                base_url: addr.map(|a| format!("http://{}", a)),
                endpoints_count,
                uptime_s: uptime,
            })
        }
        None => Ok(InstantApiStatus {
            running: false,
            port: None,
            base_url: None,
            endpoints_count,
            uptime_s: None,
        }),
    }
}

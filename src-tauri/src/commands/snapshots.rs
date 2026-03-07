// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::instrument;

use crate::engine::types::{Namespace, QueryResult};
use crate::snapshots::{SnapshotMeta, SnapshotStore};

/// Shared snapshot store state
pub type SharedSnapshotStore = std::sync::Arc<SnapshotStore>;

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveSnapshotRequest {
    pub name: String,
    pub description: Option<String>,
    pub source: String,
    pub source_type: String,
    pub connection_name: Option<String>,
    pub driver: Option<String>,
    pub namespace: Option<Namespace>,
    pub result: QueryResult,
}

#[derive(Debug, Serialize)]
pub struct SnapshotResponse {
    pub success: bool,
    pub meta: Option<SnapshotMeta>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SnapshotListResponse {
    pub success: bool,
    pub snapshots: Vec<SnapshotMeta>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SnapshotDataResponse {
    pub success: bool,
    pub result: Option<QueryResult>,
    pub meta: Option<SnapshotMeta>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SnapshotDeleteResponse {
    pub success: bool,
    pub error: Option<String>,
}

#[tauri::command]
#[instrument(skip(store, request), fields(name = %request.name))]
pub async fn save_snapshot(
    store: State<'_, SharedSnapshotStore>,
    request: SaveSnapshotRequest,
) -> Result<SnapshotResponse, String> {
    match store.save(
        request.name,
        request.description,
        request.source,
        request.source_type,
        request.connection_name,
        request.driver,
        request.namespace,
        &request.result,
    ) {
        Ok(meta) => Ok(SnapshotResponse {
            success: true,
            meta: Some(meta),
            error: None,
        }),
        Err(e) => Ok(SnapshotResponse {
            success: false,
            meta: None,
            error: Some(e),
        }),
    }
}

#[tauri::command]
#[instrument(skip(store))]
pub async fn list_snapshots(
    store: State<'_, SharedSnapshotStore>,
) -> Result<SnapshotListResponse, String> {
    match store.list() {
        Ok(snapshots) => Ok(SnapshotListResponse {
            success: true,
            snapshots,
            error: None,
        }),
        Err(e) => Ok(SnapshotListResponse {
            success: false,
            snapshots: Vec::new(),
            error: Some(e),
        }),
    }
}

#[tauri::command]
#[instrument(skip(store))]
pub async fn get_snapshot(
    store: State<'_, SharedSnapshotStore>,
    snapshot_id: String,
) -> Result<SnapshotDataResponse, String> {
    match store.get(&snapshot_id) {
        Ok(snapshot) => Ok(SnapshotDataResponse {
            success: true,
            result: Some(snapshot.to_query_result()),
            meta: Some(snapshot.meta),
            error: None,
        }),
        Err(e) => Ok(SnapshotDataResponse {
            success: false,
            result: None,
            meta: None,
            error: Some(e),
        }),
    }
}

#[tauri::command]
#[instrument(skip(store))]
pub async fn delete_snapshot(
    store: State<'_, SharedSnapshotStore>,
    snapshot_id: String,
) -> Result<SnapshotDeleteResponse, String> {
    match store.delete(&snapshot_id) {
        Ok(()) => Ok(SnapshotDeleteResponse {
            success: true,
            error: None,
        }),
        Err(e) => Ok(SnapshotDeleteResponse {
            success: false,
            error: Some(e),
        }),
    }
}

#[tauri::command]
#[instrument(skip(store))]
pub async fn rename_snapshot(
    store: State<'_, SharedSnapshotStore>,
    snapshot_id: String,
    new_name: String,
) -> Result<SnapshotResponse, String> {
    match store.rename(&snapshot_id, new_name) {
        Ok(meta) => Ok(SnapshotResponse {
            success: true,
            meta: Some(meta),
            error: None,
        }),
        Err(e) => Ok(SnapshotResponse {
            success: false,
            meta: None,
            error: Some(e),
        }),
    }
}

use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use crate::virtual_relations::VirtualRelation;

#[derive(Debug, Serialize)]
pub struct VirtualRelationsResponse {
    pub success: bool,
    pub relations: Option<Vec<VirtualRelation>>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VirtualRelationMutationResponse {
    pub success: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn list_virtual_relations(
    state: State<'_, crate::SharedState>,
    connection_id: String,
) -> Result<VirtualRelationsResponse, String> {
    let vr_store = {
        let state = state.lock().await;
        Arc::clone(&state.virtual_relations)
    };
    let relations = vr_store.list(&connection_id);
    Ok(VirtualRelationsResponse {
        success: true,
        relations: Some(relations),
        error: None,
    })
}

#[tauri::command]
pub async fn add_virtual_relation(
    state: State<'_, crate::SharedState>,
    connection_id: String,
    relation: VirtualRelation,
) -> Result<VirtualRelationMutationResponse, String> {
    let vr_store = {
        let state = state.lock().await;
        Arc::clone(&state.virtual_relations)
    };
    match vr_store.add(&connection_id, relation) {
        Ok(()) => Ok(VirtualRelationMutationResponse {
            success: true,
            error: None,
        }),
        Err(e) => Ok(VirtualRelationMutationResponse {
            success: false,
            error: Some(e),
        }),
    }
}

#[tauri::command]
pub async fn update_virtual_relation(
    state: State<'_, crate::SharedState>,
    connection_id: String,
    relation: VirtualRelation,
) -> Result<VirtualRelationMutationResponse, String> {
    let vr_store = {
        let state = state.lock().await;
        Arc::clone(&state.virtual_relations)
    };
    match vr_store.update(&connection_id, relation) {
        Ok(()) => Ok(VirtualRelationMutationResponse {
            success: true,
            error: None,
        }),
        Err(e) => Ok(VirtualRelationMutationResponse {
            success: false,
            error: Some(e),
        }),
    }
}

#[tauri::command]
pub async fn delete_virtual_relation(
    state: State<'_, crate::SharedState>,
    connection_id: String,
    relation_id: String,
) -> Result<VirtualRelationMutationResponse, String> {
    let vr_store = {
        let state = state.lock().await;
        Arc::clone(&state.virtual_relations)
    };
    match vr_store.delete(&connection_id, &relation_id) {
        Ok(()) => Ok(VirtualRelationMutationResponse {
            success: true,
            error: None,
        }),
        Err(e) => Ok(VirtualRelationMutationResponse {
            success: false,
            error: Some(e),
        }),
    }
}

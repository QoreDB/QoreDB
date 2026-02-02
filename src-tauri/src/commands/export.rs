use std::sync::Arc;

use tauri::State;
use uuid::Uuid;

use crate::engine::types::SessionId;
use crate::export::types::{ExportCancelResponse, ExportConfig, ExportStartResponse};

fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

#[tauri::command]
pub async fn start_export(
    state: State<'_, crate::SharedState>,
    window: tauri::Window,
    session_id: String,
    config: ExportConfig,
) -> Result<ExportStartResponse, String> {
    let (session_manager, export_pipeline) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.export_pipeline),
        )
    };

    let session = parse_session_id(&session_id)?;
    let export_id = export_pipeline
        .clone()
        .start_export(session_manager, session, config, window)
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

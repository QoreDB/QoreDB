// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use tauri::State;

use crate::commands::snapshots::SharedSnapshotStore;
use crate::share::types::{
    ShareCleanupResponse, SharePrepareResponse, ShareProviderConfig, ShareProviderStatus,
    ShareSnapshotRequest, ShareUploadResponse,
};
use crate::share::write_query_result_to_file;
use crate::SharedState;

fn extension_for_format(format: &crate::export::types::ExportFormat) -> &'static str {
    match format {
        crate::export::types::ExportFormat::Csv => "csv",
        crate::export::types::ExportFormat::Json => "json",
        crate::export::types::ExportFormat::SqlInsert => "sql",
        crate::export::types::ExportFormat::Html => "html",
        crate::export::types::ExportFormat::Xlsx => "xlsx",
        crate::export::types::ExportFormat::Parquet => "parquet",
    }
}

#[tauri::command]
pub async fn share_prepare_export(
    state: State<'_, SharedState>,
    file_name: String,
    extension: String,
) -> Result<SharePrepareResponse, String> {
    let share_manager = {
        let state = state.lock().await;
        Arc::clone(&state.share_manager)
    };

    let (share_id, output_path, prepared_file_name) =
        share_manager.prepare_export(&file_name, &extension)?;

    Ok(SharePrepareResponse {
        share_id,
        output_path: output_path.to_string_lossy().to_string(),
        file_name: prepared_file_name,
    })
}

#[tauri::command]
pub async fn share_cleanup_export(
    state: State<'_, SharedState>,
    share_id: String,
) -> Result<ShareCleanupResponse, String> {
    let share_manager = {
        let state = state.lock().await;
        Arc::clone(&state.share_manager)
    };

    share_manager.cleanup_prepared_export(&share_id)?;
    Ok(ShareCleanupResponse { success: true })
}

#[tauri::command]
pub async fn share_upload_prepared_export(
    state: State<'_, SharedState>,
    share_id: String,
    provider: ShareProviderConfig,
) -> Result<ShareUploadResponse, String> {
    let share_manager = {
        let state = state.lock().await;
        Arc::clone(&state.share_manager)
    };

    let share_url = share_manager
        .upload_prepared_export(&share_id, &provider)
        .await?;
    Ok(ShareUploadResponse { share_url })
}

#[tauri::command]
pub async fn share_save_provider_token(
    state: State<'_, SharedState>,
    token: String,
) -> Result<(), String> {
    let share_manager = {
        let state = state.lock().await;
        Arc::clone(&state.share_manager)
    };

    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err("Share token cannot be empty".to_string());
    }

    share_manager.save_provider_token(trimmed)
}

#[tauri::command]
pub async fn share_delete_provider_token(state: State<'_, SharedState>) -> Result<(), String> {
    let share_manager = {
        let state = state.lock().await;
        Arc::clone(&state.share_manager)
    };

    share_manager.delete_provider_token()
}

#[tauri::command]
pub async fn share_get_provider_status(
    state: State<'_, SharedState>,
) -> Result<ShareProviderStatus, String> {
    let share_manager = {
        let state = state.lock().await;
        Arc::clone(&state.share_manager)
    };

    Ok(ShareProviderStatus {
        has_token: share_manager.has_provider_token(),
    })
}

#[tauri::command]
pub async fn share_snapshot(
    state: State<'_, SharedState>,
    snapshot_store: State<'_, SharedSnapshotStore>,
    request: ShareSnapshotRequest,
) -> Result<ShareUploadResponse, String> {
    let share_manager = {
        let state = state.lock().await;
        Arc::clone(&state.share_manager)
    };

    let snapshot = snapshot_store.get(&request.snapshot_id)?;
    let file_name = request
        .file_name
        .clone()
        .unwrap_or_else(|| snapshot.meta.name.clone());
    let extension = extension_for_format(&request.format);
    let (output_path, prepared_file_name) = share_manager.create_temp_file_path(&file_name, extension)?;

    let resolved_table_name = request.table_name.clone().or_else(|| {
        if matches!(request.format, crate::export::types::ExportFormat::SqlInsert)
            && snapshot.meta.source_type == "table"
        {
            Some(snapshot.meta.source.clone())
        } else {
            None
        }
    });

    let write_result = write_query_result_to_file(
        request.format.clone(),
        &output_path.to_string_lossy(),
        request.include_headers,
        resolved_table_name,
        snapshot.meta.namespace.clone(),
        snapshot.meta.driver.as_deref().unwrap_or(""),
        &snapshot.to_query_result(),
        request.limit,
    )
    .await;

    if let Err(err) = write_result {
        let _ = std::fs::remove_file(&output_path);
        return Err(err);
    }

    let upload_result = share_manager
        .upload_file(&output_path, &prepared_file_name, &request.provider)
        .await;
    let _ = std::fs::remove_file(&output_path);

    let share_url = upload_result?;
    Ok(ShareUploadResponse { share_url })
}

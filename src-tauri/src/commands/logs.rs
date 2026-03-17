// SPDX-License-Identifier: Apache-2.0

//! Log export commands.

use std::fs::{self, OpenOptions};
use std::io::Write;

use serde::Serialize;

use crate::observability;

const UI_DEBUG_FILENAME: &str = "ui-debug.txt";

/// Response wrapper for log export
#[derive(Debug, Serialize)]
pub struct LogsExportResponse {
    pub success: bool,
    pub filename: Option<String>,
    pub content: Option<String>,
    pub error: Option<String>,
}

/// Exports backend logs for support.
#[tauri::command]
pub async fn export_logs() -> Result<LogsExportResponse, String> {
    match observability::collect_logs() {
        Ok(export) => Ok(LogsExportResponse {
            success: true,
            filename: Some(export.filename),
            content: Some(export.content),
            error: None,
        }),
        Err(err) => Ok(LogsExportResponse {
            success: false,
            filename: None,
            content: None,
            error: Some(err),
        }),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct FrontendLogEntry {
    pub level: String,
    pub message: String,
    pub stack: Option<String>,
    pub timestamp: String,
}

#[tauri::command]
pub async fn log_frontend_message(entry: FrontendLogEntry) -> Result<(), String> {
    match entry.level.as_str() {
        "error" => tracing::error!(target: "frontend", stack = ?entry.stack, "{}", entry.message),
        "warn" => tracing::warn!(target: "frontend", stack = ?entry.stack, "{}", entry.message),
        "info" => tracing::info!(target: "frontend", "{}", entry.message),
        "debug" => tracing::debug!(target: "frontend", "{}", entry.message),
        _ => tracing::info!(target: "frontend", level = %entry.level, "{}", entry.message),
    }
    Ok(())
}

#[tauri::command]
pub async fn append_ui_debug_log(content: String) -> Result<String, String> {
    let log_dir = observability::log_directory();
    fs::create_dir_all(&log_dir).map_err(|e| {
        format!(
            "Failed to create log directory {}: {}",
            log_dir.display(),
            e
        )
    })?;

    let file_path = log_dir.join(UI_DEBUG_FILENAME);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .map_err(|e| format!("Failed to open UI debug log {}: {}", file_path.display(), e))?;

    file.write_all(content.as_bytes()).map_err(|e| {
        format!(
            "Failed to append UI debug log {}: {}",
            file_path.display(),
            e
        )
    })?;
    file.flush().map_err(|e| {
        format!(
            "Failed to flush UI debug log {}: {}",
            file_path.display(),
            e
        )
    })?;

    Ok(file_path.display().to_string())
}

//! Log export commands.

use serde::Serialize;

use crate::observability;

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

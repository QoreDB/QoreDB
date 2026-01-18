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

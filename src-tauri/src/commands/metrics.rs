//! Metrics commands (dev-only).

use serde::Serialize;

use crate::metrics;

/// Response wrapper for metrics snapshot
#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub success: bool,
    pub metrics: Option<metrics::QueryMetricsSnapshot>,
    pub error: Option<String>,
}

/// Returns current metrics snapshot (dev builds only).
#[tauri::command]
pub async fn get_metrics() -> Result<MetricsResponse, String> {
    if !cfg!(debug_assertions) {
        return Ok(MetricsResponse {
            success: false,
            metrics: None,
            error: Some("Metrics are only available in dev builds".to_string()),
        });
    }

    Ok(MetricsResponse {
        success: true,
        metrics: Some(metrics::snapshot()),
        error: None,
    })
}

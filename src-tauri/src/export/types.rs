use serde::{Deserialize, Serialize};

use crate::engine::types::Namespace;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Json,
    SqlInsert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    pub query: String,
    pub namespace: Option<Namespace>,
    pub output_path: String,
    pub format: ExportFormat,
    pub table_name: Option<String>,
    pub include_headers: bool,
    pub batch_size: Option<u32>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportState {
    Pending,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportProgress {
    pub export_id: String,
    pub state: ExportState,
    pub rows_exported: u64,
    pub bytes_written: u64,
    pub elapsed_ms: u64,
    pub rows_per_second: Option<f64>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExportStartResponse {
    pub export_id: String,
}

#[derive(Debug, Serialize)]
pub struct ExportCancelResponse {
    pub success: bool,
    pub export_id: String,
    pub error: Option<String>,
}

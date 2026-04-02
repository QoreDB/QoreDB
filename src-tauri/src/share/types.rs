// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::export::types::ExportFormat;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareHttpMethod {
    Post,
    Put,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareBodyMode {
    Multipart,
    Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareProviderConfig {
    pub provider_name: Option<String>,
    pub upload_url: String,
    pub method: ShareHttpMethod,
    pub body_mode: ShareBodyMode,
    pub file_field_name: Option<String>,
    pub response_url_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ShareProviderStatus {
    pub has_token: bool,
}

#[derive(Debug, Serialize)]
pub struct SharePrepareResponse {
    pub share_id: String,
    pub output_path: String,
    pub file_name: String,
}

#[derive(Debug, Serialize)]
pub struct ShareUploadResponse {
    pub share_url: String,
}

#[derive(Debug, Serialize)]
pub struct ShareCleanupResponse {
    pub success: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShareSnapshotRequest {
    pub snapshot_id: String,
    pub format: ExportFormat,
    pub include_headers: bool,
    pub table_name: Option<String>,
    pub limit: Option<u64>,
    pub provider: ShareProviderConfig,
    pub file_name: Option<String>,
}

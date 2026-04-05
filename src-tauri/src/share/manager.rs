// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use reqwest::multipart::{Form, Part};
use reqwest::{Client, Method};
use serde_json::Value;
use url::Url;
use uuid::Uuid;

use crate::vault::backend::CredentialProvider;

use super::types::{ShareBodyMode, ShareHttpMethod, ShareProviderConfig};

const KEYRING_SERVICE: &str = "qoredb_share";
const KEYRING_TOKEN_KEY: &str = "provider_token";

struct PreparedShareFile {
    path: PathBuf,
    file_name: String,
}

pub struct ShareManager {
    temp_dir: PathBuf,
    credential_provider: Box<dyn CredentialProvider>,
    client: Client,
    prepared_files: Mutex<HashMap<String, PreparedShareFile>>,
}

impl ShareManager {
    pub fn new(temp_dir: PathBuf, credential_provider: Box<dyn CredentialProvider>) -> Self {
        let _ = std::fs::create_dir_all(&temp_dir);

        Self {
            temp_dir,
            credential_provider,
            client: Client::new(),
            prepared_files: Mutex::new(HashMap::new()),
        }
    }

    pub fn save_provider_token(&self, token: &str) -> Result<(), String> {
        self.credential_provider
            .set_password(KEYRING_SERVICE, KEYRING_TOKEN_KEY, token)
            .map_err(|e| format!("Failed to save share token: {}", e))
    }

    pub fn delete_provider_token(&self) -> Result<(), String> {
        self.credential_provider
            .delete_password(KEYRING_SERVICE, KEYRING_TOKEN_KEY)
            .map_err(|e| format!("Failed to delete share token: {}", e))
    }

    pub fn has_provider_token(&self) -> bool {
        self.credential_provider
            .get_password(KEYRING_SERVICE, KEYRING_TOKEN_KEY)
            .is_ok()
    }

    pub fn prepare_export(
        &self,
        requested_name: &str,
        extension: &str,
    ) -> Result<(String, PathBuf, String), String> {
        let share_id = Uuid::new_v4().to_string();
        let extension = sanitize_extension(extension);
        let stem = sanitize_stem(requested_name);
        let file_name = format!("{}.{}", stem, extension);
        let output_path = self.temp_dir.join(format!("{}_{}", share_id, file_name));

        let mut prepared_files = self.prepared_files.lock();
        prepared_files.insert(
            share_id.clone(),
            PreparedShareFile {
                path: output_path.clone(),
                file_name: file_name.clone(),
            },
        );

        Ok((share_id, output_path, file_name))
    }

    pub fn cleanup_prepared_export(&self, share_id: &str) -> Result<(), String> {
        validate_share_id(share_id)?;

        let prepared = self.prepared_files.lock().remove(share_id);
        if let Some(prepared) = prepared {
            delete_if_exists(&prepared.path)?;
        }

        Ok(())
    }

    pub async fn upload_prepared_export(
        &self,
        share_id: &str,
        provider: &ShareProviderConfig,
    ) -> Result<String, String> {
        validate_share_id(share_id)?;
        let prepared = self
            .prepared_files
            .lock()
            .remove(share_id)
            .ok_or_else(|| "Prepared share export not found".to_string())?;

        let upload_result = self
            .upload_file(&prepared.path, &prepared.file_name, provider)
            .await;
        let _ = delete_if_exists(&prepared.path);
        upload_result
    }

    pub async fn upload_file(
        &self,
        path: &Path,
        file_name: &str,
        provider: &ShareProviderConfig,
    ) -> Result<String, String> {
        validate_provider_config(provider)?;
        let body = tokio::fs::read(path)
            .await
            .map_err(|e| format!("Failed to read export file: {}", e))?;

        self.upload_bytes(file_name, body, provider).await
    }

    pub fn create_temp_file_path(
        &self,
        requested_name: &str,
        extension: &str,
    ) -> Result<(PathBuf, String), String> {
        let share_id = Uuid::new_v4().to_string();
        let extension = sanitize_extension(extension);
        let stem = sanitize_stem(requested_name);
        let file_name = format!("{}.{}", stem, extension);
        let output_path = self.temp_dir.join(format!("{}_{}", share_id, file_name));
        Ok((output_path, file_name))
    }

    pub async fn upload_bytes(
        &self,
        file_name: &str,
        body: Vec<u8>,
        provider: &ShareProviderConfig,
    ) -> Result<String, String> {
        validate_provider_config(provider)?;

        let method = match provider.method {
            ShareHttpMethod::Post => Method::POST,
            ShareHttpMethod::Put => Method::PUT,
        };

        let mut request = self.client.request(method, provider.upload_url.trim());
        if let Some(token) = self.provider_token() {
            request = request.bearer_auth(token);
        }

        match provider.body_mode {
            ShareBodyMode::Multipart => {
                let field_name = provider
                    .file_field_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("file");
                let content_type = content_type_for_file(file_name);
                let part = Part::bytes(body)
                    .file_name(file_name.to_string())
                    .mime_str(content_type)
                    .map_err(|e| format!("Invalid content type: {}", e))?;
                let form = Form::new().part(field_name.to_string(), part);
                request = request.multipart(form);
            }
            ShareBodyMode::Binary => {
                request = request
                    .header("Content-Type", content_type_for_file(file_name))
                    .header("Content-Disposition", format!("attachment; filename=\"{}\"", file_name))
                    .body(body);
            }
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Share upload failed: {}", e))?;

        let status = response.status();
        let response_text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(if response_text.trim().is_empty() {
                format!("Share provider returned HTTP {}", status)
            } else {
                format!("Share provider returned HTTP {}: {}", status, response_text)
            });
        }

        extract_share_url(&response_text, provider.response_url_path.as_deref())
    }

    fn provider_token(&self) -> Option<String> {
        self.credential_provider
            .get_password(KEYRING_SERVICE, KEYRING_TOKEN_KEY)
            .ok()
            .and_then(|value| {
                let trimmed = value.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            })
    }
}

fn validate_share_id(share_id: &str) -> Result<(), String> {
    Uuid::parse_str(share_id)
        .map(|_| ())
        .map_err(|_| "Invalid share ID".to_string())
}

fn sanitize_stem(input: &str) -> String {
    let raw = input.trim();
    let candidate = if raw.is_empty() { "share-export" } else { raw };

    let mut output = String::with_capacity(candidate.len());
    for ch in candidate.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            output.push(ch);
        } else {
            output.push('_');
        }
    }

    let trimmed = output.trim_matches('_');
    if trimmed.is_empty() {
        "share-export".to_string()
    } else {
        trimmed.to_string()
    }
}

fn sanitize_extension(input: &str) -> String {
    let raw = input.trim().trim_start_matches('.');
    if raw.is_empty() {
        return "bin".to_string();
    }

    let sanitized: String = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect();

    if sanitized.is_empty() {
        "bin".to_string()
    } else {
        sanitized.to_lowercase()
    }
}

fn validate_provider_config(provider: &ShareProviderConfig) -> Result<(), String> {
    let upload_url = provider.upload_url.trim();
    if upload_url.is_empty() {
        return Err("Share provider upload URL is required".to_string());
    }

    let parsed = Url::parse(upload_url).map_err(|e| format!("Invalid share upload URL: {}", e))?;
    match parsed.scheme() {
        "https" => {}
        "http" => {
            let host = parsed.host_str().unwrap_or("");
            let is_loopback = host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]";
            if !is_loopback {
                return Err("Share upload URL must use HTTPS for non-localhost hosts".to_string());
            }
        }
        _ => return Err("Share upload URL must use http or https".to_string()),
    }

    if matches!(provider.body_mode, ShareBodyMode::Multipart) {
        let field_name = provider.file_field_name.as_deref().unwrap_or("file").trim();
        if field_name.is_empty() {
            return Err("Share provider multipart field name cannot be empty".to_string());
        }
    }

    Ok(())
}

fn delete_if_exists(path: &Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|e| format!("Failed to delete temporary share file: {}", e))?;
    }
    Ok(())
}

fn content_type_for_file(file_name: &str) -> &'static str {
    if file_name.ends_with(".csv") {
        "text/csv"
    } else if file_name.ends_with(".json") {
        "application/json"
    } else if file_name.ends_with(".html") {
        "text/html"
    } else if file_name.ends_with(".sql") {
        "application/sql"
    } else if file_name.ends_with(".xlsx") {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    } else {
        "application/octet-stream"
    }
}

fn extract_share_url(
    response_text: &str,
    response_url_path: Option<&str>,
) -> Result<String, String> {
    let trimmed = response_text.trim();
    if trimmed.is_empty() {
        return Err("Share provider returned an empty response".to_string());
    }

    if let Some(path) = response_url_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let parsed: Value = serde_json::from_str(trimmed).map_err(|e| {
            format!(
                "Unable to parse JSON response from share provider for path '{}': {}",
                path, e
            )
        })?;
        let value = extract_value_at_path(&parsed, path).ok_or_else(|| {
            format!(
                "Share provider response does not contain path '{}'",
                path
            )
        })?;
        let url = value
            .as_str()
            .ok_or_else(|| format!("Share provider response path '{}' is not a string", path))?;
        return validate_share_url(url);
    }

    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        for key in [
            "url",
            "link",
            "share_url",
            "shareUrl",
            "download_url",
            "downloadUrl",
        ] {
            if let Some(url) = parsed.get(key).and_then(Value::as_str) {
                return validate_share_url(url);
            }
        }
    }

    validate_share_url(trimmed)
}

fn extract_value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        if segment.is_empty() {
            return None;
        }

        current = if let Ok(index) = segment.parse::<usize>() {
            current.as_array()?.get(index)?
        } else {
            current.as_object()?.get(segment)?
        };
    }
    Some(current)
}

fn validate_share_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    let parsed = Url::parse(trimmed).map_err(|e| format!("Invalid share URL: {}", e))?;
    match parsed.scheme() {
        "https" => Ok(parsed.to_string()),
        "http" => {
            let host = parsed.host_str().unwrap_or("");
            let is_loopback =
                host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]";
            if !is_loopback {
                return Err(
                    "Share URL must use HTTPS for non-localhost hosts".to_string(),
                );
            }
            Ok(parsed.to_string())
        }
        _ => Err("Share URL must use http or https".to_string()),
    }
}

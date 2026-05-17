// SPDX-License-Identifier: BUSL-1.1

//! Wire types for Instant Data API endpoints and server status.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Declared parameter type. Validated at request time against the query string.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EndpointParamType {
    String,
    Integer,
    Float,
    Bool,
}

/// Named parameter exposed by an endpoint. The `{{name}}` placeholder in
/// `query_source` is substituted with the typed-and-validated request value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EndpointParam {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: EndpointParamType,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// Shape hint used by [`super::handlers`] to decide pagination defaults.
/// `Rows` = paginated `{ data, page, total }`, `Object` = single object.
/// Defaults to `Rows`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum QueryShape {
    #[default]
    Rows,
    Object,
}

/// Saved endpoint definition. Token is **never** stored — only its Argon2 hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    /// Stable identifier (UUID v4).
    pub id: String,
    /// URL path segment (validated: `[A-Za-z0-9_-]{1,64}`). The full path is
    /// `/api/<name>`.
    pub name: String,
    /// Connection ID (saved connection) — resolved to a session at request
    /// time. Endpoints are tied to one connection.
    pub connection_id: String,
    /// SQL or driver-native query. `{{param}}` placeholders are substituted
    /// before execution. The query is re-checked against `sql_safety` on
    /// every request to reject mutations even if the source is rewritten.
    pub query_source: String,
    /// Declared params surfaced in OpenAPI and validated on request.
    #[serde(default)]
    pub params: Vec<EndpointParam>,
    #[serde(default)]
    pub shape: QueryShape,
    /// Argon2id hash of the issued token. The raw token is shown to the user
    /// only once at creation time.
    pub token_hash: String,
    /// Max rows returned per page when `shape = rows`. Defaults to 100.
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    pub created_at: String,
    pub updated_at: String,
}

fn default_page_size() -> u32 {
    100
}

/// Lightweight projection for listing — never carries the token hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointMeta {
    pub id: String,
    pub name: String,
    pub connection_id: String,
    pub shape: QueryShape,
    pub params_count: u32,
    pub page_size: u32,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&Endpoint> for EndpointMeta {
    fn from(e: &Endpoint) -> Self {
        Self {
            id: e.id.clone(),
            name: e.name.clone(),
            connection_id: e.connection_id.clone(),
            shape: e.shape,
            params_count: e.params.len() as u32,
            page_size: e.page_size,
            created_at: e.created_at.clone(),
            updated_at: e.updated_at.clone(),
        }
    }
}

/// Status payload returned by `get_instant_api_status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstantApiStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub base_url: Option<String>,
    pub endpoints_count: u32,
    /// Seconds since the server started, or `None` when stopped.
    pub uptime_s: Option<u64>,
    /// `true` when the running server is serving HTTPS (self-signed cert).
    /// Defaults to `false` when stopped or when running plain HTTP.
    #[serde(default)]
    pub tls: bool,
}

/// Validated request payload parsed from the request query string for one
/// endpoint. Keyed by param name, holds the verbatim string (substitution
/// happens at handler level).
pub type RequestParams = HashMap<String, String>;

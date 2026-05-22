// SPDX-License-Identifier: BUSL-1.1

//! HTTP request handlers for Instant Data API endpoints.
//!
//! One route is exposed: `GET /api/{name}`. The handler:
//! 1. Looks up the endpoint by name (404 on miss).
//! 2. Authenticates the bearer token against the Argon2 hash (401/403).
//! 3. Consumes a per-endpoint rate-limit token (429).
//! 4. Validates and substitutes query parameters (400).
//! 5. Re-classifies the substituted SQL via [`qore_sql::safety::analyze_sql`]
//!    to reject mutations *after* substitution (400).
//! 6. Executes the query against the cached session (502/500).
//! 7. Serializes rows as JSON objects keyed by column name.
//!
//! Param substitution is deliberately literal-based: each `{{name}}` is
//! replaced by a properly-typed SQL literal (escaped string, parsed
//! integer/float, normalized bool). Combined with the post-substitution
//! safety check, this prevents the substitution channel from sneaking a
//! mutation into a read-only endpoint.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::{json, Value as JsonValue};
use tokio::sync::Mutex;

use qore_core::types::{QueryId, SessionId, Value};
use qore_drivers::session_manager::SessionManager;
use qore_sql::safety as sql_safety;

use super::auth::{parse_bearer, verify_token};
use super::endpoints::EndpointStore;
use super::rate_limit::RateLimiter;
use super::types::{Endpoint, EndpointParam, EndpointParamType, QueryShape};

/// Shared state passed to every handler via `axum::extract::State`. Cloning
/// the struct is cheap — every field is `Arc`-wrapped — so axum can hand a
/// copy to each request future.
#[derive(Clone)]
pub struct ApiState {
    pub store: Arc<EndpointStore>,
    pub limiter: Arc<RateLimiter>,
    pub session_manager: Arc<SessionManager>,
    /// Per-`connection_id` cache of opened sessions. Sessions are opened
    /// lazily on first request and reused across requests; the cache is
    /// drained at server shutdown.
    pub sessions: Arc<Mutex<HashMap<String, SessionId>>>,
    /// Workspace project id (used to load saved connections at request time).
    pub project_id: String,
    /// Vault storage directory captured at server start.
    pub storage_dir: PathBuf,
    /// Server start instant — read by `/health` to compute uptime.
    pub started_at: Arc<Instant>,
}

/// Error envelope returned to clients. Lives outside `ApiError` so we can
/// build it from any handler path with one constructor.
#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Debug)]
pub enum ApiError {
    NotFound,
    Unauthorized,
    Forbidden,
    BadRequest(String),
    TooManyRequests,
    BadGateway(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, detail) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found", None),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", None),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden", None),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", Some(m)),
            ApiError::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "rate_limited", None),
            ApiError::BadGateway(m) => (StatusCode::BAD_GATEWAY, "upstream", Some(m)),
            ApiError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, "internal", Some(m)),
        };
        let body = ErrorBody {
            error: code.to_string(),
            detail,
        };
        (status, Json(body)).into_response()
    }
}

/// `GET /api/{name}` — execute a saved endpoint.
pub async fn handle_endpoint(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let endpoint = state.store.get_by_name(&name).ok_or(ApiError::NotFound)?;

    authenticate(&endpoint, &headers)?;

    if !state.limiter.try_acquire(&endpoint.id) {
        return Err(ApiError::TooManyRequests);
    }

    let final_sql = substitute_params(&endpoint, &params)?;

    let analysis = sql_safety::analyze_sql("postgres", &final_sql)
        .or_else(|_| sql_safety::analyze_sql("mysql", &final_sql))
        .map_err(|e| ApiError::BadRequest(format!("query rejected: {e}")))?;
    if analysis.is_mutation {
        return Err(ApiError::BadRequest(
            "endpoint queries must be read-only".to_string(),
        ));
    }

    let session_id = resolve_session(&state, &endpoint.connection_id).await?;
    let result = execute_query(&state.session_manager, session_id, &final_sql).await?;

    let rows = rows_to_json(&result.columns, &result.rows);
    Ok(build_response(&endpoint, rows))
}

fn authenticate(endpoint: &Endpoint, headers: &HeaderMap) -> Result<(), ApiError> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_bearer)
        .ok_or(ApiError::Unauthorized)?;
    verify_token(raw, &endpoint.token_hash).map_err(|_| ApiError::Forbidden)
}

/// Substitutes `{{name}}` placeholders with typed-and-escaped SQL literals.
///
/// Unknown query-string keys are ignored (so callers can pass `?page=…`
/// alongside endpoint params). Missing required params return 400.
fn substitute_params(
    endpoint: &Endpoint,
    values: &HashMap<String, String>,
) -> Result<String, ApiError> {
    let mut out = endpoint.query_source.clone();
    for p in &endpoint.params {
        let raw = match values.get(&p.name) {
            Some(v) => v.clone(),
            None => match &p.default {
                Some(d) => d.clone(),
                None => {
                    if p.required {
                        return Err(ApiError::BadRequest(format!(
                            "missing required parameter: {}",
                            p.name
                        )));
                    }
                    continue;
                }
            },
        };
        let literal = type_param(p, &raw)?;
        let placeholder = format!("{{{{{}}}}}", p.name);
        out = out.replace(&placeholder, &literal);
    }
    Ok(out)
}

fn type_param(param: &EndpointParam, raw: &str) -> Result<String, ApiError> {
    match param.kind {
        EndpointParamType::String => {
            // SQL-standard literal: double single quotes. Combined with
            // `standard_conforming_strings = on` (default since PG 9.1) and
            // MySQL's default backslash-escape, this is safe for all
            // currently-supported SQL drivers.
            let escaped = raw.replace('\'', "''");
            Ok(format!("'{}'", escaped))
        }
        EndpointParamType::Integer => raw.parse::<i64>().map(|n| n.to_string()).map_err(|_| {
            ApiError::BadRequest(format!(
                "parameter {} must be an integer (got {:?})",
                param.name, raw
            ))
        }),
        EndpointParamType::Float => raw.parse::<f64>().map(|n| n.to_string()).map_err(|_| {
            ApiError::BadRequest(format!(
                "parameter {} must be a float (got {:?})",
                param.name, raw
            ))
        }),
        EndpointParamType::Bool => match raw.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Ok("TRUE".to_string()),
            "false" | "0" | "no" => Ok("FALSE".to_string()),
            _ => Err(ApiError::BadRequest(format!(
                "parameter {} must be a boolean (got {:?})",
                param.name, raw
            ))),
        },
    }
}

async fn resolve_session(state: &ApiState, connection_id: &str) -> Result<SessionId, ApiError> {
    if let Some(existing) = state.sessions.lock().await.get(connection_id).copied() {
        if state.session_manager.session_exists(existing).await {
            return Ok(existing);
        }
        // Stale cache entry (session was closed elsewhere) — drop it and
        // re-open below.
        state.sessions.lock().await.remove(connection_id);
    }

    let config = load_saved_config(&state.project_id, connection_id, &state.storage_dir)
        .map_err(ApiError::BadGateway)?;

    let session_id = state
        .session_manager
        .connect(config)
        .await
        .map_err(|e| ApiError::BadGateway(e.sanitized_message()))?;

    state
        .sessions
        .lock()
        .await
        .insert(connection_id.to_string(), session_id);
    Ok(session_id)
}

fn load_saved_config(
    project_id: &str,
    connection_id: &str,
    storage_dir: &PathBuf,
) -> Result<qore_core::types::ConnectionConfig, String> {
    use crate::vault::backend::KeyringProvider;
    use crate::vault::VaultStorage;

    let storage = VaultStorage::new(
        project_id,
        storage_dir.clone(),
        Box::new(KeyringProvider::new()),
    );
    let saved = storage
        .get_connection(connection_id)
        .map_err(|e| e.to_string())?;
    if saved.project_id != project_id {
        return Err("Connection project mismatch".to_string());
    }
    let creds = storage
        .get_credentials(connection_id)
        .map_err(|e| e.to_string())?;
    saved
        .to_connection_config(&creds)
        .map_err(|e| e.to_string())
}

async fn execute_query(
    session_manager: &Arc<SessionManager>,
    session_id: SessionId,
    sql: &str,
) -> Result<qore_core::types::QueryResult, ApiError> {
    let driver = session_manager
        .get_driver(session_id)
        .await
        .map_err(|e| ApiError::BadGateway(e.sanitized_message()))?;
    driver
        .execute(session_id, sql, QueryId::new())
        .await
        .map_err(|e| ApiError::Internal(e.sanitized_message()))
}

fn rows_to_json(
    columns: &[qore_core::types::ColumnInfo],
    rows: &[qore_core::types::Row],
) -> Vec<JsonValue> {
    rows.iter()
        .map(|row| {
            let mut obj = serde_json::Map::with_capacity(columns.len());
            for (col, val) in columns.iter().zip(row.values.iter()) {
                obj.insert(col.name.to_string(), value_to_json(val));
            }
            JsonValue::Object(obj)
        })
        .collect()
}

fn value_to_json(v: &Value) -> JsonValue {
    // `Value` is `#[serde(untagged)]`, so direct serialization yields the
    // expected JSON wire form (null / bool / number / string / array /
    // object) without manual matching.
    serde_json::to_value(v).unwrap_or(JsonValue::Null)
}

fn build_response(endpoint: &Endpoint, rows: Vec<JsonValue>) -> Response {
    let cap = endpoint.page_size as usize;
    match endpoint.shape {
        QueryShape::Object => {
            let first = rows.into_iter().next().unwrap_or(JsonValue::Null);
            Json(json!({ "data": first })).into_response()
        }
        QueryShape::Rows => {
            let truncated = rows.len() > cap;
            let data: Vec<_> = if truncated {
                rows.into_iter().take(cap).collect()
            } else {
                rows
            };
            let count = data.len();
            Json(json!({
                "data": data,
                "count": count,
                "truncated": truncated,
            }))
            .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{EndpointParam, EndpointParamType};

    fn ep(query: &str, params: Vec<EndpointParam>) -> Endpoint {
        Endpoint {
            id: "id".into(),
            name: "n".into(),
            connection_id: "c".into(),
            query_source: query.into(),
            params,
            shape: QueryShape::Rows,
            token_hash: "".into(),
            page_size: 100,
            created_at: "".into(),
            updated_at: "".into(),
        }
    }

    #[test]
    fn substitutes_string_param_with_escaped_quotes() {
        let p = EndpointParam {
            name: "city".into(),
            kind: EndpointParamType::String,
            required: true,
            default: None,
        };
        let e = ep("SELECT * FROM t WHERE city = {{city}}", vec![p]);
        let mut vals = HashMap::new();
        vals.insert("city".into(), "O'Hara".into());
        let sql = substitute_params(&e, &vals).unwrap();
        assert_eq!(sql, "SELECT * FROM t WHERE city = 'O''Hara'");
    }

    #[test]
    fn rejects_missing_required_param() {
        let p = EndpointParam {
            name: "id".into(),
            kind: EndpointParamType::Integer,
            required: true,
            default: None,
        };
        let e = ep("SELECT * FROM t WHERE id = {{id}}", vec![p]);
        let err = substitute_params(&e, &HashMap::new()).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn uses_default_when_param_omitted() {
        let p = EndpointParam {
            name: "limit".into(),
            kind: EndpointParamType::Integer,
            required: false,
            default: Some("50".into()),
        };
        let e = ep("SELECT * FROM t LIMIT {{limit}}", vec![p]);
        let sql = substitute_params(&e, &HashMap::new()).unwrap();
        assert_eq!(sql, "SELECT * FROM t LIMIT 50");
    }

    #[test]
    fn rejects_non_integer_for_integer_param() {
        let p = EndpointParam {
            name: "n".into(),
            kind: EndpointParamType::Integer,
            required: true,
            default: None,
        };
        let e = ep("SELECT {{n}}", vec![p]);
        let mut vals = HashMap::new();
        vals.insert("n".into(), "not-a-number".into());
        assert!(matches!(
            substitute_params(&e, &vals).unwrap_err(),
            ApiError::BadRequest(_)
        ));
    }

    #[test]
    fn normalizes_bool_values() {
        let p = EndpointParam {
            name: "flag".into(),
            kind: EndpointParamType::Bool,
            required: true,
            default: None,
        };
        let e = ep("SELECT * WHERE active = {{flag}}", vec![p]);
        let mut vals = HashMap::new();
        vals.insert("flag".into(), "yes".into());
        let sql = substitute_params(&e, &vals).unwrap();
        assert!(sql.contains("TRUE"));
    }
}

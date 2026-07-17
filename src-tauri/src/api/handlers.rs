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

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
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

use qore_core::types::{QueryId, SessionId};
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
    /// Connections directory of the active file-based workspace, if any.
    /// When set, saved connections are read from the workspace store instead
    /// of the flat vault. `None` for the default workspace.
    pub workspace_connections_dir: Option<PathBuf>,
    /// Server start instant — read by `/health` to compute uptime.
    pub started_at: Arc<Instant>,
    /// Actual listener URL, including the runtime port and HTTP/HTTPS scheme.
    /// Set exactly once after binding and used by `/openapi.json`.
    pub openapi_base_url: Arc<OnceLock<String>>,
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

    let session_id = resolve_session(&state, &endpoint.connection_id).await?;
    let driver = state
        .session_manager
        .get_driver(session_id)
        .await
        .map_err(|e| ApiError::BadGateway(e.sanitized_message()))?;
    let dialect = ParamDialect::from_driver_id(driver.driver_id()).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "driver {} is not supported by Instant Data API",
            driver.driver_id()
        ))
    })?;

    let final_sql = substitute_params(&endpoint, &params, dialect)?;

    let analysis = sql_safety::analyze_sql(dialect.safety_driver_id(), &final_sql)
        .map_err(|e| ApiError::BadRequest(format!("query rejected: {e}")))?;
    if analysis.is_mutation {
        return Err(ApiError::BadRequest(
            "endpoint queries must be read-only".to_string(),
        ));
    }

    let result = driver
        .execute(session_id, &final_sql, QueryId::new())
        .await
        .map_err(|e| ApiError::Internal(e.sanitized_message()))?;

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
/// Unknown query-string keys are ignored. Missing required params return 400.
fn substitute_params(
    endpoint: &Endpoint,
    values: &HashMap<String, String>,
    dialect: ParamDialect,
) -> Result<String, ApiError> {
    let mut out = endpoint.query_source.clone();
    for p in &endpoint.params {
        let literal = match values.get(&p.name) {
            Some(value) => type_param(p, value, dialect)?,
            None => match &p.default {
                Some(default) => type_param(p, default, dialect)?,
                None => {
                    if p.required {
                        return Err(ApiError::BadRequest(format!(
                            "missing required parameter: {}",
                            p.name
                        )));
                    }
                    "NULL".to_string()
                }
            },
        };
        let placeholder = format!("{{{{{}}}}}", p.name);
        out = out.replace(&placeholder, &literal);
    }
    Ok(out)
}

/// Validates a new endpoint with the dialect of its saved connection before
/// it is persisted or receives a bearer token.
pub(crate) fn validate_endpoint_definition(
    driver_id: &str,
    query_source: &str,
    params: &[EndpointParam],
    max_rows: u32,
) -> Result<(), String> {
    if !(1..=10_000).contains(&max_rows) {
        return Err("Maximum rows must be between 1 and 10000".to_string());
    }

    let dialect = ParamDialect::from_driver_id(driver_id)
        .ok_or_else(|| format!("driver {driver_id} is not supported by Instant Data API"))?;
    let placeholders = extract_placeholders(query_source)?;
    let mut declared = HashSet::with_capacity(params.len());
    let mut values = HashMap::with_capacity(params.len());

    for param in params {
        if !valid_param_name(&param.name) {
            return Err(format!("invalid parameter name: {}", param.name));
        }
        if !declared.insert(param.name.as_str()) {
            return Err(format!("duplicate parameter: {}", param.name));
        }
        if !placeholders.iter().any(|name| name == &param.name) {
            return Err(format!(
                "parameter {} is not referenced in the query",
                param.name
            ));
        }
        let sample = param.default.clone().unwrap_or_else(|| match param.kind {
            EndpointParamType::String => "qoredb_validation".to_string(),
            EndpointParamType::Integer => "0".to_string(),
            EndpointParamType::Float => "0.0".to_string(),
            EndpointParamType::Bool => "false".to_string(),
        });
        values.insert(param.name.clone(), sample);
    }

    for placeholder in &placeholders {
        if !declared.contains(placeholder.as_str()) {
            return Err(format!(
                "query placeholder {placeholder} has no declared parameter"
            ));
        }
    }

    let endpoint = Endpoint {
        id: String::new(),
        name: String::new(),
        connection_id: String::new(),
        query_source: query_source.to_string(),
        params: params.to_vec(),
        shape: QueryShape::Rows,
        token_hash: String::new(),
        page_size: max_rows,
        created_at: String::new(),
        updated_at: String::new(),
    };
    let validation_sql =
        substitute_params(&endpoint, &values, dialect).map_err(|error| match error {
            ApiError::BadRequest(message) => message,
            other => format!("query validation failed: {other:?}"),
        })?;
    let safety_driver = dialect.safety_driver_id();
    let analysis = sql_safety::analyze_sql(safety_driver, &validation_sql)
        .map_err(|error| format!("invalid query for {driver_id}: {error}"))?;
    if analysis.is_mutation {
        return Err("Instant Data API endpoint queries must be read-only".to_string());
    }
    if !sql_safety::returns_rows(safety_driver, &validation_sql)
        .map_err(|error| format!("invalid query for {driver_id}: {error}"))?
    {
        return Err("Instant Data API endpoint queries must return rows".to_string());
    }
    Ok(())
}

fn valid_param_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn extract_placeholders(query_source: &str) -> Result<Vec<String>, String> {
    let mut placeholders = Vec::new();
    let mut remaining = query_source;
    while let Some(start) = remaining.find("{{") {
        let after_start = &remaining[start + 2..];
        let end = after_start
            .find("}}")
            .ok_or_else(|| "unterminated query placeholder".to_string())?;
        let name = &after_start[..end];
        if !valid_param_name(name) {
            return Err(format!("invalid query placeholder: {name:?}"));
        }
        placeholders.push(name.to_string());
        remaining = &after_start[end + 2..];
    }
    Ok(placeholders)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParamDialect {
    Postgres,
    MySql,
    Sqlite,
    DuckDb,
    SqlServer,
    ClickHouse,
}

impl ParamDialect {
    fn from_driver_id(driver_id: &str) -> Option<Self> {
        match driver_id.to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" | "cockroachdb" | "neon" | "supabase" | "timescaledb" => {
                Some(Self::Postgres)
            }
            "mysql" | "mariadb" => Some(Self::MySql),
            "sqlite" => Some(Self::Sqlite),
            "duckdb" | "motherduck" => Some(Self::DuckDb),
            "sqlserver" | "mssql" => Some(Self::SqlServer),
            "clickhouse" => Some(Self::ClickHouse),
            _ => None,
        }
    }

    fn safety_driver_id(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::MySql => "mysql",
            Self::Sqlite => "sqlite",
            Self::DuckDb => "duckdb",
            Self::SqlServer => "sqlserver",
            Self::ClickHouse => "clickhouse",
        }
    }
}

fn type_param(param: &EndpointParam, raw: &str, dialect: ParamDialect) -> Result<String, ApiError> {
    match param.kind {
        EndpointParamType::String => string_literal(raw, dialect),
        EndpointParamType::Integer => raw.parse::<i64>().map(|n| n.to_string()).map_err(|_| {
            ApiError::BadRequest(format!(
                "parameter {} must be an integer (got {:?})",
                param.name, raw
            ))
        }),
        EndpointParamType::Float => raw
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(|n| n.to_string())
            .ok_or_else(|| {
                ApiError::BadRequest(format!(
                    "parameter {} must be a finite float (got {:?})",
                    param.name, raw
                ))
            }),
        EndpointParamType::Bool => match raw.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Ok(bool_literal(true, dialect).to_string()),
            "false" | "0" | "no" => Ok(bool_literal(false, dialect).to_string()),
            _ => Err(ApiError::BadRequest(format!(
                "parameter {} must be a boolean (got {:?})",
                param.name, raw
            ))),
        },
    }
}

fn string_literal(raw: &str, dialect: ParamDialect) -> Result<String, ApiError> {
    if raw.contains('\0') {
        return Err(ApiError::BadRequest(
            "string parameters cannot contain NUL bytes".to_string(),
        ));
    }

    Ok(match dialect {
        // Explicit E-strings make backslash behavior independent from the
        // session's `standard_conforming_strings` setting.
        ParamDialect::Postgres => {
            let escaped = raw.replace('\\', "\\\\").replace('\'', "\\'");
            format!("E'{escaped}'")
        }
        // A UTF-8 hex expression avoids both quote and backslash ambiguity,
        // including sessions using NO_BACKSLASH_ESCAPES.
        ParamDialect::MySql => {
            let hex = raw
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            format!("CONVERT(X'{hex}' USING utf8mb4)")
        }
        ParamDialect::ClickHouse => {
            let escaped = raw.replace('\\', "\\\\").replace('\'', "\\'");
            format!("'{escaped}'")
        }
        ParamDialect::SqlServer => format!("N'{}'", raw.replace('\'', "''")),
        ParamDialect::Sqlite | ParamDialect::DuckDb => {
            format!("'{}'", raw.replace('\'', "''"))
        }
    })
}

fn bool_literal(value: bool, dialect: ParamDialect) -> &'static str {
    match dialect {
        ParamDialect::SqlServer | ParamDialect::ClickHouse => {
            if value {
                "1"
            } else {
                "0"
            }
        }
        _ => {
            if value {
                "TRUE"
            } else {
                "FALSE"
            }
        }
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

    let config = load_saved_config(
        &state.project_id,
        state.workspace_connections_dir.as_deref(),
        connection_id,
        &state.storage_dir,
    )
    .map_err(ApiError::BadGateway)?;

    let session_id = state
        .session_manager
        .connect(config)
        .await
        .map_err(|e| ApiError::BadGateway(e.sanitized_message()))?;
    state
        .session_manager
        .set_saved_connection_identity(
            session_id,
            connection_id.to_string(),
            connection_id.to_string(),
        )
        .await;

    state
        .sessions
        .lock()
        .await
        .insert(connection_id.to_string(), session_id);
    Ok(session_id)
}

fn load_saved_config(
    project_id: &str,
    workspace_connections_dir: Option<&std::path::Path>,
    connection_id: &str,
    storage_dir: &PathBuf,
) -> Result<qore_core::types::ConnectionConfig, String> {
    use crate::vault::backend::KeyringProvider;

    // File-based workspaces keep connections in their own directory; isolation
    // is by directory, so the flat-vault project_id guard does not apply.
    if let Some(dir) = workspace_connections_dir {
        use crate::workspace::connection_store::WorkspaceConnectionStore;

        let store = WorkspaceConnectionStore::new(
            dir.to_path_buf(),
            format!("qoredb_{}", project_id),
            Box::new(KeyringProvider::new()),
        );
        let saved = store
            .get_connection(connection_id)
            .map_err(|e| e.sanitized_message())?;
        let creds = store
            .get_credentials(connection_id)
            .map_err(|e| e.sanitized_message())?;
        return saved
            .to_connection_config(&creds)
            .map_err(|e| e.sanitized_message());
    }

    use crate::vault::VaultStorage;

    let storage = VaultStorage::new(
        project_id,
        storage_dir.clone(),
        Box::new(KeyringProvider::new()),
    );
    let saved = storage
        .get_connection(connection_id)
        .map_err(|e| e.sanitized_message())?;
    if saved.project_id != project_id {
        return Err("Connection project mismatch".to_string());
    }
    let creds = storage
        .get_credentials(connection_id)
        .map_err(|e| e.sanitized_message())?;
    saved
        .to_connection_config(&creds)
        .map_err(|e| e.sanitized_message())
}

fn rows_to_json(
    columns: &[qore_core::types::ColumnInfo],
    rows: &[qore_core::types::Row],
) -> Vec<JsonValue> {
    rows.iter()
        .map(|row| {
            let mut obj = serde_json::Map::with_capacity(columns.len());
            for (col, val) in columns.iter().zip(row.values.iter()) {
                obj.insert(col.name.to_string(), val.to_json());
            }
            JsonValue::Object(obj)
        })
        .collect()
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
    fn substitutes_postgres_string_with_explicit_escape_literal() {
        let p = EndpointParam {
            name: "city".into(),
            kind: EndpointParamType::String,
            required: true,
            default: None,
        };
        let e = ep("SELECT * FROM t WHERE city = {{city}}", vec![p]);
        let mut vals = HashMap::new();
        vals.insert("city".into(), "O'Hara".into());
        let sql = substitute_params(&e, &vals, ParamDialect::Postgres).unwrap();
        assert_eq!(sql, "SELECT * FROM t WHERE city = E'O\\'Hara'");
    }

    #[test]
    fn mysql_string_param_cannot_escape_the_literal() {
        let p = EndpointParam {
            name: "name".into(),
            kind: EndpointParamType::String,
            required: true,
            default: None,
        };
        let e = ep("SELECT * FROM users WHERE name = {{name}}", vec![p]);
        let mut vals = HashMap::new();
        vals.insert("name".into(), "\\' OR 1=1 -- ".into());

        let sql = substitute_params(&e, &vals, ParamDialect::MySql).unwrap();

        assert_eq!(
            sql,
            "SELECT * FROM users WHERE name = CONVERT(X'5c27204f5220313d31202d2d20' USING utf8mb4)"
        );
        assert!(!sql.contains("OR 1=1 --"));
        let analysis = sql_safety::analyze_sql("mysql", &sql).expect("valid MySQL query");
        assert!(!analysis.is_mutation);
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
        let err = substitute_params(&e, &HashMap::new(), ParamDialect::Postgres).unwrap_err();
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
        let sql = substitute_params(&e, &HashMap::new(), ParamDialect::Postgres).unwrap();
        assert_eq!(sql, "SELECT * FROM t LIMIT 50");
    }

    #[test]
    fn optional_param_without_default_becomes_null() {
        let p = EndpointParam {
            name: "status".into(),
            kind: EndpointParamType::String,
            required: false,
            default: None,
        };
        let e = ep("SELECT * FROM t WHERE status = {{status}}", vec![p]);
        let sql = substitute_params(&e, &HashMap::new(), ParamDialect::Postgres).unwrap();
        assert_eq!(sql, "SELECT * FROM t WHERE status = NULL");
    }

    #[test]
    fn endpoint_definition_accepts_parameterized_read_only_query() {
        let params = vec![EndpointParam {
            name: "id".into(),
            kind: EndpointParamType::Integer,
            required: true,
            default: None,
        }];
        validate_endpoint_definition(
            "timescaledb",
            "SELECT * FROM users WHERE id = {{id}}",
            &params,
            100,
        )
        .unwrap();
    }

    #[test]
    fn endpoint_definition_rejects_mutations_and_non_row_statements() {
        let mutation =
            validate_endpoint_definition("postgres", "DELETE FROM users", &[], 100).unwrap_err();
        assert!(mutation.contains("read-only"));

        let no_rows =
            validate_endpoint_definition("postgres", "SET search_path = public", &[], 100)
                .unwrap_err();
        assert!(no_rows.contains("return rows"));
    }

    #[test]
    fn endpoint_definition_rejects_invalid_or_mismatched_placeholders() {
        let param = EndpointParam {
            name: "id".into(),
            kind: EndpointParamType::Integer,
            required: true,
            default: None,
        };
        let undeclared = validate_endpoint_definition(
            "postgres",
            "SELECT * FROM users WHERE id = {{other}}",
            &[param.clone()],
            100,
        )
        .unwrap_err();
        assert!(undeclared.contains("not referenced"));

        let unterminated = validate_endpoint_definition(
            "postgres",
            "SELECT * FROM users WHERE id = {{id",
            &[param],
            100,
        )
        .unwrap_err();
        assert!(unterminated.contains("unterminated"));
    }

    #[test]
    fn endpoint_definition_validates_defaults_and_maximum_rows() {
        let invalid_default = EndpointParam {
            name: "limit".into(),
            kind: EndpointParamType::Integer,
            required: false,
            default: Some("many".into()),
        };
        let default_error = validate_endpoint_definition(
            "postgres",
            "SELECT * FROM users LIMIT {{limit}}",
            &[invalid_default],
            100,
        )
        .unwrap_err();
        assert!(default_error.contains("must be an integer"));

        assert!(validate_endpoint_definition("postgres", "SELECT 1", &[], 0)
            .unwrap_err()
            .contains("between 1 and 10000"));
        assert!(
            validate_endpoint_definition("postgres", "SELECT 1", &[], 10_001)
                .unwrap_err()
                .contains("between 1 and 10000")
        );
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
            substitute_params(&e, &vals, ParamDialect::Postgres).unwrap_err(),
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
        let sql = substitute_params(&e, &vals, ParamDialect::Postgres).unwrap();
        assert!(sql.contains("TRUE"));
    }

    #[test]
    fn sqlserver_bool_uses_bit_literal() {
        let p = EndpointParam {
            name: "flag".into(),
            kind: EndpointParamType::Bool,
            required: true,
            default: None,
        };
        let e = ep("SELECT * FROM t WHERE active = {{flag}}", vec![p]);
        let mut vals = HashMap::new();
        vals.insert("flag".into(), "true".into());
        let sql = substitute_params(&e, &vals, ParamDialect::SqlServer).unwrap();
        assert_eq!(sql, "SELECT * FROM t WHERE active = 1");
    }

    #[test]
    fn rejects_non_finite_float() {
        let p = EndpointParam {
            name: "value".into(),
            kind: EndpointParamType::Float,
            required: true,
            default: None,
        };
        let e = ep("SELECT {{value}}", vec![p]);
        let mut vals = HashMap::new();
        vals.insert("value".into(), "NaN".into());
        assert!(matches!(
            substitute_params(&e, &vals, ParamDialect::Postgres).unwrap_err(),
            ApiError::BadRequest(_)
        ));
    }

    #[test]
    fn maps_supported_driver_families_to_their_real_safety_dialect() {
        assert_eq!(
            ParamDialect::from_driver_id("timescaledb"),
            Some(ParamDialect::Postgres)
        );
        assert_eq!(
            ParamDialect::from_driver_id("mariadb"),
            Some(ParamDialect::MySql)
        );
        assert_eq!(
            ParamDialect::from_driver_id("sqlserver")
                .unwrap()
                .safety_driver_id(),
            "sqlserver"
        );
        assert_eq!(ParamDialect::from_driver_id("mongodb"), None);
    }
}

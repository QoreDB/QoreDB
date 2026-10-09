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
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Value as JsonValue, json};
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

    let mut result = driver
        .execute(session_id, &final_sql, QueryId::new())
        .await
        .map_err(|e| ApiError::Internal(e.sanitized_message()))?;
    qore_service::query::apply_masking(&state.session_manager, session_id, None, &mut result).await;

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
/// The query is scanned once so that text inserted by one parameter is never
/// re-interpreted as another placeholder.
fn substitute_params(
    endpoint: &Endpoint,
    values: &HashMap<String, String>,
    dialect: ParamDialect,
) -> Result<String, ApiError> {
    let mut literals = HashMap::with_capacity(endpoint.params.len());
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
        literals.insert(p.name.as_str(), literal);
    }

    let source = endpoint.query_source.as_str();
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find("{{") {
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            break;
        };
        out.push_str(&rest[..start]);
        match literals.get(&after_start[..end]) {
            Some(literal) => out.push_str(literal),
            None => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after_start[end + 2..];
    }
    out.push_str(rest);
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
    Snowflake,
    BigQuery,
}

impl ParamDialect {
    fn from_driver_id(driver_id: &str) -> Option<Self> {
        match driver_id.to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" | "cockroachdb" | "neon" | "supabase" | "timescaledb"
            | "yugabytedb" => Some(Self::Postgres),
            "mysql" | "mariadb" | "planetscale" | "tidb" | "starrocks" | "doris"
            | "singlestore" => Some(Self::MySql),
            "sqlite" => Some(Self::Sqlite),
            "duckdb" | "motherduck" => Some(Self::DuckDb),
            "sqlserver" | "mssql" | "azuresql" | "synapse" => Some(Self::SqlServer),
            "clickhouse" => Some(Self::ClickHouse),
            "snowflake" => Some(Self::Snowflake),
            "bigquery" => Some(Self::BigQuery),
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
            Self::Snowflake => "snowflake",
            Self::BigQuery => "bigquery",
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
        // All three honour backslash escapes inside a literal.
        ParamDialect::ClickHouse | ParamDialect::Snowflake | ParamDialect::BigQuery => {
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
    resolve_session_with_config(state, connection_id, || {
        load_saved_config(
            &state.project_id,
            state.workspace_connections_dir.as_deref(),
            connection_id,
            &state.storage_dir,
        )
    })
    .await
}

async fn resolve_session_with_config(
    state: &ApiState,
    connection_id: &str,
    load_config: impl FnOnce() -> Result<
        (
            qore_core::types::ConnectionConfig,
            qore_core::masking::ConnectionMasking,
        ),
        String,
    >,
) -> Result<SessionId, ApiError> {
    // Keep cache lookup and publication together: concurrent first requests must
    // not open untracked sessions. Query execution happens outside this lock.
    let mut sessions = state.sessions.lock().await;
    if let Some(existing) = sessions.get(connection_id).copied() {
        if state.session_manager.session_exists(existing).await {
            return Ok(existing);
        }
        // Stale cache entry (session was closed elsewhere) — drop it and
        // re-open below.
        sessions.remove(connection_id);
    }

    let (config, masking) = load_config().map_err(ApiError::BadGateway)?;

    let session_id = state
        .session_manager
        .connect(config)
        .await
        .map_err(|e| ApiError::BadGateway(e.sanitized_message()))?;
    if let Err(error) = state
        .session_manager
        .bind_workspace(session_id, &state.project_id)
        .await
    {
        if let Err(cleanup_error) = state.session_manager.disconnect(session_id).await {
            tracing::warn!(error = %cleanup_error.sanitized_message(), "Failed to disconnect unbound Instant API session");
        }
        return Err(ApiError::BadGateway(error.sanitized_message()));
    }
    state
        .session_manager
        .set_saved_connection_identity(
            session_id,
            connection_id.to_string(),
            connection_id.to_string(),
        )
        .await;
    state
        .session_manager
        .set_masking(session_id, &masking)
        .await;

    sessions.insert(connection_id.to_string(), session_id);
    Ok(session_id)
}

fn load_saved_config(
    project_id: &str,
    workspace_connections_dir: Option<&std::path::Path>,
    connection_id: &str,
    storage_dir: &PathBuf,
) -> Result<
    (
        qore_core::types::ConnectionConfig,
        qore_core::masking::ConnectionMasking,
    ),
    String,
> {
    use crate::vault::backend::KeyringProvider;

    // File-based workspaces keep connections in their own directory; isolation
    // is by directory, so the flat-vault project_id guard does not apply.
    if let Some(dir) = workspace_connections_dir {
        use crate::workspace::connection_store::WorkspaceConnectionStore;

        let store = WorkspaceConnectionStore::new(
            dir.to_path_buf(),
            qore_service::workspace::keyring_service(&project_id),
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
            .map(|config| (config, saved.masking.clone()))
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
        .map(|config| (config, saved.masking.clone()))
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

    fn session_state(tmp: &tempfile::TempDir) -> ApiState {
        let mut registry = qore_core::registry::DriverRegistry::new();
        registry.register(Arc::new(qore_drivers::drivers::sqlite::SqliteDriver::new()));
        ApiState {
            store: Arc::new(EndpointStore::new(tmp.path().to_path_buf()).unwrap()),
            limiter: Arc::new(RateLimiter::default_capacity()),
            session_manager: Arc::new(SessionManager::new(Arc::new(registry))),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            project_id: "api-workspace".into(),
            storage_dir: tmp.path().to_path_buf(),
            workspace_connections_dir: None,
            started_at: Arc::new(Instant::now()),
            openapi_base_url: Arc::new(OnceLock::new()),
        }
    }

    fn sqlite_config(tmp: &tempfile::TempDir) -> qore_core::types::ConnectionConfig {
        qore_core::types::ConnectionConfig {
            driver: "sqlite".into(),
            host: tmp.path().join("api.db").to_string_lossy().into_owned(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn expired_cached_session_recovers_without_locking_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let state = session_state(&tmp);
        state
            .sessions
            .lock()
            .await
            .insert("connection".into(), SessionId::new());
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            resolve_session_with_config(&state, "connection", || {
                Err("synthetic missing connection".into())
            }),
        )
        .await
        .expect("expired session must not deadlock");
        assert!(matches!(result, Err(ApiError::BadGateway(_))));
        assert!(state.sessions.lock().await.is_empty());
    }

    #[tokio::test]
    async fn simultaneous_requests_share_one_api_session() {
        let tmp = tempfile::tempdir().unwrap();
        let state = session_state(&tmp);
        let config = sqlite_config(&tmp);
        let (first, second) = tokio::join!(
            resolve_session_with_config(&state, "connection", || Ok((
                config.clone(),
                Default::default()
            ))),
            resolve_session_with_config(&state, "connection", || Ok((
                config.clone(),
                Default::default()
            ))),
        );
        let first = first.unwrap();
        let second = second.unwrap();
        state.session_manager.disconnect(first).await.unwrap();
        if second != first {
            state.session_manager.disconnect(second).await.unwrap();
        }
        assert_eq!(first, second, "concurrent opens must not leak a session");
    }

    #[tokio::test]
    async fn api_sessions_receive_only_their_workspaces_masking_updates() {
        use qore_core::masking::{ConnectionMasking, HIDDEN_VALUE, MaskMode, MaskingRule};
        let tmp = tempfile::tempdir().unwrap();
        let state = session_state(&tmp);
        let session = resolve_session_with_config(&state, "copied-id", || {
            Ok((sqlite_config(&tmp), Default::default()))
        })
        .await
        .unwrap();
        let mut other = state.clone();
        other.project_id = "other-workspace".into();
        other.sessions = Arc::new(Mutex::new(HashMap::new()));
        let other_session = resolve_session_with_config(&other, "copied-id", || {
            Ok((sqlite_config(&tmp), Default::default()))
        })
        .await
        .unwrap();
        let rules = ConnectionMasking {
            rules: vec![MaskingRule {
                table: "".into(),
                column: "secret".into(),
                mode: MaskMode::Hidden,
            }],
            mask_detected_columns: false,
        };
        state
            .session_manager
            .update_connection_masking(&state.project_id, "copied-id", &rules)
            .await;
        let driver = state.session_manager.get_driver(session).await.unwrap();
        let mut result = driver
            .execute(
                session,
                "SELECT 'synthetic-private-value' AS secret",
                QueryId::new(),
            )
            .await
            .unwrap();
        qore_service::query::apply_masking(&state.session_manager, session, None, &mut result)
            .await;
        assert_eq!(result.rows[0].values[0].to_json(), json!(HIDDEN_VALUE));
        assert!(state.session_manager.masking(other_session).await.is_none());
        assert_eq!(
            state.session_manager.workspace_id(session).await.as_deref(),
            Some("api-workspace")
        );
        state.session_manager.disconnect(session).await.unwrap();
        state
            .session_manager
            .disconnect(other_session)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn sqlite_handler_authenticates_masks_caps_and_rejects_mutations() {
        use qore_core::masking::{ConnectionMasking, HIDDEN_VALUE, MaskMode, MaskingRule};
        let tmp = tempfile::tempdir().unwrap();
        let state = session_state(&tmp);
        let session = resolve_session_with_config(&state, "connection", || {
            Ok((sqlite_config(&tmp), Default::default()))
        })
        .await
        .unwrap();
        let token = super::super::auth::issue_token().unwrap();
        state.store.create("rows".into(), "connection".into(),
            "SELECT 9007199254740993 AS id, 'synthetic-private' AS secret UNION ALL SELECT 2, 'second'".into(),
            vec![], QueryShape::Rows, 1, token.hash.clone()).unwrap();
        let call = |name: &str, headers| {
            handle_endpoint(
                State(state.clone()),
                Path(name.to_owned()),
                Query(HashMap::new()),
                headers,
            )
        };
        assert!(matches!(
            call("rows", HeaderMap::new()).await,
            Err(ApiError::Unauthorized)
        ));
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer invalid-synthetic-token".parse().unwrap(),
        );
        assert!(matches!(
            call("rows", headers.clone()).await,
            Err(ApiError::Forbidden)
        ));
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.value).parse().unwrap(),
        );
        state
            .session_manager
            .update_connection_masking(
                &state.project_id,
                "connection",
                &ConnectionMasking {
                    rules: vec![MaskingRule {
                        table: "".into(),
                        column: "secret".into(),
                        mode: MaskMode::Hidden,
                    }],
                    mask_detected_columns: false,
                },
            )
            .await;
        let response = call("rows", headers.clone()).await.unwrap();
        let body = axum::body::to_bytes(response.into_body(), 16_384)
            .await
            .unwrap();
        let data: JsonValue = serde_json::from_slice(&body).unwrap();
        assert_eq!(data["data"][0]["secret"], json!(HIDDEN_VALUE));
        assert_eq!(data["data"][0]["id"], json!(9_007_199_254_740_993_i64));
        assert_eq!(data["count"], json!(1));
        assert_eq!(data["truncated"], json!(true));
        let driver = state.session_manager.get_driver(session).await.unwrap();
        driver
            .execute(
                session,
                "CREATE TABLE protected (id INTEGER); INSERT INTO protected VALUES (1)",
                QueryId::new(),
            )
            .await
            .unwrap();
        state
            .store
            .create(
                "mutation".into(),
                "connection".into(),
                "DELETE FROM protected".into(),
                vec![],
                QueryShape::Rows,
                1,
                token.hash,
            )
            .unwrap();
        assert!(matches!(
            call("mutation", headers).await,
            Err(ApiError::BadRequest(_))
        ));
        let remaining = driver
            .execute(session, "SELECT COUNT(*) FROM protected", QueryId::new())
            .await
            .unwrap();
        assert_eq!(remaining.rows[0].values[0].to_json(), json!(1));
        state.session_manager.disconnect(session).await.unwrap();
    }

    #[tokio::test]
    async fn invalid_workspace_does_not_publish_or_leak_a_session() {
        let tmp = tempfile::tempdir().unwrap();
        let mut state = session_state(&tmp);
        state.project_id.clear();
        let result = resolve_session_with_config(&state, "connection", || {
            Ok((sqlite_config(&tmp), Default::default()))
        })
        .await;
        assert!(matches!(result, Err(ApiError::BadGateway(_))));
        assert!(state.sessions.lock().await.is_empty());
        assert!(state.session_manager.list_sessions().await.is_empty());
    }

    #[tokio::test]
    async fn failed_connection_allows_an_explicit_retry() {
        let tmp = tempfile::tempdir().unwrap();
        let state = session_state(&tmp);
        let mut config = sqlite_config(&tmp);
        config.driver = "missing-driver".into();
        assert!(
            resolve_session_with_config(&state, "connection", || Ok((config, Default::default())))
                .await
                .is_err()
        );
        assert!(state.sessions.lock().await.is_empty());
        let session = resolve_session_with_config(&state, "connection", || {
            Ok((sqlite_config(&tmp), Default::default()))
        })
        .await
        .unwrap();
        state.session_manager.disconnect(session).await.unwrap();
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
    fn param_value_naming_another_placeholder_is_not_substituted_again() {
        let string_param = |name: &str| EndpointParam {
            name: name.into(),
            kind: EndpointParamType::String,
            required: true,
            default: None,
        };
        let e = ep(
            "SELECT * FROM users WHERE a = {{a}} AND b = {{b}}",
            vec![string_param("a"), string_param("b")],
        );
        let mut vals = HashMap::new();
        vals.insert("a".into(), "{{b}}".into());
        vals.insert("b".into(), "OR 1=1 --".into());

        let sql = substitute_params(&e, &vals, ParamDialect::Postgres).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM users WHERE a = E'{{b}}' AND b = E'OR 1=1 --'"
        );

        for dialect in [
            ParamDialect::Postgres,
            ParamDialect::MySql,
            ParamDialect::Sqlite,
            ParamDialect::DuckDb,
            ParamDialect::SqlServer,
            ParamDialect::ClickHouse,
            ParamDialect::Snowflake,
            ParamDialect::BigQuery,
        ] {
            let sql = substitute_params(&e, &vals, dialect).unwrap();
            let expected = format!(
                "SELECT * FROM users WHERE a = {} AND b = {}",
                string_literal("{{b}}", dialect).unwrap(),
                string_literal("OR 1=1 --", dialect).unwrap()
            );
            assert_eq!(sql, expected, "{dialect:?}");
        }
    }

    #[test]
    fn repeated_and_undeclared_placeholders_are_handled_in_one_pass() {
        let p = EndpointParam {
            name: "id".into(),
            kind: EndpointParamType::Integer,
            required: true,
            default: None,
        };
        let e = ep(
            "SELECT {{id}}, {{other}} WHERE x = {{id}} AND y = '{{",
            vec![p],
        );
        let mut vals = HashMap::new();
        vals.insert("id".into(), "7".into());
        let sql = substitute_params(&e, &vals, ParamDialect::Postgres).unwrap();
        assert_eq!(sql, "SELECT 7, {{other}} WHERE x = 7 AND y = '{{");
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

        assert!(
            validate_endpoint_definition("postgres", "SELECT 1", &[], 0)
                .unwrap_err()
                .contains("between 1 and 10000")
        );
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
            ParamDialect::from_driver_id("tidb"),
            Some(ParamDialect::MySql)
        );
        assert_eq!(
            ParamDialect::from_driver_id("yugabytedb"),
            Some(ParamDialect::Postgres)
        );
        assert_eq!(
            ParamDialect::from_driver_id("sqlserver")
                .unwrap()
                .safety_driver_id(),
            "sqlserver"
        );
        assert_eq!(ParamDialect::from_driver_id("mongodb"), None);
        assert_eq!(
            ParamDialect::from_driver_id("synapse"),
            Some(ParamDialect::SqlServer)
        );
    }
}

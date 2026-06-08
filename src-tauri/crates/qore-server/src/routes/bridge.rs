// SPDX-License-Identifier: BUSL-1.1

use axum::extract::State;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use qore_core::{CollectionListOptions, Namespace, TableQueryOptions};

use crate::config::QUERY_TIMEOUT_MS;
use crate::controlplane::model::GrantLevel;
use crate::controlplane::AuthContext;
use crate::error::ApiError;
use crate::session::{connect_saved, parse_session, storage};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct InvokeRequest {
    pub command: String,
    #[serde(default)]
    pub args: Value,
}

pub async fn invoke(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<InvokeRequest>,
) -> Result<Json<Value>, ApiError> {
    let args = req.args;
    match req.command.as_str() {
        "list_saved_connections" => {
            let mut conns = storage(&state.config)
                .list_connections_full()
                .map_err(|e| ApiError::internal(e.sanitized_message()))?;
            conns.retain(|c| ctx.access(&c.id).is_some());
            serde_json::to_value(conns)
                .map(Json)
                .map_err(|e| ApiError::internal(e.to_string()))
        }

        "connect_saved_connection" => {
            let connection_id = req_str(&args, "connectionId")?;
            let Some(level) = ctx.access(&connection_id) else {
                return Ok(failure("access denied for this connection"));
            };
            let force_read_only = level == GrantLevel::Read;
            Ok(
                match connect_saved(&state, &connection_id, force_read_only).await {
                    Ok(sid) => Json(json!({ "success": true, "session_id": sid.0.to_string() })),
                    Err(e) => failure(e),
                },
            )
        }

        "disconnect" => {
            let session = match parse_session(&req_str(&args, "sessionId")?) {
                Ok(s) => s,
                Err(e) => return Ok(failure(e)),
            };
            Ok(
                match qore_service::connection::disconnect(
                    &state.ctx.session_manager,
                    &state.ctx.query_rate_limiter,
                    session,
                )
                .await
                {
                    Ok(()) => Json(json!({ "success": true })),
                    Err(e) => failure(e.sanitized()),
                },
            )
        }

        "list_namespaces" => {
            let session = match parse_session(&req_str(&args, "sessionId")?) {
                Ok(s) => s,
                Err(e) => return Ok(failure(e)),
            };
            let driver = match state.ctx.session_manager.get_driver(session).await {
                Ok(d) => d,
                Err(e) => return Ok(failure(e.sanitized_message())),
            };
            Ok(match driver.list_namespaces(session).await {
                Ok(namespaces) => Json(json!({ "success": true, "namespaces": namespaces })),
                Err(e) => failure(e.sanitized_message()),
            })
        }

        "list_collections" => {
            let session = match parse_session(&req_str(&args, "sessionId")?) {
                Ok(s) => s,
                Err(e) => return Ok(failure(e)),
            };
            let namespace = req_namespace(&args)?;
            let options = CollectionListOptions {
                search: args.get("search").and_then(Value::as_str).map(String::from),
                page: args.get("page").and_then(Value::as_u64).map(|p| p as u32),
                page_size: args
                    .get("page_size")
                    .and_then(Value::as_u64)
                    .map(|p| p as u32),
            };
            let driver = match state.ctx.session_manager.get_driver(session).await {
                Ok(d) => d,
                Err(e) => return Ok(failure(e.sanitized_message())),
            };
            Ok(
                match driver.list_collections(session, &namespace, options).await {
                    Ok(data) => Json(json!({ "success": true, "data": data })),
                    Err(e) => failure(e.sanitized_message()),
                },
            )
        }

        "describe_table" => {
            let session = match parse_session(&req_str(&args, "sessionId")?) {
                Ok(s) => s,
                Err(e) => return Ok(failure(e)),
            };
            let namespace = req_namespace(&args)?;
            let table = req_str(&args, "table")?;
            let connection_id = args.get("connectionId").and_then(Value::as_str);
            Ok(
                match qore_service::query::describe_table(
                    &state.ctx.session_manager,
                    &state.ctx.virtual_relations,
                    session,
                    &namespace,
                    &table,
                    connection_id,
                )
                .await
                {
                    Ok(schema) => Json(json!({ "success": true, "schema": schema })),
                    Err(e) => failure(e.sanitized()),
                },
            )
        }

        "query_table" => {
            let session = match parse_session(&req_str(&args, "sessionId")?) {
                Ok(s) => s,
                Err(e) => return Ok(failure(e)),
            };
            let namespace = req_namespace(&args)?;
            let table = req_str(&args, "table")?;
            let options: TableQueryOptions = args
                .get("options")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            let bypass_cache = args
                .get("bypassCache")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Ok(
                match qore_service::query::query_table(
                    &state.ctx.session_manager,
                    &state.ctx.query_manager,
                    &state.ctx.query_cache,
                    &state.ctx.policy,
                    session,
                    &namespace,
                    &table,
                    options,
                    bypass_cache,
                )
                .await
                {
                    Ok((result, age_ms)) => Json(json!({
                        "success": true,
                        "result": result,
                        "cached": age_ms.is_some(),
                        "cached_age_ms": age_ms,
                    })),
                    Err(e) => failure(e.sanitized()),
                },
            )
        }

        "execute_query" => Ok(execute_query(&state, &args).await),

        other => Err(ApiError::bad_request(format!(
            "command not supported over HTTP: {other}"
        ))),
    }
}

async fn execute_query(state: &AppState, args: &Value) -> Json<Value> {
    let session = match req_str(args, "sessionId")
        .and_then(|s| parse_session(&s).map_err(ApiError::bad_request))
    {
        Ok(s) => s,
        Err(_) => return failure("invalid session id"),
    };
    let query = match args.get("query").and_then(Value::as_str) {
        Some(q) => q.to_string(),
        None => return failure("missing arg: query"),
    };
    let namespace: Option<Namespace> = args
        .get("namespace")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok());
    let acknowledged = args
        .get("acknowledgedDangerous")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let bypass_limits = args
        .get("bypassLimits")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let timeout = args
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(QUERY_TIMEOUT_MS);

    let ctx = &state.ctx;
    let session_id = session.0.to_string();
    let pf = match qore_service::query::preflight(
        &ctx.session_manager,
        &ctx.query_rate_limiter,
        &ctx.interceptor,
        &ctx.policy,
        session,
        &session_id,
        &query,
        namespace.as_ref(),
        acknowledged,
    )
    .await
    {
        Ok(pf) => pf,
        Err(e) => return failure(e),
    };

    let query_id = ctx.query_manager.register(session).await;
    let outcome = qore_service::query::execute(
        &ctx.query_manager,
        &ctx.query_cache,
        &ctx.interceptor,
        &ctx.policy,
        pf.driver,
        &pf.context,
        session,
        namespace,
        &query,
        query_id,
        pf.is_mutation,
        pf.connection_key.as_deref(),
        pf.safety_warning.as_deref(),
        Some(timeout),
        bypass_limits,
        None,
        None,
        |_, _| {},
    )
    .await;

    Json(json!({
        "success": outcome.success,
        "result": outcome.result,
        "error": outcome.error,
        "query_id": args.get("queryId").and_then(Value::as_str),
        "truncated": outcome.truncated,
        "truncated_total": outcome.truncated_total,
    }))
}

fn failure(message: impl Into<String>) -> Json<Value> {
    Json(json!({ "success": false, "error": message.into() }))
}

fn req_str(args: &Value, key: &str) -> Result<String, ApiError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| ApiError::bad_request(format!("missing arg: {key}")))
}

fn req_namespace(args: &Value) -> Result<Namespace, ApiError> {
    args.get("namespace")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(|| ApiError::bad_request("missing or invalid arg: namespace"))
}

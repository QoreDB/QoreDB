// SPDX-License-Identifier: Apache-2.0

//! Query Tauri Commands
//!
//! Commands for executing queries and exploring database schema.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tracing::{field, instrument};
use uuid::Uuid;

use crate::commands::stream_msg::StreamDispatcher;
use crate::engine::{
    sql_safety,
    types::{
        CollectionList, CollectionListOptions, CreationOptions, EventList, EventListOptions,
        ForeignKey, Namespace, PaginatedQueryResult, QueryId, QueryResult, RoutineList,
        RoutineListOptions, RoutineType, SequenceList, SequenceListOptions, SessionId,
        TableQueryOptions, TriggerList, TriggerListOptions, Value,
    },
    TableSchema,
};
use crate::interceptor::{Environment, QueryContext, QueryExecutionResult, SafetyAction};
use crate::metrics;
use crate::plugins::runtime::{
    HookContext as PluginHookContext, PluginHost, PostExecuteResult, QueryReadPayload,
};
use qore_service::governance;
use tauri::ipc::{Channel, InvokeResponseBody};

const READ_ONLY_BLOCKED: &str = "Operation blocked: read-only mode";
const DANGEROUS_BLOCKED: &str = "Dangerous query blocked: confirmation required";
const TRANSACTIONS_NOT_SUPPORTED: &str = "Transactions are not supported by this driver";
const SAFETY_RULE_BLOCKED: &str = "Query blocked by safety rule";

/// Past this, the `queryRead` payload is dropped and the plugin sees `None`.
const QUERY_READ_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

fn build_query_read_payload(result: &QueryResult) -> Option<Arc<QueryReadPayload>> {
    let json = serde_json::to_string(result).ok()?;
    if json.len() > QUERY_READ_MAX_PAYLOAD_BYTES {
        return None;
    }
    Some(Arc::new(QueryReadPayload { json }))
}

/// Schedules `postExecute` off the query critical path. Never propagates
/// individual plugin failures.
fn dispatch_plugin_post_execute(
    plugin_host: &Arc<PluginHost>,
    interceptor_context: &QueryContext,
    exec: &QueryExecutionResult,
    payload: Option<Arc<QueryReadPayload>>,
) {
    let hook_ctx = PluginHookContext {
        query: interceptor_context.query.clone(),
        driver_id: interceptor_context.driver_id.clone(),
        environment: format!("{:?}", interceptor_context.environment),
        operation_type: format!("{:?}", interceptor_context.operation_type),
        is_mutation: interceptor_context.is_mutation,
        is_dangerous: interceptor_context.is_dangerous,
        read_only: interceptor_context.read_only,
    };
    let post_result = PostExecuteResult {
        success: exec.success,
        execution_time_ms: exec.execution_time_ms as u64,
        row_count: exec.row_count.map(|r| r.max(0) as u64),
        error: exec.error.clone(),
    };
    plugin_host.schedule_post_execute(hook_ctx, post_result, payload);
}

fn map_environment(env: &str) -> Environment {
    match env {
        "production" => Environment::Production,
        "staging" => Environment::Staging,
        _ => Environment::Development,
    }
}

/// Response wrapper for query results
#[derive(Debug, Serialize)]
pub struct QueryResponse {
    pub success: bool,
    pub result: Option<QueryResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_results: Vec<QueryResult>,
    pub error: Option<String>,
    pub query_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated_total: Option<u64>,
}

/// Response wrapper for namespace listing
#[derive(Debug, Serialize)]
pub struct NamespacesResponse {
    pub success: bool,
    pub namespaces: Option<Vec<Namespace>>,
    pub error: Option<String>,
}

/// Response wrapper for collection listing
#[derive(Debug, Serialize)]
pub struct CollectionsResponse {
    pub success: bool,
    pub data: Option<CollectionList>,
    pub error: Option<String>,
}

/// Parses a session ID string into SessionId
fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

/// Executes a query on the given session
#[tauri::command]
#[instrument(
    skip(state, query, on_stream),
    fields(
        session_id = %session_id,
        query_id = ?query_id,
        query_len = query.len(),
        driver = field::Empty
    )
)]
pub async fn execute_query(
    state: State<'_, crate::SharedState>,
    window: tauri::Window,
    session_id: String,
    query: String,
    namespace: Option<Namespace>,
    acknowledged_dangerous: Option<bool>,
    query_id: Option<String>,
    timeout_ms: Option<u64>,
    stream: Option<bool>,
    bypass_limits: Option<bool>,
    on_stream: Channel<InvokeResponseBody>,
) -> Result<QueryResponse, String> {
    let requested_bypass = bypass_limits.unwrap_or(false);
    let (
        session_manager,
        query_manager,
        query_rate_limiter,
        query_cache,
        policy,
        interceptor,
        plugin_host,
        license_tier,
    ) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.query_manager),
            Arc::clone(&state.query_rate_limiter),
            Arc::clone(&state.query_cache),
            state.policy.clone(),
            Arc::clone(&state.interceptor),
            Arc::clone(&state.plugin_host),
            state.license_manager.effective_status().tier,
        )
    };

    // Gate governance-limit bypass behind Team+. Without this check, any JS in
    // the webview (or a DevTools call in debug) can pass `bypass_limits=true`
    // and dodge max_query_duration_ms / max_result_rows / concurrency caps.
    let bypass_limits = if requested_bypass {
        if !license_tier.includes(crate::license::status::LicenseTier::Team) {
            tracing::warn!(
                session = %session_id,
                tier = ?license_tier,
                "bypass_limits rejected: requires Team+ license"
            );
            return Ok(QueryResponse {
                extra_results: Vec::new(),
                success: false,
                result: None,
                error: Some(
                    "Governance limit bypass requires a Team or Enterprise license".to_string(),
                ),
                query_id: None,
                truncated: None,
                truncated_total: None,
            });
        }
        true
    } else {
        false
    };

    let session = parse_session_id(&session_id)?;

    let preflight = match qore_service::query::preflight(
        &session_manager,
        &query_rate_limiter,
        &interceptor,
        &policy,
        session,
        &session_id,
        &query,
        namespace.as_ref(),
        acknowledged_dangerous.unwrap_or(false),
    )
    .await
    {
        Ok(pf) => pf,
        Err(msg) => {
            return Ok(QueryResponse {
                extra_results: Vec::new(),
                success: false,
                result: None,
                error: Some(msg),
                query_id: None,
                truncated: None,
                truncated_total: None,
            });
        }
    };

    let qore_service::query::Preflight {
        driver,
        context: interceptor_context,
        environment: interceptor_env,
        read_only,
        is_mutation: is_mutation_for_context,
        is_dangerous,
        is_sql_driver,
        connection_key,
        safety_warning,
    } = preflight;
    tracing::Span::current().record("driver", field::display(driver.driver_id()));

    let plugin_decision = plugin_host
        .run_pre_execute(crate::plugins::runtime::HookContext {
            query: query.clone(),
            driver_id: driver.driver_id().to_string(),
            environment: format!("{:?}", interceptor_env),
            operation_type: query
                .trim_start()
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_uppercase(),
            is_mutation: is_mutation_for_context,
            is_dangerous,
            read_only,
        })
        .await;
    if let crate::plugins::runtime::Decision::Block { reason } = plugin_decision {
        return Ok(QueryResponse {
            extra_results: Vec::new(),
            success: false,
            result: None,
            error: Some(format!("Query blocked by plugin: {reason}")),
            query_id: None,
            truncated: None,
            truncated_total: None,
        });
    }

    if let Some(limit) = policy.max_concurrent_queries {
        let active = query_manager.count_active().await;
        if active >= limit as usize {
            return Ok(QueryResponse {
                extra_results: Vec::new(),
                success: false,
                result: None,
                error: Some(format!(
                    "Too many concurrent queries ({}/{})",
                    active, limit
                )),
                query_id: None,
                truncated: None,
                truncated_total: None,
            });
        }
    }

    let query_id = if let Some(raw) = query_id {
        let parsed = Uuid::parse_str(&raw).map_err(|e| format!("Invalid query ID: {}", e))?;
        let qid = QueryId(parsed);
        query_manager
            .register_with_id(session, qid)
            .await
            .map_err(|e| format!("Failed to register query ID: {}", e))?;
        qid
    } else {
        query_manager.register(session).await
    };
    let query_id_str = query_id.0.to_string();

    let sql_statements = if is_sql_driver {
        match sql_safety::split_sql_statements(driver.driver_id(), &query) {
            Ok(statements) if statements.len() > 1 => Some(statements),
            _ => None,
        }
    } else {
        None
    };

    let should_stream =
        sql_statements.is_none() && stream.unwrap_or(false) && driver.capabilities().streaming;

    // Absolute cap (1h) applied even when bypass_limits is granted, so a
    // misconfigured Team+ client cannot pin a query indefinitely.
    const BYPASS_TIMEOUT_CAP_MS: u64 = 3_600_000;
    let effective_timeout = if bypass_limits {
        Some(
            timeout_ms
                .unwrap_or(BYPASS_TIMEOUT_CAP_MS)
                .min(BYPASS_TIMEOUT_CAP_MS),
        )
    } else {
        timeout_ms.or(policy.max_query_duration_ms)
    };

    if bypass_limits {
        tracing::warn!(
            session = %session_id,
            query_id = %query_id_str,
            driver = %driver.driver_id(),
            env = ?interceptor_env,
            tier = ?license_tier,
            effective_timeout_ms = ?effective_timeout,
            "Governance limits bypassed for single query (Team+ override)"
        );
    }

    let stream_sender = if should_stream {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1024);
        let qid_cloned = query_id_str.clone();
        let window_cloned = window.clone();
        let on_stream_cloned = on_stream.clone();

        // A long-lived `StreamDispatcher` lets the buffer-capacity hint
        // accumulate across batches and avoids the realloc cascade in rmp_serde.
        tokio::spawn(async move {
            let mut dispatcher =
                StreamDispatcher::new(Some(&on_stream_cloned), &window_cloned, &qid_cloned);
            while let Some(event) = receiver.recv().await {
                dispatcher.dispatch(event);
            }
        });
        Some(sender)
    } else {
        None
    };

    let plugin_ctx = interceptor_context.clone();
    let plugin_host_for_complete = Arc::clone(&plugin_host);
    let on_complete = move |exec: &QueryExecutionResult, result: Option<&QueryResult>| {
        let payload = result.and_then(build_query_read_payload);
        dispatch_plugin_post_execute(&plugin_host_for_complete, &plugin_ctx, exec, payload);
    };

    let outcome = qore_service::query::execute(
        &query_manager,
        &query_cache,
        &interceptor,
        &policy,
        driver,
        &interceptor_context,
        session,
        namespace.clone(),
        &query,
        query_id,
        is_mutation_for_context,
        connection_key.as_deref(),
        safety_warning.as_deref(),
        effective_timeout,
        bypass_limits,
        sql_statements,
        stream_sender,
        on_complete,
    )
    .await;

    Ok(QueryResponse {
        success: outcome.success,
        result: outcome.result,
        extra_results: outcome.extra_results,
        error: outcome.error,
        query_id: Some(query_id_str),
        truncated: outcome.truncated,
        truncated_total: outcome.truncated_total,
    })
}

/// Cancels a running query
#[tauri::command]
#[instrument(
    skip(state),
    fields(session_id = %session_id, query_id = ?query_id, driver = field::Empty)
)]
pub async fn cancel_query(
    state: State<'_, crate::SharedState>,
    session_id: String,
    query_id: Option<String>,
) -> Result<QueryResponse, String> {
    let (session_manager, query_manager) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.query_manager),
        )
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(QueryResponse {
                extra_results: Vec::new(),
                success: false,
                result: None,
                error: Some(e.sanitized_message()),
                query_id: None,
                truncated: None,
                truncated_total: None,
            });
        }
    };
    tracing::Span::current().record("driver", field::display(driver.driver_id()));

    let query_id = if let Some(raw) = query_id {
        let parsed = Uuid::parse_str(&raw).map_err(|e| format!("Invalid query ID: {}", e))?;
        QueryId(parsed)
    } else {
        match query_manager.last_for_session(session).await {
            Some(qid) => qid,
            None => {
                return Ok(QueryResponse {
                    extra_results: Vec::new(),
                    success: false,
                    result: None,
                    error: Some("No active query found".to_string()),
                    query_id: None,
                    truncated: None,
                    truncated_total: None,
                });
            }
        }
    };
    let query_id_str = query_id.0.to_string();

    match driver.cancel(session, Some(query_id)).await {
        Ok(()) => {
            metrics::record_cancel();
            Ok(QueryResponse {
                extra_results: Vec::new(),
                success: true,
                result: None,
                error: None,
                query_id: Some(query_id_str),
                truncated: None,
                truncated_total: None,
            })
        }
        Err(e) => Ok(QueryResponse {
            extra_results: Vec::new(),
            success: false,
            result: None,
            error: Some(e.sanitized_message()),
            query_id: Some(query_id_str),
            truncated: None,
            truncated_total: None,
        }),
    }
}

/// Lists all namespaces (databases/schemas) for a session
#[tauri::command]
pub async fn list_namespaces(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<NamespacesResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(NamespacesResponse {
                success: false,
                namespaces: None,
                error: Some(e.sanitized_message()),
            });
        }
    };

    match driver.list_namespaces(session).await {
        Ok(namespaces) => Ok(NamespacesResponse {
            success: true,
            namespaces: Some(namespaces),
            error: None,
        }),
        Err(e) => Ok(NamespacesResponse {
            success: false,
            namespaces: None,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Lists all collections (tables/views) in a namespace
#[tauri::command]
pub async fn list_collections(
    state: State<'_, crate::SharedState>,
    session_id: String,
    namespace: Namespace,
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<CollectionsResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(CollectionsResponse {
                success: false,
                data: None,
                error: Some(e.sanitized_message()),
            });
        }
    };

    let options = CollectionListOptions {
        search,
        page,
        page_size,
    };

    match driver.list_collections(session, &namespace, options).await {
        Ok(list) => Ok(CollectionsResponse {
            success: true,
            data: Some(list),
            error: None,
        }),
        Err(e) => Ok(CollectionsResponse {
            success: false,
            data: None,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Response wrapper for routine listing
#[derive(Debug, Serialize)]
pub struct RoutinesResponse {
    pub success: bool,
    pub data: Option<RoutineList>,
    pub error: Option<String>,
}

/// Lists all routines (functions/procedures) in a namespace
#[tauri::command]
pub async fn list_routines(
    state: State<'_, crate::SharedState>,
    session_id: String,
    namespace: Namespace,
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
    routine_type: Option<String>,
) -> Result<RoutinesResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(RoutinesResponse {
                success: false,
                data: None,
                error: Some(e.sanitized_message()),
            });
        }
    };

    let routine_type_enum = routine_type.as_ref().and_then(|t| match t.as_str() {
        "Function" => Some(RoutineType::Function),
        "Procedure" => Some(RoutineType::Procedure),
        _ => None,
    });

    let options = RoutineListOptions {
        search,
        page,
        page_size,
        routine_type: routine_type_enum,
    };

    match driver.list_routines(session, &namespace, options).await {
        Ok(list) => Ok(RoutinesResponse {
            success: true,
            data: Some(list),
            error: None,
        }),
        Err(e) => Ok(RoutinesResponse {
            success: false,
            data: None,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Response wrapper for trigger listing
#[derive(Debug, Serialize)]
pub struct TriggersResponse {
    pub success: bool,
    pub data: Option<TriggerList>,
    pub error: Option<String>,
}

/// Lists all triggers in a namespace
#[tauri::command]
pub async fn list_triggers(
    state: State<'_, crate::SharedState>,
    session_id: String,
    namespace: Namespace,
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<TriggersResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(TriggersResponse {
                success: false,
                data: None,
                error: Some(e.sanitized_message()),
            });
        }
    };

    let options = TriggerListOptions {
        search,
        page,
        page_size,
    };

    match driver.list_triggers(session, &namespace, options).await {
        Ok(list) => Ok(TriggersResponse {
            success: true,
            data: Some(list),
            error: None,
        }),
        Err(e) => Ok(TriggersResponse {
            success: false,
            data: None,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Response wrapper for event listing
#[derive(Debug, Serialize)]
pub struct EventsResponse {
    pub success: bool,
    pub data: Option<EventList>,
    pub error: Option<String>,
}

/// Lists all scheduled events in a namespace (MySQL only)
#[tauri::command]
pub async fn list_events(
    state: State<'_, crate::SharedState>,
    session_id: String,
    namespace: Namespace,
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<EventsResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(EventsResponse {
                success: false,
                data: None,
                error: Some(e.sanitized_message()),
            });
        }
    };

    let options = EventListOptions {
        search,
        page,
        page_size,
    };

    match driver.list_events(session, &namespace, options).await {
        Ok(list) => Ok(EventsResponse {
            success: true,
            data: Some(list),
            error: None,
        }),
        Err(e) => Ok(EventsResponse {
            success: false,
            data: None,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Response wrapper for sequence listing
#[derive(Debug, Serialize)]
pub struct SequencesResponse {
    pub success: bool,
    pub data: Option<SequenceList>,
    pub error: Option<String>,
}

/// Lists all sequences in a namespace (MariaDB 10.3+)
#[tauri::command]
pub async fn list_sequences(
    state: State<'_, crate::SharedState>,
    session_id: String,
    namespace: Namespace,
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<SequencesResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(SequencesResponse {
                success: false,
                data: None,
                error: Some(e.sanitized_message()),
            });
        }
    };

    let options = SequenceListOptions {
        search,
        page,
        page_size,
    };

    match driver.list_sequences(session, &namespace, options).await {
        Ok(list) => Ok(SequencesResponse {
            success: true,
            data: Some(list),
            error: None,
        }),
        Err(e) => Ok(SequencesResponse {
            success: false,
            data: None,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Response wrapper for table schema
#[derive(Debug, Serialize)]
pub struct TableSchemaResponse {
    pub success: bool,
    pub schema: Option<TableSchema>,
    pub error: Option<String>,
}

/// Gets the schema of a table/collection
#[tauri::command]
pub async fn describe_table(
    state: State<'_, crate::SharedState>,
    session_id: String,
    namespace: Namespace,
    table: String,
    connection_id: Option<String>,
) -> Result<TableSchemaResponse, String> {
    let (session_manager, vr_store) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.virtual_relations),
        )
    };
    let session = parse_session_id(&session_id)?;

    match qore_service::query::describe_table(
        &session_manager,
        &vr_store,
        session,
        &namespace,
        &table,
        connection_id.as_deref(),
    )
    .await
    {
        Ok(schema) => Ok(TableSchemaResponse {
            success: true,
            schema: Some(schema),
            error: None,
        }),
        Err(e) => Ok(TableSchemaResponse {
            success: false,
            schema: None,
            error: Some(e.sanitized()),
        }),
    }
}

/// Gets a preview of table data (first N rows)
#[tauri::command]
pub async fn preview_table(
    state: State<'_, crate::SharedState>,
    session_id: String,
    namespace: Namespace,
    table: String,
    limit: u32,
    bypass_cache: Option<bool>,
) -> Result<QueryResponse, String> {
    let (session_manager, query_manager, policy, query_cache) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.query_manager),
            state.policy.clone(),
            Arc::clone(&state.query_cache),
        )
    };
    let session = parse_session_id(&session_id)?;

    match qore_service::query::preview_table(
        &session_manager,
        &query_manager,
        &query_cache,
        &policy,
        session,
        &namespace,
        &table,
        limit,
        bypass_cache.unwrap_or(false),
    )
    .await
    {
        Ok(result) => Ok(QueryResponse {
            extra_results: Vec::new(),
            success: true,
            result: Some(result),
            error: None,
            query_id: None,
            truncated: None,
            truncated_total: None,
        }),
        Err(e) => Ok(QueryResponse {
            extra_results: Vec::new(),
            success: false,
            result: None,
            error: Some(e.sanitized()),
            query_id: None,
            truncated: None,
            truncated_total: None,
        }),
    }
}

/// Response wrapper for paginated table queries
#[derive(Debug, Serialize)]
pub struct PaginatedQueryResponse {
    pub success: bool,
    pub result: Option<PaginatedQueryResult>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated_total: Option<u64>,
    /// `Some(true)` when this result was served from the query cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached: Option<bool>,
    /// Age of the cached entry in milliseconds, when served from cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_age_ms: Option<u64>,
}

/// Queries table data with pagination, sorting, and filtering support
#[tauri::command]
pub async fn query_table(
    state: State<'_, crate::SharedState>,
    session_id: String,
    namespace: Namespace,
    table: String,
    options: TableQueryOptions,
    bypass_cache: Option<bool>,
) -> Result<PaginatedQueryResponse, String> {
    let (session_manager, query_manager, policy, query_cache) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.query_manager),
            state.policy.clone(),
            Arc::clone(&state.query_cache),
        )
    };
    let session = parse_session_id(&session_id)?;

    match qore_service::query::query_table(
        &session_manager,
        &query_manager,
        &query_cache,
        &policy,
        session,
        &namespace,
        &table,
        options,
        bypass_cache.unwrap_or(false),
    )
    .await
    {
        Ok((result, cached_age_ms)) => Ok(PaginatedQueryResponse {
            success: true,
            result: Some(result),
            error: None,
            truncated: None,
            truncated_total: None,
            cached: cached_age_ms.map(|_| true),
            cached_age_ms,
        }),
        Err(e) => Ok(PaginatedQueryResponse {
            success: false,
            result: None,
            error: Some(e.sanitized()),
            truncated: None,
            truncated_total: None,
            cached: None,
            cached_age_ms: None,
        }),
    }
}

/// Fetches a related row based on a foreign key value
#[tauri::command]
pub async fn peek_foreign_key(
    state: State<'_, crate::SharedState>,
    session_id: String,
    namespace: Namespace,
    foreign_key: ForeignKey,
    value: Value,
    limit: Option<u32>,
) -> Result<QueryResponse, String> {
    let (session_manager, query_manager, policy) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.query_manager),
            state.policy.clone(),
        )
    };
    let session = parse_session_id(&session_id)?;
    // UX cap for the tooltip preview; policy may tighten further.
    let requested = limit.unwrap_or(3).clamp(1, 25);
    let limit = governance::clamp_rows(&policy, requested);

    if foreign_key.referenced_table.trim().is_empty()
        || foreign_key.referenced_column.trim().is_empty()
        || matches!(value, Value::Null)
    {
        return Ok(QueryResponse {
            extra_results: Vec::new(),
            success: true,
            result: Some(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: None,
                execution_time_ms: 0.0,
            }),
            error: None,
            query_id: None,
            truncated: None,
            truncated_total: None,
        });
    }

    if let Err(msg) = governance::check_concurrent_limit(&policy, &query_manager).await {
        return Ok(QueryResponse {
            extra_results: Vec::new(),
            success: false,
            result: None,
            error: Some(msg),
            query_id: None,
            truncated: None,
            truncated_total: None,
        });
    }

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(QueryResponse {
                extra_results: Vec::new(),
                success: false,
                result: None,
                error: Some(e.sanitized_message()),
                query_id: None,
                truncated: None,
                truncated_total: None,
            });
        }
    };

    let result = governance::with_timeout(
        &policy,
        driver.peek_foreign_key(session, &namespace, &foreign_key, &value, limit),
    )
    .await;

    match result {
        Ok(Ok(result)) => Ok(QueryResponse {
            extra_results: Vec::new(),
            success: true,
            result: Some(result),
            error: None,
            query_id: None,
            truncated: None,
            truncated_total: None,
        }),
        Ok(Err(e)) => Ok(QueryResponse {
            extra_results: Vec::new(),
            success: false,
            result: None,
            error: Some(e.sanitized_message()),
            query_id: None,
            truncated: None,
            truncated_total: None,
        }),
        Err(timeout_msg) => Ok(QueryResponse {
            extra_results: Vec::new(),
            success: false,
            result: None,
            error: Some(timeout_msg),
            query_id: None,
            truncated: None,
            truncated_total: None,
        }),
    }
}

/// Creates a new database (or schema)
#[tauri::command]
pub async fn create_database(
    state: State<'_, crate::SharedState>,
    session_id: String,
    name: String,
    options: Option<serde_json::Value>,
    acknowledged_dangerous: Option<bool>,
) -> Result<QueryResponse, String> {
    let (session_manager, interceptor) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.interceptor),
        )
    };
    let session = parse_session_id(&session_id)?;

    let read_only = session_manager.is_read_only(session).await.unwrap_or(false);
    if read_only {
        return Ok(QueryResponse {
            extra_results: Vec::new(),
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
            query_id: None,
            truncated: None,
            truncated_total: None,
        });
    }

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(QueryResponse {
                extra_results: Vec::new(),
                success: false,
                result: None,
                error: Some(e.sanitized_message()),
                query_id: None,
                truncated: None,
                truncated_total: None,
            });
        }
    };

    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let interceptor_env = map_environment(&environment);

    let query_preview = format!("CREATE DATABASE {}", name);
    let acknowledged = acknowledged_dangerous.unwrap_or(false);
    let interceptor_context = interceptor.build_context(
        &session_id,
        &query_preview,
        driver.driver_id(),
        interceptor_env,
        read_only,
        acknowledged,
        Some(&name),
        None,
        true,
    );

    let safety_result = interceptor.pre_execute(&interceptor_context);
    if !safety_result.allowed {
        interceptor.post_execute(
            &interceptor_context,
            &QueryExecutionResult {
                success: false,
                error: safety_result.message.clone(),
                execution_time_ms: 0.0,
                row_count: None,
            },
            true,
            safety_result.triggered_rule.as_deref(),
        );

        let error_msg = match safety_result.action {
            SafetyAction::Block => {
                format!(
                    "{}: {}",
                    SAFETY_RULE_BLOCKED,
                    safety_result.message.unwrap_or_default()
                )
            }
            SafetyAction::RequireConfirmation => {
                format!(
                    "{}: {}",
                    DANGEROUS_BLOCKED,
                    safety_result.message.unwrap_or_default()
                )
            }
            SafetyAction::Warn => "Warning triggered".to_string(),
        };

        return Ok(QueryResponse {
            extra_results: Vec::new(),
            success: false,
            result: None,
            error: Some(error_msg),
            query_id: None,
            truncated: None,
            truncated_total: None,
        });
    }

    let safety_warning = if matches!(safety_result.action, SafetyAction::Warn) {
        safety_result.triggered_rule.clone()
    } else {
        None
    };

    let engine_options = options.map(crate::engine::types::Value::Json);

    let start_time = std::time::Instant::now();
    match driver.create_database(session, &name, engine_options).await {
        Ok(()) => {
            let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: true,
                    error: None,
                    execution_time_ms: duration_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(QueryResponse {
                extra_results: Vec::new(),
                success: true,
                result: None,
                error: None,
                query_id: None,
                truncated: None,
                truncated_total: None,
            })
        }
        Err(e) => {
            let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: false,
                    error: Some(e.sanitized_message()),
                    execution_time_ms: duration_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(QueryResponse {
                extra_results: Vec::new(),
                success: false,
                result: None,
                error: Some(e.sanitized_message()),
                query_id: None,
                truncated: None,
                truncated_total: None,
            })
        }
    }
}

/// Drops an existing database (or schema)
#[tauri::command]
pub async fn drop_database(
    state: State<'_, crate::SharedState>,
    session_id: String,
    name: String,
    acknowledged_dangerous: Option<bool>,
) -> Result<QueryResponse, String> {
    let (session_manager, interceptor) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.interceptor),
        )
    };
    let session = parse_session_id(&session_id)?;

    let read_only = session_manager.is_read_only(session).await.unwrap_or(false);
    if read_only {
        return Ok(QueryResponse {
            extra_results: Vec::new(),
            success: false,
            result: None,
            error: Some(READ_ONLY_BLOCKED.to_string()),
            query_id: None,
            truncated: None,
            truncated_total: None,
        });
    }

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(QueryResponse {
                extra_results: Vec::new(),
                success: false,
                result: None,
                error: Some(e.sanitized_message()),
                query_id: None,
                truncated: None,
                truncated_total: None,
            });
        }
    };

    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let interceptor_env = map_environment(&environment);

    let query_preview = format!("DROP DATABASE {}", name);
    let acknowledged = acknowledged_dangerous.unwrap_or(false);
    let interceptor_context = interceptor.build_context(
        &session_id,
        &query_preview,
        driver.driver_id(),
        interceptor_env,
        read_only,
        acknowledged,
        Some(&name),
        None,
        true,
    );

    let safety_result = interceptor.pre_execute(&interceptor_context);
    if !safety_result.allowed {
        interceptor.post_execute(
            &interceptor_context,
            &QueryExecutionResult {
                success: false,
                error: safety_result.message.clone(),
                execution_time_ms: 0.0,
                row_count: None,
            },
            true,
            safety_result.triggered_rule.as_deref(),
        );

        let error_msg = match safety_result.action {
            SafetyAction::Block => {
                format!(
                    "{}: {}",
                    SAFETY_RULE_BLOCKED,
                    safety_result.message.unwrap_or_default()
                )
            }
            SafetyAction::RequireConfirmation => {
                format!(
                    "{}: {}",
                    DANGEROUS_BLOCKED,
                    safety_result.message.unwrap_or_default()
                )
            }
            SafetyAction::Warn => "Warning triggered".to_string(),
        };

        return Ok(QueryResponse {
            extra_results: Vec::new(),
            success: false,
            result: None,
            error: Some(error_msg),
            query_id: None,
            truncated: None,
            truncated_total: None,
        });
    }

    let safety_warning = if matches!(safety_result.action, SafetyAction::Warn) {
        safety_result.triggered_rule.clone()
    } else {
        None
    };

    let start_time = std::time::Instant::now();
    match driver.drop_database(session, &name).await {
        Ok(()) => {
            let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: true,
                    error: None,
                    execution_time_ms: duration_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(QueryResponse {
                extra_results: Vec::new(),
                success: true,
                result: None,
                error: None,
                query_id: None,
                truncated: None,
                truncated_total: None,
            })
        }
        Err(e) => {
            let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: false,
                    error: Some(e.sanitized_message()),
                    execution_time_ms: duration_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(QueryResponse {
                extra_results: Vec::new(),
                success: false,
                result: None,
                error: Some(e.sanitized_message()),
                query_id: None,
                truncated: None,
                truncated_total: None,
            })
        }
    }
}

// ==================== Creation Options Commands ====================

/// Response wrapper for database creation options
#[derive(Debug, Serialize)]
pub struct CreationOptionsResponse {
    pub success: bool,
    pub options: Option<CreationOptions>,
    pub error: Option<String>,
}

/// Returns the creation options (charsets, collations) available for the driver
#[tauri::command]
pub async fn get_creation_options(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<CreationOptionsResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(CreationOptionsResponse {
                success: false,
                options: None,
                error: Some(e.sanitized_message()),
            });
        }
    };

    match driver.get_creation_options(session).await {
        Ok(options) => Ok(CreationOptionsResponse {
            success: true,
            options: Some(options),
            error: None,
        }),
        Err(e) => Ok(CreationOptionsResponse {
            success: false,
            options: None,
            error: Some(e.sanitized_message()),
        }),
    }
}

// ==================== Transaction Commands ====================

/// Response wrapper for transaction operations
#[derive(Debug, Serialize)]
pub struct TransactionResponse {
    pub success: bool,
    pub error: Option<String>,
}

/// Response for transaction support check
#[derive(Debug, Serialize)]
pub struct TransactionSupportResponse {
    pub supported: bool,
}

/// Begins a transaction on the given session
///
/// Acquires a dedicated connection from the pool and executes BEGIN.
/// All subsequent queries on this session will use this connection
/// until commit or rollback is called.
#[tauri::command]
pub async fn begin_transaction(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<TransactionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(TransactionResponse {
                success: false,
                error: Some(e.sanitized_message()),
            });
        }
    };

    if !driver.supports_transactions_for_session(session).await {
        return Ok(TransactionResponse {
            success: false,
            error: Some(TRANSACTIONS_NOT_SUPPORTED.to_string()),
        });
    }

    match driver.begin_transaction(session).await {
        Ok(()) => Ok(TransactionResponse {
            success: true,
            error: None,
        }),
        Err(e) => Ok(TransactionResponse {
            success: false,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Commits the current transaction on the given session
///
/// Executes COMMIT and releases the dedicated connection back to the pool.
#[tauri::command]
pub async fn commit_transaction(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<TransactionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(TransactionResponse {
                success: false,
                error: Some(e.sanitized_message()),
            });
        }
    };

    if !driver.supports_transactions_for_session(session).await {
        return Ok(TransactionResponse {
            success: false,
            error: Some(TRANSACTIONS_NOT_SUPPORTED.to_string()),
        });
    }

    match driver.commit(session).await {
        Ok(()) => Ok(TransactionResponse {
            success: true,
            error: None,
        }),
        Err(e) => Ok(TransactionResponse {
            success: false,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Rolls back the current transaction on the given session
///
/// Executes ROLLBACK and releases the dedicated connection back to the pool.
#[tauri::command]
pub async fn rollback_transaction(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<TransactionResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(TransactionResponse {
                success: false,
                error: Some(e.sanitized_message()),
            });
        }
    };

    if !driver.supports_transactions_for_session(session).await {
        return Ok(TransactionResponse {
            success: false,
            error: Some(TRANSACTIONS_NOT_SUPPORTED.to_string()),
        });
    }

    match driver.rollback(session).await {
        Ok(()) => Ok(TransactionResponse {
            success: true,
            error: None,
        }),
        Err(e) => Ok(TransactionResponse {
            success: false,
            error: Some(e.sanitized_message()),
        }),
    }
}

/// Checks if the driver for the given session supports transactions
#[tauri::command]
pub async fn supports_transactions(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<TransactionSupportResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(_) => {
            return Ok(TransactionSupportResponse { supported: false });
        }
    };

    Ok(TransactionSupportResponse {
        supported: driver.supports_transactions_for_session(session).await,
    })
}

// --- Query Governance ---

#[derive(Debug, Serialize, Deserialize)]
pub struct GovernanceLimits {
    pub max_query_duration_ms: Option<u64>,
    pub max_result_rows: Option<u64>,
    pub max_concurrent_queries: Option<u32>,
}

#[tauri::command]
pub async fn get_governance_limits(
    state: State<'_, crate::SharedState>,
) -> Result<GovernanceLimits, String> {
    let policy = {
        let state = state.lock().await;
        state.policy.clone()
    };
    Ok(GovernanceLimits {
        max_query_duration_ms: policy.max_query_duration_ms,
        max_result_rows: policy.max_result_rows,
        max_concurrent_queries: policy.max_concurrent_queries,
    })
}

/// Clamp ranges for [`update_governance_limits`]. Without these, a frontend
/// caller (or compromised webview JS) could send `max_query_duration_ms = 0`
/// — every query times out instantly — or `max_result_rows = u64::MAX` —
/// the limit is effectively gone (cf. audit B6-H4).
const MIN_QUERY_DURATION_MS: u64 = 100;
const MAX_QUERY_DURATION_MS: u64 = 60 * 60 * 1000; // 1h hard cap
const MIN_RESULT_ROWS: u64 = 1;
const MAX_RESULT_ROWS_CAP: u64 = 100_000_000;
const MIN_CONCURRENT_QUERIES: u32 = 1;
const MAX_CONCURRENT_QUERIES: u32 = 256;

fn clamp_governance_limits(mut limits: GovernanceLimits) -> GovernanceLimits {
    limits.max_query_duration_ms = limits
        .max_query_duration_ms
        .map(|v| v.clamp(MIN_QUERY_DURATION_MS, MAX_QUERY_DURATION_MS));
    limits.max_result_rows = limits
        .max_result_rows
        .map(|v| v.clamp(MIN_RESULT_ROWS, MAX_RESULT_ROWS_CAP));
    limits.max_concurrent_queries = limits
        .max_concurrent_queries
        .map(|v| v.clamp(MIN_CONCURRENT_QUERIES, MAX_CONCURRENT_QUERIES));
    limits
}

#[tauri::command]
pub async fn update_governance_limits(
    state: State<'_, crate::SharedState>,
    limits: GovernanceLimits,
) -> Result<GovernanceLimits, String> {
    let limits = clamp_governance_limits(limits);
    let mut state = state.lock().await;
    state.policy.max_query_duration_ms = limits.max_query_duration_ms;
    state.policy.max_result_rows = limits.max_result_rows;
    state.policy.max_concurrent_queries = limits.max_concurrent_queries;
    state
        .policy
        .save_to_file()
        .map_err(|e| format!("Failed to save governance limits: {}", e))?;
    Ok(GovernanceLimits {
        max_query_duration_ms: state.policy.max_query_duration_ms,
        max_result_rows: state.policy.max_result_rows,
        max_concurrent_queries: state.policy.max_concurrent_queries,
    })
}

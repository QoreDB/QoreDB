// SPDX-License-Identifier: Apache-2.0

//! Query Tauri Commands
//!
//! Commands for executing queries and exploring database schema.

use serde::Serialize;
use tauri::State;
use uuid::Uuid;
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tracing::{field, instrument};

use crate::engine::{
    mongo_safety,
    redis_safety,
    sql_safety,
    TableSchema,
    traits::StreamEvent,
    types::{CollectionList, CollectionListOptions, ForeignKey, Namespace, QueryId, QueryResult, SessionId, Value, TableQueryOptions, PaginatedQueryResult, RoutineList, RoutineListOptions, RoutineType, TriggerList, TriggerListOptions, EventList, EventListOptions, CreationOptions},
};
use crate::interceptor::{Environment, QueryExecutionResult, SafetyAction};
use crate::metrics;
use tauri::Emitter;

const READ_ONLY_BLOCKED: &str = "Operation blocked: read-only mode";
const DANGEROUS_BLOCKED: &str = "Dangerous query blocked: confirmation required";
const DANGEROUS_BLOCKED_POLICY: &str = "Dangerous query blocked by policy";
const SQL_PARSE_BLOCKED: &str = "Operation blocked: SQL parser could not classify the query";
const TRANSACTIONS_NOT_SUPPORTED: &str = "Transactions are not supported by this driver";
const SAFETY_RULE_BLOCKED: &str = "Query blocked by safety rule";

fn is_mongo_mutation(query: &str) -> bool {
    matches!(
        mongo_safety::classify(query),
        mongo_safety::MongoQueryClass::Mutation | mongo_safety::MongoQueryClass::Unknown
    )
}

fn is_redis_mutation(query: &str) -> bool {
    matches!(
        redis_safety::classify(query),
        redis_safety::RedisQueryClass::Mutation
            | redis_safety::RedisQueryClass::Dangerous
            | redis_safety::RedisQueryClass::Unknown
    )
}

fn is_redis_dangerous(query: &str) -> bool {
    matches!(
        redis_safety::classify(query),
        redis_safety::RedisQueryClass::Dangerous
    )
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
    pub error: Option<String>,
    pub query_id: Option<String>,
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
    skip(state, query),
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
) -> Result<QueryResponse, String> {
    let (session_manager, query_manager, policy, interceptor) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.query_manager),
            state.policy.clone(),
            Arc::clone(&state.interceptor),
        )
    };
    let session = parse_session_id(&session_id)?;

    let read_only = match session_manager.is_read_only(session).await {
        Ok(read_only) => read_only,
        Err(e) => {
            return Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                query_id: None,
            });
        }
    };

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                query_id: None,
            });
        }
    };
    tracing::Span::current().record("driver", &field::display(driver.driver_id()));

    let environment = match session_manager.get_environment(session).await {
        Ok(value) => value,
        Err(_) => "development".to_string(),
    };
    let interceptor_env = map_environment(&environment);
    let is_production = matches!(interceptor_env, Environment::Production);

    let acknowledged = acknowledged_dangerous.unwrap_or(false);
    let is_mongo_driver = driver.driver_id().eq_ignore_ascii_case("mongodb");
    let is_redis_driver = driver.driver_id().eq_ignore_ascii_case("redis");
    let is_sql_driver = !is_mongo_driver && !is_redis_driver;
    let sql_analysis = if is_sql_driver {
        match sql_safety::analyze_sql(driver.driver_id(), &query) {
            Ok(analysis) => Some(analysis),
            Err(err) => {
                if read_only {
                    return Ok(QueryResponse {
                        success: false,
                        result: None,
                        error: Some(format!("{SQL_PARSE_BLOCKED}: {err}")),
                        query_id: None,
                    });
                }

                if is_production {
                    if policy.prod_block_dangerous_sql {
                        return Ok(QueryResponse {
                            success: false,
                            result: None,
                            error: Some(format!(
                                "{DANGEROUS_BLOCKED_POLICY}: SQL parse error: {err}"
                            )),
                            query_id: None,
                        });
                    }

                    if policy.prod_require_confirmation && !acknowledged {
                        return Ok(QueryResponse {
                            success: false,
                            result: None,
                            error: Some(format!(
                                "{DANGEROUS_BLOCKED}: SQL parse error: {err}"
                            )),
                            query_id: None,
                        });
                    }
                }

                None
            }
        }
    } else {
        None
    };

    if read_only {
        let is_mutation = if is_sql_driver {
            sql_analysis
                .as_ref()
                .map(|analysis| analysis.is_mutation)
                .unwrap_or(false)
        } else if is_mongo_driver {
            is_mongo_mutation(&query)
        } else {
            is_redis_mutation(&query)
        };

        if is_mutation {
            return Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(READ_ONLY_BLOCKED.to_string()),
                query_id: None,
            });
        }
    }

    if is_production {
        let is_dangerous = if is_sql_driver {
            sql_analysis
                .as_ref()
                .map(|analysis| analysis.is_dangerous)
                .unwrap_or(false)
        } else if is_redis_driver {
            is_redis_dangerous(&query)
        } else {
            false
        };

        if is_dangerous {
            if policy.prod_block_dangerous_sql {
                return Ok(QueryResponse {
                    success: false,
                    result: None,
                    error: Some(DANGEROUS_BLOCKED_POLICY.to_string()),
                    query_id: None,
                });
            }

            if policy.prod_require_confirmation && !acknowledged {
                return Ok(QueryResponse {
                    success: false,
                    result: None,
                    error: Some(DANGEROUS_BLOCKED.to_string()),
                    query_id: None,
                });
            }
        }
    }

    // Build interceptor context
    let is_mutation_for_context = if is_sql_driver {
        sql_analysis
            .as_ref()
            .map(|a| a.is_mutation)
            .unwrap_or(false)
    } else if is_mongo_driver {
        is_mongo_mutation(&query)
    } else {
        is_redis_mutation(&query)
    };

    let interceptor_context = interceptor.build_context(
        &session_id,
        &query,
        driver.driver_id(),
        interceptor_env,
        read_only,
        acknowledged,
        namespace.as_ref().map(|n| n.database.as_str()),
        sql_analysis.as_ref(),
        is_mutation_for_context,
    );

    // Run interceptor pre-execution checks (safety rules)
    let safety_result = interceptor.pre_execute(&interceptor_context);
    if !safety_result.allowed {
        // Record blocked query in interceptor
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
            SafetyAction::Warn => {
                // Warn allows execution, so this shouldn't happen
                "Warning triggered".to_string()
            }
        };

        return Ok(QueryResponse {
            success: false,
            result: None,
            error: Some(error_msg),
            query_id: None,
        });
    }

    let safety_warning = if matches!(safety_result.action, SafetyAction::Warn) {
        safety_result.triggered_rule.clone()
    } else {
        None
    };

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

    let should_stream = sql_statements.is_none()
        && stream.unwrap_or(false)
        && driver.capabilities().streaming;

    if should_stream {
        // Create channel for stream events
        let (sender, mut receiver) = tokio::sync::mpsc::channel(100);
        let qid_cloned = query_id_str.clone();
        let window_cloned = window.clone();

        // Spawn task to handle events and emit to frontend
        tokio::spawn(async move {
            while let Some(event) = receiver.recv().await {
                match event {
                    StreamEvent::Columns(cols) => {
                        let _ = window_cloned.emit(&format!("query_stream_columns:{}", qid_cloned), cols);
                    }
                    StreamEvent::Row(row) => {
                        let _ = window_cloned.emit(&format!("query_stream_row:{}", qid_cloned), row);
                    }
                    StreamEvent::Error(e) => {
                        let _ = window_cloned.emit(&format!("query_stream_error:{}", qid_cloned), e);
                    }
                    StreamEvent::Done(affected) => {
                        let _ = window_cloned.emit(&format!("query_stream_done:{}", qid_cloned), affected);
                    }
                }
            }
        });

        // Execute streaming
        let start_time = std::time::Instant::now();
        let execution = driver.execute_stream_in_namespace(session, namespace.clone(), &query, query_id, sender);

        // Handle timeout for the *start* or completion?
        // With streaming, the execution future completes when the stream is DONE.
        // So we can still await it with timeout.

        let result = if let Some(timeout_value) = timeout_ms {
            match timeout(Duration::from_millis(timeout_value), execution).await {
                Ok(res) => res,
                Err(_) => {
                    let _ = driver.cancel(session, Some(query_id)).await;
                    query_manager.finish(query_id).await;
                    metrics::record_timeout();

                    // Record timeout in interceptor
                    let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
                    interceptor.post_execute(
                        &interceptor_context,
                        &QueryExecutionResult {
                            success: false,
                            error: Some(format!("Operation timed out after {}ms", timeout_value)),
                            execution_time_ms: duration_ms,
                            row_count: None,
                        },
                        false,
                        safety_warning.as_deref(),
                    );

                     // Emit timeout error as stream event
                    let _ = window.emit(&format!("query_stream_error:{}", query_id_str), "Operation timed out");
                    return Ok(QueryResponse {
                        success: false,
                        result: None,
                        error: Some(format!("Operation timed out after {}ms", timeout_value)),
                        query_id: Some(query_id_str),
                    });
                }
            }
        } else {
            execution.await
        };

        let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
        query_manager.finish(query_id).await;

        match result {
            Ok(_) => {
                // Record successful streaming execution
                interceptor.post_execute(
                    &interceptor_context,
                    &QueryExecutionResult {
                        success: true,
                        error: None,
                        execution_time_ms: duration_ms,
                        row_count: None, // Row count tracked via stream events
                    },
                    false,
                    safety_warning.as_deref(),
                );

                Ok(QueryResponse {
                    success: true,
                    result: None, // Results are streamed
                    error: None,
                    query_id: Some(query_id_str),
                })
            }
            Err(e) => {
                // Record failed streaming execution
                interceptor.post_execute(
                    &interceptor_context,
                    &QueryExecutionResult {
                        success: false,
                        error: Some(e.to_string()),
                        execution_time_ms: duration_ms,
                        row_count: None,
                    },
                    false,
                    safety_warning.as_deref(),
                );

                Ok(QueryResponse {
                    success: false,
                    result: None,
                    error: Some(e.to_string()),
                    query_id: Some(query_id_str),
                })
            }
        }

    } else {
        // Normal execution
        let start_time = std::time::Instant::now();
        let execution = async {
            if let Some(statements) = sql_statements {
                let mut last_result = None;
                let mut executed_count = 0usize;
                for (idx, statement) in statements.iter().enumerate() {
                    match driver
                        .execute_in_namespace(session, namespace.clone(), statement, query_id)
                        .await
                    {
                        Ok(result) => {
                            executed_count += 1;
                            last_result = Some(result);
                        }
                        Err(e) => {
                            return Err(crate::engine::error::EngineError::execution_error(
                                format!(
                                    "Statement {} failed after {} succeeded: {}",
                                    idx + 1,
                                    executed_count,
                                    e
                                ),
                            ));
                        }
                    }
                }

                last_result.ok_or_else(|| {
                    crate::engine::error::EngineError::syntax_error("Empty SQL".to_string())
                })
            } else {
                driver
                    .execute_in_namespace(session, namespace.clone(), &query, query_id)
                    .await
            }
        };

        let result = if let Some(timeout_value) = timeout_ms {
            match timeout(Duration::from_millis(timeout_value), execution).await {
                Ok(res) => res,
                Err(_) => {
                    let _ = driver.cancel(session, Some(query_id)).await;
                    query_manager.finish(query_id).await;
                    metrics::record_timeout();

                    // Record timeout in interceptor
                    let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
                    interceptor.post_execute(
                        &interceptor_context,
                        &QueryExecutionResult {
                            success: false,
                            error: Some(format!("Operation timed out after {}ms", timeout_value)),
                            execution_time_ms: duration_ms,
                            row_count: None,
                        },
                        false,
                        safety_warning.as_deref(),
                    );

                    return Ok(QueryResponse {
                        success: false,
                        result: None,
                        error: Some(format!("Operation timed out after {}ms", timeout_value)),
                        query_id: Some(query_id_str),
                    });
                }
            }
        } else {
            execution.await
        };

        let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
        let response = match result {
            Ok(mut result) => {
                result.execution_time_ms = duration_ms;
                metrics::record_query(duration_ms, true);

                // Record successful execution in interceptor
                interceptor.post_execute(
                    &interceptor_context,
                    &QueryExecutionResult {
                        success: true,
                        error: None,
                        execution_time_ms: duration_ms,
                        row_count: result.affected_rows.map(|a| a as i64),
                    },
                    false,
                    safety_warning.as_deref(),
                );

                Ok(QueryResponse {
                    success: true,
                    result: Some(result),
                    error: None,
                    query_id: Some(query_id_str),
                })
            }
            Err(e) => {
                metrics::record_query(duration_ms, false);

                // Record failed execution in interceptor
                interceptor.post_execute(
                    &interceptor_context,
                    &QueryExecutionResult {
                        success: false,
                        error: Some(e.to_string()),
                        execution_time_ms: duration_ms,
                        row_count: None,
                    },
                    false,
                    safety_warning.as_deref(),
                );

                Ok(QueryResponse {
                    success: false,
                    result: None,
                    error: Some(e.to_string()),
                    query_id: Some(query_id_str),
                })
            }
        };

        query_manager.finish(query_id).await;
        response
    }
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
        (Arc::clone(&state.session_manager), Arc::clone(&state.query_manager))
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                query_id: None,
            });
        }
    };
    tracing::Span::current().record("driver", &field::display(driver.driver_id()));

    let query_id = if let Some(raw) = query_id {
        let parsed = Uuid::parse_str(&raw).map_err(|e| format!("Invalid query ID: {}", e))?;
        QueryId(parsed)
    } else {
        match query_manager.last_for_session(session).await {
            Some(qid) => qid,
            None => {
                return Ok(QueryResponse {
                    success: false,
                    result: None,
                    error: Some("No active query found".to_string()),
                    query_id: None,
                });
            }
        }
    };
    let query_id_str = query_id.0.to_string();

    match driver.cancel(session, Some(query_id)).await {
        Ok(()) => {
            metrics::record_cancel();
            Ok(QueryResponse {
                success: true,
                result: None,
                error: None,
                query_id: Some(query_id_str),
            })
        }
        Err(e) => Ok(QueryResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
            query_id: Some(query_id_str),
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
                error: Some(e.to_string()),
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
            error: Some(e.to_string()),
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
                error: Some(e.to_string()),
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
            error: Some(e.to_string()),
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
                error: Some(e.to_string()),
            });
        }
    };

    // Parse routine_type string to enum
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
            error: Some(e.to_string()),
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
                error: Some(e.to_string()),
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
            error: Some(e.to_string()),
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
                error: Some(e.to_string()),
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
            error: Some(e.to_string()),
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

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(TableSchemaResponse {
                success: false,
                schema: None,
                error: Some(e.to_string()),
            });
        }
    };

    match driver.describe_table(session, &namespace, &table).await {
        Ok(mut schema) => {
            // Merge virtual foreign keys if connection_id is provided
            if let Some(ref conn_id) = connection_id {
                let virtual_fks = vr_store.get_foreign_keys_for_table(
                    conn_id,
                    &namespace.database,
                    namespace.schema.as_deref(),
                    &table,
                );
                // Filter out virtual FKs that duplicate real ones
                for vfk in virtual_fks {
                    let is_duplicate = schema.foreign_keys.iter().any(|fk| {
                        fk.column == vfk.column
                            && fk.referenced_table == vfk.referenced_table
                            && fk.referenced_column == vfk.referenced_column
                    });
                    if !is_duplicate {
                        schema.foreign_keys.push(vfk);
                    }
                }
            }

            Ok(TableSchemaResponse {
                success: true,
                schema: Some(schema),
                error: None,
            })
        }
        Err(e) => Ok(TableSchemaResponse {
            success: false,
            schema: None,
            error: Some(e.to_string()),
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
) -> Result<QueryResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                query_id: None,
            });
        }
    };

    match driver.preview_table(session, &namespace, &table, limit).await {
        Ok(result) => Ok(QueryResponse {
            success: true,
            result: Some(result),
            error: None,
            query_id: None,
        }),
        Err(e) => Ok(QueryResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
            query_id: None,
        }),
    }
}

/// Response wrapper for paginated table queries
#[derive(Debug, Serialize)]
pub struct PaginatedQueryResponse {
    pub success: bool,
    pub result: Option<PaginatedQueryResult>,
    pub error: Option<String>,
}

/// Queries table data with pagination, sorting, and filtering support
#[tauri::command]
pub async fn query_table(
    state: State<'_, crate::SharedState>,
    session_id: String,
    namespace: Namespace,
    table: String,
    options: TableQueryOptions,
) -> Result<PaginatedQueryResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(PaginatedQueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
            });
        }
    };

    match driver.query_table(session, &namespace, &table, options).await {
        Ok(result) => Ok(PaginatedQueryResponse {
            success: true,
            result: Some(result),
            error: None,
        }),
        Err(e) => Ok(PaginatedQueryResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
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
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
    };
    let session = parse_session_id(&session_id)?;
    let limit = limit.unwrap_or(3).max(1).min(25);

    if foreign_key.referenced_table.trim().is_empty()
        || foreign_key.referenced_column.trim().is_empty()
        || matches!(value, Value::Null)
    {
        return Ok(QueryResponse {
            success: true,
            result: Some(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: None,
                execution_time_ms: 0.0,
            }),
            error: None,
            query_id: None,
        });
    }

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                query_id: None,
            });
        }
    };

    match driver
        .peek_foreign_key(session, &namespace, &foreign_key, &value, limit)
        .await
    {
        Ok(result) => Ok(QueryResponse {
            success: true,
            result: Some(result),
            error: None,
            query_id: None,
        }),
        Err(e) => Ok(QueryResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
            query_id: None,
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

    let read_only = session_manager
        .is_read_only(session)
        .await
        .unwrap_or(false);

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
             return Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                query_id: None,
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
            success: false,
            result: None,
            error: Some(error_msg),
            query_id: None,
        });
    }

    let safety_warning = if matches!(safety_result.action, SafetyAction::Warn) {
        safety_result.triggered_rule.clone()
    } else {
        None
    };

    let engine_options = options.map(|v| crate::engine::types::Value::Json(v));

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
                success: true,
                result: None,
                error: None,
                query_id: None,
            })
        }
        Err(e) => {
            let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: false,
                    error: Some(e.to_string()),
                    execution_time_ms: duration_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                query_id: None,
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

    let read_only = session_manager
        .is_read_only(session)
        .await
        .unwrap_or(false);

    let driver = match session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                query_id: None,
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
            success: false,
            result: None,
            error: Some(error_msg),
            query_id: None,
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
                success: true,
                result: None,
                error: None,
                query_id: None,
            })
        }
        Err(e) => {
            let duration_ms = start_time.elapsed().as_micros() as f64 / 1000.0;
            interceptor.post_execute(
                &interceptor_context,
                &QueryExecutionResult {
                    success: false,
                    error: Some(e.to_string()),
                    execution_time_ms: duration_ms,
                    row_count: None,
                },
                false,
                safety_warning.as_deref(),
            );
            Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                query_id: None,
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
                error: Some(e.to_string()),
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
            error: Some(e.to_string()),
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
                error: Some(e.to_string()),
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
            error: Some(e.to_string()),
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
                error: Some(e.to_string()),
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
            error: Some(e.to_string()),
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
                error: Some(e.to_string()),
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
            error: Some(e.to_string()),
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
            return Ok(TransactionSupportResponse {
                supported: false,
            });
        }
    };

    Ok(TransactionSupportResponse {
        supported: driver.supports_transactions_for_session(session).await,
    })
}

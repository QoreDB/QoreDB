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
    sql_safety,
    TableSchema,
    traits::StreamEvent,
    types::{CollectionList, CollectionListOptions, ForeignKey, Namespace, QueryId, QueryResult, SessionId, Value},
};
use crate::metrics;
use tauri::Emitter;

const READ_ONLY_BLOCKED: &str = "Operation blocked: read-only mode";
const DANGEROUS_BLOCKED: &str = "Dangerous query blocked: confirmation required";
const DANGEROUS_BLOCKED_POLICY: &str = "Dangerous query blocked by policy";
const SQL_PARSE_BLOCKED: &str = "Operation blocked: SQL parser could not classify the query";
const TRANSACTIONS_NOT_SUPPORTED: &str = "Transactions are not supported by this driver";

fn is_mongo_mutation(query: &str) -> bool {
    matches!(
        mongo_safety::classify(query),
        mongo_safety::MongoQueryClass::Mutation | mongo_safety::MongoQueryClass::Unknown
    )
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
    let (session_manager, query_manager, policy) = {
        let state = state.lock().await;
        (
            Arc::clone(&state.session_manager),
            Arc::clone(&state.query_manager),
            state.policy.clone(),
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

    let is_production = match session_manager.is_production(session).await {
        Ok(value) => value,
        Err(_) => false,
    };

    let acknowledged = acknowledged_dangerous.unwrap_or(false);
    let is_sql_driver = !driver.driver_id().eq_ignore_ascii_case("mongodb");
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
        } else {
            is_mongo_mutation(&query)
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

        query_manager.finish(query_id).await;

        match result {
            Ok(_) => Ok(QueryResponse {
                success: true,
                result: None, // Results are streamed
                error: None,
                query_id: Some(query_id_str),
            }),
            Err(e) => Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                query_id: Some(query_id_str),
            }),
        }

    } else {
        // Normal execution (non-streaming)
        // When executing multiple SQL statements:
        // - All statements are executed sequentially
        // - Only the result of the last statement is returned
        // - If any statement fails, execution stops and an error is returned
        //   indicating which statements succeeded before the failure
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

                Ok(QueryResponse {
                    success: true,
                    result: Some(result),
                    error: None,
                    query_id: Some(query_id_str),
                })
            }
            Err(e) => {
                metrics::record_query(duration_ms, false);
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
) -> Result<TableSchemaResponse, String> {
    let session_manager = {
        let state = state.lock().await;
        Arc::clone(&state.session_manager)
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
        Ok(schema) => Ok(TableSchemaResponse {
            success: true,
            schema: Some(schema),
            error: None,
        }),
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

    let engine_options = options.map(|v| crate::engine::types::Value::Json(v));

    match driver.create_database(session, &name, engine_options).await {
        Ok(()) => Ok(QueryResponse {
            success: true,
            result: None,
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

/// Drops an existing database (or schema)
#[tauri::command]
pub async fn drop_database(
    state: State<'_, crate::SharedState>,
    session_id: String,
    name: String,
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

    match driver.drop_database(session, &name).await {
        Ok(()) => Ok(QueryResponse {
            success: true,
            result: None,
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

    if !driver.capabilities().transactions {
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

    if !driver.capabilities().transactions {
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

    if !driver.capabilities().transactions {
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
        supported: driver.capabilities().transactions,
    })
}

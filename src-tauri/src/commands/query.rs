//! Query Tauri Commands
//!
//! Commands for executing queries and exploring database schema.

use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::engine::{TableSchema, types::{Collection, Namespace, QueryResult, SessionId}};

/// Response wrapper for query results
#[derive(Debug, Serialize)]
pub struct QueryResponse {
    pub success: bool,
    pub result: Option<QueryResult>,
    pub error: Option<String>,
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
    pub collections: Option<Vec<Collection>>,
    pub error: Option<String>,
}

/// Parses a session ID string into SessionId
fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

/// Executes a query on the given session
#[tauri::command]
pub async fn execute_query(
    state: State<'_, crate::SharedState>,
    session_id: String,
    query: String,
) -> Result<QueryResponse, String> {
    let state = state.lock().await;
    let session = parse_session_id(&session_id)?;

    let driver = match state.session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
            });
        }
    };

    match driver.execute(session, &query).await {
        Ok(result) => Ok(QueryResponse {
            success: true,
            result: Some(result),
            error: None,
        }),
        Err(e) => Ok(QueryResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Cancels a running query
#[tauri::command]
pub async fn cancel_query(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<QueryResponse, String> {
    let state = state.lock().await;
    let session = parse_session_id(&session_id)?;

    let driver = match state.session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
            });
        }
    };

    match driver.cancel(session).await {
        Ok(()) => Ok(QueryResponse {
            success: true,
            result: None,
            error: None,
        }),
        Err(e) => Ok(QueryResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Lists all namespaces (databases/schemas) for a session
#[tauri::command]
pub async fn list_namespaces(
    state: State<'_, crate::SharedState>,
    session_id: String,
) -> Result<NamespacesResponse, String> {
    let state = state.lock().await;
    let session = parse_session_id(&session_id)?;

    let driver = match state.session_manager.get_driver(session).await {
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
) -> Result<CollectionsResponse, String> {
    let state = state.lock().await;
    let session = parse_session_id(&session_id)?;

    let driver = match state.session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(CollectionsResponse {
                success: false,
                collections: None,
                error: Some(e.to_string()),
            });
        }
    };

    match driver.list_collections(session, &namespace).await {
        Ok(collections) => Ok(CollectionsResponse {
            success: true,
            collections: Some(collections),
            error: None,
        }),
        Err(e) => Ok(CollectionsResponse {
            success: false,
            collections: None,
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
    let state = state.lock().await;
    let session = parse_session_id(&session_id)?;

    let driver = match state.session_manager.get_driver(session).await {
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
    let state = state.lock().await;
    let session = parse_session_id(&session_id)?;

    let driver = match state.session_manager.get_driver(session).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(QueryResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
            });
        }
    };

    match driver.preview_table(session, &namespace, &table, limit).await {
        Ok(result) => Ok(QueryResponse {
            success: true,
            result: Some(result),
            error: None,
        }),
        Err(e) => Ok(QueryResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
        }),
    }
}

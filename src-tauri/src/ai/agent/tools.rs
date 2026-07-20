// SPDX-License-Identifier: BUSL-1.1

//! Tools advertised to the model and their execution against a scoped
//! session. Results are redacted and capped before being fed back.

use std::collections::HashSet;

use qore_core::{Namespace, QueryResult, SessionId};
use qore_service::agent_tools::{self, AgentToolContext};
use qore_service::interceptor::QuerySource;
use serde_json::{Value, json};

use super::types::AgentTool;
use crate::federation::types::{
    AliasEntry, ConnectionAliasMap, FederationQueryOptions, normalize_alias,
};

/// Rows fed back to the model per query; the UI can show more.
pub const MAX_ROWS_TO_MODEL: usize = 50;
pub const MAX_CELL_CHARS: usize = 200;
pub const TOOL_TIMEOUT_MS: u64 = 60_000;

const PRESEED_TIMEOUT_MS: u64 = 3_000;
const PRESEED_MAX_NAMESPACES: usize = 8;
const PRESEED_TABLES_PER_NAMESPACE: usize = 40;
const PRESEED_MAX_CHARS: usize = 2_000;

/// Compact table listing injected into the system prompt so the model
/// doesn't spend its first iterations on discovery calls. Best-effort:
/// capped in size, sorted for stable cache prefixes, skipped when the
/// connection is slow to answer.
pub async fn schema_overview(ctx: &AgentToolContext, session: SessionId) -> Option<String> {
    tokio::time::timeout(
        std::time::Duration::from_millis(PRESEED_TIMEOUT_MS),
        schema_overview_inner(ctx, session),
    )
    .await
    .ok()
    .flatten()
}

async fn schema_overview_inner(ctx: &AgentToolContext, session: SessionId) -> Option<String> {
    let mut namespaces = agent_tools::list_namespaces(ctx, session).await.ok()?;
    if namespaces.is_empty() {
        return None;
    }
    namespaces.sort_by(|a, b| {
        (a.database.as_str(), a.schema.as_deref()).cmp(&(b.database.as_str(), b.schema.as_deref()))
    });

    let shown = namespaces.len().min(PRESEED_MAX_NAMESPACES);
    let mut out = String::new();
    for namespace in &namespaces[..shown] {
        let Ok(list) = agent_tools::list_tables(ctx, session, namespace, None).await else {
            continue;
        };
        let mut names: Vec<&str> = list.collections.iter().map(|c| c.name.as_str()).collect();
        names.sort_unstable();
        let label = match &namespace.schema {
            Some(schema) => format!("{}.{schema}", namespace.database),
            None => namespace.database.clone(),
        };
        let visible = names.len().min(PRESEED_TABLES_PER_NAMESPACE);
        let mut line = format!("{label}: {}", names[..visible].join(", "));
        let hidden = (list.total_count as usize).saturating_sub(visible);
        if hidden > 0 {
            line.push_str(&format!(" (+{hidden} more)"));
        }
        if out.chars().count() + line.chars().count() > PRESEED_MAX_CHARS {
            out.push_str("(more namespaces omitted)\n");
            break;
        }
        out.push_str(&line);
        out.push('\n');
    }
    if namespaces.len() > shown && !out.ends_with("(more namespaces omitted)\n") {
        out.push_str(&format!(
            "(+{} more namespaces)\n",
            namespaces.len() - shown
        ));
    }
    (!out.is_empty()).then(|| out.trim_end().to_string())
}

pub fn definitions() -> Vec<AgentTool> {
    vec![
        AgentTool {
            name: "list_connections".to_string(),
            description: "List the open database connections (id, alias, driver, environment). Accessing a connection outside the current scope requires user approval.".to_string(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        AgentTool {
            name: "list_namespaces".to_string(),
            description: "List the databases/schemas (namespaces) available on a connection.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection": connection_property()
                },
                "additionalProperties": false
            }),
        },
        AgentTool {
            name: "list_tables".to_string(),
            description: "List tables/collections in a namespace of a connection.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "database": { "type": "string", "description": "Database name" },
                    "schema": { "type": "string", "description": "Schema name (optional, e.g. PostgreSQL schema)" },
                    "search": { "type": "string", "description": "Optional name filter" },
                    "connection": connection_property()
                },
                "required": ["database"],
                "additionalProperties": false
            }),
        },
        AgentTool {
            name: "describe_table".to_string(),
            description: "Describe a table/collection: columns, keys, indexes.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "database": { "type": "string", "description": "Database name" },
                    "schema": { "type": "string", "description": "Schema name (optional)" },
                    "table": { "type": "string", "description": "Table/collection name" },
                    "connection": connection_property()
                },
                "required": ["database", "table"],
                "additionalProperties": false
            }),
        },
        AgentTool {
            name: "run_query".to_string(),
            description: "Execute a read-only query (SELECT) and return the rows. Never use this for writes.".to_string(),
            input_schema: query_schema(),
        },
        AgentTool {
            name: "run_mutation".to_string(),
            description: "Execute a write statement (INSERT/UPDATE/DELETE/DDL) on a single connection. Requires explicit user approval and is refused in production.".to_string(),
            input_schema: query_schema(),
        },
        AgentTool {
            name: "run_federated_query".to_string(),
            description: "Run a read-only SQL query joining data across the connections in scope. Reference tables as alias.database.table (aliases from list_connections). SELECT only; requires user approval.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Federated SELECT with 3-part identifiers (alias.database.table)" }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
    ]
}

fn connection_property() -> Value {
    json!({
        "type": "string",
        "description": "Connection id from list_connections; defaults to the current connection"
    })
}

fn query_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": { "type": "string", "description": "The query to execute, in the connection's dialect" },
            "connection": connection_property()
        },
        "required": ["query"],
        "additionalProperties": false
    })
}

pub struct ToolOutcome {
    /// JSON payload (or error message) fed back to the model.
    pub content: String,
    pub is_error: bool,
}

impl ToolOutcome {
    fn from_result(result: Result<String, String>) -> Self {
        match result {
            Ok(content) => Self {
                content,
                is_error: false,
            },
            Err(message) => Self {
                content: message,
                is_error: true,
            },
        }
    }
}

pub async fn execute(
    ctx: &AgentToolContext,
    session: SessionId,
    connection_id: Option<&str>,
    scope: &HashSet<String>,
    tool_name: &str,
    input: &Value,
    acknowledged: bool,
) -> ToolOutcome {
    let result = match tool_name {
        "list_connections" => list_connections(ctx, scope).await,
        "run_federated_query" => match require_str(input, "query") {
            Ok(query) => run_federated_query(ctx, scope, query).await,
            Err(e) => Err(e),
        },
        "list_namespaces" => agent_tools::list_namespaces(ctx, session)
            .await
            .and_then(|v| serde_json::to_string(&v).map_err(|e| e.to_string())),
        "list_tables" => match parse_namespace(input) {
            Ok(namespace) => {
                let search = input["search"].as_str().map(String::from);
                agent_tools::list_tables(ctx, session, &namespace, search)
                    .await
                    .and_then(|v| serde_json::to_string(&v).map_err(|e| e.to_string()))
            }
            Err(e) => Err(e),
        },
        "describe_table" => match (parse_namespace(input), require_str(input, "table")) {
            (Ok(namespace), Ok(table)) => {
                agent_tools::describe_table(ctx, session, &namespace, table, connection_id)
                    .await
                    .and_then(|schema| {
                        serde_json::to_value(&schema)
                            .map(|v| redact_schema_value(v).to_string())
                            .map_err(|e| e.to_string())
                    })
            }
            (Err(e), _) | (_, Err(e)) => Err(e),
        },
        "run_query" | "run_mutation" => match require_str(input, "query") {
            Ok(query) => agent_tools::run_query(
                ctx,
                session,
                query,
                None,
                acknowledged,
                Some(TOOL_TIMEOUT_MS),
                QuerySource::Ai,
            )
            .await
            .map(|result| format_query_result(&result)),
            Err(e) => Err(e),
        },
        other => Err(format!("Unknown tool: {other}")),
    };

    ToolOutcome::from_result(result)
}

async fn list_connections(
    ctx: &AgentToolContext,
    scope: &HashSet<String>,
) -> Result<String, String> {
    let sessions = ctx.session_manager.list_sessions().await;
    let mut out = Vec::with_capacity(sessions.len());
    for (session_id, display_name) in sessions {
        let id = session_id.0.to_string();
        let driver = match ctx.session_manager.get_driver(session_id).await {
            Ok(driver) => driver.driver_id().to_string(),
            Err(_) => continue,
        };
        let environment = ctx
            .session_manager
            .get_environment(session_id)
            .await
            .unwrap_or_else(|_| "development".to_string());
        out.push(json!({
            "id": id,
            "name": display_name,
            "alias": normalize_alias(&display_name),
            "driver": driver,
            "environment": environment,
            "in_scope": scope.contains(&id),
        }));
    }
    serde_json::to_string(&out).map_err(|e| e.to_string())
}

/// Builds the alias map from the connections already granted to this run;
/// reaching a new connection must go through the scope gate first.
async fn run_federated_query(
    ctx: &AgentToolContext,
    scope: &HashSet<String>,
    query: &str,
) -> Result<String, String> {
    let mut alias_map = ConnectionAliasMap::new();
    for (session_id, display_name) in ctx.session_manager.list_sessions().await {
        if !scope.contains(&session_id.0.to_string()) {
            continue;
        }
        let Ok(driver) = ctx.session_manager.get_driver(session_id).await else {
            continue;
        };
        alias_map.insert(
            normalize_alias(&display_name),
            AliasEntry {
                session_id,
                driver_id: driver.driver_id().to_string(),
                display_name,
            },
        );
    }

    let options = FederationQueryOptions {
        timeout_ms: Some(TOOL_TIMEOUT_MS),
        stream: Some(false),
        query_id: None,
        row_limit_per_source: None,
    };
    let (result, meta) = crate::federation::manager::execute_federation(
        query,
        &alias_map,
        &ctx.session_manager,
        &options,
    )
    .await
    .map_err(|e| e.sanitized_message())?;

    let mut payload = query_result_payload(&result);
    payload["sources"] = Value::Array(
        meta.source_results
            .iter()
            .map(|s| {
                json!({
                    "alias": s.alias,
                    "table": s.table,
                    "row_count": s.row_count,
                    "row_limit_hit": s.row_limit_hit,
                })
            })
            .collect(),
    );
    Ok(payload.to_string())
}

fn parse_namespace(input: &Value) -> Result<Namespace, String> {
    Ok(Namespace {
        database: require_str(input, "database")?.to_string(),
        schema: input["schema"].as_str().map(String::from),
    })
}

fn require_str<'a>(input: &'a Value, key: &str) -> Result<&'a str, String> {
    input[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("Missing required argument `{key}`"))
}

/// Rows sent back to the model: sensitive columns masked, long cells
/// clamped, row count capped.
fn format_query_result(result: &QueryResult) -> String {
    query_result_payload(result).to_string()
}

fn query_result_payload(result: &QueryResult) -> Value {
    let sensitive: Vec<bool> = result
        .columns
        .iter()
        .map(|c| crate::redaction::is_sensitive_column(&c.name))
        .collect();
    let total = result.rows.len();
    let rows: Vec<Vec<Value>> = result
        .rows
        .iter()
        .take(MAX_ROWS_TO_MODEL)
        .map(|row| {
            row.values
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    if sensitive.get(i).copied().unwrap_or(false) {
                        Value::String("<redacted>".to_string())
                    } else {
                        clamp_cell(v.to_json())
                    }
                })
                .collect()
        })
        .collect();

    json!({
        "columns": result.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        "rows": rows,
        "row_count": total,
        "truncated": total > MAX_ROWS_TO_MODEL,
        "affected_rows": result.affected_rows,
    })
}

fn clamp_cell(value: Value) -> Value {
    match value {
        Value::String(s) if s.chars().count() > MAX_CELL_CHARS => {
            let clamped: String = s.chars().take(MAX_CELL_CHARS).collect();
            Value::String(format!("{clamped}…"))
        }
        other => other,
    }
}

/// Same posture as the schema context sent to the text assistant: the model
/// never sees sensitive column names.
fn redact_schema_value(mut schema: Value) -> Value {
    if let Some(columns) = schema["columns"].as_array_mut() {
        for column in columns {
            let is_sensitive = column["name"]
                .as_str()
                .is_some_and(crate::redaction::is_sensitive_column);
            if is_sensitive {
                column["name"] = Value::String("<redacted>".to_string());
            }
        }
    }
    schema
}

#[cfg(test)]
mod tests {
    use qore_core::{ColumnInfo, Row};

    use super::*;

    #[test]
    fn query_result_masks_sensitive_columns_and_caps_rows() {
        let result = QueryResult {
            columns: vec![
                ColumnInfo {
                    name: "email".into(),
                    data_type: "text".into(),
                    nullable: false,
                },
                ColumnInfo {
                    name: "city".into(),
                    data_type: "text".into(),
                    nullable: false,
                },
            ],
            rows: (0..60)
                .map(|i| Row {
                    values: vec![
                        qore_core::Value::Text("a@b.c".to_string()),
                        qore_core::Value::Text(format!("city-{i}")),
                    ],
                })
                .collect(),
            affected_rows: None,
            execution_time_ms: 1.0,
        };
        let payload: Value = serde_json::from_str(&format_query_result(&result)).unwrap();
        assert_eq!(payload["rows"].as_array().unwrap().len(), MAX_ROWS_TO_MODEL);
        assert_eq!(payload["row_count"], 60);
        assert_eq!(payload["truncated"], true);
        assert_eq!(payload["rows"][0][0], "<redacted>");
        assert_eq!(payload["rows"][0][1], "city-0");
    }

    #[test]
    fn schema_redaction_masks_sensitive_column_names() {
        let schema = json!({
            "columns": [
                { "name": "password", "data_type": "text" },
                { "name": "city", "data_type": "text" }
            ]
        });
        let redacted = redact_schema_value(schema);
        assert_eq!(redacted["columns"][0]["name"], "<redacted>");
        assert_eq!(redacted["columns"][1]["name"], "city");
    }
}

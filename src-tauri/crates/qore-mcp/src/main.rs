// SPDX-License-Identifier: Apache-2.0

mod prompts;
mod resources;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, GetPromptRequestParams, GetPromptResult, Implementation,
    InitializeRequestParams, InitializeResult, ListPromptsResult, ListResourceTemplatesResult,
    ListResourcesResult, PaginatedRequestParams, ProtocolVersion, ReadResourceRequestParams,
    ReadResourceResult, ResourceContents, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::transport::stdio;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt, tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use qore_core::{Namespace, SessionId};
use qore_service::ServiceContext;
use qore_service::agent_access::{self, AgentSessions, AgentVault};
use qore_service::agent_tools::{self, AgentToolContext, PREVIEW_MAX_ROWS};
use qore_service::federation::types::normalize_alias;
use qore_service::interceptor::QuerySource;
use qore_service::license::LicenseManager;
use qore_service::license::status::LicenseTier;
use qore_service::paths::{QUERY_TIMEOUT_MS, config_dir};
use qore_service::policy::SafetyPolicy;
use qore_service::vault::backend::KeyringProvider;
use qore_service::workspace::query_library::{self, SavedQuery};

const INSTRUCTIONS: &str = "QoreDB gives read-only access to the database connections the user \
explicitly exposed to AI agents. Every session is forced read-only, the safety policy applies \
(row cap, timeout, rate limit) and each call is written to the audit log.\n\
\n\
Tools:\n\
- list_connections: the exposed connections (id, alias, driver, host, environment). Start here.\n\
- list_namespaces: databases/schemas of a connection.\n\
- list_tables: tables or collections of a namespace, with an optional name filter.\n\
- describe_table: columns, primary key, foreign keys, indexes and row estimate of a table.\n\
- preview_table: a sample of rows (max 100) through the engine's cheapest path.\n\
- search_schema: find tables and columns whose name contains a pattern.\n\
- run_query: a read-only query, optionally scoped to a database/schema.\n\
- explain_query: the execution plan of a read-only query.\n\
- run_federated_query (Pro): a SELECT joining tables of several exposed connections, referenced \
as alias.database.table with the aliases from list_connections.\n\
- list_saved_queries: the queries saved in the workspace query library, with their variables.\n\
- run_saved_query: a saved query run with values for its variables, read-only like run_query.\n\
\n\
Resources: qore://{connection_id} lists namespaces and tables; \
qore://{connection_id}/{database}[/{schema}]/{table} returns a table schema as JSON.\n\
Prompts: audit_table, explain_slow_query, document_schema.\n\
\n\
Writes are never possible from this server; suggest DDL or DML to the user instead of trying.";

#[derive(Clone)]
struct QoreMcp {
    ctx: Arc<ServiceContext>,
    storage_dir: PathBuf,
    workspace: Option<PathBuf>,
    sessions: Arc<Mutex<AgentSessions>>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RunQueryReq {
    #[schemars(description = "ID of the saved connection to query")]
    connection_id: String,
    #[schemars(description = "Read-only SQL/query to execute")]
    query: String,
    #[schemars(
        description = "Database/namespace to run in (optional, defaults to the connection's)"
    )]
    #[serde(default)]
    database: Option<String>,
    #[schemars(description = "Schema name (optional, e.g. PostgreSQL schema)")]
    #[serde(default)]
    schema: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ConnReq {
    #[schemars(description = "ID of the saved connection")]
    connection_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListTablesReq {
    #[schemars(description = "ID of the saved connection")]
    connection_id: String,
    #[schemars(description = "Database/namespace name")]
    database: String,
    #[schemars(description = "Schema name (optional, e.g. PostgreSQL schema)")]
    #[serde(default)]
    schema: Option<String>,
    #[schemars(description = "Optional name filter")]
    #[serde(default)]
    search: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DescribeTableReq {
    #[schemars(description = "ID of the saved connection")]
    connection_id: String,
    #[schemars(description = "Database/namespace name")]
    database: String,
    #[schemars(description = "Schema name (optional)")]
    #[serde(default)]
    schema: Option<String>,
    #[schemars(description = "Table/collection name")]
    table: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PreviewTableReq {
    #[schemars(description = "ID of the saved connection")]
    connection_id: String,
    #[schemars(description = "Database/namespace name")]
    database: String,
    #[schemars(description = "Schema name (optional)")]
    #[serde(default)]
    schema: Option<String>,
    #[schemars(description = "Table/collection name")]
    table: String,
    #[schemars(description = "Number of rows to return (1-100, default 20)")]
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchSchemaReq {
    #[schemars(description = "ID of the saved connection")]
    connection_id: String,
    #[schemars(description = "Database/namespace name")]
    database: String,
    #[schemars(description = "Schema name (optional)")]
    #[serde(default)]
    schema: Option<String>,
    #[schemars(description = "Case-insensitive substring to look for in table and column names")]
    pattern: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FederatedQueryReq {
    #[schemars(description = "IDs of the exposed connections the query joins")]
    connection_ids: Vec<String>,
    #[schemars(description = "SELECT referencing tables as alias.database.table or \
                       alias.database.schema.table, with the aliases from list_connections")]
    query: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RunSavedQueryReq {
    #[schemars(description = "ID of the saved connection to run the query on")]
    connection_id: String,
    #[schemars(description = "ID of the saved query, from list_saved_queries")]
    query_id: String,
    #[schemars(description = "Values for the query variables, by name; defaults apply otherwise")]
    #[serde(default)]
    variables: HashMap<String, String>,
    #[schemars(description = "Database/namespace to run in (optional, defaults to the saved one)")]
    #[serde(default)]
    database: Option<String>,
    #[schemars(description = "Schema name (optional)")]
    #[serde(default)]
    schema: Option<String>,
}

fn text_result(result: Result<String, String>) -> CallToolResult {
    match result {
        Ok(json) => CallToolResult::success(vec![Content::text(json)]),
        Err(msg) => CallToolResult::error(vec![Content::text(msg)]),
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}

fn namespace_of(database: &str, schema: Option<&str>) -> Namespace {
    Namespace {
        database: database.to_string(),
        schema: schema.map(str::to_string),
    }
}

#[tool_router]
impl QoreMcp {
    fn new(storage_dir: PathBuf, workspace: Option<PathBuf>) -> Self {
        Self {
            ctx: Arc::new(ServiceContext::new()),
            storage_dir,
            workspace,
            sessions: Arc::new(Mutex::new(AgentSessions::default())),
            tool_router: Self::tool_router(),
        }
    }

    fn vault(&self) -> AgentVault {
        AgentVault::open(self.storage_dir.clone(), self.workspace.as_deref())
    }

    /// The policy is reloaded from disk on every call, like the vault: a limit
    /// changed in Settings applies to the next agent query, not to the next
    /// server start.
    fn tool_ctx(&self) -> AgentToolContext {
        let mut ctx = AgentToolContext::from_service(&self.ctx);
        ctx.policy = SafetyPolicy::load();
        ctx
    }

    /// The safety policy's duration wins when it is stricter than the
    /// headless default.
    fn query_timeout(&self, ctx: &AgentToolContext) -> u64 {
        ctx.policy
            .max_query_duration_ms
            .map_or(QUERY_TIMEOUT_MS, |ms| ms.min(QUERY_TIMEOUT_MS))
    }

    async fn close_idle_sessions(&self) {
        let idle = self.sessions.lock().await.take_idle();
        for session in idle {
            if let Err(err) = qore_service::connection::disconnect(
                &self.ctx.session_manager,
                &self.ctx.query_rate_limiter,
                session,
            )
            .await
            {
                tracing::warn!("failed to close idle session: {}", err.sanitized());
            }
        }
    }

    async fn ensure_session(&self, connection_id: &str) -> Result<SessionId, String> {
        self.close_idle_sessions().await;
        self.sessions
            .lock()
            .await
            .ensure_session(
                &self.vault(),
                &self.ctx.session_manager,
                &self.ctx.query_rate_limiter,
                connection_id,
            )
            .await
    }

    async fn do_run_query(&self, req: &RunQueryReq) -> Result<String, String> {
        let session = self.ensure_session(&req.connection_id).await?;
        let namespace = req
            .database
            .as_deref()
            .map(|db| namespace_of(db, req.schema.as_deref()));
        let ctx = self.tool_ctx();
        let result = agent_tools::run_query(
            &ctx,
            session,
            &req.query,
            namespace.as_ref(),
            false,
            Some(self.query_timeout(&ctx)),
            QuerySource::Mcp,
        )
        .await?;
        to_json(&result)
    }

    async fn do_explain_query(&self, req: &RunQueryReq) -> Result<String, String> {
        let session = self.ensure_session(&req.connection_id).await?;
        let namespace = req
            .database
            .as_deref()
            .map(|db| namespace_of(db, req.schema.as_deref()));
        let ctx = self.tool_ctx();
        let result = agent_tools::explain_query(
            &ctx,
            session,
            namespace.as_ref(),
            &req.query,
            Some(self.query_timeout(&ctx)),
            QuerySource::Mcp,
        )
        .await?;
        to_json(&result)
    }

    async fn do_list_namespaces(&self, connection_id: &str) -> Result<String, String> {
        let session = self.ensure_session(connection_id).await?;
        let namespaces = agent_tools::list_namespaces(&self.tool_ctx(), session).await?;
        to_json(&namespaces)
    }

    async fn do_list_tables(&self, req: &ListTablesReq) -> Result<String, String> {
        let session = self.ensure_session(&req.connection_id).await?;
        let namespace = namespace_of(&req.database, req.schema.as_deref());
        let list =
            agent_tools::list_tables(&self.tool_ctx(), session, &namespace, req.search.clone())
                .await?;
        to_json(&list)
    }

    async fn do_describe_table(&self, req: &DescribeTableReq) -> Result<String, String> {
        let session = self.ensure_session(&req.connection_id).await?;
        let namespace = namespace_of(&req.database, req.schema.as_deref());
        let schema =
            agent_tools::describe_table(&self.tool_ctx(), session, &namespace, &req.table, None)
                .await?;
        to_json(&schema)
    }

    async fn do_preview_table(&self, req: &PreviewTableReq) -> Result<String, String> {
        let session = self.ensure_session(&req.connection_id).await?;
        let namespace = namespace_of(&req.database, req.schema.as_deref());
        let result = agent_tools::preview_table(
            &self.tool_ctx(),
            session,
            &namespace,
            &req.table,
            req.limit.unwrap_or(20).min(PREVIEW_MAX_ROWS),
            QuerySource::Mcp,
        )
        .await?;
        to_json(&result)
    }

    async fn do_search_schema(&self, req: &SearchSchemaReq) -> Result<String, String> {
        let session = self.ensure_session(&req.connection_id).await?;
        let namespace = namespace_of(&req.database, req.schema.as_deref());
        let matches =
            agent_tools::search_schema(&self.tool_ctx(), session, &namespace, &req.pattern).await?;
        to_json(&matches)
    }

    /// The licence is read from the keyring on every call, like the vault, so
    /// activating Pro in the app applies without restarting the server.
    async fn do_run_federated_query(&self, req: &FederatedQueryReq) -> Result<String, String> {
        let tier = LicenseManager::new(Box::new(KeyringProvider::new()))
            .effective_status()
            .tier;
        if !tier.includes(LicenseTier::Pro) {
            return Err(
                "run_federated_query requires a QoreDB Pro license, activated in the QoreDB app."
                    .to_string(),
            );
        }

        let vault = self.vault();
        let mut aliases = HashSet::new();
        let mut sources = Vec::with_capacity(req.connection_ids.len());
        for connection_id in &req.connection_ids {
            let session = self.ensure_session(connection_id).await?;
            let name = vault.get(connection_id)?.name;
            let alias = normalize_alias(&name);
            if !aliases.insert(alias.clone()) {
                return Err(format!(
                    "Several connections resolve to the alias `{alias}`: pass each connection \
                     once and rename duplicates in QoreDB."
                ));
            }
            sources.push((name, session));
        }

        let ctx = self.tool_ctx();
        let (result, meta) = agent_tools::run_federated_query(
            &ctx,
            &sources,
            &req.query,
            Some(self.query_timeout(&ctx)),
            QuerySource::Mcp,
        )
        .await?;
        to_json(&serde_json::json!({
            "result": result,
            "sources": meta.source_results,
            "warnings": meta.warnings,
        }))
    }

    fn saved_queries(&self) -> Result<Vec<SavedQuery>, String> {
        let vault = self.vault();
        let Some(workspace) = vault.workspace_path() else {
            return Err(
                "The query library is only readable from a .qoredb workspace: start qore-mcp \
                 from the project folder or pass --workspace <dir>."
                    .to_string(),
            );
        };
        query_library::read(workspace).map(|library| query_library::saved_queries(&library))
    }

    async fn do_run_saved_query(&self, req: &RunSavedQueryReq) -> Result<String, String> {
        let saved = self
            .saved_queries()?
            .into_iter()
            .find(|q| q.id == req.query_id)
            .ok_or_else(|| format!("No saved query with id '{}'", req.query_id))?;
        let query =
            query_library::substitute_variables(&saved.query, &saved.variables, &req.variables)?;
        self.do_run_query(&RunQueryReq {
            connection_id: req.connection_id.clone(),
            query,
            database: req.database.clone().or(saved.database),
            schema: req.schema.clone(),
        })
        .await
    }

    #[tool(description = "List the saved connections exposed to AI agents (read-only access)")]
    async fn list_connections(&self) -> Result<CallToolResult, McpError> {
        let summary = self.vault().exposed().map(|connections| {
            connections
                .iter()
                .map(|connection| {
                    let mut summary = agent_access::connection_summary(connection);
                    summary["alias"] = normalize_alias(&connection.name).into();
                    summary
                })
                .collect::<Vec<_>>()
        });
        Ok(text_result(summary.and_then(|s| to_json(&s))))
    }

    #[tool(
        description = "Run a read-only SELECT joining tables across several exposed connections \
                          (Pro license). Reference tables as alias.database.table or \
                          alias.database.schema.table, with the aliases from list_connections."
    )]
    async fn run_federated_query(
        &self,
        Parameters(req): Parameters<FederatedQueryReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.do_run_federated_query(&req).await))
    }

    #[tool(
        description = "List the queries saved in the workspace query library (id, title, query, \
                          variables with their type and default)"
    )]
    async fn list_saved_queries(&self) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.saved_queries().and_then(|q| to_json(&q))))
    }

    #[tool(
        description = "Run a saved query from the workspace library on an exposed connection, \
                          with values for its variables. Read-only, like run_query."
    )]
    async fn run_saved_query(
        &self,
        Parameters(req): Parameters<RunSavedQueryReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.do_run_saved_query(&req).await))
    }

    #[tool(
        description = "Run a read-only query against a saved connection and return the rows. \
                          Pass database/schema to target a namespace explicitly."
    )]
    async fn run_query(
        &self,
        Parameters(req): Parameters<RunQueryReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.do_run_query(&req).await))
    }

    #[tool(
        description = "Return the execution plan of a read-only query (EXPLAIN in the \
                          engine's dialect). Refused on engines without EXPLAIN."
    )]
    async fn explain_query(
        &self,
        Parameters(req): Parameters<RunQueryReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.do_explain_query(&req).await))
    }

    #[tool(description = "List databases/schemas (namespaces) for a saved connection")]
    async fn list_namespaces(
        &self,
        Parameters(req): Parameters<ConnReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(
            self.do_list_namespaces(&req.connection_id).await,
        ))
    }

    #[tool(description = "List tables/collections in a namespace")]
    async fn list_tables(
        &self,
        Parameters(req): Parameters<ListTablesReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.do_list_tables(&req).await))
    }

    #[tool(
        description = "Describe a table: columns, primary key, foreign keys, indexes, row estimate"
    )]
    async fn describe_table(
        &self,
        Parameters(req): Parameters<DescribeTableReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.do_describe_table(&req).await))
    }

    #[tool(
        description = "Return a sample of rows from a table (max 100) using the engine's \
                          cheapest read path; cached and free of charge on BigQuery"
    )]
    async fn preview_table(
        &self,
        Parameters(req): Parameters<PreviewTableReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.do_preview_table(&req).await))
    }

    #[tool(
        description = "Find tables and columns of a namespace whose name contains a pattern. \
                          Returns table, column and type; never searches row data."
    )]
    async fn search_schema(
        &self,
        Parameters(req): Parameters<SearchSchemaReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.do_search_schema(&req).await))
    }
}

impl QoreMcp {
    /// Listing never touches the network: one resource per exposed connection.
    fn connection_resources(&self) -> Result<Vec<rmcp::model::Resource>, String> {
        Ok(self
            .vault()
            .exposed()?
            .iter()
            .map(|c| resources::connection_resource(&c.id, &c.name, &c.driver))
            .collect())
    }

    async fn read_resource_json(&self, uri: &str) -> Result<String, String> {
        match resources::parse_uri(uri)? {
            resources::ResourceRef::Connection(connection_id) => {
                self.connection_overview(&connection_id).await
            }
            resources::ResourceRef::Table(table) => {
                let session = self.ensure_session(&table.connection_id).await?;
                let schema = agent_tools::describe_table(
                    &self.tool_ctx(),
                    session,
                    &table.namespace,
                    &table.table,
                    None,
                )
                .await?;
                to_json(&schema)
            }
        }
    }

    async fn connection_overview(&self, connection_id: &str) -> Result<String, String> {
        let session = self.ensure_session(connection_id).await?;
        let ctx = self.tool_ctx();
        let mut namespaces = Vec::new();
        for namespace in agent_tools::list_namespaces(&ctx, session).await? {
            let tables = match agent_tools::list_tables(&ctx, session, &namespace, None).await {
                Ok(list) => list
                    .collections
                    .iter()
                    .take(resources::MAX_TABLES_PER_NAMESPACE)
                    .map(|t| {
                        serde_json::json!({
                            "name": t.name,
                            "uri": resources::format_uri(connection_id, &namespace, &t.name),
                        })
                    })
                    .collect::<Vec<_>>(),
                Err(err) => {
                    tracing::warn!("skipping namespace {}: {err}", namespace.database);
                    continue;
                }
            };
            namespaces.push(serde_json::json!({
                "database": namespace.database,
                "schema": namespace.schema,
                "tables": tables,
            }));
        }
        to_json(&serde_json::json!({ "connection_id": connection_id, "namespaces": namespaces }))
    }
}

#[tool_handler]
impl ServerHandler for QoreMcp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::V_2025_06_18;
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_prompts()
            .build();
        info.server_info = Implementation::from_build_env();
        info.instructions = Some(format!(
            "{INSTRUCTIONS}\n\nConnection store in use: {}.",
            self.vault().describe()
        ));
        info
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        Ok(self.get_info())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let resources = self
            .connection_resources()
            .map_err(|e| McpError::internal_error(e, None))?;
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult::with_all_items(vec![
            resources::template(),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let text = self
            .read_resource_json(&request.uri)
            .await
            .map_err(|e| McpError::resource_not_found(e, None))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::TextResourceContents {
                uri: request.uri,
                mime_type: Some(resources::MIME_TYPE.to_string()),
                text,
                meta: None,
            },
        ]))
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult::with_all_items(prompts::definitions()))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        prompts::render(&request)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args()
        .skip(1)
        .any(|arg| arg == "--version" || arg == "-V")
    {
        println!("qore-mcp {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if std::env::args()
        .skip(1)
        .any(|arg| arg == "--help" || arg == "-h")
    {
        println!(
            "Usage: qore-mcp [--workspace <dir>]\n\nMCP server (stdio) over the QoreDB connections exposed to AI agents.\n\n  --workspace <dir>  use the .qoredb workspace at <dir> (or its parent) instead of the\n                     one detected from the working directory / the default vault\n  --version          print the version"
        );
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let explicit = std::env::args().skip(1).collect::<Vec<_>>();
    let explicit = explicit
        .iter()
        .position(|arg| arg == "--workspace")
        .and_then(|i| explicit.get(i + 1))
        .map(PathBuf::from);
    let workspace = agent_access::detect_workspace(explicit.as_deref());
    if explicit.is_some() && workspace.is_none() {
        eprintln!("error: no .qoredb/workspace.json found at the given --workspace path");
        std::process::exit(2);
    }
    tracing::info!(
        "starting qore-mcp (stdio), store: {}",
        AgentVault::open(config_dir(), workspace.as_deref()).describe()
    );

    let service = QoreMcp::new(config_dir(), workspace).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

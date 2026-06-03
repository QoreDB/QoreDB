// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeRequestParams, InitializeResult,
    ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::transport::stdio;
use rmcp::{
    tool, tool_handler, tool_router, ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use qore_core::{CollectionListOptions, Namespace, SessionId};
use qore_service::vault::backend::KeyringProvider;
use qore_service::vault::VaultStorage;
use qore_service::ServiceContext;

const PROJECT_ID: &str = "default";
const QUERY_TIMEOUT_MS: u64 = 30_000;

#[derive(Clone)]
struct QoreMcp {
    ctx: Arc<ServiceContext>,
    storage_dir: PathBuf,
    sessions: Arc<Mutex<HashMap<String, SessionId>>>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RunQueryReq {
    #[schemars(description = "ID of the saved connection to query")]
    connection_id: String,
    #[schemars(description = "Read-only SQL/query to execute")]
    query: String,
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

fn text_result(result: Result<String, String>) -> CallToolResult {
    match result {
        Ok(json) => CallToolResult::success(vec![Content::text(json)]),
        Err(msg) => CallToolResult::error(vec![Content::text(msg)]),
    }
}

#[tool_router]
impl QoreMcp {
    fn new(storage_dir: PathBuf) -> Self {
        Self {
            ctx: Arc::new(ServiceContext::new()),
            storage_dir,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            tool_router: Self::tool_router(),
        }
    }

    fn storage(&self) -> VaultStorage {
        VaultStorage::new(
            PROJECT_ID,
            self.storage_dir.clone(),
            Box::new(KeyringProvider::new()),
        )
    }

    async fn ensure_session(&self, connection_id: &str) -> Result<SessionId, String> {
        if let Some(session) = self.sessions.lock().await.get(connection_id) {
            return Ok(*session);
        }

        let storage = self.storage();
        let saved = storage.get_connection(connection_id).map_err(|e| e.to_string())?;
        let creds = storage
            .get_credentials(connection_id)
            .map_err(|e| e.to_string())?;
        let mut config = saved.to_connection_config(&creds).map_err(|e| e.to_string())?;
        config.read_only = true;

        let session = qore_service::connection::connect(&self.ctx.session_manager, config)
            .await
            .map_err(|e| e.sanitized())?;
        self.sessions
            .lock()
            .await
            .insert(connection_id.to_string(), session);
        Ok(session)
    }

    async fn do_run_query(&self, req: &RunQueryReq) -> Result<String, String> {
        let session = self.ensure_session(&req.connection_id).await?;
        let session_id = session.0.to_string();

        let pf = qore_service::query::preflight(
            &self.ctx.session_manager,
            &self.ctx.query_rate_limiter,
            &self.ctx.interceptor,
            &self.ctx.policy,
            session,
            &session_id,
            &req.query,
            None,
            false,
        )
        .await?;

        let query_id = self.ctx.query_manager.register(session).await;
        let outcome = qore_service::query::execute(
            &self.ctx.query_manager,
            &self.ctx.query_cache,
            &self.ctx.interceptor,
            &self.ctx.policy,
            pf.driver,
            &pf.context,
            session,
            None,
            &req.query,
            query_id,
            pf.is_mutation,
            pf.connection_key.as_deref(),
            pf.safety_warning.as_deref(),
            Some(QUERY_TIMEOUT_MS),
            false,
            None,
            None,
            |_, _| {},
        )
        .await;

        if let Some(err) = outcome.error {
            return Err(err);
        }
        serde_json::to_string(&outcome.result).map_err(|e| e.to_string())
    }

    async fn do_list_namespaces(&self, connection_id: &str) -> Result<String, String> {
        let session = self.ensure_session(connection_id).await?;
        let driver = self
            .ctx
            .session_manager
            .get_driver(session)
            .await
            .map_err(|e| e.sanitized_message())?;
        let namespaces = driver
            .list_namespaces(session)
            .await
            .map_err(|e| e.sanitized_message())?;
        serde_json::to_string(&namespaces).map_err(|e| e.to_string())
    }

    async fn do_list_tables(&self, req: &ListTablesReq) -> Result<String, String> {
        let session = self.ensure_session(&req.connection_id).await?;
        let driver = self
            .ctx
            .session_manager
            .get_driver(session)
            .await
            .map_err(|e| e.sanitized_message())?;
        let namespace = Namespace {
            database: req.database.clone(),
            schema: req.schema.clone(),
        };
        let options = CollectionListOptions {
            search: req.search.clone(),
            page: None,
            page_size: None,
        };
        let list = driver
            .list_collections(session, &namespace, options)
            .await
            .map_err(|e| e.sanitized_message())?;
        serde_json::to_string(&list).map_err(|e| e.to_string())
    }

    async fn do_describe_table(&self, req: &DescribeTableReq) -> Result<String, String> {
        let session = self.ensure_session(&req.connection_id).await?;
        let namespace = Namespace {
            database: req.database.clone(),
            schema: req.schema.clone(),
        };
        let schema = qore_service::query::describe_table(
            &self.ctx.session_manager,
            &self.ctx.virtual_relations,
            session,
            &namespace,
            &req.table,
            None,
        )
        .await
        .map_err(|e| e.sanitized())?;
        serde_json::to_string(&schema).map_err(|e| e.to_string())
    }

    #[tool(description = "List the saved database connections available to query")]
    async fn list_connections(&self) -> Result<CallToolResult, McpError> {
        let connections = self
            .storage()
            .list_connections_full()
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let summary: Vec<_> = connections
            .into_iter()
            .map(|c| {
                serde_json::json!({
                    "id": c.id,
                    "name": c.name,
                    "driver": c.driver,
                    "host": c.host,
                    "database": c.database,
                    "environment": c.environment.as_str(),
                    "read_only": c.read_only,
                })
            })
            .collect();

        let text = serde_json::to_string(&summary)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Run a read-only query against a saved connection and return the rows")]
    async fn run_query(
        &self,
        Parameters(req): Parameters<RunQueryReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.do_run_query(&req).await))
    }

    #[tool(description = "List databases/schemas (namespaces) for a saved connection")]
    async fn list_namespaces(
        &self,
        Parameters(req): Parameters<ConnReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.do_list_namespaces(&req.connection_id).await))
    }

    #[tool(description = "List tables/collections in a namespace of a saved connection")]
    async fn list_tables(
        &self,
        Parameters(req): Parameters<ListTablesReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.do_list_tables(&req).await))
    }

    #[tool(description = "Describe a table/collection schema (columns, keys) of a saved connection")]
    async fn describe_table(
        &self,
        Parameters(req): Parameters<DescribeTableReq>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(self.do_describe_table(&req).await))
    }
}

#[tool_handler]
impl ServerHandler for QoreMcp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::V_2025_06_18;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = Implementation::from_build_env();
        info.instructions = Some(
            "QoreDB read-only data access. Tools:\n\
             - list_connections: discover saved connections.\n\
             - run_query: execute a read-only query on a connection."
                .to_string(),
        );
        info
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        Ok(self.get_info())
    }
}

fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("QOREDB_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.rapha.qoredb")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("starting qore-mcp (stdio)");

    let service = QoreMcp::new(config_dir()).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

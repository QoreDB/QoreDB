// SPDX-License-Identifier: Apache-2.0

//! Agent-facing data tools shared by the MCP server and the in-app AI agent.
//! Sessions are opened and scoped by the caller; read-only enforcement comes
//! from the session config and the preflight pipeline, not from this layer.

use std::sync::Arc;

use qore_core::{
    CollectionList, CollectionListOptions, Namespace, QueryResult, SessionId, TableSchema,
};
use qore_drivers::query_manager::QueryManager;
use qore_drivers::session_manager::SessionManager;

use crate::ServiceContext;
use crate::cache::QueryCache;
use crate::interceptor::{InterceptorPipeline, QuerySource};
use crate::policy::SafetyPolicy;
use crate::ratelimit::QueryRateLimiter;
use crate::virtual_relations::VirtualRelationStore;

/// Cheap snapshot of the service pieces the tools need. Callers that hold a
/// `ServiceContext` behind a lock can build one and release the lock; the
/// policy is a point-in-time copy.
#[derive(Clone)]
pub struct AgentToolContext {
    pub session_manager: Arc<SessionManager>,
    pub query_manager: Arc<QueryManager>,
    pub query_rate_limiter: Arc<QueryRateLimiter>,
    pub query_cache: Arc<QueryCache>,
    pub interceptor: Arc<InterceptorPipeline>,
    pub virtual_relations: Arc<VirtualRelationStore>,
    pub policy: SafetyPolicy,
}

impl AgentToolContext {
    pub fn from_service(ctx: &ServiceContext) -> Self {
        Self {
            session_manager: Arc::clone(&ctx.session_manager),
            query_manager: Arc::clone(&ctx.query_manager),
            query_rate_limiter: Arc::clone(&ctx.query_rate_limiter),
            query_cache: Arc::clone(&ctx.query_cache),
            interceptor: Arc::clone(&ctx.interceptor),
            virtual_relations: Arc::clone(&ctx.virtual_relations),
            policy: ctx.policy.clone(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_query(
    ctx: &AgentToolContext,
    session: SessionId,
    query: &str,
    namespace: Option<&Namespace>,
    acknowledged: bool,
    timeout_ms: Option<u64>,
    source: QuerySource,
) -> Result<QueryResult, String> {
    let session_id = session.0.to_string();

    let pf = crate::query::preflight_with_source(
        &ctx.session_manager,
        &ctx.query_rate_limiter,
        &ctx.interceptor,
        &ctx.policy,
        session,
        &session_id,
        query,
        namespace,
        acknowledged,
        source,
    )
    .await?;

    let query_id = ctx.query_manager.register(session).await;
    let outcome = crate::query::execute(
        &ctx.query_manager,
        &ctx.query_cache,
        &ctx.interceptor,
        &ctx.policy,
        pf.driver,
        &pf.context,
        session,
        namespace.cloned(),
        query,
        query_id,
        pf.is_mutation,
        pf.connection_key.as_deref(),
        pf.safety_warning.as_deref(),
        timeout_ms,
        false,
        None,
        None,
        |_, _| {},
    )
    .await;

    if let Some(err) = outcome.error {
        return Err(err);
    }
    outcome
        .result
        .ok_or_else(|| "Query produced no result".to_string())
}

pub async fn list_namespaces(
    ctx: &AgentToolContext,
    session: SessionId,
) -> Result<Vec<Namespace>, String> {
    let driver = ctx
        .session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;
    driver
        .list_namespaces(session)
        .await
        .map_err(|e| e.sanitized_message())
}

pub async fn list_tables(
    ctx: &AgentToolContext,
    session: SessionId,
    namespace: &Namespace,
    search: Option<String>,
) -> Result<CollectionList, String> {
    let driver = ctx
        .session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;
    let options = CollectionListOptions {
        search,
        page: None,
        page_size: None,
    };
    driver
        .list_collections(session, namespace, options)
        .await
        .map_err(|e| e.sanitized_message())
}

pub async fn describe_table(
    ctx: &AgentToolContext,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    connection_id: Option<&str>,
) -> Result<TableSchema, String> {
    crate::query::describe_table(
        &ctx.session_manager,
        &ctx.virtual_relations,
        session,
        namespace,
        table,
        connection_id,
    )
    .await
    .map_err(|e| e.sanitized())
}

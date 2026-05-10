// SPDX-License-Identifier: Apache-2.0

//! ClickHouse driver — implements `DataEngine` over the HTTP protocol.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::DataEngine;
use qore_core::types::{
    CancelSupport, CollectionList, CollectionListOptions, ConnectionConfig, Namespace,
    PaginatedQueryResult, QueryId, QueryResult, SessionId, TableQueryOptions, TableSchema, Value,
};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use super::client::ClickHouseClient;
use super::describe::{describe_table, list_databases, list_tables, ping};
use super::response::parse_query_result;

type SessionMap = Arc<RwLock<HashMap<SessionId, Arc<ClickHouseClient>>>>;

/// Maps QoreDB `QueryId` to the ClickHouse server-side `query_id` so
/// `cancel()` can `KILL QUERY` the right thing.
type QueryRegistry = Arc<Mutex<HashMap<QueryId, (SessionId, Uuid)>>>;

pub struct ClickHouseDriver {
    sessions: SessionMap,
    queries: QueryRegistry,
}

impl ClickHouseDriver {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            queries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn get(&self, session: SessionId) -> EngineResult<Arc<ClickHouseClient>> {
        self.sessions
            .read()
            .await
            .get(&session)
            .cloned()
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))
    }

    async fn track_query(&self, session: SessionId, query_id: QueryId, server_id: Uuid) {
        self.queries
            .lock()
            .await
            .insert(query_id, (session, server_id));
    }

    async fn untrack_query(&self, query_id: &QueryId) -> Option<(SessionId, Uuid)> {
        self.queries.lock().await.remove(query_id)
    }
}

impl Default for ClickHouseDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for ClickHouseDriver {
    fn driver_id(&self) -> &'static str {
        "clickhouse"
    }

    fn driver_name(&self) -> &'static str {
        "ClickHouse"
    }

    // ==================== Connection ====================

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        let client = ClickHouseClient::new(config)?;
        ping(&client).await
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        let client = Arc::new(ClickHouseClient::new(config)?);
        ping(&client).await?;
        let id = SessionId::new();
        self.sessions.write().await.insert(id, client);
        Ok(id)
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        self.sessions.write().await.remove(&session);
        // Drop any tracked queries belonging to this session.
        let mut queries = self.queries.lock().await;
        queries.retain(|_, (sid, _)| *sid != session);
        Ok(())
    }

    async fn ping(&self, session: SessionId) -> EngineResult<()> {
        let client = self.get(session).await?;
        ping(&client).await
    }

    // ==================== Namespaces ====================

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let client = self.get(session).await?;
        list_databases(&client).await
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let client = self.get(session).await?;
        list_tables(&client, namespace, options).await
    }

    // ==================== Schema ====================

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        let client = self.get(session).await?;
        describe_table(&client, namespace, table).await
    }

    // ==================== Execute ====================

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        self.execute_in_namespace(session, None, query, query_id)
            .await
    }

    async fn execute_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let client = self.get(session).await?;
        if let Some(ns) = namespace {
            client.set_current_database(ns.database);
        }

        let server_id = Uuid::new_v4();
        self.track_query(session, query_id, server_id).await;

        let trimmed = query.trim_start();
        let upper = trimmed.to_ascii_uppercase();
        let is_query =
            upper.starts_with("SELECT") || upper.starts_with("WITH") || upper.starts_with("SHOW")
                || upper.starts_with("DESCRIBE") || upper.starts_with("DESC")
                || upper.starts_with("EXPLAIN");

        let started = Instant::now();
        let res = if is_query {
            client.fetch_json(query, Some(&server_id)).await
        } else {
            client.execute(query, Some(&server_id)).await
        };
        let elapsed_ms = started.elapsed().as_micros() as f64 / 1000.0;

        self.untrack_query(&query_id).await;

        match res {
            Ok(body) => {
                if is_query {
                    parse_query_result(&body, elapsed_ms)
                } else {
                    Ok(QueryResult::with_affected_rows(0, elapsed_ms))
                }
            }
            Err(e) => Err(e),
        }
    }

    // ==================== Preview / table query ====================

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let sql = format!(
            "SELECT * FROM {}.{} LIMIT {}",
            quote_ident(&namespace.database),
            quote_ident(table),
            limit
        );
        self.execute(session, &sql, QueryId::new()).await
    }

    async fn query_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        let page = options.effective_page();
        let page_size = options.effective_page_size();
        let offset = page.saturating_sub(1) as u64 * page_size as u64;

        let qualified = format!(
            "{}.{}",
            quote_ident(&namespace.database),
            quote_ident(table)
        );

        // Total count first — uses the engine's optimized counter when
        // available (MergeTree family).
        let total_sql = format!("SELECT count() FROM {qualified}");
        let total = self
            .execute(session, &total_sql, QueryId::new())
            .await?
            .rows
            .into_iter()
            .next()
            .and_then(|r| match r.values.into_iter().next() {
                Some(Value::Int(i)) if i >= 0 => Some(i as u64),
                _ => None,
            })
            .unwrap_or(0);

        let mut sql = format!("SELECT * FROM {qualified}");
        if let Some(col) = options.sort_column.as_ref() {
            let dir = match options.sort_direction {
                Some(qore_core::types::SortDirection::Desc) => "DESC",
                _ => "ASC",
            };
            sql.push_str(&format!(" ORDER BY {} {dir}", quote_ident(col)));
        }
        sql.push_str(&format!(" LIMIT {} OFFSET {}", page_size, offset));

        let result = self.execute(session, &sql, QueryId::new()).await?;
        Ok(PaginatedQueryResult::new(result, total, page, page_size))
    }

    // ==================== Cancel ====================

    async fn cancel(&self, _session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        let qid = match query_id {
            Some(q) => q,
            None => return Ok(()),
        };
        let entry = self.untrack_query(&qid).await;
        if let Some((session, server_id)) = entry {
            if let Ok(client) = self.get(session).await {
                let _ = client.kill_query(&server_id).await;
            }
        }
        Ok(())
    }

    fn cancel_support(&self) -> CancelSupport {
        // ClickHouse exposes `KILL QUERY WHERE query_id = ...` — best-effort
        // because the running query might have already finished.
        CancelSupport::BestEffort
    }

    // ==================== Schema ops ====================

    async fn create_database(
        &self,
        session: SessionId,
        name: &str,
        _options: Option<Value>,
    ) -> EngineResult<()> {
        if !is_safe_ident(name) {
            return Err(EngineError::validation(format!(
                "Invalid database name: {name}"
            )));
        }
        let sql = format!("CREATE DATABASE IF NOT EXISTS {}", quote_ident(name));
        self.execute(session, &sql, QueryId::new()).await?;
        Ok(())
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        if !is_safe_ident(name) {
            return Err(EngineError::validation(format!(
                "Invalid database name: {name}"
            )));
        }
        let sql = format!("DROP DATABASE IF EXISTS {} SYNC", quote_ident(name));
        self.execute(session, &sql, QueryId::new()).await?;
        Ok(())
    }

    fn supports_transactions(&self) -> bool {
        // ClickHouse supports limited transactions only on a few engines;
        // not exposed in V1.
        false
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn supports_explain(&self) -> bool {
        true
    }
}

/// ClickHouse identifier quoting uses backticks. We only escape backticks to
/// avoid double-encoding upstream-quoted identifiers.
fn quote_ident(raw: &str) -> String {
    let escaped = raw.replace('`', "``");
    format!("`{escaped}`")
}

/// Conservative validator — DDL identifiers (database/table) come straight
/// from the user. We enforce `[A-Za-z_][A-Za-z0-9_]*` for safety so the
/// generated SQL can never be subverted by injection through the name.
fn is_safe_ident(raw: &str) -> bool {
    let mut chars = raw.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_ident_escapes_backticks() {
        assert_eq!(quote_ident("orders"), "`orders`");
        assert_eq!(quote_ident("a`b"), "`a``b`");
    }

    #[test]
    fn safe_ident_accepts_letters_underscore_digits() {
        assert!(is_safe_ident("foo"));
        assert!(is_safe_ident("foo_bar"));
        assert!(is_safe_ident("_foo"));
        assert!(is_safe_ident("foo123"));
    }

    #[test]
    fn safe_ident_rejects_special_chars() {
        assert!(!is_safe_ident(""));
        assert!(!is_safe_ident("foo bar"));
        assert!(!is_safe_ident("foo;DROP"));
        assert!(!is_safe_ident("a`b"));
        assert!(!is_safe_ident("123foo"));
        assert!(!is_safe_ident("foo--bar"));
    }
}

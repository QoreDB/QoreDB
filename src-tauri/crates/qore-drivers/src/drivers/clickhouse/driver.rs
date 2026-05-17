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
    PaginatedQueryResult, QueryId, QueryResult, RowData, SessionId, TableQueryOptions,
    TableSchema, Value,
};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use super::client::ClickHouseClient;
use super::describe::{describe_table, list_databases, list_tables, ping};
use super::literal::format_literal;
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
        let mut queries = self.queries.lock().await;
        queries.retain(|_, (sid, _)| *sid != session);
        Ok(())
    }

    async fn ping(&self, session: SessionId) -> EngineResult<()> {
        let client = self.get(session).await?;
        ping(&client).await
    }

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

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        let client = self.get(session).await?;
        describe_table(&client, namespace, table).await
    }

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

        let is_query = is_result_query(query);

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

        // MergeTree-family engines answer `count()` from a metadata counter, so do the count first.
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
        // `KILL QUERY` is best-effort: the running query may already have finished.
        CancelSupport::BestEffort
    }

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
        let client = self.get(session).await?;
        let on_cluster = format_on_cluster(client.cluster())?;
        let sql = format!(
            "CREATE DATABASE IF NOT EXISTS {}{on_cluster}",
            quote_ident(name),
        );
        self.execute(session, &sql, QueryId::new()).await?;
        Ok(())
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        if !is_safe_ident(name) {
            return Err(EngineError::validation(format!(
                "Invalid database name: {name}"
            )));
        }
        let client = self.get(session).await?;
        let on_cluster = format_on_cluster(client.cluster())?;
        let sql = format!(
            "DROP DATABASE IF EXISTS {}{on_cluster} SYNC",
            quote_ident(name),
        );
        self.execute(session, &sql, QueryId::new()).await?;
        Ok(())
    }

    fn supports_transactions(&self) -> bool {
        // ClickHouse only offers limited transaction support on a few engines and we don't expose it in V1.
        false
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn supports_explain(&self) -> bool {
        true
    }

    fn supports_mutations(&self) -> bool {
        true
    }

    async fn insert_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let client = self.get(session).await?;
        let qualified = qualified_table(namespace, table);

        let mut keys: Vec<&String> = data.columns.keys().collect();
        keys.sort();

        let sql = if keys.is_empty() {
            // ClickHouse has no "DEFAULT VALUES" — emit an empty column list.
            format!("INSERT INTO {qualified} VALUES ()")
        } else {
            let cols: Vec<String> = keys.iter().map(|k| quote_ident(k)).collect();
            let vals: Vec<String> = keys
                .iter()
                .map(|k| format_literal(data.columns.get(*k).unwrap()))
                .collect();
            format!(
                "INSERT INTO {qualified} ({}) VALUES ({})",
                cols.join(", "),
                vals.join(", ")
            )
        };

        let server_id = Uuid::new_v4();
        let started = Instant::now();
        client.execute(&sql, Some(&server_id)).await?;
        let elapsed_ms = started.elapsed().as_micros() as f64 / 1000.0;
        Ok(QueryResult::with_affected_rows(1, elapsed_ms))
    }

    async fn update_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        if primary_key.columns.is_empty() {
            return Err(EngineError::validation(
                "Primary key required for update operations",
            ));
        }
        if data.columns.is_empty() {
            return Ok(QueryResult::with_affected_rows(0, 0.0));
        }

        let client = self.get(session).await?;
        let qualified = qualified_table(namespace, table);

        let mut data_keys: Vec<&String> = data.columns.keys().collect();
        data_keys.sort();
        let mut pk_keys: Vec<&String> = primary_key.columns.keys().collect();
        pk_keys.sort();

        let set: Vec<String> = data_keys
            .iter()
            .map(|k| {
                format!(
                    "{} = {}",
                    quote_ident(k),
                    format_literal(data.columns.get(*k).unwrap())
                )
            })
            .collect();

        let where_clause = build_pk_predicate(&pk_keys, primary_key)?;

        let sql = format!(
            "ALTER TABLE {qualified} UPDATE {} WHERE {}",
            set.join(", "),
            where_clause
        );

        let server_id = Uuid::new_v4();
        let started = Instant::now();
        // mutations_sync=2 waits for the mutation to finish on all replicas; otherwise the
        // UI refetch would race the still-running rewrite.
        client
            .execute_with_settings(&sql, Some(&server_id), &[("mutations_sync", "2")])
            .await?;
        let elapsed_ms = started.elapsed().as_micros() as f64 / 1000.0;
        Ok(QueryResult::with_affected_rows(1, elapsed_ms))
    }

    async fn delete_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
    ) -> EngineResult<QueryResult> {
        if primary_key.columns.is_empty() {
            return Err(EngineError::validation(
                "Primary key required for delete operations",
            ));
        }

        let client = self.get(session).await?;
        let qualified = qualified_table(namespace, table);

        let mut pk_keys: Vec<&String> = primary_key.columns.keys().collect();
        pk_keys.sort();
        let where_clause = build_pk_predicate(&pk_keys, primary_key)?;

        // Lightweight DELETE (GA since 23.3) is synchronous and much cheaper than
        // `ALTER TABLE … DELETE`. Older servers surface a clear error message.
        let sql = format!("DELETE FROM {qualified} WHERE {where_clause}");

        let server_id = Uuid::new_v4();
        let started = Instant::now();
        client.execute(&sql, Some(&server_id)).await?;
        let elapsed_ms = started.elapsed().as_micros() as f64 / 1000.0;
        Ok(QueryResult::with_affected_rows(1, elapsed_ms))
    }
}

/// ClickHouse identifier quoting uses backticks. We only escape backticks to
/// avoid double-encoding upstream-quoted identifiers.
fn quote_ident(raw: &str) -> String {
    let escaped = raw.replace('`', "``");
    format!("`{escaped}`")
}

fn qualified_table(namespace: &Namespace, table: &str) -> String {
    format!("{}.{}", quote_ident(&namespace.database), quote_ident(table))
}

fn build_pk_predicate(pk_keys: &[&String], primary_key: &RowData) -> EngineResult<String> {
    let parts: Vec<String> = pk_keys
        .iter()
        .map(|k| {
            let v = primary_key.columns.get(*k).unwrap();
            // ClickHouse rejects `col = NULL` — use IS NULL for a tombstoned PK.
            if matches!(v, Value::Null) {
                format!("{} IS NULL", quote_ident(k))
            } else {
                format!("{} = {}", quote_ident(k), format_literal(v))
            }
        })
        .collect();
    if parts.is_empty() {
        return Err(EngineError::validation("Empty primary key predicate"));
    }
    Ok(parts.join(" AND "))
}

/// Whether a statement returns a result set (vs. a side-effecting mutation).
/// Keep in lock-step with `clickhouse_safety::ClickHouseQueryClass::Read`.
fn is_result_query(query: &str) -> bool {
    use crate::clickhouse_safety::{classify, ClickHouseQueryClass};
    matches!(classify(query), ClickHouseQueryClass::Read)
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

/// Returns the `" ON CLUSTER <ident>"` fragment when `cluster` is set, or an
/// empty string otherwise. The cluster name is validated the same way as any
/// other DDL identifier — distributed DDL is meaningless without a safe name.
fn format_on_cluster(cluster: Option<&str>) -> EngineResult<String> {
    match cluster {
        Some(name) => {
            if !is_safe_ident(name) {
                return Err(EngineError::validation(format!(
                    "Invalid ClickHouse cluster name: {name}"
                )));
            }
            Ok(format!(" ON CLUSTER {}", quote_ident(name)))
        }
        None => Ok(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_cluster_empty_when_none() {
        assert_eq!(format_on_cluster(None).unwrap(), "");
    }

    #[test]
    fn on_cluster_renders_quoted_name() {
        assert_eq!(
            format_on_cluster(Some("shard_1")).unwrap(),
            " ON CLUSTER `shard_1`"
        );
    }

    #[test]
    fn on_cluster_rejects_unsafe_name() {
        assert!(format_on_cluster(Some("evil; DROP")).is_err());
        assert!(format_on_cluster(Some("with space")).is_err());
        assert!(format_on_cluster(Some("0digit-first")).is_err());
    }

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

    #[test]
    fn qualified_table_quotes_both_sides() {
        let ns = Namespace {
            database: "metrics".into(),
            schema: None,
        };
        assert_eq!(qualified_table(&ns, "events"), "`metrics`.`events`");
    }

    #[test]
    fn is_result_query_classification() {
        assert!(is_result_query("SELECT 1"));
        assert!(is_result_query("  -- c\nSELECT 1"));
        assert!(is_result_query("EXPLAIN SELECT 1"));
        assert!(!is_result_query("INSERT INTO t VALUES (1)"));
        assert!(!is_result_query("ALTER TABLE t UPDATE x = 1 WHERE id = 1"));
        assert!(!is_result_query("DELETE FROM t WHERE id = 1"));
    }

    #[test]
    fn build_pk_predicate_emits_is_null_for_null() {
        let mut pk = RowData::new();
        pk.columns.insert("id".to_string(), Value::Int(7));
        pk.columns.insert("region".to_string(), Value::Null);
        let keys: Vec<&String> = {
            let mut k: Vec<&String> = pk.columns.keys().collect();
            k.sort();
            k
        };
        let out = build_pk_predicate(&keys, &pk).unwrap();
        assert_eq!(out, "`id` = 7 AND `region` IS NULL");
    }

    #[test]
    fn build_pk_predicate_rejects_empty() {
        let pk = RowData::new();
        let empty: Vec<&String> = Vec::new();
        assert!(build_pk_predicate(&empty, &pk).is_err());
    }
}

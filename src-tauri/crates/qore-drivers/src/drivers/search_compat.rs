// SPDX-License-Identifier: Apache-2.0

//! Shared engine for Elasticsearch / OpenSearch.
//!
//! Both products speak the same HTTP/REST API (search, aggregations, index,
//! bulk, `_cat`, `_cluster`…). We expose a single shared module parameterised
//! by [`SearchFlavor`]; the per-product drivers (`elasticsearch.rs`,
//! `opensearch.rs`) are thin wrappers that pick a flavor and a `driver_id`.
//!
//! Transport is `reqwest` (no SQLx — there is no SQL wire protocol). The query
//! interface is the "Dev Tools" console format: a first line `METHOD /path`
//! followed by an optional JSON (or NDJSON for `_bulk`) body.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine};
use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::{StreamEvent, StreamSender};
use qore_core::types::{
    Collection, CollectionList, CollectionListOptions, CollectionType, ColumnInfo,
    ConnectionConfig, Namespace, PaginatedQueryResult, QueryId, QueryResult, Row, RowData,
    SessionId, SortDirection, TableColumn, TableQueryOptions, TableSchema, Value,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client as HttpClient, Method, Url};
use serde_json::{json, Map as JsonMap, Value as Json};
use tokio::sync::RwLock;

/// Document meta fields produced by the search hit mapping. They are never sent
/// back as part of a document body on mutation.
const META_FIELDS: [&str; 4] = ["_id", "_index", "_score", "_source"];

/// Which product a session talks to. ~95 % of the behaviour is identical; the
/// enum only gates the few divergences (flavor verification, SQL endpoint path
/// in phase 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFlavor {
    Elasticsearch,
    OpenSearch,
}

impl SearchFlavor {
    pub fn driver_id(self) -> &'static str {
        match self {
            SearchFlavor::Elasticsearch => "elasticsearch",
            SearchFlavor::OpenSearch => "opensearch",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            SearchFlavor::Elasticsearch => "Elasticsearch",
            SearchFlavor::OpenSearch => "OpenSearch",
        }
    }

    /// REST endpoint backing the SQL mode. Elasticsearch exposes `_sql`;
    /// OpenSearch ships it behind the SQL plugin at `_plugins/_sql`.
    fn sql_endpoint(self) -> &'static str {
        match self {
            SearchFlavor::Elasticsearch => "/_sql?format=json",
            SearchFlavor::OpenSearch => "/_plugins/_sql",
        }
    }
}

/// A live connection to a search cluster. Owns one `reqwest::Client` so TLS and
/// keep-alive are reused across requests.
pub struct SearchSession {
    http: HttpClient,
    base_url: Url,
    flavor: SearchFlavor,
    /// Cluster name discovered at connect time; used as the single logical
    /// namespace (`database`).
    cluster_name: String,
}

pub type SessionMap = Arc<RwLock<HashMap<SessionId, Arc<SearchSession>>>>;

pub fn new_session_map() -> SessionMap {
    Arc::new(RwLock::new(HashMap::new()))
}

impl SearchSession {
    /// Builds the HTTP client and base URL from a connection config without
    /// touching the network.
    pub fn new(config: &ConnectionConfig, flavor: SearchFlavor) -> EngineResult<Self> {
        let base_url = build_base_url(config)?;
        let is_https = base_url.scheme() == "https";
        let mode = auth_mode(config);

        // Refuse to leak credentials over cleartext HTTP: the Authorization
        // header (Basic/ApiKey/Bearer) is trivially sniffable. Mirrors the
        // ClickHouse driver's stance.
        if !is_https && mode != "none" && !config.password.is_empty() {
            return Err(EngineError::connection_failed(
                "Search: refusing to send credentials over cleartext HTTP. \
                 Enable TLS (ssl=true) or remove the credentials.",
            ));
        }

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(mut auth) = build_auth_header(config)? {
            auth.set_sensitive(true);
            headers.insert(AUTHORIZATION, auth);
        }

        let timeout = Duration::from_secs(config.pool_acquire_timeout_secs.unwrap_or(30) as u64);
        let mut builder = HttpClient::builder()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(10))
            .timeout(timeout.max(Duration::from_secs(60)))
            .pool_idle_timeout(Duration::from_secs(90));

        // `ssl_mode = "insecure"` disables certificate verification (dev only;
        // the UI surfaces a warning). Anything else keeps strict verification.
        if is_https && matches!(config.ssl_mode.as_deref(), Some("insecure")) {
            builder = builder.danger_accept_invalid_certs(true);
        }

        // Custom CA certificate (PEM) for clusters signed by an internal CA.
        if is_https {
            if let Some(path) = config
                .ssl_ca_cert
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
            {
                let pem = std::fs::read(path).map_err(|e| {
                    EngineError::connection_failed(format!(
                        "Cannot read CA certificate '{path}': {e}"
                    ))
                })?;
                let cert = reqwest::Certificate::from_pem(&pem).map_err(|e| {
                    EngineError::connection_failed(format!("Invalid CA certificate: {e}"))
                })?;
                builder = builder.add_root_certificate(cert);
            }
        }

        let http = builder.build().map_err(|e| {
            EngineError::connection_failed(format!("HTTP client build failed: {e}"))
        })?;

        Ok(Self {
            http,
            base_url,
            flavor,
            cluster_name: "cluster".to_string(),
        })
    }

    pub fn flavor(&self) -> SearchFlavor {
        self.flavor
    }

    pub fn cluster_name(&self) -> &str {
        &self.cluster_name
    }

    fn join(&self, path: &str) -> EngineResult<Url> {
        let p = path.trim_start_matches('/');
        self.base_url
            .join(p)
            .map_err(|e| EngineError::execution_error(format!("Invalid path '{path}': {e}")))
    }

    /// Issues a single REST request and returns the parsed JSON body. Non-2xx
    /// responses are turned into engine errors carrying the server reason.
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<String>,
    ) -> EngineResult<Json> {
        self.send(method, path, body, None).await
    }

    /// Like [`request`], but tags the request with an `X-Opaque-Id` header so a
    /// later `cancel` can locate the running task via `_tasks`.
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<String>,
        opaque_id: Option<&str>,
    ) -> EngineResult<Json> {
        let url = self.join(path)?;
        let mut req = self.http.request(method, url);

        if let Some(id) = opaque_id {
            if let Ok(value) = HeaderValue::from_str(id) {
                req = req.header(HeaderName::from_static("x-opaque-id"), value);
            }
        }

        if let Some(b) = body {
            // `_bulk` requires NDJSON content type and a trailing newline.
            if path.contains("_bulk") {
                req = req.header(CONTENT_TYPE, "application/x-ndjson");
            }
            req = req.body(b);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| EngineError::execution_error(format!("Search request failed: {e}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| EngineError::execution_error(format!("Search read body: {e}")))?;

        let json = if text.trim().is_empty() {
            Json::Null
        } else {
            serde_json::from_str(&text).unwrap_or_else(|_| Json::String(text.clone()))
        };

        if status.is_success() {
            Ok(json)
        } else {
            Err(EngineError::execution_error(format_search_error(
                status.as_u16(),
                &json,
                &text,
            )))
        }
    }
}

// ==================== Connection lifecycle ====================

pub async fn test_connection(config: &ConnectionConfig, flavor: SearchFlavor) -> EngineResult<()> {
    let session = SearchSession::new(config, flavor)?;
    let root = session.request(Method::GET, "/", None).await?;
    verify_flavor(&root, flavor)
}

pub async fn connect(
    map: &SessionMap,
    config: &ConnectionConfig,
    flavor: SearchFlavor,
) -> EngineResult<SessionId> {
    let mut session = SearchSession::new(config, flavor)?;
    let root = session.request(Method::GET, "/", None).await?;
    verify_flavor(&root, flavor)?;
    session.cluster_name = root
        .get("cluster_name")
        .and_then(|v| v.as_str())
        .unwrap_or("cluster")
        .to_string();
    // Ping cluster health to confirm the node is actually serving requests.
    session
        .request(Method::GET, "/_cluster/health", None)
        .await?;

    let id = SessionId::new();
    map.write().await.insert(id, Arc::new(session));
    Ok(id)
}

pub async fn disconnect(map: &SessionMap, session: SessionId) -> EngineResult<()> {
    map.write().await.remove(&session);
    Ok(())
}

pub async fn ping(map: &SessionMap, session: SessionId) -> EngineResult<()> {
    let s = get(map, session).await?;
    s.request(Method::GET, "/_cluster/health", None).await?;
    Ok(())
}

async fn get(map: &SessionMap, session: SessionId) -> EngineResult<Arc<SearchSession>> {
    map.read()
        .await
        .get(&session)
        .cloned()
        .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))
}

// ==================== Schema / catalog ====================

pub async fn list_namespaces(map: &SessionMap, session: SessionId) -> EngineResult<Vec<Namespace>> {
    let s = get(map, session).await?;
    Ok(vec![Namespace::new(s.cluster_name().to_string())])
}

pub async fn list_collections(
    map: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
    options: CollectionListOptions,
) -> EngineResult<CollectionList> {
    let s = get(map, session).await?;
    let search = options.search.as_deref().map(str::to_ascii_lowercase);

    let mut collections: Vec<Collection> = Vec::new();

    // Indices.
    let indices = s
        .request(Method::GET, "/_cat/indices?format=json&h=index", None)
        .await?;
    if let Some(arr) = indices.as_array() {
        for obj in arr {
            if let Some(name) = obj.get("index").and_then(|v| v.as_str()) {
                if name.starts_with('.') {
                    continue; // system index, hidden by default
                }
                if !matches_search(name, &search) {
                    continue;
                }
                collections.push(Collection {
                    namespace: namespace.clone(),
                    name: name.to_string(),
                    collection_type: CollectionType::Table,
                });
            }
        }
    }

    // Aliases (treated as views).
    let aliases = s
        .request(Method::GET, "/_cat/aliases?format=json&h=alias", None)
        .await?;
    if let Some(arr) = aliases.as_array() {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for obj in arr {
            if let Some(name) = obj.get("alias").and_then(|v| v.as_str()) {
                if name.starts_with('.') || !seen.insert(name.to_string()) {
                    continue;
                }
                if !matches_search(name, &search) {
                    continue;
                }
                collections.push(Collection {
                    namespace: namespace.clone(),
                    name: name.to_string(),
                    collection_type: CollectionType::View,
                });
            }
        }
    }

    // Data streams (ES & OpenSearch). They are queryable like indices, so we
    // expose them as tables. The `_data_stream` endpoint may be unavailable on
    // older clusters — ignore failures rather than aborting the listing.
    if let Ok(ds) = s.request(Method::GET, "/_data_stream", None).await {
        if let Some(arr) = ds.get("data_streams").and_then(|v| v.as_array()) {
            for obj in arr {
                if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                    if name.starts_with('.') || !matches_search(name, &search) {
                        continue;
                    }
                    collections.push(Collection {
                        namespace: namespace.clone(),
                        name: name.to_string(),
                        collection_type: CollectionType::Table,
                    });
                }
            }
        }
    }

    collections.sort_by(|a, b| a.name.cmp(&b.name));
    collections.dedup_by(|a, b| a.name == b.name);
    let total = collections.len() as u32;
    Ok(CollectionList {
        collections,
        total_count: total,
    })
}

pub async fn describe_table(
    map: &SessionMap,
    session: SessionId,
    index: &str,
) -> EngineResult<TableSchema> {
    let s = get(map, session).await?;
    let mapping = s
        .request(Method::GET, &format!("/{index}/_mapping"), None)
        .await?;

    // `_mapping` is keyed by the concrete index name (which may differ from the
    // alias passed in), so take whichever top-level object came back.
    let props = mapping
        .as_object()
        .and_then(|m| m.values().next())
        .and_then(|idx| idx.get("mappings"))
        .and_then(|m| m.get("properties"))
        .and_then(|p| p.as_object());

    let mut columns: Vec<TableColumn> = vec![TableColumn {
        name: "_id".to_string(),
        data_type: "_id".to_string(),
        nullable: false,
        default_value: None,
        is_primary_key: true,
        is_auto_increment: true,
    }];

    if let Some(props) = props {
        flatten_properties(props, "", &mut columns);
    }

    let count = s
        .request(Method::GET, &format!("/{index}/_count"), None)
        .await
        .ok()
        .and_then(|j| j.get("count").and_then(|c| c.as_u64()));

    Ok(TableSchema {
        columns,
        primary_key: Some(vec!["_id".to_string()]),
        foreign_keys: Vec::new(),
        row_count_estimate: count,
        indexes: Vec::new(),
    })
}

/// Recursively flattens an ES mapping `properties` object into flat columns.
/// `object`/`nested` types are recursed with a dotted prefix; multi-fields
/// (`fields`) are emitted as `parent.sub`.
fn flatten_properties(props: &JsonMap<String, Json>, prefix: &str, out: &mut Vec<TableColumn>) {
    for (name, spec) in props {
        let full = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };

        let field_type = spec.get("type").and_then(|t| t.as_str());

        if let Some(sub) = spec.get("properties").and_then(|p| p.as_object()) {
            // object / nested container — emit the container then recurse.
            out.push(make_column(&full, field_type.unwrap_or("object")));
            flatten_properties(sub, &full, out);
        } else {
            out.push(make_column(&full, field_type.unwrap_or("object")));
        }

        // Multi-fields: e.g. `title.keyword`.
        if let Some(fields) = spec.get("fields").and_then(|f| f.as_object()) {
            for (sub_name, sub_spec) in fields {
                let sub_type = sub_spec
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("keyword");
                out.push(make_column(&format!("{full}.{sub_name}"), sub_type));
            }
        }
    }
}

fn make_column(name: &str, data_type: &str) -> TableColumn {
    TableColumn {
        name: name.to_string(),
        data_type: data_type.to_string(),
        nullable: true,
        default_value: None,
        is_primary_key: false,
        is_auto_increment: false,
    }
}

// ==================== Query execution ====================

pub async fn execute(
    map: &SessionMap,
    session: SessionId,
    query: &str,
    query_id: QueryId,
) -> EngineResult<QueryResult> {
    let s = get(map, session).await?;
    let opaque = query_id.0.to_string();
    let started = Instant::now();

    // SQL mode: a query that doesn't start with an HTTP method is treated as a
    // SQL statement and sent to the `_sql` endpoint (ES) / SQL plugin (OS).
    if let Some(sql) = sql_query(query) {
        let body = json!({ "query": sql }).to_string();
        let json = s
            .send(
                Method::POST,
                s.flavor.sql_endpoint(),
                Some(body),
                Some(&opaque),
            )
            .await?;
        let elapsed_ms = started.elapsed().as_micros() as f64 / 1000.0;
        return Ok(map_sql_response(&json, elapsed_ms));
    }

    let (method, path, body) = parse_console(query)?;
    let json = s.send(method, &path, body, Some(&opaque)).await?;
    let elapsed_ms = started.elapsed().as_micros() as f64 / 1000.0;
    Ok(map_response(&json, elapsed_ms))
}

/// Returns the trimmed query if it should run as SQL rather than a console
/// command. Console commands always begin with an HTTP method keyword; anything
/// else (`SELECT`, `SHOW`, `DESCRIBE`…) is SQL.
fn sql_query(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let first = trimmed
        .split(|c: char| c.is_whitespace())
        .next()
        .unwrap_or("");
    let is_http = matches!(
        first.to_ascii_uppercase().as_str(),
        "GET" | "POST" | "PUT" | "DELETE" | "HEAD"
    );
    (!is_http).then_some(trimmed)
}

/// Splits a multi-request console buffer into individual console commands. A new
/// command begins at each line starting with an HTTP method keyword, so JSON
/// bodies containing blank lines are never split mid-request.
pub fn split_requests(input: &str) -> Vec<String> {
    let is_method_line = |line: &str| {
        let mut parts = line.trim_start().split_whitespace();
        matches!(
            parts.next().unwrap_or("").to_ascii_uppercase().as_str(),
            "GET" | "POST" | "PUT" | "DELETE" | "HEAD"
        )
    };

    let mut blocks: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in input.lines() {
        if is_method_line(line) && !current.trim().is_empty() {
            blocks.push(current.trim_end().to_string());
            current = String::new();
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        blocks.push(current.trim_end().to_string());
    }
    blocks
}

// ==================== Cancellation (best effort via _tasks) ====================

pub async fn cancel(
    map: &SessionMap,
    session: SessionId,
    query_id: Option<QueryId>,
) -> EngineResult<()> {
    let Some(qid) = query_id else {
        return Ok(());
    };
    let s = get(map, session).await?;
    let opaque = qid.0.to_string();

    // Locate running search / SQL tasks tagged with our X-Opaque-Id, cancel them.
    let tasks = match s
        .request(Method::GET, "/_tasks?actions=*search*,*sql*&detailed", None)
        .await
    {
        Ok(t) => t,
        Err(_) => return Ok(()), // tasks API unavailable — nothing to do
    };

    for task_id in tasks_matching_opaque(&tasks, &opaque) {
        let _ = s
            .request(Method::POST, &format!("/_tasks/{task_id}/_cancel"), None)
            .await;
    }
    Ok(())
}

/// Collects task ids (`node:num`) whose `X-Opaque-Id` header matches `opaque`.
fn tasks_matching_opaque(tasks: &Json, opaque: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let Some(nodes) = tasks.get("nodes").and_then(|n| n.as_object()) else {
        return ids;
    };
    for node in nodes.values() {
        let Some(node_tasks) = node.get("tasks").and_then(|t| t.as_object()) else {
            continue;
        };
        for (task_id, task) in node_tasks {
            if task
                .pointer("/headers/X-Opaque-Id")
                .and_then(|v| v.as_str())
                == Some(opaque)
            {
                ids.push(task_id.clone());
            }
        }
    }
    ids
}

// ============ Streaming (search_after + PIT, SQL cursor, fallback) ============

/// Page size for streamed search / SQL pagination.
const STREAM_PAGE: u64 = 1000;

/// A `_search` is streamed (PIT + search_after) only when its requested `size`
/// exceeds this — i.e. it could not be served by a single request anyway
/// (`index.max_result_window` defaults to 10 000). Smaller searches keep Dev
/// Tools semantics and run once.
const STREAM_SIZE_THRESHOLD: u64 = 10_000;

pub async fn execute_stream(
    map: &SessionMap,
    session: SessionId,
    query: &str,
    query_id: QueryId,
    sender: StreamSender,
) -> EngineResult<()> {
    let s = get(map, session).await?;
    let opaque = query_id.0.to_string();

    // SQL → cursor pagination.
    if let Some(sql) = sql_query(query) {
        return stream_sql(&s, sql, &opaque, &sender).await;
    }

    let (method, path, body) = parse_console(query)?;

    // Only an explicit, large `_search` (size beyond the single-request window)
    // is streamed via PIT + search_after. A plain `_search` keeps Dev Tools
    // semantics (size-limited) and runs once, so we never flood the grid with a
    // whole index from an ad-hoc query.
    if matches!(method, Method::GET | Method::POST) {
        if let Some((index, target)) = streamable_search(&path, body.as_deref()) {
            return stream_search(&s, &index, body.as_deref(), target, &opaque, &sender).await;
        }
    }

    // Everything else (cat, mapping, bulk, count, cluster, small searches…) →
    // run once and emit as a one-shot stream.
    let json = s.send(method, &path, body, Some(&opaque)).await?;
    emit_result(&sender, map_response(&json, 0.0)).await
}

/// Emits a non-streamed result as a one-shot stream (columns + rows + done).
async fn emit_result(sender: &StreamSender, result: QueryResult) -> EngineResult<()> {
    let affected = result.affected_rows.unwrap_or(0);
    let _ = sender.send(StreamEvent::Columns(result.columns)).await;
    if !result.rows.is_empty() {
        let _ = sender.send(StreamEvent::RowBatch(result.rows)).await;
    }
    let _ = sender.send(StreamEvent::Done(affected)).await;
    Ok(())
}

/// Returns `(index, target_size)` when `path` is a streamable `/{index}/_search`
/// on a concrete index, with no aggregations and an explicit `size` greater than
/// [`STREAM_SIZE_THRESHOLD`]; otherwise `None`.
fn streamable_search(path: &str, body: Option<&str>) -> Option<(String, u64)> {
    let p = path.trim_start_matches('/');
    let p = p.split('?').next().unwrap_or(p);
    let segs: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() != 2 || segs[1] != "_search" {
        return None; // bare /_search, _msearch, _async_search, etc.
    }
    let index = segs[0];
    if index.starts_with('_') {
        return None;
    }
    let parsed: Json = serde_json::from_str(body?).ok()?;
    if parsed.get("aggs").is_some() || parsed.get("aggregations").is_some() {
        return None; // aggregations don't paginate by hits
    }
    let size = parsed.get("size").and_then(|v| v.as_u64())?;
    if size <= STREAM_SIZE_THRESHOLD {
        return None;
    }
    Some((index.to_string(), size))
}

async fn stream_search(
    s: &SearchSession,
    index: &str,
    body: Option<&str>,
    target: u64,
    opaque: &str,
    sender: &StreamSender,
) -> EngineResult<()> {
    let mut user: JsonMap<String, Json> = body
        .and_then(|b| serde_json::from_str::<Json>(b).ok())
        .and_then(|j| j.as_object().cloned())
        .unwrap_or_default();

    // Open a point-in-time for a consistent view across pages.
    let pit = s
        .send(
            Method::POST,
            &format!("/{index}/_pit?keep_alive=2m"),
            None,
            Some(opaque),
        )
        .await?;
    let mut pit_id = pit
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| EngineError::execution_error("PIT open returned no id"))?
        .to_string();

    // Deterministic sort: user sort (if any) + a `_shard_doc` tiebreaker (PIT).
    let mut sort = user
        .get("sort")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    sort.push(json!({ "_shard_doc": "asc" }));
    let query = user.remove("query").unwrap_or(json!({ "match_all": {} }));

    let _ = sender.send(StreamEvent::Columns(hit_columns())).await;

    let mut search_after: Option<Json> = None;
    let mut total: u64 = 0;
    let outcome: EngineResult<()> = loop {
        let remaining = target.saturating_sub(total);
        if remaining == 0 {
            break Ok(());
        }
        let page = remaining.min(STREAM_PAGE);

        let mut req = JsonMap::new();
        req.insert("size".into(), json!(page));
        req.insert("track_total_hits".into(), json!(false));
        req.insert("query".into(), query.clone());
        req.insert("sort".into(), Json::Array(sort.clone()));
        req.insert("pit".into(), json!({ "id": pit_id, "keep_alive": "2m" }));
        if let Some(after) = &search_after {
            req.insert("search_after".into(), after.clone());
        }

        let resp = match s
            .send(
                Method::POST,
                "/_search",
                Some(Json::Object(req).to_string()),
                Some(opaque),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => break Err(e),
        };

        if let Some(id) = resp.get("pit_id").and_then(|v| v.as_str()) {
            pit_id = id.to_string();
        }

        let hits = resp
            .pointer("/hits/hits")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let page_len = hits.len() as u64;
        if hits.is_empty() {
            break Ok(());
        }
        search_after = hits.last().and_then(|h| h.get("sort").cloned());

        let batch: Vec<Row> = hits.iter().map(hit_to_row).collect();
        total += batch.len() as u64;
        if sender.send(StreamEvent::RowBatch(batch)).await.is_err() {
            break Ok(()); // receiver gone (cancelled)
        }
        if page_len < page {
            break Ok(()); // exhausted before reaching the target
        }
    };

    // Best-effort PIT close.
    let _ = s
        .request(
            Method::DELETE,
            "/_pit",
            Some(json!({ "id": pit_id }).to_string()),
        )
        .await;

    match outcome {
        Ok(()) => {
            let _ = sender.send(StreamEvent::Done(total)).await;
            Ok(())
        }
        Err(e) => {
            let _ = sender.send(StreamEvent::Error(e.to_string())).await;
            Err(e)
        }
    }
}

async fn stream_sql(
    s: &SearchSession,
    sql: &str,
    opaque: &str,
    sender: &StreamSender,
) -> EngineResult<()> {
    let endpoint = s.flavor.sql_endpoint();
    let first = s
        .send(
            Method::POST,
            endpoint,
            Some(json!({ "query": sql, "fetch_size": STREAM_PAGE }).to_string()),
            Some(opaque),
        )
        .await?;

    // Columns are present only on the first page.
    let head = map_sql_response(&first, 0.0);
    let _ = sender.send(StreamEvent::Columns(head.columns)).await;

    let mut total: u64 = 0;
    let mut rows = head.rows;
    let mut cursor = first
        .get("cursor")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    loop {
        if !rows.is_empty() {
            total += rows.len() as u64;
            if sender.send(StreamEvent::RowBatch(rows)).await.is_err() {
                break;
            }
        }
        let Some(c) = cursor.take() else { break };
        let resp = s
            .send(
                Method::POST,
                endpoint,
                Some(json!({ "cursor": c }).to_string()),
                Some(opaque),
            )
            .await?;
        rows = sql_rows(&resp);
        cursor = resp
            .get("cursor")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if rows.is_empty() && cursor.is_none() {
            break;
        }
    }

    let _ = sender.send(StreamEvent::Done(total)).await;
    Ok(())
}

fn hit_columns() -> Vec<ColumnInfo> {
    vec![
        col("_id", "text"),
        col("_index", "text"),
        col("_score", "float"),
        col("_source", "json"),
    ]
}

fn hit_to_row(h: &Json) -> Row {
    Row {
        values: vec![
            json_to_text_value(h.get("_id")),
            json_to_text_value(h.get("_index")),
            h.get("_score")
                .and_then(|v| v.as_f64())
                .map(Value::Float)
                .unwrap_or(Value::Null),
            Value::Json(h.get("_source").cloned().unwrap_or(Json::Null)),
        ],
    }
}

pub async fn preview_table(
    map: &SessionMap,
    session: SessionId,
    index: &str,
    limit: u32,
) -> EngineResult<QueryResult> {
    let s = get(map, session).await?;
    let body = json!({ "size": limit, "query": { "match_all": {} } }).to_string();
    let started = Instant::now();
    let json = s
        .request(Method::POST, &format!("/{index}/_search"), Some(body))
        .await?;
    let elapsed_ms = started.elapsed().as_micros() as f64 / 1000.0;
    Ok(map_response(&json, elapsed_ms))
}

pub async fn query_table(
    map: &SessionMap,
    session: SessionId,
    index: &str,
    options: TableQueryOptions,
) -> EngineResult<PaginatedQueryResult> {
    let s = get(map, session).await?;
    let page = options.effective_page();
    let page_size = options.effective_page_size();
    let offset = page.saturating_sub(1) as u64 * page_size as u64;

    let mut body = JsonMap::new();
    body.insert("from".into(), json!(offset));
    body.insert("size".into(), json!(page_size));
    body.insert("track_total_hits".into(), json!(true));
    body.insert("query".into(), json!({ "match_all": {} }));

    if let Some(col) = options.sort_column.as_ref() {
        if !META_FIELDS.contains(&col.as_str()) {
            let dir = match options.sort_direction {
                Some(SortDirection::Desc) => "desc",
                _ => "asc",
            };
            body.insert("sort".into(), json!([{ col: { "order": dir } }]));
        }
    }

    let started = Instant::now();
    let json = s
        .request(
            Method::POST,
            &format!("/{index}/_search"),
            Some(Json::Object(body).to_string()),
        )
        .await?;
    let elapsed_ms = started.elapsed().as_micros() as f64 / 1000.0;

    let total = json
        .pointer("/hits/total/value")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let result = map_response(&json, elapsed_ms);
    Ok(PaginatedQueryResult::new(result, total, page, page_size))
}

// ==================== Mutations (document CRUD via the grid) ====================

pub async fn insert_row(
    map: &SessionMap,
    session: SessionId,
    index: &str,
    data: &RowData,
) -> EngineResult<QueryResult> {
    let s = get(map, session).await?;
    let doc = document_from_rowdata(data);
    let started = Instant::now();
    let (method, path) = match doc_id(data) {
        Some(id) => (Method::PUT, format!("/{index}/_doc/{id}?refresh=wait_for")),
        None => (Method::POST, format!("/{index}/_doc?refresh=wait_for")),
    };
    let json = s.request(method, &path, Some(doc.to_string())).await?;
    let elapsed_ms = started.elapsed().as_micros() as f64 / 1000.0;
    Ok(mutation_result(&json, 1, elapsed_ms))
}

pub async fn update_row(
    map: &SessionMap,
    session: SessionId,
    index: &str,
    primary_key: &RowData,
    data: &RowData,
) -> EngineResult<QueryResult> {
    let id = doc_id(primary_key).ok_or_else(|| {
        EngineError::validation("Updating a document requires its _id in the primary key")
    })?;
    if data.columns.is_empty() {
        return Ok(QueryResult::with_affected_rows(0, 0.0));
    }
    let s = get(map, session).await?;
    let doc = document_from_rowdata(data);
    let body = json!({ "doc": doc }).to_string();
    let started = Instant::now();
    let json = s
        .request(
            Method::POST,
            &format!("/{index}/_update/{id}?refresh=wait_for"),
            Some(body),
        )
        .await?;
    let elapsed_ms = started.elapsed().as_micros() as f64 / 1000.0;
    Ok(mutation_result(&json, 1, elapsed_ms))
}

pub async fn delete_row(
    map: &SessionMap,
    session: SessionId,
    index: &str,
    primary_key: &RowData,
) -> EngineResult<QueryResult> {
    let id = doc_id(primary_key).ok_or_else(|| {
        EngineError::validation("Deleting a document requires its _id in the primary key")
    })?;
    let s = get(map, session).await?;
    let started = Instant::now();
    let json = s
        .request(
            Method::DELETE,
            &format!("/{index}/_doc/{id}?refresh=wait_for"),
            None,
        )
        .await?;
    let elapsed_ms = started.elapsed().as_micros() as f64 / 1000.0;
    Ok(mutation_result(&json, 1, elapsed_ms))
}

/// Extracts the `_id` from a row's data, if present.
fn doc_id(data: &RowData) -> Option<String> {
    match data.columns.get("_id")? {
        Value::Text(s) => Some(s.clone()),
        Value::Int(i) => Some(i.to_string()),
        Value::Json(Json::String(s)) => Some(s.clone()),
        _ => None,
    }
}

/// Builds the document body to send for an insert/update. If the row carries a
/// `_source` column it is used verbatim; otherwise the non-meta columns are
/// assembled into an object.
fn document_from_rowdata(data: &RowData) -> Json {
    if let Some(src) = data.columns.get("_source") {
        return value_to_json(src);
    }
    let mut obj = JsonMap::new();
    for (k, v) in &data.columns {
        if META_FIELDS.contains(&k.as_str()) {
            continue;
        }
        obj.insert(k.clone(), value_to_json(v));
    }
    Json::Object(obj)
}

// ==================== Response mapping ====================

/// Maps an arbitrary search response into a tabular [`QueryResult`] by
/// inspecting its shape (see the spec's response-mapping table).
pub fn map_response(json: &Json, elapsed_ms: f64) -> QueryResult {
    // 1. Search hits.
    if let Some(hits) = json.pointer("/hits/hits").and_then(|v| v.as_array()) {
        return map_hits(hits, json.get("aggregations"), elapsed_ms);
    }

    // 2. Pure aggregation response (no hits).
    if let Some(aggs) = json.get("aggregations") {
        return single_json_column("aggregations", aggs.clone(), elapsed_ms);
    }

    // 3. `_cat/*` array of objects.
    if let Some(arr) = json.as_array() {
        return map_cat(arr, elapsed_ms);
    }

    // 4. Index / update / delete / bulk side effects.
    if let Some(rows) = mutation_affected(json) {
        return mutation_result(json, rows, elapsed_ms);
    }

    // 5. Generic fallback (cluster, raw mapping…).
    single_json_column("response", json.clone(), elapsed_ms)
}

fn map_hits(hits: &[Json], aggs: Option<&Json>, elapsed_ms: f64) -> QueryResult {
    let mut columns = vec![
        col("_id", "text"),
        col("_index", "text"),
        col("_score", "float"),
        col("_source", "json"),
    ];
    let include_aggs = aggs.is_some();
    if include_aggs {
        columns.push(col("aggregations", "json"));
    }

    let rows = hits
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let mut values = vec![
                json_to_text_value(h.get("_id")),
                json_to_text_value(h.get("_index")),
                h.get("_score")
                    .and_then(|v| v.as_f64())
                    .map(Value::Float)
                    .unwrap_or(Value::Null),
                Value::Json(h.get("_source").cloned().unwrap_or(Json::Null)),
            ];
            if include_aggs {
                values.push(if i == 0 {
                    Value::Json(aggs.cloned().unwrap_or(Json::Null))
                } else {
                    Value::Null
                });
            }
            Row { values }
        })
        .collect();

    QueryResult {
        columns,
        rows,
        affected_rows: None,
        execution_time_ms: elapsed_ms,
    }
}

fn map_cat(arr: &[Json], elapsed_ms: f64) -> QueryResult {
    let first = arr.iter().find_map(|v| v.as_object());
    let Some(first) = first else {
        // Array of scalars (or empty) — fall back to a single json column.
        return single_json_column("response", Json::Array(arr.to_vec()), elapsed_ms);
    };

    let keys: Vec<String> = first.keys().cloned().collect();
    let columns = keys.iter().map(|k| col(k, "text")).collect();
    let rows = arr
        .iter()
        .map(|row| Row {
            values: keys
                .iter()
                .map(|k| json_to_text_value(row.get(k)))
                .collect(),
        })
        .collect();

    QueryResult {
        columns,
        rows,
        affected_rows: None,
        execution_time_ms: elapsed_ms,
    }
}

/// Returns an affected-row count if the JSON looks like a write side effect.
fn mutation_affected(json: &Json) -> Option<u64> {
    if json.get("result").and_then(|v| v.as_str()).is_some() {
        return Some(1);
    }
    if let Some(items) = json.get("items").and_then(|v| v.as_array()) {
        return Some(items.len() as u64);
    }
    if let Some(n) = json.get("deleted").and_then(|v| v.as_u64()) {
        return Some(n);
    }
    if let Some(n) = json.get("updated").and_then(|v| v.as_u64()) {
        return Some(n);
    }
    if json.get("acknowledged").and_then(|v| v.as_bool()).is_some() {
        return Some(0);
    }
    None
}

fn mutation_result(json: &Json, affected: u64, elapsed_ms: f64) -> QueryResult {
    QueryResult {
        columns: vec![col("result", "json")],
        rows: vec![Row {
            values: vec![Value::Json(json.clone())],
        }],
        affected_rows: Some(affected),
        execution_time_ms: elapsed_ms,
    }
}

/// Maps a SQL response into a tabular [`QueryResult`]. Elasticsearch returns
/// `columns`/`rows`; OpenSearch's SQL plugin returns `schema`/`datarows`. Both
/// shapes are accepted; anything else falls back to a raw JSON column.
fn map_sql_response(json: &Json, elapsed_ms: f64) -> QueryResult {
    let cols = json
        .get("columns")
        .or_else(|| json.get("schema"))
        .and_then(|c| c.as_array());
    let data = json
        .get("rows")
        .or_else(|| json.get("datarows"))
        .and_then(|r| r.as_array());

    let (Some(cols), Some(data)) = (cols, data) else {
        return single_json_column("response", json.clone(), elapsed_ms);
    };

    let columns: Vec<ColumnInfo> = cols
        .iter()
        .map(|c| {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let ty = c.get("type").and_then(|v| v.as_str()).unwrap_or("text");
            col(name, ty)
        })
        .collect();

    QueryResult {
        columns,
        rows: sql_rows_from(data),
        affected_rows: None,
        execution_time_ms: elapsed_ms,
    }
}

/// Maps the `rows`/`datarows` cell matrix of a SQL response into [`Row`]s.
/// Used both for the first page and for cursor-continuation pages (which carry
/// no `columns`).
fn sql_rows(json: &Json) -> Vec<Row> {
    let data = json
        .get("rows")
        .or_else(|| json.get("datarows"))
        .and_then(|r| r.as_array());
    match data {
        Some(d) => sql_rows_from(d),
        None => Vec::new(),
    }
}

fn sql_rows_from(data: &[Json]) -> Vec<Row> {
    data.iter()
        .map(|row| Row {
            values: row
                .as_array()
                .map(|cells| cells.iter().map(json_cell_to_value).collect())
                .unwrap_or_default(),
        })
        .collect()
}

/// Converts a SQL result cell into a typed [`Value`], preserving numbers and
/// booleans (objects/arrays are kept as JSON).
fn json_cell_to_value(v: &Json) -> Value {
    match v {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Text(n.to_string())
            }
        }
        Json::String(s) => Value::Text(s.clone()),
        other => Value::Json(other.clone()),
    }
}

fn single_json_column(name: &str, value: Json, elapsed_ms: f64) -> QueryResult {
    QueryResult {
        columns: vec![col(name, "json")],
        rows: vec![Row {
            values: vec![Value::Json(value)],
        }],
        affected_rows: None,
        execution_time_ms: elapsed_ms,
    }
}

fn col(name: &str, data_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: true,
    }
}

// ==================== Console parsing ====================

/// Parses a Dev Tools console block: first line `METHOD /path`, the rest is an
/// optional JSON / NDJSON body.
pub fn parse_console(input: &str) -> EngineResult<(Method, String, Option<String>)> {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return Err(EngineError::validation("Empty query"));
    }

    let (first_line, rest) = match trimmed.split_once('\n') {
        Some((a, b)) => (a, Some(b)),
        None => (trimmed, None),
    };

    let first = first_line.trim();
    let (method_str, path) = first
        .split_once(char::is_whitespace)
        .ok_or_else(|| EngineError::syntax_error("Expected 'METHOD /path' on the first line"))?;

    let method = match method_str.to_ascii_uppercase().as_str() {
        "GET" => Method::GET,
        "POST" => Method::POST,
        "PUT" => Method::PUT,
        "DELETE" => Method::DELETE,
        "HEAD" => Method::HEAD,
        other => {
            return Err(EngineError::syntax_error(format!(
                "Unsupported HTTP method: {other}"
            )))
        }
    };

    let path = path.trim().to_string();
    if path.is_empty() {
        return Err(EngineError::syntax_error("Missing request path"));
    }

    let body = rest.and_then(|r| {
        let trimmed_body = r.trim();
        if trimmed_body.is_empty() {
            None
        } else if path.contains("_bulk") {
            // Bulk needs a trailing newline after the last action/source line.
            Some(format!("{trimmed_body}\n"))
        } else {
            Some(trimmed_body.to_string())
        }
    });

    Ok((method, path, body))
}

// ==================== Auth & URL building ====================

fn auth_mode(config: &ConnectionConfig) -> &str {
    config
        .search_auth_mode
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("none")
}

fn build_auth_header(config: &ConnectionConfig) -> EngineResult<Option<HeaderValue>> {
    let value = match auth_mode(config) {
        "none" => return Ok(None),
        "basic" => {
            let token = STANDARD.encode(format!("{}:{}", config.username, config.password));
            format!("Basic {token}")
        }
        "api_key" => format!("ApiKey {}", config.password),
        "bearer" => format!("Bearer {}", config.password),
        other => {
            return Err(EngineError::validation(format!(
                "Unknown search auth mode: {other}"
            )))
        }
    };
    HeaderValue::from_str(&value)
        .map(Some)
        .map_err(|_| EngineError::validation("Invalid characters in credentials"))
}

/// Builds the base URL, honouring an Elastic Cloud ID in `host` if present.
fn build_base_url(config: &ConnectionConfig) -> EngineResult<Url> {
    if let Some(endpoint) = decode_cloud_id(&config.host) {
        return Url::parse(&format!("{endpoint}/")).map_err(|e| {
            EngineError::connection_failed(format!("Invalid Cloud ID endpoint: {e}"))
        });
    }

    let scheme = if config.ssl { "https" } else { "http" };
    let host = if config.host.trim().is_empty() {
        "localhost"
    } else {
        config.host.trim()
    };
    let port = if config.port == 0 { 9200 } else { config.port };

    Url::parse(&format!("{scheme}://{host}:{port}/"))
        .map_err(|e| EngineError::connection_failed(format!("Invalid search URL: {e}")))
}

/// Decodes an Elastic Cloud ID (`name:base64(host$es_uuid$kibana_uuid)`) into a
/// concrete `https://` endpoint. Returns `None` for plain host strings.
fn decode_cloud_id(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let (_name, b64) = raw.split_once(':')?;
    // base64 has no dots; a `host:port` string would, so this also screens out
    // ordinary `host:9200` inputs cheaply.
    if b64.is_empty() || b64.contains('.') {
        return None;
    }
    let decoded = STANDARD.decode(b64).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let mut parts = decoded.split('$');
    let host = parts.next()?;
    let es_uuid = parts.next()?;
    if host.is_empty() || es_uuid.is_empty() {
        return None;
    }
    let (host_name, port) = match host.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => (h, p),
        _ => (host, "443"),
    };
    Some(format!("https://{es_uuid}.{host_name}:{port}"))
}

// ==================== Flavor & error helpers ====================

/// Verifies the server matches the expected flavor. OpenSearch advertises
/// `version.distribution == "opensearch"`; Elasticsearch never does.
fn verify_flavor(root: &Json, flavor: SearchFlavor) -> EngineResult<()> {
    let distribution = root
        .pointer("/version/distribution")
        .and_then(|v| v.as_str());
    let is_opensearch = distribution == Some("opensearch");
    match flavor {
        SearchFlavor::OpenSearch if !is_opensearch => Err(EngineError::connection_failed(
            "Expected an OpenSearch cluster but the server reports Elasticsearch. \
             Use the Elasticsearch driver instead.",
        )),
        SearchFlavor::Elasticsearch if is_opensearch => Err(EngineError::connection_failed(
            "Expected an Elasticsearch cluster but the server reports OpenSearch. \
             Use the OpenSearch driver instead.",
        )),
        _ => Ok(()),
    }
}

fn format_search_error(status: u16, json: &Json, raw: &str) -> String {
    if let Some(err) = json.get("error") {
        let kind = err.get("type").and_then(|v| v.as_str());
        let reason = err
            .get("reason")
            .and_then(|v| v.as_str())
            .or_else(|| err.as_str());
        match (kind, reason) {
            (Some(k), Some(r)) => return format!("Search {status}: {k}: {r}"),
            (None, Some(r)) => return format!("Search {status}: {r}"),
            _ => {}
        }
    }
    format!("Search {status}: {}", raw.trim())
}

// ==================== Value <-> JSON conversions ====================

fn json_to_text_value(v: Option<&Json>) -> Value {
    match v {
        None | Some(Json::Null) => Value::Null,
        Some(Json::String(s)) => Value::Text(s.clone()),
        Some(other) => Value::Text(other.to_string()),
    }
}

fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(i) => Json::Number((*i).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(Json::Number)
            .unwrap_or(Json::Null),
        Value::Text(s) => {
            // Allow pasting a JSON object/array into a text cell.
            let t = s.trim();
            if t.starts_with('{') || t.starts_with('[') {
                serde_json::from_str(t).unwrap_or_else(|_| Json::String(s.clone()))
            } else {
                Json::String(s.clone())
            }
        }
        Value::Bytes(b) => Json::String(STANDARD.encode(b)),
        Value::Json(j) => j.clone(),
        Value::Array(arr) => Json::Array(arr.iter().map(value_to_json).collect()),
    }
}

fn matches_search(name: &str, search: &Option<String>) -> bool {
    match search {
        Some(q) => name.to_ascii_lowercase().contains(q),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flavor_ids() {
        assert_eq!(SearchFlavor::Elasticsearch.driver_id(), "elasticsearch");
        assert_eq!(SearchFlavor::OpenSearch.driver_id(), "opensearch");
    }

    #[test]
    fn parse_console_method_and_path() {
        let (m, p, b) = parse_console("GET /my-index/_search").unwrap();
        assert_eq!(m, Method::GET);
        assert_eq!(p, "/my-index/_search");
        assert!(b.is_none());
    }

    #[test]
    fn parse_console_with_body() {
        let input = "POST /idx/_search\n{\n  \"query\": { \"match_all\": {} }\n}";
        let (m, p, b) = parse_console(input).unwrap();
        assert_eq!(m, Method::POST);
        assert_eq!(p, "/idx/_search");
        assert!(b.unwrap().contains("match_all"));
    }

    #[test]
    fn parse_console_bulk_keeps_trailing_newline() {
        let input = "POST /_bulk\n{\"index\":{}}\n{\"a\":1}";
        let (_, _, b) = parse_console(input).unwrap();
        assert!(b.unwrap().ends_with('\n'));
    }

    #[test]
    fn parse_console_lowercase_method() {
        let (m, _, _) = parse_console("get _cat/indices").unwrap();
        assert_eq!(m, Method::GET);
    }

    #[test]
    fn parse_console_rejects_bad_method() {
        assert!(parse_console("FETCH /x").is_err());
        assert!(parse_console("").is_err());
        assert!(parse_console("GET").is_err());
    }

    #[test]
    fn map_response_hits() {
        let json = serde_json::json!({
            "took": 5,
            "hits": { "total": {"value": 1}, "hits": [
                { "_id": "1", "_index": "books", "_score": 1.5, "_source": {"title": "rust"} }
            ]}
        });
        let r = map_response(&json, 1.0);
        assert_eq!(r.columns.len(), 4);
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.columns[0].name, "_id");
        matches!(r.rows[0].values[3], Value::Json(_));
    }

    #[test]
    fn map_response_aggregations_only() {
        let json = serde_json::json!({
            "hits": { "hits": [] },
            "aggregations": { "by_year": { "buckets": [] } }
        });
        // hits present (empty) + aggs => hits table with extra aggregations col
        let r = map_response(&json, 1.0);
        assert!(r.columns.iter().any(|c| c.name == "aggregations"));
    }

    #[test]
    fn map_response_pure_aggregations() {
        let json = serde_json::json!({
            "aggregations": { "by_year": { "buckets": [] } }
        });
        let r = map_response(&json, 1.0);
        assert_eq!(r.columns.len(), 1);
        assert_eq!(r.columns[0].name, "aggregations");
    }

    #[test]
    fn map_response_cat_array() {
        let json = serde_json::json!([
            { "index": "books", "health": "green" },
            { "index": "users", "health": "yellow" }
        ]);
        let r = map_response(&json, 1.0);
        assert_eq!(r.rows.len(), 2);
        assert!(r.columns.iter().any(|c| c.name == "index"));
        assert!(r.columns.iter().any(|c| c.name == "health"));
    }

    #[test]
    fn map_response_index_result() {
        let json = serde_json::json!({ "_id": "1", "result": "created" });
        let r = map_response(&json, 1.0);
        assert_eq!(r.affected_rows, Some(1));
        assert_eq!(r.columns[0].name, "result");
    }

    #[test]
    fn map_response_generic() {
        let json = serde_json::json!({ "cluster_name": "qoredb", "status": "green" });
        let r = map_response(&json, 1.0);
        assert_eq!(r.columns.len(), 1);
        assert_eq!(r.columns[0].name, "response");
    }

    #[test]
    fn sql_query_detects_sql_vs_console() {
        assert_eq!(
            sql_query("SELECT * FROM books"),
            Some("SELECT * FROM books")
        );
        assert_eq!(sql_query("  show tables  "), Some("show tables"));
        assert!(sql_query("GET /_cat/indices").is_none());
        assert!(sql_query("post /idx/_search").is_none());
        assert!(sql_query("   ").is_none());
    }

    #[test]
    fn sql_endpoint_per_flavor() {
        assert_eq!(
            SearchFlavor::Elasticsearch.sql_endpoint(),
            "/_sql?format=json"
        );
        assert_eq!(SearchFlavor::OpenSearch.sql_endpoint(), "/_plugins/_sql");
    }

    #[test]
    fn map_sql_response_elasticsearch_shape() {
        let json = serde_json::json!({
            "columns": [{ "name": "title", "type": "text" }, { "name": "year", "type": "long" }],
            "rows": [["rust", 2010], ["go", 2009]]
        });
        let r = map_sql_response(&json, 1.0);
        assert_eq!(r.columns.len(), 2);
        assert_eq!(r.columns[0].name, "title");
        assert_eq!(r.rows.len(), 2);
        assert!(matches!(r.rows[0].values[1], Value::Int(2010)));
    }

    #[test]
    fn map_sql_response_opensearch_shape() {
        let json = serde_json::json!({
            "schema": [{ "name": "title", "type": "text" }],
            "datarows": [["opensearch"]],
            "total": 1, "size": 1
        });
        let r = map_sql_response(&json, 1.0);
        assert_eq!(r.columns.len(), 1);
        assert_eq!(r.columns[0].name, "title");
        assert_eq!(r.rows.len(), 1);
        assert!(matches!(r.rows[0].values[0], Value::Text(_)));
    }

    #[test]
    fn map_sql_response_unknown_shape_falls_back() {
        let json = serde_json::json!({ "error": "boom" });
        let r = map_sql_response(&json, 1.0);
        assert_eq!(r.columns.len(), 1);
        assert_eq!(r.columns[0].name, "response");
    }

    #[test]
    fn split_requests_splits_on_method_lines() {
        let input = "GET /a/_search\n{\n  \"query\": {\n\n    \"match_all\": {}\n  }\n}\nPOST /b/_doc\n{\"x\":1}\nGET /_cat/indices";
        let blocks = split_requests(input);
        assert_eq!(blocks.len(), 3);
        assert!(blocks[0].starts_with("GET /a/_search"));
        // A blank line *inside* a JSON body must not split the request.
        assert!(blocks[0].contains("match_all"));
        assert!(blocks[1].starts_with("POST /b/_doc"));
        assert!(blocks[2].starts_with("GET /_cat/indices"));
    }

    #[test]
    fn split_requests_single_block() {
        assert_eq!(split_requests("GET /_cat/indices").len(), 1);
        assert_eq!(split_requests("SELECT * FROM books").len(), 1);
    }

    #[test]
    fn streamable_search_detection() {
        // Streamed only with an explicit large size.
        assert_eq!(
            streamable_search("/books/_search", Some("{\"size\": 50000}")),
            Some(("books".to_string(), 50000))
        );
        assert_eq!(
            streamable_search("/books/_search?pretty", Some("{\"size\":20000}")),
            Some(("books".to_string(), 20000))
        );
        // Default / small / missing size keeps Dev Tools semantics (not streamed).
        assert!(streamable_search("/books/_search", None).is_none());
        assert!(streamable_search("/books/_search", Some("{\"query\":{}}")).is_none());
        assert!(streamable_search("/books/_search", Some("{\"size\": 100}")).is_none());
        // bare _search (all indices), msearch, aggregations are not streamed
        assert!(streamable_search("/_search", Some("{\"size\":50000}")).is_none());
        assert!(streamable_search("/books/_msearch", Some("{\"size\":50000}")).is_none());
        assert!(streamable_search(
            "/books/_search",
            Some("{\"size\":50000,\"aggs\":{\"x\":{}}}")
        )
        .is_none());
    }

    #[test]
    fn tasks_matching_opaque_filters_by_header() {
        let tasks = serde_json::json!({
            "nodes": {
                "node1": { "tasks": {
                    "node1:42": { "headers": { "X-Opaque-Id": "abc" } },
                    "node1:43": { "headers": { "X-Opaque-Id": "other" } },
                    "node1:44": {}
                }}
            }
        });
        let ids = tasks_matching_opaque(&tasks, "abc");
        assert_eq!(ids, vec!["node1:42".to_string()]);
    }

    #[test]
    fn sql_rows_reads_both_shapes() {
        let es = serde_json::json!({ "rows": [[1, "a"], [2, "b"]], "cursor": "x" });
        assert_eq!(sql_rows(&es).len(), 2);
        let os = serde_json::json!({ "datarows": [["a"]], "schema": [] });
        assert_eq!(sql_rows(&os).len(), 1);
        let none = serde_json::json!({ "cursor": "x" });
        assert!(sql_rows(&none).is_empty());
    }

    #[test]
    fn verify_flavor_distinguishes_products() {
        let os =
            serde_json::json!({ "version": { "distribution": "opensearch", "number": "2.11.0" } });
        let es = serde_json::json!({ "version": { "number": "8.12.0" } });
        assert!(verify_flavor(&os, SearchFlavor::OpenSearch).is_ok());
        assert!(verify_flavor(&os, SearchFlavor::Elasticsearch).is_err());
        assert!(verify_flavor(&es, SearchFlavor::Elasticsearch).is_ok());
        assert!(verify_flavor(&es, SearchFlavor::OpenSearch).is_err());
    }

    #[test]
    fn decode_cloud_id_roundtrip() {
        // host = "example.aws.found.io:443", es uuid = "abc123"
        let decoded = "example.aws.found.io:443$abc123$def456";
        let b64 = STANDARD.encode(decoded);
        let cloud_id = format!("my-deploy:{b64}");
        let endpoint = decode_cloud_id(&cloud_id).unwrap();
        assert_eq!(endpoint, "https://abc123.example.aws.found.io:443");
    }

    #[test]
    fn decode_cloud_id_ignores_plain_host() {
        assert!(decode_cloud_id("localhost:9200").is_none());
        assert!(decode_cloud_id("es.example.com").is_none());
    }

    #[test]
    fn flatten_properties_handles_nested_and_multifield() {
        let props = serde_json::json!({
            "title": { "type": "text", "fields": { "keyword": { "type": "keyword" } } },
            "author": { "properties": { "name": { "type": "text" } } }
        });
        let mut cols = Vec::new();
        flatten_properties(props.as_object().unwrap(), "", &mut cols);
        let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"title"));
        assert!(names.contains(&"title.keyword"));
        assert!(names.contains(&"author"));
        assert!(names.contains(&"author.name"));
    }

    #[test]
    fn build_auth_header_modes() {
        let mut cfg = base_cfg();
        cfg.search_auth_mode = Some("none".into());
        assert!(build_auth_header(&cfg).unwrap().is_none());

        cfg.search_auth_mode = Some("basic".into());
        cfg.username = "elastic".into();
        cfg.password = "pw".into();
        let h = build_auth_header(&cfg).unwrap().unwrap();
        assert!(h.to_str().unwrap().starts_with("Basic "));

        cfg.search_auth_mode = Some("api_key".into());
        cfg.password = "KEYVALUE".into();
        let h = build_auth_header(&cfg).unwrap().unwrap();
        assert_eq!(h.to_str().unwrap(), "ApiKey KEYVALUE");

        cfg.search_auth_mode = Some("bearer".into());
        cfg.password = "TOKEN".into();
        let h = build_auth_header(&cfg).unwrap().unwrap();
        assert_eq!(h.to_str().unwrap(), "Bearer TOKEN");
    }

    #[test]
    fn cleartext_credentials_refused() {
        let mut cfg = base_cfg();
        cfg.ssl = false;
        cfg.search_auth_mode = Some("basic".into());
        cfg.password = "pw".into();
        assert!(SearchSession::new(&cfg, SearchFlavor::Elasticsearch).is_err());
    }

    #[test]
    fn base_url_defaults_to_9200() {
        let cfg = base_cfg();
        let url = build_base_url(&cfg).unwrap();
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.port(), Some(9200));
    }

    fn base_cfg() -> ConnectionConfig {
        ConnectionConfig {
            driver: "elasticsearch".into(),
            host: "localhost".into(),
            port: 9200,
            username: String::new(),
            password: String::new(),
            database: None,
            ssl: false,
            ssl_mode: None,
            environment: "development".into(),
            read_only: false,
            pool_max_connections: None,
            pool_min_connections: None,
            pool_acquire_timeout_secs: None,
            ssh_tunnel: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
        }
    }
}

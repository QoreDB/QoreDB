// SPDX-License-Identifier: Apache-2.0

//! Cassandra and ScyllaDB, on the hand-written CQL client in `cql/`.
//!
//! CQL is not SQL: no joins, no subqueries, and no arbitrary `WHERE`. The
//! driver reflects that rather than papering over it — a predicate the ring
//! cannot serve from a partition key comes back as an explicit error pointing
//! at the editor, not as a query that quietly scans every node.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::DataEngine;
use qore_core::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType, ColumnInfo,
    ConnectionConfig, FilterOperator, Namespace, PaginatedQueryResult, PaginationCapability,
    QueryId, QueryResult, Row, RowData, SessionId, TableColumn, TableQueryOptions, TableSchema,
    Value,
};
use tokio::sync::{Mutex, RwLock};

use crate::cassandra_safety::{self, CqlQueryClass};
use crate::drivers::cql::connection::{
    CqlConnection, CqlResult, CqlRows, TlsOptions, quote_identifier,
};

/// Ceiling on the rows a single `execute` accumulates in memory. Lower than the
/// document stores': a CQL statement that returns this much has almost always
/// scanned the ring, and finishing the walk would cost far more than refusing.
const MAX_EXECUTE_ROWS: usize = 200_000;

/// Rows per protocol page while walking a result set.
const SERVER_PAGE_SIZE: i32 = 5_000;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CassandraFlavor {
    Cassandra,
    ScyllaDb,
}

impl CassandraFlavor {
    fn driver_id(self) -> &'static str {
        match self {
            CassandraFlavor::Cassandra => "cassandra",
            CassandraFlavor::ScyllaDb => "scylladb",
        }
    }

    fn driver_name(self) -> &'static str {
        match self {
            CassandraFlavor::Cassandra => "Cassandra",
            CassandraFlavor::ScyllaDb => "ScyllaDB",
        }
    }
}

struct CassandraSession {
    conn: Mutex<CqlConnection>,
    /// Copied out of the config rather than holding it: the full struct keeps
    /// the password alive for the life of the session.
    read_only: bool,
    environment: String,
}

pub struct CassandraDriver {
    flavor: CassandraFlavor,
    sessions: Arc<RwLock<HashMap<SessionId, Arc<CassandraSession>>>>,
}

impl CassandraDriver {
    pub fn new() -> Self {
        Self::with_flavor(CassandraFlavor::Cassandra)
    }

    pub fn scylladb() -> Self {
        Self::with_flavor(CassandraFlavor::ScyllaDb)
    }

    fn with_flavor(flavor: CassandraFlavor) -> Self {
        Self {
            flavor,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn open(config: &ConnectionConfig) -> EngineResult<CqlConnection> {
        let tls = TlsOptions {
            enabled: config.ssl,
            ca_cert_path: config
                .ssl_ca_cert
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::to_string),
        };
        let mut conn = CqlConnection::connect(
            &config.host,
            config.port,
            &config.username,
            &config.password,
            &tls,
            CONNECT_TIMEOUT,
            IO_TIMEOUT,
        )
        .await?;

        if let Some(keyspace) = config
            .database
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
        {
            conn.use_keyspace(keyspace).await?;
        }
        Ok(conn)
    }

    async fn get_session(&self, session: SessionId) -> EngineResult<Arc<CassandraSession>> {
        let sessions = self.sessions.read().await;
        sessions
            .get(&session)
            .cloned()
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))
    }

    /// Applies the read-only and production policies before anything reaches the
    /// wire. An unrecognised statement is refused rather than allowed: the
    /// classifier is lexical, and the safe direction to be wrong in is "no".
    fn guard(session: &CassandraSession, query: &str) -> EngineResult<()> {
        let class = cassandra_safety::classify(query);
        if session.read_only && class != CqlQueryClass::Read {
            return Err(EngineError::validation(
                "This connection is read-only; only SELECT statements are allowed",
            ));
        }
        if session.environment.eq_ignore_ascii_case("production")
            && let Some(reason) = cassandra_safety::production_refusal(query)
        {
            return Err(EngineError::validation(format!(
                "Refused on a production connection: {reason}"
            )));
        }
        Ok(())
    }
}

impl Default for CassandraDriver {
    fn default() -> Self {
        Self::new()
    }
}

fn qualified(namespace: &Namespace, table: &str) -> String {
    format!(
        "{}.{}",
        quote_identifier(&namespace.database),
        quote_identifier(table)
    )
}

fn to_query_result(rows: CqlRows, elapsed: Instant) -> QueryResult {
    QueryResult {
        columns: rows
            .columns
            .iter()
            .map(|c| ColumnInfo {
                name: c.name.as_str().into(),
                data_type: c.ty.name().into(),
                // CQL has no NOT NULL: any non-key column may be absent from a
                // row, so claiming otherwise would be a lie the grid acts on.
                nullable: true,
            })
            .collect(),
        rows: rows.rows.into_iter().map(|values| Row { values }).collect(),
        affected_rows: None,
        execution_time_ms: elapsed.elapsed().as_micros() as f64 / 1000.0,
    }
}

fn empty_result(elapsed: Instant) -> QueryResult {
    QueryResult {
        columns: Vec::new(),
        rows: Vec::new(),
        affected_rows: None,
        execution_time_ms: elapsed.elapsed().as_micros() as f64 / 1000.0,
    }
}

/// Cassandra answers a predicate it cannot serve from the primary key with a
/// message about `ALLOW FILTERING`. Left alone it reads like a suggestion to
/// add two words; what it actually means is that the query has no partition to
/// start from.
fn explain_filtering_error(error: EngineError) -> EngineError {
    let text = error.to_string();
    if text.contains("ALLOW FILTERING") {
        return EngineError::validation(
            "This filter is not on the primary key, so the ring has no partition to start from. \
             Filter on the partition key, or run the query with ALLOW FILTERING from the editor.",
        );
    }
    error
}

fn first_text(rows: &CqlRows, row: usize, column: usize) -> Option<String> {
    match rows.rows.get(row)?.get(column)? {
        Value::Text(s) => Some(s.clone()),
        other => Some(format!("{other:?}")),
    }
}

impl CassandraDriver {
    async fn fetch_all(conn: &mut CqlConnection, cql: &str) -> EngineResult<CqlRows> {
        let mut collected: Option<CqlRows> = None;
        let mut state: Option<Vec<u8>> = None;
        loop {
            let page = conn
                .query(cql, Some(SERVER_PAGE_SIZE), state.as_deref())
                .await?
                .into_rows();
            let next = page.paging_state.clone();
            match collected.as_mut() {
                Some(acc) => acc.rows.extend(page.rows),
                None => collected = Some(page),
            }
            let total = collected.as_ref().map(|c| c.rows.len()).unwrap_or(0);
            if total > MAX_EXECUTE_ROWS {
                return Err(EngineError::result_too_large(
                    total as u64,
                    MAX_EXECUTE_ROWS as u64,
                ));
            }
            match next {
                Some(s) => state = Some(s),
                None => break,
            }
        }
        Ok(collected.unwrap_or_default())
    }

    /// Reads the primary key of a table in ring order: partition-key columns
    /// first, then clustering columns, each by their own position. That order is
    /// not cosmetic — it is the order a `WHERE` clause and a bound mutation must
    /// present them in.
    async fn primary_key(
        conn: &mut CqlConnection,
        keyspace: &str,
        table: &str,
    ) -> EngineResult<Vec<String>> {
        let cql = format!(
            "SELECT column_name, kind, position FROM system_schema.columns \
             WHERE keyspace_name = '{}' AND table_name = '{}'",
            escape_literal(keyspace),
            escape_literal(table)
        );
        let rows = Self::fetch_all(conn, &cql).await?;
        let mut partition: Vec<(i64, String)> = Vec::new();
        let mut clustering: Vec<(i64, String)> = Vec::new();
        for row in &rows.rows {
            let (Some(Value::Text(name)), Some(Value::Text(kind))) = (row.first(), row.get(1))
            else {
                continue;
            };
            let position = match row.get(2) {
                Some(Value::Int(p)) => *p,
                _ => 0,
            };
            match kind.as_str() {
                "partition_key" => partition.push((position, name.clone())),
                "clustering" => clustering.push((position, name.clone())),
                _ => {}
            }
        }
        partition.sort_by_key(|(p, _)| *p);
        clustering.sort_by_key(|(p, _)| *p);
        Ok(partition
            .into_iter()
            .chain(clustering)
            .map(|(_, name)| name)
            .collect())
    }
}

/// Escapes a CQL string literal for the catalog queries, which take keyspace and
/// table names as values rather than identifiers. Nothing user-authored reaches
/// this: names come from the catalog or from the connection form, and doubling
/// the quote is the CQL escape.
fn escape_literal(value: &str) -> String {
    value.replace('\'', "''")
}

#[async_trait]
impl DataEngine for CassandraDriver {
    fn driver_id(&self) -> &'static str {
        self.flavor.driver_id()
    }

    fn driver_name(&self) -> &'static str {
        self.flavor.driver_name()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        let mut conn = Self::open(config).await?;
        conn.query("SELECT release_version FROM system.local", None, None)
            .await?;
        Ok(())
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        let conn = Self::open(config).await?;
        let session_id = SessionId::new();
        let session = Arc::new(CassandraSession {
            conn: Mutex::new(conn),
            read_only: config.read_only,
            environment: config.environment.clone(),
        });
        self.sessions.write().await.insert(session_id, session);
        Ok(session_id)
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        self.sessions
            .write()
            .await
            .remove(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;
        Ok(())
    }

    async fn ping(&self, session: SessionId) -> EngineResult<()> {
        let session = self.get_session(session).await?;
        let mut conn = session.conn.lock().await;
        conn.query("SELECT release_version FROM system.local", None, None)
            .await?;
        Ok(())
    }

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let session = self.get_session(session).await?;
        let mut conn = session.conn.lock().await;
        let rows = Self::fetch_all(
            &mut conn,
            "SELECT keyspace_name FROM system_schema.keyspaces",
        )
        .await?;
        let mut namespaces: Vec<Namespace> = (0..rows.rows.len())
            .filter_map(|i| first_text(&rows, i, 0))
            .map(|database| Namespace {
                database,
                // A keyspace is the only level of nesting: there is nothing
                // below it for `schema` to name.
                schema: None,
            })
            .collect();
        namespaces.sort_by(|a, b| a.database.cmp(&b.database));
        Ok(namespaces)
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let session = self.get_session(session).await?;
        let mut conn = session.conn.lock().await;
        let keyspace = escape_literal(&namespace.database);

        let tables = Self::fetch_all(
            &mut conn,
            &format!(
                "SELECT table_name FROM system_schema.tables WHERE keyspace_name = '{keyspace}'"
            ),
        )
        .await?;
        let views = Self::fetch_all(
            &mut conn,
            &format!(
                "SELECT view_name FROM system_schema.views WHERE keyspace_name = '{keyspace}'"
            ),
        )
        .await?;

        let mut collections: Vec<Collection> = (0..tables.rows.len())
            .filter_map(|i| first_text(&tables, i, 0))
            .map(|name| (name, CollectionType::Table))
            .chain(
                (0..views.rows.len())
                    .filter_map(|i| first_text(&views, i, 0))
                    .map(|name| (name, CollectionType::MaterializedView)),
            )
            .filter(|(name, _)| match options.search.as_deref() {
                Some(term) if !term.is_empty() => {
                    name.to_lowercase().contains(&term.to_lowercase())
                }
                _ => true,
            })
            .map(|(name, collection_type)| Collection {
                namespace: namespace.clone(),
                name,
                collection_type,
            })
            .collect();
        collections.sort_by(|a, b| a.name.cmp(&b.name));

        let total_count = collections.len() as u32;
        if let (Some(page), Some(page_size)) = (options.page, options.page_size) {
            let start = (page.saturating_sub(1) as usize).saturating_mul(page_size as usize);
            collections = collections
                .into_iter()
                .skip(start)
                .take(page_size as usize)
                .collect();
        }

        Ok(CollectionList {
            collections,
            total_count,
        })
    }

    async fn create_database(
        &self,
        session: SessionId,
        name: &str,
        options: Option<Value>,
    ) -> EngineResult<()> {
        let session = self.get_session(session).await?;
        // A keyspace carries its replication strategy, and there is no sane
        // default for a real cluster. SimpleStrategy with one replica is the
        // development default; anything else has to be stated.
        let replication = match options.as_ref() {
            Some(Value::Text(text)) if !text.trim().is_empty() => text.trim().to_string(),
            _ => "{'class': 'SimpleStrategy', 'replication_factor': 1}".to_string(),
        };
        let cql = format!(
            "CREATE KEYSPACE IF NOT EXISTS {} WITH replication = {replication}",
            quote_identifier(name)
        );
        Self::guard(&session, &cql)?;
        let mut conn = session.conn.lock().await;
        conn.query(&cql, None, None).await?;
        Ok(())
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        let session = self.get_session(session).await?;
        let cql = format!("DROP KEYSPACE {}", quote_identifier(name));
        Self::guard(&session, &cql)?;
        let mut conn = session.conn.lock().await;
        conn.query(&cql, None, None).await?;
        Ok(())
    }

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        _query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let session = self.get_session(session).await?;
        Self::guard(&session, query)?;
        let started = Instant::now();
        let mut conn = session.conn.lock().await;

        // A single statement only. Splitting on `;` here would mean parsing CQL
        // string literals, and running half a script before failing on the rest
        // is worse than asking for one statement.
        let trimmed = query.trim().trim_end_matches(';');
        if trimmed.contains(';') {
            return Err(EngineError::validation(
                "Run one CQL statement at a time; this driver does not split scripts",
            ));
        }

        // `?` on the call itself would escape the rewrite below, so the error
        // is mapped at every point it can surface.
        let first = conn
            .query(trimmed, Some(SERVER_PAGE_SIZE), None)
            .await
            .map_err(explain_filtering_error)?;

        match first {
            CqlResult::Rows(mut acc) => {
                let mut state = acc.paging_state.clone();
                while let Some(cursor) = state {
                    if acc.rows.len() > MAX_EXECUTE_ROWS {
                        return Err(EngineError::result_too_large(
                            acc.rows.len() as u64,
                            MAX_EXECUTE_ROWS as u64,
                        ));
                    }
                    let page = conn
                        .query(trimmed, Some(SERVER_PAGE_SIZE), Some(&cursor))
                        .await
                        .map_err(explain_filtering_error)?
                        .into_rows();
                    state = page.paging_state.clone();
                    acc.rows.extend(page.rows);
                }
                Ok(to_query_result(acc, started))
            }
            CqlResult::SetKeyspace(_)
            | CqlResult::Void
            | CqlResult::SchemaChange
            | CqlResult::Prepared(_) => Ok(empty_result(started)),
        }
    }

    async fn execute_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        if let Some(ns) = namespace {
            let handle = self.get_session(session).await?;
            let mut conn = handle.conn.lock().await;
            if conn.keyspace() != Some(ns.database.as_str()) {
                conn.use_keyspace(&ns.database).await?;
            }
        }
        self.execute(session, query, query_id).await
    }

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        let session = self.get_session(session).await?;
        let mut conn = session.conn.lock().await;
        let keyspace = escape_literal(&namespace.database);
        let table_literal = escape_literal(table);

        let rows = Self::fetch_all(
            &mut conn,
            &format!(
                "SELECT column_name, type, kind, position FROM system_schema.columns \
                 WHERE keyspace_name = '{keyspace}' AND table_name = '{table_literal}'"
            ),
        )
        .await?;

        let mut partition: Vec<(i64, String)> = Vec::new();
        let mut clustering: Vec<(i64, String)> = Vec::new();
        let mut columns: Vec<TableColumn> = Vec::new();
        for row in &rows.rows {
            let (Some(Value::Text(name)), Some(Value::Text(data_type)), Some(Value::Text(kind))) =
                (row.first(), row.get(1), row.get(2))
            else {
                continue;
            };
            let position = match row.get(3) {
                Some(Value::Int(p)) => *p,
                _ => 0,
            };
            match kind.as_str() {
                "partition_key" => partition.push((position, name.clone())),
                "clustering" => clustering.push((position, name.clone())),
                _ => {}
            }
            columns.push(TableColumn {
                name: name.clone(),
                data_type: data_type.clone(),
                // Only a primary-key column is guaranteed present.
                nullable: kind != "partition_key" && kind != "clustering",
                default_value: None,
                is_primary_key: kind == "partition_key" || kind == "clustering",
                // Cassandra has no server-assigned column; `uuid()` and `now()`
                // are functions the client calls, not a column default.
                is_auto_increment: false,
            });
        }

        if columns.is_empty() {
            return Err(EngineError::execution_error(format!(
                "Table {}.{} was not found",
                namespace.database, table
            )));
        }

        partition.sort_by_key(|(p, _)| *p);
        clustering.sort_by_key(|(p, _)| *p);
        let primary_key: Vec<String> = partition
            .into_iter()
            .chain(clustering)
            .map(|(_, name)| name)
            .collect();

        // Sort the columns so the primary key leads, in ring order, the way
        // every CQL tool shows a table.
        columns.sort_by_key(|c| {
            primary_key
                .iter()
                .position(|pk| pk == &c.name)
                .unwrap_or(usize::MAX)
        });

        Ok(TableSchema {
            columns,
            primary_key: (!primary_key.is_empty()).then_some(primary_key),
            // CQL has no foreign keys, and denormalisation is the data model.
            foreign_keys: Vec::new(),
            row_count_estimate: None,
            indexes: Vec::new(),
        })
    }

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let session = self.get_session(session).await?;
        let started = Instant::now();
        let mut conn = session.conn.lock().await;
        // The LIMIT is what keeps a preview from turning into a ring-wide scan;
        // it is never optional here.
        let limit = limit.clamp(1, 10_000);
        let cql = format!(
            "SELECT * FROM {} LIMIT {limit}",
            qualified(namespace, table)
        );
        // One protocol page wide enough for the whole preview: paging here would
        // mean a second round-trip to deliver rows the LIMIT already bounded.
        let rows = conn
            .query(&cql, Some(limit as i32), None)
            .await
            .map_err(explain_filtering_error)?
            .into_rows();
        Ok(to_query_result(rows, started))
    }

    async fn query_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        let session = self.get_session(session).await?;
        let started = Instant::now();

        if options.sort_column.is_some() {
            return Err(EngineError::not_supported(
                "CQL can only order by a clustering column, and only within a single partition. \
                 Sort from the editor with an explicit ORDER BY.",
            ));
        }
        if options.effective_search().is_some() {
            return Err(EngineError::not_supported(
                "CQL has no cross-column search. Filter on the primary key instead.",
            ));
        }

        let page = options.effective_page();
        let page_size = options.effective_page_size();
        let filters = options.filters.clone().unwrap_or_default();

        let mut predicates: Vec<String> = Vec::new();
        let mut bound: Vec<Value> = Vec::new();
        for filter in &filters {
            let operator = match filter.operator {
                FilterOperator::Eq => "=",
                FilterOperator::Gt => ">",
                FilterOperator::Gte => ">=",
                FilterOperator::Lt => "<",
                FilterOperator::Lte => "<=",
                other => {
                    return Err(EngineError::not_supported(format!(
                        "CQL cannot express the {other:?} filter on a primary key"
                    )));
                }
            };
            predicates.push(format!("{} {operator} ?", quote_identifier(&filter.column)));
            bound.push(filter.value.clone());
        }

        let mut cql = format!("SELECT * FROM {}", qualified(namespace, table));
        if !predicates.is_empty() {
            cql.push_str(" WHERE ");
            cql.push_str(&predicates.join(" AND "));
        }

        let cursor = match options.cursor.as_deref() {
            Some(raw) => Some(BASE64.decode(raw).map_err(|_| {
                EngineError::validation("The pagination cursor is not valid base64")
            })?),
            None => None,
        };
        // A page beyond the first without a cursor cannot be served: the paging
        // state is opaque and there is no offset to jump to.
        if cursor.is_none() && page > 1 {
            return Err(EngineError::not_supported(
                "Cassandra pages forward through an opaque cursor; jumping to a page number \
                 would require re-reading every page before it.",
            ));
        }

        let mut conn = session.conn.lock().await;
        let result = if bound.is_empty() {
            conn.query(&cql, Some(page_size as i32), cursor.as_deref())
                .await
        } else {
            conn.execute(&cql, &bound, Some(page_size as i32), cursor.as_deref())
                .await
        }
        .map_err(explain_filtering_error)?;

        let rows = result.into_rows();
        let next_cursor = rows.paging_state.clone().map(|s| BASE64.encode(s));
        let query_result = to_query_result(rows, started);

        // No total: `SELECT COUNT(*)` on a Cassandra table is a ring-wide scan,
        // which is exactly what the driver exists to avoid issuing implicitly.
        let mut paginated =
            PaginatedQueryResult::from_optional_total(query_result, None, page, page_size);
        paginated.has_more = next_cursor.is_some();
        Ok(paginated.with_keyset(next_cursor))
    }

    async fn insert_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let handle = self.get_session(session).await?;
        let started = Instant::now();
        let mut conn = handle.conn.lock().await;

        let key = Self::primary_key(&mut conn, &namespace.database, table).await?;
        let missing: Vec<&String> = key
            .iter()
            .filter(|col| !data.columns.contains_key(*col))
            .collect();
        if !missing.is_empty() {
            return Err(EngineError::validation(format!(
                "A row needs its full primary key: {} is missing",
                missing
                    .iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }

        let mut names: Vec<String> = Vec::new();
        let mut values: Vec<Value> = Vec::new();
        for (column, value) in &data.columns {
            names.push(quote_identifier(column));
            values.push(value.clone());
        }
        let placeholders = vec!["?"; names.len()].join(", ");
        let cql = format!(
            "INSERT INTO {} ({}) VALUES ({placeholders})",
            qualified(namespace, table),
            names.join(", ")
        );
        Self::guard(&handle, &cql)?;
        conn.execute(&cql, &values, None, None).await?;
        Ok(empty_result(started))
    }

    async fn update_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let handle = self.get_session(session).await?;
        let started = Instant::now();
        let mut conn = handle.conn.lock().await;

        let key = Self::primary_key(&mut conn, &namespace.database, table).await?;
        // An UPDATE that does not pin every key column would rewrite an
        // unbounded set of rows, and Cassandra has no way to preview which.
        let missing: Vec<&String> = key
            .iter()
            .filter(|col| !primary_key.columns.contains_key(*col))
            .collect();
        if !missing.is_empty() {
            return Err(EngineError::validation(format!(
                "An update needs the full primary key: {} is missing",
                missing
                    .iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }

        let assignments: Vec<String> = data
            .columns
            .keys()
            .filter(|c| !key.contains(c))
            .map(|c| format!("{} = ?", quote_identifier(c)))
            .collect();
        if assignments.is_empty() {
            return Err(EngineError::validation(
                "Nothing to update: a primary-key column cannot be changed in place, \
                 it has to be deleted and re-inserted",
            ));
        }
        let mut values: Vec<Value> = data
            .columns
            .iter()
            .filter(|(c, _)| !key.contains(c))
            .map(|(_, v)| v.clone())
            .collect();

        let conditions: Vec<String> = key
            .iter()
            .map(|c| format!("{} = ?", quote_identifier(c)))
            .collect();
        for column in &key {
            values.push(primary_key.columns[column].clone());
        }

        let cql = format!(
            "UPDATE {} SET {} WHERE {}",
            qualified(namespace, table),
            assignments.join(", "),
            conditions.join(" AND ")
        );
        Self::guard(&handle, &cql)?;
        conn.execute(&cql, &values, None, None).await?;
        Ok(empty_result(started))
    }

    async fn delete_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
    ) -> EngineResult<QueryResult> {
        let handle = self.get_session(session).await?;
        let started = Instant::now();
        let mut conn = handle.conn.lock().await;

        let key = Self::primary_key(&mut conn, &namespace.database, table).await?;
        let missing: Vec<&String> = key
            .iter()
            .filter(|col| !primary_key.columns.contains_key(*col))
            .collect();
        if !missing.is_empty() {
            return Err(EngineError::validation(format!(
                "A delete needs the full primary key: {} is missing",
                missing
                    .iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }

        let conditions: Vec<String> = key
            .iter()
            .map(|c| format!("{} = ?", quote_identifier(c)))
            .collect();
        let values: Vec<Value> = key.iter().map(|c| primary_key.columns[c].clone()).collect();

        let cql = format!(
            "DELETE FROM {} WHERE {}",
            qualified(namespace, table),
            conditions.join(" AND ")
        );
        Self::guard(&handle, &cql)?;
        conn.execute(&cql, &values, None, None).await?;
        Ok(empty_result(started))
    }

    fn cancel_support(&self) -> CancelSupport {
        // The protocol has no cancel frame; a statement runs to completion or
        // times out. Claiming best-effort would promise something we cannot do.
        CancelSupport::None
    }

    fn supports_transactions(&self) -> bool {
        // Lightweight transactions are a compare-and-set on one partition, not
        // a multi-statement transaction, and nothing here can BEGIN one.
        false
    }

    fn supports_mutations(&self) -> bool {
        true
    }

    fn supports_schema(&self) -> bool {
        true
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn supports_explain(&self) -> bool {
        false
    }

    fn supports_maintenance(&self) -> bool {
        false
    }

    fn pagination_capability(&self) -> PaginationCapability {
        PaginationCapability {
            // The paging state is a server-side cursor over the ring. It needs
            // no unique key of ours, which no other driver here can say.
            keyset: true,
            requires_unique_key: false,
            supports_backward: false,
            snapshot: qore_core::types::SnapshotSupport::None,
            max_offset_window: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_flavors_have_distinct_identities_and_share_capabilities() {
        let cassandra = CassandraDriver::new();
        let scylla = CassandraDriver::scylladb();

        assert_eq!(cassandra.driver_id(), "cassandra");
        assert_eq!(cassandra.driver_name(), "Cassandra");
        assert_eq!(scylla.driver_id(), "scylladb");
        assert_eq!(scylla.driver_name(), "ScyllaDB");
        assert_eq!(scylla.capabilities(), cassandra.capabilities());
    }

    #[test]
    fn capabilities_state_what_cql_actually_offers() {
        let driver = CassandraDriver::new();
        assert!(!driver.supports_transactions(), "LWT are not transactions");
        assert!(!driver.supports_explain());
        assert!(!driver.supports_maintenance());
        assert!(driver.supports_mutations());
        assert!(driver.supports_schema());
        assert_eq!(driver.cancel_support(), CancelSupport::None);

        // The native paging state is a cursor that needs no unique key of ours.
        let pagination = driver.pagination_capability();
        assert!(pagination.keyset);
        assert!(!pagination.requires_unique_key);
        assert!(!pagination.supports_backward);
    }

    #[test]
    fn ssh_stays_available_because_the_protocol_is_a_plain_socket() {
        // Unlike the HTTPS warehouses, CQL runs on TCP 9042 and tunnels fine.
        assert!(CassandraDriver::new().supports_ssh());
    }

    #[test]
    fn identifiers_and_literals_are_escaped_on_their_own_terms() {
        let namespace = Namespace {
            database: "we\"ird".to_string(),
            schema: None,
        };
        assert_eq!(qualified(&namespace, "t\"bl"), "\"we\"\"ird\".\"t\"\"bl\"");
        assert_eq!(escape_literal("O'Brien"), "O''Brien");
    }

    #[test]
    fn the_read_only_guard_allows_only_reads() {
        let session = CassandraSession {
            conn: Mutex::new(unreachable_connection()),
            read_only: true,
            environment: "development".to_string(),
        };
        assert!(CassandraDriver::guard(&session, "SELECT * FROM t WHERE id = 1").is_ok());
        assert!(CassandraDriver::guard(&session, "INSERT INTO t (a) VALUES (1)").is_err());
        assert!(CassandraDriver::guard(&session, "TRUNCATE t").is_err());
        // An unrecognised statement is refused rather than waved through.
        assert!(CassandraDriver::guard(&session, "FLARP t").is_err());
    }

    #[test]
    fn production_refuses_the_ring_wide_statements() {
        let session = CassandraSession {
            conn: Mutex::new(unreachable_connection()),
            read_only: false,
            environment: "production".to_string(),
        };
        assert!(CassandraDriver::guard(&session, "SELECT * FROM t WHERE id = 1").is_ok());
        assert!(CassandraDriver::guard(&session, "INSERT INTO t (a) VALUES (1)").is_ok());
        assert!(CassandraDriver::guard(&session, "TRUNCATE t").is_err());
        assert!(CassandraDriver::guard(&session, "SELECT * FROM t").is_err());
        assert!(CassandraDriver::guard(&session, "SELECT * FROM t ALLOW FILTERING").is_err());
    }

    #[test]
    fn the_filtering_error_is_rewritten_into_something_actionable() {
        let raw = EngineError::execution_error(
            "Cannot execute this query as it might involve data filtering ... use ALLOW FILTERING",
        );
        let rewritten = explain_filtering_error(raw);
        assert!(
            rewritten.to_string().contains("partition key"),
            "{rewritten}"
        );

        // An unrelated error is passed through untouched.
        let other = EngineError::execution_error("unconfigured table t");
        assert_eq!(
            explain_filtering_error(other).to_string(),
            "Query execution error: unconfigured table t"
        );
    }

    /// The guard never touches the connection, so the tests above only need
    /// something that satisfies the type.
    fn unreachable_connection() -> CqlConnection {
        CqlConnection::for_tests()
    }
}

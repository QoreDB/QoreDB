// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use compact_str::CompactString;
use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::DataEngine;
use qore_core::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType, ColumnInfo,
    ConnectionConfig, FilterOperator, ForeignKey, Namespace, PaginatedQueryResult, QueryId,
    QueryResult, Row, RowData, SessionId, SortDirection, TableColumn, TableQueryOptions,
    TableSchema, Value,
};
use tokio::sync::{Mutex, RwLock};

use super::client::{BigQueryClient, JobRef, QueryRequest};
use super::response::{Param, param};

const MAX_ROWS: usize = 200_000;
/// Listing every dataset of every project the account can see is one
/// request per project; past this many it is a directory, not a tree.
const MAX_PROJECTS: usize = 50;

pub struct BigQueryDriver {
    sessions: RwLock<HashMap<SessionId, Arc<BigQueryClient>>>,
    queries: Mutex<HashMap<QueryId, (SessionId, JobRef)>>,
}

impl Default for BigQueryDriver {
    fn default() -> Self {
        Self::new()
    }
}

struct Binder {
    params: Vec<Param>,
}

impl Binder {
    fn new() -> Self {
        Self { params: Vec::new() }
    }

    fn slot(&mut self, value: &Value) -> String {
        match param(value) {
            Some(p) => {
                self.params.push(p);
                "?".to_string()
            }
            None => "NULL".to_string(),
        }
    }
}

struct Target {
    project: String,
    dataset: String,
    table: String,
}

impl Target {
    fn sql(&self) -> String {
        format!(
            "{}.{}.{}",
            quote_ident(&self.project),
            quote_ident(&self.dataset),
            quote_ident(&self.table)
        )
    }
}

fn quote_ident(raw: &str) -> String {
    format!("`{}`", raw.replace('`', "\\`"))
}

fn target(namespace: &Namespace, table: &str) -> EngineResult<Target> {
    let dataset = namespace
        .schema
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            EngineError::validation("A BigQuery table lives in a dataset; none was given")
        })?;
    Ok(Target {
        project: namespace.database.clone(),
        dataset: dataset.to_string(),
        table: table.to_string(),
    })
}

fn escape_like(term: &str) -> String {
    term.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn pk_predicate(binder: &mut Binder, primary_key: &RowData) -> EngineResult<String> {
    if primary_key.columns.is_empty() {
        return Err(EngineError::validation(
            "Primary key required for this operation",
        ));
    }
    let mut keys: Vec<&String> = primary_key.columns.keys().collect();
    keys.sort();
    Ok(keys
        .into_iter()
        .map(|k| {
            let value = &primary_key.columns[k];
            if matches!(value, Value::Null) {
                format!("{} IS NULL", quote_ident(k))
            } else {
                format!("{} = {}", quote_ident(k), binder.slot(value))
            }
        })
        .collect::<Vec<_>>()
        .join(" AND "))
}

/// `EXPLAIN <query>` has no BigQuery equivalent; a dry run answers the
/// question the button asks: what would this cost?
fn explain_target(query: &str) -> Option<&str> {
    let trimmed = query.trim_start();
    let keyword = trimmed.get(..7)?;
    let rest = trimmed.get(7..)?;
    (keyword.eq_ignore_ascii_case("EXPLAIN") && rest.starts_with(char::is_whitespace))
        .then(|| rest.trim_start())
}

impl BigQueryDriver {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            queries: Mutex::new(HashMap::new()),
        }
    }

    async fn get(&self, session: SessionId) -> EngineResult<Arc<BigQueryClient>> {
        self.sessions
            .read()
            .await
            .get(&session)
            .cloned()
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))
    }

    async fn run(
        &self,
        session: SessionId,
        sql: &str,
        params: Vec<Param>,
        default_dataset: Option<(String, String)>,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let client = self.get(session).await?;
        let started = Instant::now();
        if let Some(inner) = explain_target(sql) {
            let page = client
                .start(QueryRequest {
                    sql: inner,
                    params,
                    default_dataset,
                    dry_run: true,
                })
                .await?;
            let column = |name: &str, ty: &str| ColumnInfo {
                name: CompactString::new(name),
                data_type: CompactString::new(ty),
                nullable: true,
                masked: false,
            };
            return Ok(QueryResult {
                columns: vec![
                    column("total_bytes_processed", "INT64"),
                    column("cache_hit", "BOOL"),
                ],
                rows: vec![Row {
                    values: vec![
                        page.total_bytes_processed
                            .map(|b| Value::Int(b as i64))
                            .unwrap_or(Value::Null),
                        page.cache_hit.map(Value::Bool).unwrap_or(Value::Null),
                    ],
                }],
                affected_rows: None,
                execution_time_ms: started.elapsed().as_secs_f64() * 1000.0,
            });
        }
        let first = client
            .start(QueryRequest {
                sql,
                params,
                default_dataset,
                dry_run: false,
            })
            .await?;
        if let Some(job) = first.job.clone() {
            self.queries.lock().await.insert(query_id, (session, job));
        }
        let outcome = client.finish(first, MAX_ROWS).await;
        self.queries.lock().await.remove(&query_id);
        let page = outcome?;
        Ok(QueryResult {
            columns: page.columns,
            rows: page.rows,
            affected_rows: page.affected_rows,
            execution_time_ms: started.elapsed().as_secs_f64() * 1000.0,
        })
    }

    async fn mutate(
        &self,
        session: SessionId,
        sql: &str,
        binder: Binder,
    ) -> EngineResult<QueryResult> {
        let result = self
            .run(session, sql, binder.params, None, QueryId::new())
            .await?;
        Ok(QueryResult::with_affected_rows(
            result.affected_rows.unwrap_or(0),
            result.execution_time_ms,
        ))
    }

    async fn search_columns(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        requested: Option<&[String]>,
    ) -> EngineResult<Vec<String>> {
        if let Some(columns) = requested.filter(|c| !c.is_empty()) {
            return Ok(columns.to_vec());
        }
        let schema = self.describe_table(session, namespace, table).await?;
        Ok(schema
            .columns
            .into_iter()
            .filter(|c| c.data_type == "STRING")
            .map(|c| c.name)
            .collect())
    }
}

#[async_trait]
impl DataEngine for BigQueryDriver {
    fn driver_id(&self) -> &'static str {
        "bigquery"
    }

    fn driver_name(&self) -> &'static str {
        "BigQuery"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        let session = self.connect(config).await?;
        self.disconnect(session).await
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        let client = Arc::new(BigQueryClient::new(config)?);
        let id = SessionId::new();
        self.sessions.write().await.insert(id, client);
        if let Err(err) = self.ping(id).await {
            self.sessions.write().await.remove(&id);
            return Err(err);
        }
        Ok(id)
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        self.sessions.write().await.remove(&session);
        self.queries
            .lock()
            .await
            .retain(|_, (sid, _)| *sid != session);
        Ok(())
    }

    async fn ping(&self, session: SessionId) -> EngineResult<()> {
        // A metadata read proves the token and the project without a job.
        let client = self.get(session).await?;
        client.list_datasets(&client.project).await.map(|_| ())
    }

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let client = self.get(session).await?;
        let mut projects = client.list_projects().await.unwrap_or_default();
        if !projects.iter().any(|p| *p == client.project) {
            projects.insert(0, client.project.clone());
        }
        projects.truncate(MAX_PROJECTS);
        let mut out = Vec::new();
        for project in projects {
            let datasets = match client.list_datasets(&project).await {
                Ok(datasets) => datasets,
                // A project the account can list but not read still shows
                // up in `projects.list`; it simply has no datasets for us.
                Err(_) if project != client.project => continue,
                Err(err) => return Err(err),
            };
            out.extend(
                datasets
                    .into_iter()
                    .map(|dataset| Namespace::with_schema(project.clone(), dataset)),
            );
        }
        Ok(out)
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let client = self.get(session).await?;
        let dataset = namespace.schema.as_deref().ok_or_else(|| {
            EngineError::validation("A BigQuery dataset is required to list its tables")
        })?;
        let needle = options.search.as_deref().map(str::to_ascii_lowercase);
        let mut collections: Vec<Collection> = client
            .list_tables(&namespace.database, dataset)
            .await?
            .into_iter()
            .filter(|(name, _)| {
                needle
                    .as_ref()
                    .is_none_or(|n| name.to_ascii_lowercase().contains(n.as_str()))
            })
            .map(|(name, kind)| Collection {
                namespace: namespace.clone(),
                name,
                collection_type: match kind.as_str() {
                    "VIEW" => CollectionType::View,
                    "MATERIALIZED_VIEW" => CollectionType::MaterializedView,
                    _ => CollectionType::Table,
                },
            })
            .collect();
        collections.sort_by(|a, b| a.name.cmp(&b.name));
        let total_count = collections.len() as u32;
        if let Some(limit) = options.page_size {
            let offset = (options.page.unwrap_or(1).max(1) - 1) as usize * limit as usize;
            collections = collections
                .into_iter()
                .skip(offset)
                .take(limit as usize)
                .collect();
        }
        Ok(CollectionList {
            collections,
            total_count,
        })
    }

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        let client = self.get(session).await?;
        let target = target(namespace, table)?;
        let info = client
            .get_table(&target.project, &target.dataset, &target.table)
            .await?;
        let primary_key: Vec<String> = info
            .table_constraints
            .primary_key
            .map(|pk| pk.columns)
            .unwrap_or_default();
        let columns = info
            .schema
            .fields
            .iter()
            .map(|f| TableColumn {
                name: f.name.clone(),
                data_type: f.declared_type(),
                nullable: f.nullable(),
                default_value: None,
                is_primary_key: primary_key.contains(&f.name),
                is_auto_increment: false,
            })
            .collect();
        let foreign_keys = info
            .table_constraints
            .foreign_keys
            .into_iter()
            .flat_map(|fk| {
                let referenced = fk.referenced_table;
                fk.column_references.into_iter().map(move |cr| ForeignKey {
                    column: cr.referencing_column,
                    referenced_table: referenced.table_id.clone(),
                    referenced_column: cr.referenced_column,
                    referenced_schema: referenced.dataset_id.clone(),
                    referenced_database: referenced.project_id.clone(),
                    constraint_name: fk.name.clone(),
                    is_virtual: false,
                })
            })
            .collect();
        Ok(TableSchema {
            columns,
            primary_key: (!primary_key.is_empty()).then_some(primary_key),
            foreign_keys,
            row_count_estimate: info.num_rows.and_then(|n| n.parse().ok()),
            indexes: Vec::new(),
        })
    }

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        self.run(session, query, Vec::new(), None, query_id).await
    }

    async fn execute_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let default_dataset =
            namespace.and_then(|ns| ns.schema.map(|schema| (ns.database, schema)));
        self.run(session, query, Vec::new(), default_dataset, query_id)
            .await
    }

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let client = self.get(session).await?;
        let target = target(namespace, table)?;
        let started = Instant::now();
        let info = client
            .get_table(&target.project, &target.dataset, &target.table)
            .await?;
        let data = client
            .table_data(
                &target.project,
                &target.dataset,
                &target.table,
                0,
                limit.clamp(1, 10_000),
            )
            .await?;
        Ok(QueryResult {
            columns: info.schema.fields.iter().map(|f| f.column()).collect(),
            rows: data.into_rows(&info.schema.fields)?,
            affected_rows: None,
            execution_time_ms: started.elapsed().as_secs_f64() * 1000.0,
        })
    }

    async fn query_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        let target = target(namespace, table)?;
        let page = options.effective_page();
        let page_size = options.effective_page_size();
        let fetch_size = options.fetch_size();
        let offset = page.saturating_sub(1) as u64 * page_size as u64;

        let has_filters = options.filters.as_ref().is_some_and(|f| !f.is_empty());
        let has_search = options
            .search
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());
        if !has_filters && !has_search && options.sort_column.is_none() {
            // Storage order, straight from `tabledata.list`: free, and the
            // total comes with it.
            let client = self.get(session).await?;
            let started = Instant::now();
            let info = client
                .get_table(&target.project, &target.dataset, &target.table)
                .await?;
            let data = client
                .table_data(
                    &target.project,
                    &target.dataset,
                    &target.table,
                    offset,
                    fetch_size,
                )
                .await?;
            let total = data.total_rows();
            let result = QueryResult {
                columns: info.schema.fields.iter().map(|f| f.column()).collect(),
                rows: data.into_rows(&info.schema.fields)?,
                affected_rows: None,
                execution_time_ms: started.elapsed().as_secs_f64() * 1000.0,
            };
            return Ok(PaginatedQueryResult::from_optional_total(
                result, total, page, page_size,
            ));
        }

        let mut binder = Binder::new();
        let mut predicates = Vec::new();
        for filter in options.filters.iter().flatten() {
            let column = quote_ident(&filter.column);
            let predicate = match (&filter.operator, &filter.value) {
                (FilterOperator::IsNull, _) | (FilterOperator::Eq, Value::Null) => {
                    format!("{column} IS NULL")
                }
                (FilterOperator::IsNotNull, _) | (FilterOperator::Neq, Value::Null) => {
                    format!("{column} IS NOT NULL")
                }
                (FilterOperator::Eq, v) => format!("{column} = {}", binder.slot(v)),
                (FilterOperator::Neq, v) => format!("{column} != {}", binder.slot(v)),
                (FilterOperator::Gt, v) => format!("{column} > {}", binder.slot(v)),
                (FilterOperator::Gte, v) => format!("{column} >= {}", binder.slot(v)),
                (FilterOperator::Lt, v) => format!("{column} < {}", binder.slot(v)),
                (FilterOperator::Lte, v) => format!("{column} <= {}", binder.slot(v)),
                (FilterOperator::Like, v) => format!("{column} LIKE {}", binder.slot(v)),
                (other, _) => {
                    return Err(EngineError::not_supported(format!(
                        "Filter {other:?} is not supported on BigQuery"
                    )));
                }
            };
            predicates.push(predicate);
        }
        if let Some(term) = options.search.as_deref().filter(|t| !t.trim().is_empty()) {
            let columns = self
                .search_columns(session, namespace, table, options.search_columns.as_deref())
                .await?;
            if !columns.is_empty() {
                let pattern =
                    Value::Text(format!("%{}%", escape_like(&term.trim().to_lowercase())));
                let clauses: Vec<String> = columns
                    .iter()
                    .map(|c| format!("LOWER({}) LIKE {}", quote_ident(c), binder.slot(&pattern)))
                    .collect();
                predicates.push(format!("({})", clauses.join(" OR ")));
            }
        }
        let where_clause = if predicates.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", predicates.join(" AND "))
        };
        let table_sql = target.sql();

        let total = if options.wants_any_total() {
            let count = self
                .run(
                    session,
                    &format!("SELECT COUNT(*) FROM {table_sql}{where_clause}"),
                    binder.params.clone(),
                    None,
                    options.query_id.unwrap_or_default(),
                )
                .await?;
            Some(
                count
                    .rows
                    .into_iter()
                    .next()
                    .and_then(|r| match r.values.into_iter().next() {
                        Some(Value::Int(n)) if n >= 0 => Some(n as u64),
                        _ => None,
                    })
                    .unwrap_or(0),
            )
        } else {
            None
        };

        let mut sql = format!("SELECT * FROM {table_sql}{where_clause}");
        if let Some(column) = options.sort_column.as_deref() {
            let direction = match options.sort_direction {
                Some(SortDirection::Desc) => "DESC",
                _ => "ASC",
            };
            sql.push_str(&format!(" ORDER BY {} {direction}", quote_ident(column)));
        }
        sql.push_str(&format!(" LIMIT {fetch_size} OFFSET {offset}"));
        let result = self
            .run(
                session,
                &sql,
                binder.params,
                None,
                options.query_id.unwrap_or_default(),
            )
            .await?;
        Ok(PaginatedQueryResult::from_optional_total(
            result, total, page, page_size,
        ))
    }

    async fn cancel(&self, _session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        let Some(query_id) = query_id else {
            return Ok(());
        };
        let entry = self.queries.lock().await.remove(&query_id);
        if let Some((session, job)) = entry {
            self.get(session).await?.cancel(&job).await?;
        }
        Ok(())
    }

    fn cancel_support(&self) -> CancelSupport {
        CancelSupport::Driver
    }

    fn supports_ssh(&self) -> bool {
        false
    }

    fn supports_transactions(&self) -> bool {
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

    async fn create_database(
        &self,
        session: SessionId,
        name: &str,
        _options: Option<Value>,
    ) -> EngineResult<()> {
        let client = self.get(session).await?;
        client.create_dataset(&client.project, name).await
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        let client = self.get(session).await?;
        client.delete_dataset(&client.project, name).await
    }

    async fn insert_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let target = target(namespace, table)?;
        let mut keys: Vec<&String> = data.columns.keys().collect();
        keys.sort();
        if keys.is_empty() {
            return Err(EngineError::validation(
                "BigQuery cannot insert a row without any column",
            ));
        }
        let mut binder = Binder::new();
        let columns: Vec<String> = keys.iter().map(|k| quote_ident(k)).collect();
        let values: Vec<String> = keys
            .iter()
            .map(|k| binder.slot(&data.columns[*k]))
            .collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            target.sql(),
            columns.join(", "),
            values.join(", ")
        );
        self.mutate(session, &sql, binder).await
    }

    async fn update_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        if data.columns.is_empty() {
            return Ok(QueryResult::with_affected_rows(0, 0.0));
        }
        let target = target(namespace, table)?;
        let mut keys: Vec<&String> = data.columns.keys().collect();
        keys.sort();
        let mut binder = Binder::new();
        let assignments: Vec<String> = keys
            .iter()
            .map(|k| format!("{} = {}", quote_ident(k), binder.slot(&data.columns[*k])))
            .collect();
        let predicate = pk_predicate(&mut binder, primary_key)?;
        let sql = format!(
            "UPDATE {} SET {} WHERE {predicate}",
            target.sql(),
            assignments.join(", ")
        );
        self.mutate(session, &sql, binder).await
    }

    async fn delete_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
    ) -> EngineResult<QueryResult> {
        let target = target(namespace, table)?;
        let mut binder = Binder::new();
        let predicate = pk_predicate(&mut binder, primary_key)?;
        let sql = format!("DELETE FROM {} WHERE {predicate}", target.sql());
        self.mutate(session, &sql, binder).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn explain_only_dry_runs_in_the_selected_namespace_without_polling() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "test-token", "expires_in": 3600
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/bigquery/v2/projects/bill/queries"))
            .and(body_partial_json(serde_json::json!({
                "query": "SELECT * FROM orders",
                "dryRun": true,
                "useLegacySql": false,
                "defaultDataset": {"projectId": "data-project", "datasetId": "sales"}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jobComplete": true, "totalBytesProcessed": "1024"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let driver = BigQueryDriver::new();
        let session = SessionId::new();
        driver
            .sessions
            .write()
            .await
            .insert(session, Arc::new(BigQueryClient::for_tests(&server.uri())));
        let result = driver
            .execute_in_namespace(
                session,
                Some(Namespace::with_schema("data-project", "sales")),
                "EXPLAIN\nSELECT * FROM orders",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert!(matches!(result.rows[0].values[0], Value::Int(1024)));
        assert!(matches!(result.rows[0].values[1], Value::Null));
        assert!(driver.queries.lock().await.is_empty());
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[test]
    fn identifiers_are_backticked_and_fully_qualified() {
        assert_eq!(quote_ident("orders"), "`orders`");
        assert_eq!(quote_ident("a`b"), "`a\\`b`");
        let t = target(&Namespace::with_schema("proj", "sales"), "orders").unwrap();
        assert_eq!(t.sql(), "`proj`.`sales`.`orders`");
        assert!(target(&Namespace::new("proj"), "orders").is_err());
    }

    #[test]
    fn explain_is_recognised_case_insensitively() {
        assert_eq!(explain_target("explain SELECT 1"), Some("SELECT 1"));
        assert_eq!(explain_target("  EXPLAIN\tSELECT 1"), Some("SELECT 1"));
        assert_eq!(explain_target("EXPLAIN\nSELECT 1"), Some("SELECT 1"));
        assert_eq!(explain_target("EXPLAINED"), None);
        assert_eq!(explain_target("SELECT 1"), None);
    }

    #[test]
    fn the_primary_key_predicate_binds_in_order() {
        let mut pk = RowData::new();
        pk.columns.insert("id".into(), Value::Int(7));
        pk.columns.insert("region".into(), Value::Null);
        let mut binder = Binder::new();
        assert_eq!(
            pk_predicate(&mut binder, &pk).unwrap(),
            "`id` = ? AND `region` IS NULL"
        );
        assert_eq!(binder.params.len(), 1);
        assert!(pk_predicate(&mut Binder::new(), &RowData::new()).is_err());
    }
}

// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::DataEngine;
use qore_core::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType,
    ConnectionConfig, FilterOperator, ForeignKey, Namespace, PaginatedQueryResult, QueryId,
    QueryResult, Row, RowData, SessionId, SortDirection, TableColumn, TableQueryOptions,
    TableSchema, Value,
};
use tokio::sync::{Mutex, RwLock};

use super::client::{Context, SnowflakeClient};
use super::response::{Bindings, bind};

/// Past this the grid is not a grid any more, and every row was paid for
/// in warehouse credits.
const MAX_ROWS: usize = 200_000;

pub struct SnowflakeDriver {
    sessions: RwLock<HashMap<SessionId, Arc<SnowflakeClient>>>,
    queries: Mutex<HashMap<QueryId, (SessionId, String)>>,
}

impl Default for SnowflakeDriver {
    fn default() -> Self {
        Self::new()
    }
}

/// Collects `?` placeholders and their positional bindings as a statement
/// is assembled; a NULL is written inline since the API cannot bind one.
struct Binder {
    bindings: Bindings,
}

impl Binder {
    fn new() -> Self {
        Self {
            bindings: Bindings::new(),
        }
    }

    fn slot(&mut self, value: &Value) -> String {
        match bind(value) {
            Some(binding) => {
                let key = (self.bindings.len() + 1).to_string();
                self.bindings.insert(key, binding);
                "?".to_string()
            }
            None => "NULL".to_string(),
        }
    }

    fn finish(self) -> Option<Bindings> {
        (!self.bindings.is_empty()).then_some(self.bindings)
    }
}

impl SnowflakeDriver {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            queries: Mutex::new(HashMap::new()),
        }
    }

    async fn get(&self, session: SessionId) -> EngineResult<Arc<SnowflakeClient>> {
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
        bindings: Option<Bindings>,
        context: &Context,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let client = self.get(session).await?;
        let started = Instant::now();
        let handle = client.submit(sql, bindings.as_ref(), context).await?;
        self.queries
            .lock()
            .await
            .insert(query_id, (session, handle.clone()));
        let outcome = client.wait(&handle, MAX_ROWS).await;
        self.queries.lock().await.remove(&query_id);
        let body = outcome?;
        Ok(QueryResult {
            columns: body.columns,
            rows: body.rows,
            affected_rows: None,
            execution_time_ms: started.elapsed().as_secs_f64() * 1000.0,
        })
    }

    async fn run_here(&self, session: SessionId, sql: &str) -> EngineResult<QueryResult> {
        self.run(session, sql, None, &Context::default(), QueryId::new())
            .await
    }

    async fn mutate(
        &self,
        session: SessionId,
        sql: &str,
        binder: Binder,
    ) -> EngineResult<QueryResult> {
        let result = self
            .run(
                session,
                sql,
                binder.finish(),
                &Context::default(),
                QueryId::new(),
            )
            .await?;
        // A DML answer is one row holding the count under a column such as
        // "number of rows inserted".
        let affected = result
            .rows
            .first()
            .and_then(|r| r.values.first())
            .and_then(|v| match v {
                Value::Int(n) if *n >= 0 => Some(*n as u64),
                _ => None,
            })
            .unwrap_or(0);
        Ok(QueryResult::with_affected_rows(
            affected,
            result.execution_time_ms,
        ))
    }

    /// Columns the free-text search runs on: the requested ones, or the
    /// textual columns of the table when the grid did not choose.
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
            .filter(|c| {
                let ty = c.data_type.to_ascii_uppercase();
                ty.starts_with("VARCHAR") || ty.starts_with("TEXT") || ty.starts_with("STRING")
            })
            .map(|c| c.name)
            .collect())
    }
}

fn quote_ident(raw: &str) -> String {
    format!("\"{}\"", raw.replace('"', "\"\""))
}

fn qualified(namespace: &Namespace, table: &str) -> EngineResult<String> {
    let schema = namespace
        .schema
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            EngineError::validation("A Snowflake table lives in a schema; none was given")
        })?;
    Ok(format!(
        "{}.{}.{}",
        quote_ident(&namespace.database),
        quote_ident(schema),
        quote_ident(table)
    ))
}

fn escape_like(term: &str) -> String {
    term.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// `SHOW` and `DESCRIBE` answer with named columns whose order is not a
/// contract; rows are read by name.
struct Named<'a> {
    columns: HashMap<String, usize>,
    row: &'a Row,
}

impl<'a> Named<'a> {
    fn index(result: &QueryResult) -> HashMap<String, usize> {
        result
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.to_ascii_lowercase().to_string(), i))
            .collect()
    }

    fn text(&self, column: &str) -> Option<&'a str> {
        match self.columns.get(column).map(|i| &self.row.values[*i]) {
            Some(Value::Text(s)) => Some(s.as_str()),
            _ => None,
        }
    }
}

fn each_named(result: &QueryResult) -> impl Iterator<Item = Named<'_>> {
    let columns = Named::index(result);
    result.rows.iter().map(move |row| Named {
        columns: columns.clone(),
        row,
    })
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

#[async_trait]
impl DataEngine for SnowflakeDriver {
    fn driver_id(&self) -> &'static str {
        "snowflake"
    }

    fn driver_name(&self) -> &'static str {
        "Snowflake"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        let session = self.connect(config).await?;
        self.disconnect(session).await
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        let client = Arc::new(SnowflakeClient::new(config)?);
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
        // Metadata statements run without a warehouse, so a missing or
        // suspended one does not masquerade as a bad credential.
        self.run_here(session, "SELECT CURRENT_VERSION()")
            .await
            .map(|_| ())
    }

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let client = self.get(session).await?;
        let sql = match client.database.as_deref() {
            Some(db) => format!("SHOW SCHEMAS IN DATABASE {}", quote_ident(db)),
            None => "SHOW SCHEMAS IN ACCOUNT".to_string(),
        };
        let result = self.run_here(session, &sql).await?;
        Ok(each_named(&result)
            .filter_map(|row| {
                let name = row.text("name")?;
                let database = row.text("database_name")?;
                (name != "INFORMATION_SCHEMA").then(|| Namespace::with_schema(database, name))
            })
            .collect())
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let schema = namespace.schema.as_deref().ok_or_else(|| {
            EngineError::validation("A Snowflake schema is required to list its tables")
        })?;
        let sql = format!(
            "SHOW TERSE OBJECTS IN SCHEMA {}.{}",
            quote_ident(&namespace.database),
            quote_ident(schema)
        );
        let result = self.run_here(session, &sql).await?;
        let needle = options.search.as_deref().map(str::to_ascii_lowercase);
        let mut collections: Vec<Collection> = each_named(&result)
            .filter_map(|row| {
                let name = row.text("name")?;
                if let Some(needle) = &needle
                    && !name.to_ascii_lowercase().contains(needle.as_str())
                {
                    return None;
                }
                let collection_type = match row.text("kind").unwrap_or("TABLE") {
                    "VIEW" => CollectionType::View,
                    "MATERIALIZED_VIEW" => CollectionType::MaterializedView,
                    _ => CollectionType::Table,
                };
                Some(Collection {
                    namespace: namespace.clone(),
                    name: name.to_string(),
                    collection_type,
                })
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
        let target = qualified(namespace, table)?;
        let described = match self
            .run_here(session, &format!("DESCRIBE TABLE {target}"))
            .await
        {
            Ok(result) => result,
            Err(_) => {
                self.run_here(session, &format!("DESCRIBE VIEW {target}"))
                    .await?
            }
        };
        let mut columns = Vec::new();
        let mut primary_key = Vec::new();
        for row in each_named(&described) {
            let Some(name) = row.text("name") else {
                continue;
            };
            let is_pk = row.text("primary key") == Some("Y");
            if is_pk {
                primary_key.push(name.to_string());
            }
            columns.push(TableColumn {
                name: name.to_string(),
                data_type: row.text("type").unwrap_or("").to_string(),
                nullable: row.text("null?") != Some("N"),
                default_value: row
                    .text("default")
                    .filter(|d| !d.is_empty())
                    .map(str::to_string),
                is_primary_key: is_pk,
                is_auto_increment: row
                    .text("default")
                    .is_some_and(|d| d.to_ascii_uppercase().contains("IDENTITY")),
            });
        }
        if columns.is_empty() {
            return Err(EngineError::execution_error(format!(
                "Table {target} not found"
            )));
        }

        let foreign_keys = self
            .run_here(session, &format!("SHOW IMPORTED KEYS IN TABLE {target}"))
            .await
            .map(|result| {
                each_named(&result)
                    .filter_map(|row| {
                        Some(ForeignKey {
                            column: row.text("fk_column_name")?.to_string(),
                            referenced_table: row.text("pk_table_name")?.to_string(),
                            referenced_column: row.text("pk_column_name")?.to_string(),
                            referenced_schema: row.text("pk_schema_name").map(str::to_string),
                            referenced_database: row.text("pk_database_name").map(str::to_string),
                            constraint_name: row.text("fk_name").map(str::to_string),
                            is_virtual: false,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // The row count is a catalog read but a SQL one, so it needs a
        // warehouse; without one the schema is still worth showing.
        let mut binder = Binder::new();
        let count_sql = format!(
            "SELECT ROW_COUNT FROM {}.INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA = {} AND TABLE_NAME = {}",
            quote_ident(&namespace.database),
            binder.slot(&Value::Text(namespace.schema.clone().unwrap_or_default())),
            binder.slot(&Value::Text(table.to_string()))
        );
        let row_count_estimate = self
            .run(
                session,
                &count_sql,
                binder.finish(),
                &Context::default(),
                QueryId::new(),
            )
            .await
            .ok()
            .and_then(|r| r.rows.into_iter().next())
            .and_then(|r| match r.values.into_iter().next() {
                Some(Value::Int(n)) if n >= 0 => Some(n as u64),
                _ => None,
            });

        Ok(TableSchema {
            columns,
            primary_key: (!primary_key.is_empty()).then_some(primary_key),
            foreign_keys,
            row_count_estimate,
            indexes: Vec::new(),
        })
    }

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        self.run(session, query, None, &Context::default(), query_id)
            .await
    }

    async fn execute_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let context = namespace
            .map(|ns| Context {
                database: Some(ns.database),
                schema: ns.schema,
            })
            .unwrap_or_default();
        self.run(session, query, None, &context, query_id).await
    }

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let limit = limit.clamp(1, 10_000);
        let sql = format!(
            "SELECT * FROM {} LIMIT {limit}",
            qualified(namespace, table)?
        );
        self.run_here(session, &sql).await
    }

    async fn query_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        let target = qualified(namespace, table)?;
        let page = options.effective_page();
        let page_size = options.effective_page_size();
        let fetch_size = options.fetch_size();
        let offset = page.saturating_sub(1) as u64 * page_size as u64;

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
                (FilterOperator::Neq, v) => format!("{column} <> {}", binder.slot(v)),
                (FilterOperator::Gt, v) => format!("{column} > {}", binder.slot(v)),
                (FilterOperator::Gte, v) => format!("{column} >= {}", binder.slot(v)),
                (FilterOperator::Lt, v) => format!("{column} < {}", binder.slot(v)),
                (FilterOperator::Lte, v) => format!("{column} <= {}", binder.slot(v)),
                (FilterOperator::Like, v) => format!("{column} LIKE {}", binder.slot(v)),
                (other, _) => {
                    return Err(EngineError::not_supported(format!(
                        "Filter {other:?} is not supported on Snowflake"
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
                let pattern = Value::Text(format!("%{}%", escape_like(term.trim())));
                let clauses: Vec<String> = columns
                    .iter()
                    .map(|c| {
                        format!(
                            "{} ILIKE {} ESCAPE '\\\\'",
                            quote_ident(c),
                            binder.slot(&pattern)
                        )
                    })
                    .collect();
                predicates.push(format!("({})", clauses.join(" OR ")));
            }
        }
        let where_clause = if predicates.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", predicates.join(" AND "))
        };
        let bindings = binder.finish();

        let total = if options.wants_any_total() {
            let count = self
                .run(
                    session,
                    &format!("SELECT COUNT(*) FROM {target}{where_clause}"),
                    bindings.clone(),
                    &Context::default(),
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

        let mut sql = format!("SELECT * FROM {target}{where_clause}");
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
                bindings,
                &Context::default(),
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
        if let Some((session, handle)) = entry {
            self.get(session).await?.cancel(&handle).await?;
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
        let database = client.database.as_deref().ok_or_else(|| {
            EngineError::validation("Set a database on the connection to create a schema in it")
        })?;
        let sql = format!(
            "CREATE SCHEMA IF NOT EXISTS {}.{}",
            quote_ident(database),
            quote_ident(name)
        );
        self.run_here(session, &sql).await.map(|_| ())
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        let client = self.get(session).await?;
        let database = client.database.as_deref().ok_or_else(|| {
            EngineError::validation("Set a database on the connection to drop a schema in it")
        })?;
        let sql = format!(
            "DROP SCHEMA IF EXISTS {}.{}",
            quote_ident(database),
            quote_ident(name)
        );
        self.run_here(session, &sql).await.map(|_| ())
    }

    async fn insert_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let target = qualified(namespace, table)?;
        let mut keys: Vec<&String> = data.columns.keys().collect();
        keys.sort();
        let mut binder = Binder::new();
        let sql = if keys.is_empty() {
            format!("INSERT INTO {target} DEFAULT VALUES")
        } else {
            let columns: Vec<String> = keys.iter().map(|k| quote_ident(k)).collect();
            let values: Vec<String> = keys
                .iter()
                .map(|k| binder.slot(&data.columns[*k]))
                .collect();
            format!(
                "INSERT INTO {target} ({}) VALUES ({})",
                columns.join(", "),
                values.join(", ")
            )
        };
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
        let target = qualified(namespace, table)?;
        let mut keys: Vec<&String> = data.columns.keys().collect();
        keys.sort();
        let mut binder = Binder::new();
        let assignments: Vec<String> = keys
            .iter()
            .map(|k| format!("{} = {}", quote_ident(k), binder.slot(&data.columns[*k])))
            .collect();
        let predicate = pk_predicate(&mut binder, primary_key)?;
        let sql = format!(
            "UPDATE {target} SET {} WHERE {predicate}",
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
        let target = qualified(namespace, table)?;
        let mut binder = Binder::new();
        let predicate = pk_predicate(&mut binder, primary_key)?;
        let sql = format!("DELETE FROM {target} WHERE {predicate}");
        self.mutate(session, &sql, binder).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_keep_their_case_and_double_their_quotes() {
        assert_eq!(quote_ident("Orders"), "\"Orders\"");
        assert_eq!(quote_ident("a\"b"), "\"a\"\"b\"");
        let ns = Namespace::with_schema("ANALYTICS", "PUBLIC");
        assert_eq!(
            qualified(&ns, "events").unwrap(),
            "\"ANALYTICS\".\"PUBLIC\".\"events\""
        );
        assert!(qualified(&Namespace::new("ANALYTICS"), "events").is_err());
    }

    #[test]
    fn a_binder_numbers_placeholders_and_inlines_null() {
        let mut binder = Binder::new();
        assert_eq!(binder.slot(&Value::Int(1)), "?");
        assert_eq!(binder.slot(&Value::Null), "NULL");
        assert_eq!(binder.slot(&Value::Text("x".into())), "?");
        let bindings = binder.finish().unwrap();
        assert_eq!(bindings.keys().collect::<Vec<_>>(), ["1", "2"]);
        assert!(Binder::new().finish().is_none());
    }

    #[test]
    fn the_primary_key_predicate_is_sorted_and_null_aware() {
        let mut pk = RowData::new();
        pk.columns.insert("id".into(), Value::Int(7));
        pk.columns.insert("region".into(), Value::Null);
        let mut binder = Binder::new();
        assert_eq!(
            pk_predicate(&mut binder, &pk).unwrap(),
            "\"id\" = ? AND \"region\" IS NULL"
        );
        assert!(pk_predicate(&mut Binder::new(), &RowData::new()).is_err());
    }

    #[test]
    fn like_terms_escape_the_wildcards() {
        assert_eq!(escape_like("50%_a\\"), "50\\%\\_a\\\\");
    }
}

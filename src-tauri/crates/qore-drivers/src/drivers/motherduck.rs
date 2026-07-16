// SPDX-License-Identifier: Apache-2.0

//! MotherDuck Driver
//!
//! MotherDuck exposes a PostgreSQL wire-protocol endpoint
//! (`pg.<region>.motherduck.com:5432`, token as password) that runs DuckDB SQL
//! underneath. Transport therefore reuses the shared `pg_compat` helpers, but
//! schema introspection must use the DuckDB catalog (`information_schema` +
//! `duckdb_*` table functions): the vanilla PostgreSQL introspection references
//! catalogs DuckDB doesn't have (`pg_matviews`, `pg_stat_user_tables`, ...),
//! which is why a MotherDuck connection routed through the Postgres driver
//! connects yet shows an empty schema explorer.

use async_trait::async_trait;

use crate::drivers::pg_compat::{self, SessionMap};
use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::{DataEngine, StreamSender};
use qore_core::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType,
    ConnectionConfig, ForeignKey, Namespace, PaginatedQueryResult, QueryId, QueryResult, RowData,
    SessionId, TableColumn, TableIndex, TableQueryOptions, TableSchema, Value,
};

pub struct MotherDuckDriver {
    sessions: SessionMap,
}

impl MotherDuckDriver {
    pub fn new() -> Self {
        Self {
            sessions: pg_compat::new_session_map(),
        }
    }

    /// `my_db` is the personal database provisioned for every MotherDuck account;
    /// used only when the connection specifies no database.
    fn conn_str(config: &ConnectionConfig) -> String {
        pg_compat::build_pg_connection_string(config, "my_db")
    }
}

impl Default for MotherDuckDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for MotherDuckDriver {
    fn driver_id(&self) -> &'static str {
        "motherduck"
    }

    fn driver_name(&self) -> &'static str {
        "MotherDuck"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        pg_compat::test_connection(&Self::conn_str(config)).await
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        pg_compat::connect(&self.sessions, config, &Self::conn_str(config)).await
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        pg_compat::disconnect(&self.sessions, session).await
    }

    async fn ping(&self, session: SessionId) -> EngineResult<()> {
        pg_compat::ping(&self.sessions, session).await
    }

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let pool = &pg.pool;

        let current_db: (String,) = sqlx::query_as("SELECT current_database()")
            .fetch_one(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT DISTINCT schema_name
            FROM information_schema.schemata
            WHERE catalog_name = current_database()
              AND schema_name NOT IN ('information_schema', 'pg_catalog')
            ORDER BY schema_name
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(name,)| Namespace::with_schema(&current_db.0, name))
            .collect())
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let pool = &pg.pool;

        let schema = namespace.schema.as_deref().unwrap_or("main");
        let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

        let count_row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM information_schema.tables
            WHERE table_schema = $1
              AND ($2 IS NULL OR table_name LIKE $3)
            "#,
        )
        .bind(schema)
        .bind(&search_pattern)
        .bind(&search_pattern)
        .fetch_one(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let mut query_str = r#"
            SELECT table_name, table_type
            FROM information_schema.tables
            WHERE table_schema = $1
              AND ($2 IS NULL OR table_name LIKE $3)
            ORDER BY table_name
        "#
        .to_string();

        if let Some(limit) = options.page_size {
            query_str.push_str(&format!(" LIMIT {}", limit));
            if let Some(page) = options.page {
                let offset = (page.max(1) - 1) * limit;
                query_str.push_str(&format!(" OFFSET {}", offset));
            }
        }

        let rows: Vec<(String, String)> = sqlx::query_as(&query_str)
            .bind(schema)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_all(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let collections = rows
            .into_iter()
            .map(|(name, table_type)| {
                let collection_type = if table_type.to_ascii_uppercase().contains("VIEW") {
                    CollectionType::View
                } else {
                    CollectionType::Table
                };
                Collection {
                    namespace: namespace.clone(),
                    name,
                    collection_type,
                }
            })
            .collect();

        Ok(CollectionList {
            collections,
            total_count: count_row.0 as u32,
        })
    }

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let pool = &pg.pool;
        let schema = namespace.schema.as_deref().unwrap_or("main");

        let column_rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT column_name, data_type, is_nullable, column_default
            FROM information_schema.columns
            WHERE table_schema = $1 AND table_name = $2
            ORDER BY ordinal_position
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        // Constraints and indexes come from DuckDB table functions that a given
        // MotherDuck endpoint may not expose — treat them as best-effort so the
        // core column list still renders if they fail.
        let pk_columns: Vec<String> = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT unnest(constraint_column_names)
            FROM duckdb_constraints()
            WHERE schema_name = $1 AND table_name = $2 AND constraint_type = 'PRIMARY KEY'
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .map(|rows| rows.into_iter().map(|(c,)| c).collect())
        .unwrap_or_default();

        let fk_rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT unnest(constraint_column_names), unnest(referenced_column_names),
                   referenced_table, constraint_name
            FROM duckdb_constraints()
            WHERE schema_name = $1 AND table_name = $2 AND constraint_type = 'FOREIGN KEY'
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let foreign_keys = fk_rows
            .into_iter()
            .map(
                |(column, referenced_column, referenced_table, constraint_name)| ForeignKey {
                    column,
                    referenced_table,
                    referenced_column,
                    referenced_schema: Some(schema.to_string()),
                    referenced_database: None,
                    constraint_name,
                    is_virtual: false,
                },
            )
            .collect();

        let idx_rows: Vec<(String, bool)> = sqlx::query_as(
            r#"
            SELECT index_name, is_unique
            FROM duckdb_indexes()
            WHERE schema_name = $1 AND table_name = $2
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let indexes = idx_rows
            .into_iter()
            .map(|(name, is_unique)| TableIndex {
                name,
                columns: Vec::new(),
                is_unique,
                is_primary: false,
                index_type: None,
            })
            .collect();

        let row_count_estimate: Option<u64> = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT estimated_size
            FROM duckdb_tables()
            WHERE schema_name = $1 AND table_name = $2
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|c| if c < 0 { None } else { Some(c as u64) });

        let columns = column_rows
            .into_iter()
            .map(
                |(name, data_type, is_nullable, default_value)| TableColumn {
                    is_primary_key: pk_columns.contains(&name),
                    name,
                    data_type,
                    nullable: is_nullable == "YES",
                    default_value,
                    is_auto_increment: false,
                },
            )
            .collect();

        Ok(TableSchema {
            columns,
            primary_key: if pk_columns.is_empty() {
                None
            } else {
                Some(pk_columns)
            },
            foreign_keys,
            row_count_estimate,
            indexes,
        })
    }

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        pg_compat::execute_in_namespace(
            &self.sessions,
            self.driver_id(),
            session,
            None,
            query,
            query_id,
        )
        .await
    }

    async fn execute_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        pg_compat::execute_in_namespace(
            &self.sessions,
            self.driver_id(),
            session,
            namespace,
            query,
            query_id,
        )
        .await
    }

    async fn execute_stream(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        pg_compat::execute_stream_in_namespace(
            &self.sessions,
            self.driver_id(),
            session,
            None,
            query,
            query_id,
            sender,
        )
        .await
    }

    async fn execute_stream_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        pg_compat::execute_stream_in_namespace(
            &self.sessions,
            self.driver_id(),
            session,
            namespace,
            query,
            query_id,
            sender,
        )
        .await
    }

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let schema = namespace.schema.as_deref().unwrap_or("main");
        let query = format!(
            "SELECT * FROM {}.{} LIMIT {}",
            pg_compat::quote_ident(schema),
            pg_compat::quote_ident(table),
            limit
        );
        self.execute(session, &query, QueryId::new()).await
    }

    async fn query_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        pg_compat::query_table(&self.sessions, session, namespace, table, options).await
    }

    async fn peek_foreign_key(
        &self,
        session: SessionId,
        namespace: &Namespace,
        foreign_key: &ForeignKey,
        value: &Value,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        pg_compat::peek_foreign_key(
            &self.sessions,
            session,
            namespace,
            foreign_key,
            value,
            limit,
        )
        .await
    }

    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        pg_compat::cancel(&self.sessions, session, query_id).await
    }

    fn cancel_support(&self) -> CancelSupport {
        pg_compat::cancel_support()
    }

    async fn begin_transaction(&self, session: SessionId) -> EngineResult<()> {
        pg_compat::begin_transaction(&self.sessions, session).await
    }

    async fn commit(&self, session: SessionId) -> EngineResult<()> {
        pg_compat::commit(&self.sessions, session).await
    }

    async fn rollback(&self, session: SessionId) -> EngineResult<()> {
        pg_compat::rollback(&self.sessions, session).await
    }

    fn supports_transactions(&self) -> bool {
        true
    }

    async fn insert_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        pg_compat::insert_row(&self.sessions, session, namespace, table, data).await
    }

    async fn update_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        pg_compat::update_row(&self.sessions, session, namespace, table, primary_key, data).await
    }

    async fn delete_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
    ) -> EngineResult<QueryResult> {
        pg_compat::delete_row(&self.sessions, session, namespace, table, primary_key).await
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
        pg_compat::create_schema(&self.sessions, session, name, "MotherDuck").await
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        pg_compat::drop_schema(&self.sessions, session, name, "MotherDuck").await
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_explain(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> ConnectionConfig {
        ConnectionConfig {
            driver: "motherduck".to_string(),
            host: "pg.us-east-1-aws.motherduck.com".to_string(),
            port: 5432,
            username: "postgres".to_string(),
            password: "md_token".to_string(),
            database: None,
            ssl: true,
            ssl_mode: Some("verify-full".to_string()),
            environment: "production".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
        }
    }

    #[test]
    fn motherduck_driver_identity() {
        let d = MotherDuckDriver::new();
        assert_eq!(d.driver_id(), "motherduck");
        assert_eq!(d.driver_name(), "MotherDuck");
    }

    #[test]
    fn motherduck_connection_string() {
        let cfg = make_config();
        let conn = MotherDuckDriver::conn_str(&cfg);
        assert!(conn.contains("pg.us-east-1-aws.motherduck.com"));
        assert!(conn.contains(":5432"));
        assert!(conn.contains("sslmode=verify-full"));
    }

    #[test]
    fn motherduck_default_db_when_missing() {
        let cfg = make_config();
        let conn = MotherDuckDriver::conn_str(&cfg);
        assert!(conn.contains("/my_db?"));
    }
}

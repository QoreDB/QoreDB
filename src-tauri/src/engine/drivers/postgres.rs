// SPDX-License-Identifier: Apache-2.0

//! PostgreSQL Driver
//!
//! Implements the DataEngine trait for PostgreSQL databases.
//! Most of the heavy lifting is done by the shared `pg_compat` module;
//! this file only contains PostgreSQL-specific overrides (materialized views
//! in list_collections, full maintenance ops, connection string defaults).

use std::time::Instant;

use async_trait::async_trait;

use crate::engine::drivers::pg_compat::{self, SessionMap};
use crate::engine::error::{EngineError, EngineResult};
use crate::engine::traits::{DataEngine, StreamSender};
use crate::engine::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType,
    ConnectionConfig, ForeignKey, MaintenanceMessage, MaintenanceMessageLevel,
    MaintenanceOperationInfo, MaintenanceOperationType, MaintenanceRequest, MaintenanceResult,
    Namespace, PaginatedQueryResult, QueryId, QueryResult, RoutineDefinition, RoutineList,
    RoutineListOptions, RoutineOperationResult, RoutineType, RowData, SessionId, TableQueryOptions,
    TableSchema, TriggerDefinition, TriggerList, TriggerListOptions, TriggerOperationResult, Value,
};

/// PostgreSQL driver implementation
pub struct PostgresDriver {
    sessions: SessionMap,
}

impl PostgresDriver {
    pub fn new() -> Self {
        Self {
            sessions: pg_compat::new_session_map(),
        }
    }

    fn conn_str(config: &ConnectionConfig) -> String {
        pg_compat::build_pg_connection_string(config, "postgres")
    }
}

impl Default for PostgresDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for PostgresDriver {
    fn driver_id(&self) -> &'static str {
        "postgres"
    }

    fn driver_name(&self) -> &'static str {
        "PostgreSQL"
    }

    // ==================== Connection ====================

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

    // ==================== Namespaces ====================

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let pool = &pg.pool;

        let current_db: (String,) = sqlx::query_as("SELECT current_database()")
            .fetch_one(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let rows: Vec<(String,)> = sqlx::query_as(
            r#"
            SELECT nspname
            FROM pg_catalog.pg_namespace
            WHERE nspname NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
              AND nspname NOT LIKE 'pg_temp_%'
            ORDER BY nspname
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

    // ==================== Collections ====================
    // PostgreSQL-specific: includes materialized views

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let pool = &pg.pool;

        let schema = namespace.schema.as_deref().unwrap_or("public");
        let search_pattern = options.search.as_ref().map(|s| format!("%{}%", s));

        let count_row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM (
                SELECT table_name AS name
                FROM information_schema.tables
                WHERE table_schema = $1
                AND ($2 IS NULL OR table_name LIKE $3)
                UNION ALL
                SELECT matviewname AS name
                FROM pg_matviews
                WHERE schemaname = $1
                AND ($2 IS NULL OR matviewname LIKE $3)
            ) combined
            "#,
        )
        .bind(schema)
        .bind(&search_pattern)
        .bind(&search_pattern)
        .fetch_one(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let mut query_str = r#"
            SELECT name, ctype FROM (
                SELECT table_name AS name,
                    CASE WHEN table_type = 'VIEW' THEN 'View' ELSE 'Table' END AS ctype
                FROM information_schema.tables
                WHERE table_schema = $1
                AND ($2 IS NULL OR table_name LIKE $3)
                UNION ALL
                SELECT matviewname AS name, 'MaterializedView' AS ctype
                FROM pg_matviews
                WHERE schemaname = $1
                AND ($2 IS NULL OR matviewname LIKE $3)
            ) combined ORDER BY name
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
            .map(|(name, ctype)| {
                let collection_type = match ctype.as_str() {
                    "View" => CollectionType::View,
                    "MaterializedView" => CollectionType::MaterializedView,
                    _ => CollectionType::Table,
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

    // ==================== Describe Table ====================

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        pg_compat::describe_table_core(&self.sessions, session, namespace, table, true).await
    }

    // ==================== Execute ====================

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

    // ==================== Preview / Query Table / Peek FK ====================

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let schema = namespace.schema.as_deref().unwrap_or("public");
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

    // ==================== Cancel ====================

    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        pg_compat::cancel(&self.sessions, session, query_id).await
    }

    fn cancel_support(&self) -> CancelSupport {
        pg_compat::cancel_support()
    }

    // ==================== Transactions ====================

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

    // ==================== Mutations ====================

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

    // ==================== Routines ====================

    fn supports_routines(&self) -> bool {
        true
    }

    async fn list_routines(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: RoutineListOptions,
    ) -> EngineResult<RoutineList> {
        pg_compat::list_routines(&self.sessions, session, namespace, options).await
    }

    async fn get_routine_definition(
        &self,
        session: SessionId,
        namespace: &Namespace,
        routine_name: &str,
        routine_type: RoutineType,
        arguments: Option<&str>,
    ) -> EngineResult<RoutineDefinition> {
        pg_compat::get_routine_definition(
            &self.sessions,
            session,
            namespace,
            routine_name,
            routine_type,
            arguments,
        )
        .await
    }

    async fn drop_routine(
        &self,
        session: SessionId,
        namespace: &Namespace,
        routine_name: &str,
        routine_type: RoutineType,
        arguments: Option<&str>,
    ) -> EngineResult<RoutineOperationResult> {
        pg_compat::drop_routine(
            &self.sessions,
            session,
            namespace,
            routine_name,
            routine_type,
            arguments,
        )
        .await
    }

    // ==================== Triggers ====================

    fn supports_triggers(&self) -> bool {
        true
    }

    async fn list_triggers(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: TriggerListOptions,
    ) -> EngineResult<TriggerList> {
        pg_compat::list_triggers(&self.sessions, session, namespace, options).await
    }

    async fn get_trigger_definition(
        &self,
        session: SessionId,
        namespace: &Namespace,
        trigger_name: &str,
    ) -> EngineResult<TriggerDefinition> {
        pg_compat::get_trigger_definition(&self.sessions, session, namespace, trigger_name).await
    }

    async fn drop_trigger(
        &self,
        session: SessionId,
        namespace: &Namespace,
        trigger_name: &str,
        table_name: &str,
    ) -> EngineResult<TriggerOperationResult> {
        pg_compat::drop_trigger(&self.sessions, session, namespace, trigger_name, table_name).await
    }

    async fn toggle_trigger(
        &self,
        session: SessionId,
        namespace: &Namespace,
        trigger_name: &str,
        table_name: &str,
        enable: bool,
    ) -> EngineResult<TriggerOperationResult> {
        pg_compat::toggle_trigger(
            &self.sessions,
            session,
            namespace,
            trigger_name,
            table_name,
            enable,
        )
        .await
    }

    // ==================== Schema operations ====================

    async fn create_database(
        &self,
        session: SessionId,
        name: &str,
        _options: Option<Value>,
    ) -> EngineResult<()> {
        pg_compat::create_schema(&self.sessions, session, name, "Postgres").await
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        pg_compat::drop_schema(&self.sessions, session, name, "Postgres").await
    }

    // ==================== Maintenance ====================
    // PostgreSQL-specific: VACUUM, ANALYZE, REINDEX, CLUSTER

    fn supports_maintenance(&self) -> bool {
        true
    }

    async fn list_maintenance_operations(
        &self,
        _session: SessionId,
        _namespace: &Namespace,
        _table: &str,
    ) -> EngineResult<Vec<MaintenanceOperationInfo>> {
        Ok(vec![
            MaintenanceOperationInfo {
                operation: MaintenanceOperationType::Vacuum,
                is_heavy: false,
                has_options: true,
            },
            MaintenanceOperationInfo {
                operation: MaintenanceOperationType::Analyze,
                is_heavy: false,
                has_options: false,
            },
            MaintenanceOperationInfo {
                operation: MaintenanceOperationType::Reindex,
                is_heavy: true,
                has_options: false,
            },
            MaintenanceOperationInfo {
                operation: MaintenanceOperationType::Cluster,
                is_heavy: true,
                has_options: true,
            },
        ])
    }

    async fn run_maintenance(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        request: &MaintenanceRequest,
    ) -> EngineResult<MaintenanceResult> {
        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let schema = namespace.schema.as_deref().unwrap_or("public");
        let qualified_table = format!(
            "{}.{}",
            pg_compat::quote_ident(schema),
            pg_compat::quote_ident(table)
        );

        let sql = match request.operation {
            MaintenanceOperationType::Vacuum => {
                let full = if request.options.full.unwrap_or(false) {
                    "FULL "
                } else {
                    ""
                };
                let analyze = if request.options.with_analyze.unwrap_or(false) {
                    "ANALYZE "
                } else {
                    ""
                };
                let verbose = if request.options.verbose.unwrap_or(false) {
                    "VERBOSE "
                } else {
                    ""
                };
                format!("VACUUM {full}{analyze}{verbose}{qualified_table}")
            }
            MaintenanceOperationType::Analyze => {
                format!("ANALYZE {qualified_table}")
            }
            MaintenanceOperationType::Reindex => {
                format!("REINDEX TABLE {qualified_table}")
            }
            MaintenanceOperationType::Cluster => {
                if let Some(ref idx) = request.options.index_name {
                    format!(
                        "CLUSTER {qualified_table} USING {}",
                        pg_compat::quote_ident(idx)
                    )
                } else {
                    format!("CLUSTER {qualified_table}")
                }
            }
            _ => {
                return Err(EngineError::not_supported(
                    "Operation not supported for PostgreSQL",
                ));
            }
        };

        let start = Instant::now();
        // VACUUM cannot run inside a transaction, so always use pool directly
        sqlx::query(&sql)
            .execute(&pg.pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

        Ok(MaintenanceResult {
            executed_command: sql,
            messages: vec![MaintenanceMessage {
                level: MaintenanceMessageLevel::Info,
                text: "Operation completed successfully".into(),
            }],
            execution_time_ms,
            success: true,
        })
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

    fn make_config(username: &str, password: &str) -> ConnectionConfig {
        ConnectionConfig {
            driver: "postgres".to_string(),
            host: "localhost".to_string(),
            port: 5432,
            username: username.to_string(),
            password: password.to_string(),
            database: Some("testdb".to_string()),
            ssl: false,
            environment: "development".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
        }
    }

    #[test]
    fn test_connection_string_building() {
        let config = make_config("user", "pass");

        let conn_str = PostgresDriver::conn_str(&config);
        assert!(conn_str.contains("localhost:5432"));
        assert!(conn_str.contains("testdb"));
        assert!(conn_str.contains("sslmode=disable"));
    }

    #[test]
    fn test_connection_string_special_chars_in_password() {
        let config = make_config("admin", "p@ss:word/123?#&=!");

        let conn_str = PostgresDriver::conn_str(&config);
        // Password must be percent-encoded so it doesn't break the URL structure
        assert!(!conn_str.contains("p@ss:word/123?#&=!"));
        assert!(conn_str.contains("p%40ss%3Aword%2F123%3F%23%26%3D%21"));
        // Host and port must remain intact
        assert!(conn_str.contains("@localhost:5432"));
    }

    #[test]
    fn test_connection_string_special_chars_in_username() {
        let config = make_config("user@domain", "pass");

        let conn_str = PostgresDriver::conn_str(&config);
        assert!(conn_str.contains("user%40domain"));
        assert!(conn_str.contains("@localhost:5432"));
    }
}

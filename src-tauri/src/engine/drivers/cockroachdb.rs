// SPDX-License-Identifier: Apache-2.0

//! CockroachDB Driver
//!
//! CockroachDB is PostgreSQL wire-compatible. This driver is a thin wrapper
//! over the shared `pg_compat` helpers, overriding only CockroachDB-specific
//! behaviour (namespace filtering, maintenance, list_collections, etc.).

use async_trait::async_trait;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::drivers::pg_compat::{self, SessionMap};
use crate::engine::traits::{DataEngine, StreamSender};
use crate::engine::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType,
    ConnectionConfig, ForeignKey, Namespace, QueryId, QueryResult, RowData, SessionId,
    TableQueryOptions, PaginatedQueryResult, TableSchema, Value,
    RoutineList, RoutineListOptions, RoutineType, RoutineDefinition, RoutineOperationResult,
    TriggerList, TriggerListOptions, TriggerDefinition, TriggerOperationResult,
    MaintenanceOperationInfo, MaintenanceOperationType, MaintenanceRequest, MaintenanceResult,
    MaintenanceMessage, MaintenanceMessageLevel,
};

/// CockroachDB driver implementation
pub struct CockroachDbDriver {
    sessions: SessionMap,
}

impl CockroachDbDriver {
    pub fn new() -> Self {
        Self {
            sessions: pg_compat::new_session_map(),
        }
    }

    fn conn_str(config: &ConnectionConfig) -> String {
        pg_compat::build_pg_connection_string(config, "defaultdb")
    }
}

impl Default for CockroachDbDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for CockroachDbDriver {
    fn driver_id(&self) -> &'static str {
        "cockroachdb"
    }

    fn driver_name(&self) -> &'static str {
        "CockroachDB"
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

    // ==================== Namespaces ====================
    // CockroachDB-specific: filter out crdb_internal, pg_extension

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
            WHERE nspname NOT IN ('information_schema', 'pg_catalog', 'pg_toast',
                                  'crdb_internal', 'pg_extension')
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
    // CockroachDB-specific: no materialized views

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
            SELECT table_name AS name,
                CASE WHEN table_type = 'VIEW' THEN 'View' ELSE 'Table' END AS ctype
            FROM information_schema.tables
            WHERE table_schema = $1
            AND ($2 IS NULL OR table_name LIKE $3)
            ORDER BY name
        "#
        .to_string();

        if let Some(limit) = options.page_size {
            query_str.push_str(&format!(" LIMIT {}", limit));
            if let Some(page) = options.page {
                query_str.push_str(&format!(" OFFSET {}", (page.max(1) - 1) * limit));
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
            .map(|(name, ctype)| Collection {
                namespace: namespace.clone(),
                name,
                collection_type: if ctype == "View" {
                    CollectionType::View
                } else {
                    CollectionType::Table
                },
            })
            .collect();

        Ok(CollectionList {
            collections,
            total_count: count_row.0 as u32,
        })
    }

    // ==================== Describe Table ====================
    // CockroachDB-specific: don't rely on pg_stat_user_tables

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        pg_compat::describe_table_core(&self.sessions, session, namespace, table, false).await
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
        pg_compat::peek_foreign_key(&self.sessions, session, namespace, foreign_key, value, limit)
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
        pg_compat::create_schema(&self.sessions, session, name, "CockroachDB").await
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        pg_compat::drop_schema(&self.sessions, session, name, "CockroachDB").await
    }

    // ==================== Maintenance ====================
    // CockroachDB-specific: only ANALYZE is supported

    fn supports_maintenance(&self) -> bool {
        true
    }

    async fn list_maintenance_operations(
        &self,
        _session: SessionId,
        _namespace: &Namespace,
        _table: &str,
    ) -> EngineResult<Vec<MaintenanceOperationInfo>> {
        Ok(vec![MaintenanceOperationInfo {
            operation: MaintenanceOperationType::Analyze,
            is_heavy: false,
            has_options: false,
        }])
    }

    async fn run_maintenance(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        request: &MaintenanceRequest,
    ) -> EngineResult<MaintenanceResult> {
        if !matches!(request.operation, MaintenanceOperationType::Analyze) {
            return Err(EngineError::not_supported(
                "CockroachDB only supports ANALYZE for maintenance operations",
            ));
        }

        let pg = pg_compat::get_session(&self.sessions, session).await?;
        let schema = namespace.schema.as_deref().unwrap_or("public");
        let qualified = format!(
            "{}.{}",
            pg_compat::quote_ident(schema),
            pg_compat::quote_ident(table)
        );

        let sql = format!("ANALYZE {qualified}");
        let start = std::time::Instant::now();

        sqlx::query(&sql)
            .execute(&pg.pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        Ok(MaintenanceResult {
            executed_command: sql,
            messages: vec![MaintenanceMessage {
                level: MaintenanceMessageLevel::Info,
                text: "Operation completed successfully".into(),
            }],
            execution_time_ms: start.elapsed().as_micros() as f64 / 1000.0,
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
            driver: "cockroachdb".to_string(),
            host: "localhost".to_string(),
            port: 26257,
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
        let conn_str = CockroachDbDriver::conn_str(&config);
        assert!(conn_str.contains("localhost:26257"));
        assert!(conn_str.contains("testdb"));
        assert!(conn_str.contains("sslmode=disable"));
    }

    #[test]
    fn test_connection_string_default_db() {
        let mut config = make_config("root", "");
        config.database = None;
        let conn_str = CockroachDbDriver::conn_str(&config);
        assert!(conn_str.contains("/defaultdb?"));
    }

    #[test]
    fn test_connection_string_special_chars_in_password() {
        let config = make_config("admin", "p@ss:word/123?#&=!");
        let conn_str = CockroachDbDriver::conn_str(&config);
        assert!(!conn_str.contains("p@ss:word/123?#&=!"));
        assert!(conn_str.contains("p%40ss%3Aword%2F123%3F%23%26%3D%21"));
        assert!(conn_str.contains("@localhost:26257"));
    }
}

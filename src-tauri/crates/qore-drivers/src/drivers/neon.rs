// SPDX-License-Identifier: Apache-2.0

//! Neon Driver
//!
//! Neon is a serverless PostgreSQL with branching. This driver is a thin
//! wrapper over the shared `pg_compat` helpers exposing a distinct
//! `driver_id` for telemetry and UI branding. Future work may add Neon
//! API integration (branch listing, endpoint suspend/resume).

use async_trait::async_trait;

use crate::drivers::pg_compat::{self, SessionMap};
use qore_core::error::EngineResult;
use qore_core::traits::{DataEngine, StreamSender};
use qore_core::types::{
    CancelSupport, CollectionList, CollectionListOptions, ConnectionConfig, ForeignKey, Namespace,
    PaginatedQueryResult, QueryId, QueryResult, RoutineDefinition, RoutineList, RoutineListOptions,
    RoutineOperationResult, RoutineType, RowData, SessionId, TableQueryOptions, TableSchema,
    TriggerDefinition, TriggerList, TriggerListOptions, TriggerOperationResult, Value,
};

pub struct NeonDriver {
    sessions: SessionMap,
}

impl NeonDriver {
    pub fn new() -> Self {
        Self {
            sessions: pg_compat::new_session_map(),
        }
    }

    fn conn_str(config: &ConnectionConfig) -> String {
        pg_compat::build_pg_connection_string(config, "neondb")
    }
}

impl Default for NeonDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for NeonDriver {
    fn driver_id(&self) -> &'static str {
        "neon"
    }

    fn driver_name(&self) -> &'static str {
        "Neon"
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
        pg_compat::list_namespaces_default(&self.sessions, session).await
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        pg_compat::list_collections_default(&self.sessions, session, namespace, options).await
    }

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        pg_compat::describe_table_core(&self.sessions, session, namespace, table, true).await
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

    async fn create_database(
        &self,
        session: SessionId,
        name: &str,
        _options: Option<Value>,
    ) -> EngineResult<()> {
        pg_compat::create_schema(&self.sessions, session, name, "Neon").await
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        pg_compat::drop_schema(&self.sessions, session, name, "Neon").await
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
            driver: "neon".to_string(),
            host: "ep-cool-darkness-123.us-east-1.aws.neon.tech".to_string(),
            port: 5432,
            username: "user".to_string(),
            password: "secret".to_string(),
            database: Some("neondb".to_string()),
            ssl: true,
            ssl_mode: Some("require".to_string()),
            environment: "production".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
            proxy: None,
            mssql_auth: None,
        }
    }

    #[test]
    fn neon_driver_identity() {
        let d = NeonDriver::new();
        assert_eq!(d.driver_id(), "neon");
        assert_eq!(d.driver_name(), "Neon");
    }

    #[test]
    fn neon_connection_string() {
        let cfg = make_config();
        let conn = NeonDriver::conn_str(&cfg);
        assert!(conn.contains(".neon.tech"));
        assert!(conn.contains("/neondb"));
        assert!(conn.contains("sslmode=require"));
    }

    #[test]
    fn neon_default_db_when_missing() {
        let mut cfg = make_config();
        cfg.database = None;
        let conn = NeonDriver::conn_str(&cfg);
        assert!(conn.contains("/neondb?"));
    }
}

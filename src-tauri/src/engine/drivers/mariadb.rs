// SPDX-License-Identifier: Apache-2.0

//! MariaDB Driver
//!
//! Thin wrapper over the MySQL driver that provides MariaDB-specific behavior.
//! MariaDB uses the same wire protocol and information_schema as MySQL, but
//! differs in system schema presence, storage engines (Aria), and some features.

use async_trait::async_trait;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::traits::{DataEngine, StreamSender};
use crate::engine::types::{
    CancelSupport, CollectionList, CollectionListOptions, ConnectionConfig, CreationOptions,
    DriverCapabilities, EventDefinition, EventList, EventListOptions, EventOperationResult,
    MaintenanceOperationInfo, MaintenanceRequest, MaintenanceResult,
    Namespace, PaginatedQueryResult, QueryId, QueryResult, RoutineDefinition, RoutineList,
    RoutineListOptions, RoutineOperationResult, RoutineType, RowData, SessionId, TableQueryOptions,
    TableSchema, TriggerDefinition, TriggerList, TriggerListOptions, TriggerOperationResult, Value,
};

use super::mysql::MySqlDriver;
use crate::engine::types::ForeignKey;

/// MariaDB driver — delegates to MySqlDriver with MariaDB-specific overrides.
pub struct MariaDbDriver {
    inner: MySqlDriver,
}

impl MariaDbDriver {
    pub fn new() -> Self {
        Self {
            inner: MySqlDriver::new(),
        }
    }
}

impl Default for MariaDbDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for MariaDbDriver {
    fn driver_id(&self) -> &'static str {
        "mariadb"
    }

    fn driver_name(&self) -> &'static str {
        "MariaDB"
    }

    // ==================== Connection ====================

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        self.inner.test_connection(config).await
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        self.inner.connect(config).await
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        self.inner.disconnect(session).await
    }

    async fn ping(&self, session: SessionId) -> EngineResult<()> {
        self.inner.ping(session).await
    }

    // ==================== Namespaces ====================

    /// MariaDB-specific namespace filtering.
    /// Unlike MySQL, MariaDB may not have `performance_schema` or `sys` enabled by default.
    /// We filter only the guaranteed system schemas.
    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let mysql_session = self.inner.get_session(session).await?;
        let pool = &mysql_session.pool;

        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT CAST(schema_name AS CHAR) FROM information_schema.schemata")
                .fetch_all(pool)
                .await
                .map_err(|e| EngineError::execution_error(e.to_string()))?;

        // MariaDB system schemas — performance_schema and sys are optional
        let system_dbs = ["information_schema", "mysql"];
        let namespaces = rows
            .into_iter()
            .map(|(db,)| db)
            .filter(|db| !system_dbs.contains(&db.as_str()))
            .map(Namespace::new)
            .collect();

        Ok(namespaces)
    }

    // ==================== Collections ====================

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        self.inner.list_collections(session, namespace, options).await
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
        self.inner.list_routines(session, namespace, options).await
    }

    async fn get_routine_definition(
        &self,
        session: SessionId,
        namespace: &Namespace,
        routine_name: &str,
        routine_type: RoutineType,
        arguments: Option<&str>,
    ) -> EngineResult<RoutineDefinition> {
        self.inner
            .get_routine_definition(session, namespace, routine_name, routine_type, arguments)
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
        self.inner
            .drop_routine(session, namespace, routine_name, routine_type, arguments)
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
        self.inner.list_triggers(session, namespace, options).await
    }

    async fn get_trigger_definition(
        &self,
        session: SessionId,
        namespace: &Namespace,
        trigger_name: &str,
    ) -> EngineResult<TriggerDefinition> {
        self.inner
            .get_trigger_definition(session, namespace, trigger_name)
            .await
    }

    async fn drop_trigger(
        &self,
        session: SessionId,
        namespace: &Namespace,
        trigger_name: &str,
        table_name: &str,
    ) -> EngineResult<TriggerOperationResult> {
        self.inner
            .drop_trigger(session, namespace, trigger_name, table_name)
            .await
    }

    // ==================== Events ====================

    fn supports_events(&self) -> bool {
        true
    }

    async fn list_events(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: EventListOptions,
    ) -> EngineResult<EventList> {
        self.inner.list_events(session, namespace, options).await
    }

    async fn get_event_definition(
        &self,
        session: SessionId,
        namespace: &Namespace,
        event_name: &str,
    ) -> EngineResult<EventDefinition> {
        self.inner
            .get_event_definition(session, namespace, event_name)
            .await
    }

    async fn drop_event(
        &self,
        session: SessionId,
        namespace: &Namespace,
        event_name: &str,
    ) -> EngineResult<EventOperationResult> {
        self.inner
            .drop_event(session, namespace, event_name)
            .await
    }

    // ==================== Database Management ====================

    async fn get_creation_options(&self, session: SessionId) -> EngineResult<CreationOptions> {
        self.inner.get_creation_options(session).await
    }

    async fn create_database(
        &self,
        session: SessionId,
        name: &str,
        options: Option<Value>,
    ) -> EngineResult<()> {
        self.inner.create_database(session, name, options).await
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        self.inner.drop_database(session, name).await
    }

    // ==================== Query Execution ====================

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        self.inner.execute(session, query, query_id).await
    }

    async fn execute_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        self.inner
            .execute_in_namespace(session, namespace, query, query_id)
            .await
    }

    async fn execute_stream(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        self.inner
            .execute_stream(session, query, query_id, sender)
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
        self.inner
            .execute_stream_in_namespace(session, namespace, query, query_id, sender)
            .await
    }

    // ==================== Table Inspection ====================

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        self.inner.describe_table(session, namespace, table).await
    }

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        self.inner
            .preview_table(session, namespace, table, limit)
            .await
    }

    async fn query_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        self.inner
            .query_table(session, namespace, table, options)
            .await
    }

    async fn peek_foreign_key(
        &self,
        session: SessionId,
        namespace: &Namespace,
        foreign_key: &ForeignKey,
        value: &Value,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        self.inner
            .peek_foreign_key(session, namespace, foreign_key, value, limit)
            .await
    }

    // ==================== Cancellation ====================

    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        self.inner.cancel(session, query_id).await
    }

    fn cancel_support(&self) -> CancelSupport {
        self.inner.cancel_support()
    }

    // ==================== Transactions ====================

    async fn begin_transaction(&self, session: SessionId) -> EngineResult<()> {
        self.inner.begin_transaction(session).await
    }

    async fn commit(&self, session: SessionId) -> EngineResult<()> {
        self.inner.commit(session).await
    }

    async fn rollback(&self, session: SessionId) -> EngineResult<()> {
        self.inner.rollback(session).await
    }

    fn supports_transactions(&self) -> bool {
        true
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_explain(&self) -> bool {
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
        self.inner.insert_row(session, namespace, table, data).await
    }

    async fn update_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        self.inner
            .update_row(session, namespace, table, primary_key, data)
            .await
    }

    async fn delete_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
    ) -> EngineResult<QueryResult> {
        self.inner
            .delete_row(session, namespace, table, primary_key)
            .await
    }

    fn supports_mutations(&self) -> bool {
        true
    }

    // ==================== Maintenance ====================
    // MariaDB supports all MySQL maintenance ops plus the Aria storage engine.

    fn supports_maintenance(&self) -> bool {
        true
    }

    async fn list_maintenance_operations(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<Vec<MaintenanceOperationInfo>> {
        self.inner
            .list_maintenance_operations(session, namespace, table)
            .await
    }

    async fn run_maintenance(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        request: &MaintenanceRequest,
    ) -> EngineResult<MaintenanceResult> {
        self.inner
            .run_maintenance(session, namespace, table, request)
            .await
    }

    // ==================== Capabilities ====================

    fn capabilities(&self) -> DriverCapabilities {
        DriverCapabilities {
            transactions: true,
            mutations: true,
            cancel: CancelSupport::Driver,
            supports_ssh: true,
            schema: true,
            streaming: true,
            explain: true,
            maintenance: true,
        }
    }
}

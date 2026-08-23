// SPDX-License-Identifier: Apache-2.0

//! PlanetScale Driver
//!
//! Thin wrapper over the MySQL driver. PlanetScale speaks the MySQL wire
//! protocol and exposes the same `information_schema`, so behaviour is
//! delegated wholesale; only the identity and the TLS requirement differ.
//! PlanetScale terminates every connection over TLS and rejects plaintext, so
//! the flag is forced here rather than left to the connection form.

use async_trait::async_trait;

use qore_core::error::EngineResult;
use qore_core::traits::{DataEngine, StreamSender};
use qore_core::types::{
    CancelSupport, CollectionList, CollectionListOptions, ConnectionConfig, CreationOptions,
    EventDefinition, EventList, EventListOptions, EventOperationResult, ForeignKey,
    MaintenanceOperationInfo, MaintenanceRequest, MaintenanceResult, Namespace,
    PaginatedQueryResult, PaginationCapability, QueryId, QueryResult, RoutineDefinition,
    RoutineList, RoutineListOptions, RoutineOperationResult, RoutineType, RowData,
    SequenceDefinition, SequenceList, SequenceListOptions, SequenceOperationResult, SessionId,
    TableQueryOptions, TableSchema, TriggerDefinition, TriggerList, TriggerListOptions,
    TriggerOperationResult, TruncateAllResult, Value,
};

use super::mysql::MySqlDriver;

/// PlanetScale driver — delegates to MySqlDriver, over TLS.
pub struct PlanetScaleDriver {
    inner: MySqlDriver,
}

impl PlanetScaleDriver {
    pub fn new() -> Self {
        Self {
            inner: MySqlDriver::new(),
        }
    }

    /// PlanetScale refuses plaintext connections; a form left on "no TLS"
    /// would fail with a protocol error rather than a readable one.
    ///
    /// An explicit `ssl_mode` outranks the `ssl` flag in the MySQL driver, so
    /// clearing a disabling mode is part of the job — otherwise
    /// `ssl_mode = "disable"` would quietly defeat the whole point.
    fn require_tls(config: &ConnectionConfig) -> ConnectionConfig {
        let disabled_mode = matches!(
            config.ssl_mode.as_deref(),
            Some("disabled" | "disable" | "preferred" | "prefer")
        );
        if config.ssl && !disabled_mode {
            return config.clone();
        }
        let mut secured = config.clone();
        secured.ssl = true;
        if disabled_mode {
            secured.ssl_mode = Some("required".to_string());
        }
        secured
    }
}

impl Default for PlanetScaleDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for PlanetScaleDriver {
    fn driver_id(&self) -> &'static str {
        "planetscale"
    }

    fn driver_name(&self) -> &'static str {
        "PlanetScale"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        self.inner.test_connection(&Self::require_tls(config)).await
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        self.inner.connect(&Self::require_tls(config)).await
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        self.inner.disconnect(session).await
    }

    async fn ping(&self, session: SessionId) -> EngineResult<()> {
        self.inner.ping(session).await
    }

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        self.inner.list_namespaces(session).await
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        self.inner
            .list_collections(session, namespace, options)
            .await
    }

    /// Vitess implements neither stored procedures nor functions.
    fn supports_routines(&self) -> bool {
        false
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

    /// Vitess does not support triggers.
    fn supports_triggers(&self) -> bool {
        false
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

    /// Vitess does not support the event scheduler.
    fn supports_events(&self) -> bool {
        false
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
        self.inner.drop_event(session, namespace, event_name).await
    }

    fn supports_sequences(&self) -> bool {
        self.inner.supports_sequences()
    }

    async fn list_sequences(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: SequenceListOptions,
    ) -> EngineResult<SequenceList> {
        self.inner.list_sequences(session, namespace, options).await
    }

    async fn get_sequence_definition(
        &self,
        session: SessionId,
        namespace: &Namespace,
        name: &str,
    ) -> EngineResult<SequenceDefinition> {
        self.inner
            .get_sequence_definition(session, namespace, name)
            .await
    }

    async fn drop_sequence(
        &self,
        session: SessionId,
        namespace: &Namespace,
        name: &str,
    ) -> EngineResult<SequenceOperationResult> {
        self.inner.drop_sequence(session, namespace, name).await
    }

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

    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        self.inner.cancel(session, query_id).await
    }

    fn cancel_support(&self) -> CancelSupport {
        self.inner.cancel_support()
    }

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

    fn supports_truncate_all(&self) -> bool {
        self.inner.supports_truncate_all()
    }

    async fn truncate_all(
        &self,
        session: SessionId,
        namespace: &Namespace,
    ) -> EngineResult<TruncateAllResult> {
        self.inner.truncate_all(session, namespace).await
    }

    fn pagination_capability(&self) -> PaginationCapability {
        self.inner.pagination_capability()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PlanetScale must behave like MySQL apart from its identity, the forced
    /// TLS and the features Vitess drops (see the test below): a divergence
    /// anywhere else would be a silent capability loss.
    #[test]
    fn everything_but_vitess_gaps_matches_mysql() {
        let mysql = MySqlDriver::new();
        let planetscale = PlanetScaleDriver::new();

        assert_eq!(planetscale.driver_id(), "planetscale");
        assert_eq!(planetscale.driver_name(), "PlanetScale");
        assert_eq!(mysql.capabilities(), planetscale.capabilities());
        assert_eq!(mysql.cancel_support(), planetscale.cancel_support());
        assert_eq!(
            mysql.supports_transactions(),
            planetscale.supports_transactions()
        );
        assert_eq!(mysql.supports_mutations(), planetscale.supports_mutations());
        assert_eq!(mysql.supports_sequences(), planetscale.supports_sequences());
        assert_eq!(
            mysql.supports_maintenance(),
            planetscale.supports_maintenance()
        );
        assert_eq!(
            mysql.supports_truncate_all(),
            planetscale.supports_truncate_all()
        );
        assert_eq!(
            mysql.pagination_capability(),
            planetscale.pagination_capability()
        );
    }

    /// Vitess drops several MySQL features. Announcing them would offer the
    /// user schema objects the engine refuses to create.
    #[test]
    fn vitess_only_features_are_not_announced() {
        let planetscale = PlanetScaleDriver::new();
        assert!(!planetscale.supports_routines());
        assert!(!planetscale.supports_triggers());
        assert!(!planetscale.supports_events());
        assert!(!planetscale.supports_sequences());
    }

    /// An explicit `ssl_mode` outranks the `ssl` flag downstream, so a
    /// disabling mode must not survive the forcing.
    #[test]
    fn an_explicit_disabling_ssl_mode_is_overridden() {
        for mode in ["disable", "disabled", "prefer", "preferred"] {
            let config = ConnectionConfig {
                driver: "planetscale".into(),
                ssl: true,
                ssl_mode: Some(mode.to_string()),
                ..Default::default()
            };
            let secured = PlanetScaleDriver::require_tls(&config);
            assert!(secured.ssl);
            assert_eq!(
                secured.ssl_mode.as_deref(),
                Some("required"),
                "ssl_mode={mode} must not defeat the forced TLS"
            );
        }

        // A stricter mode is left alone.
        let strict = ConnectionConfig {
            ssl: true,
            ssl_mode: Some("verify-full".to_string()),
            ..Default::default()
        };
        assert_eq!(
            PlanetScaleDriver::require_tls(&strict).ssl_mode.as_deref(),
            Some("verify-full")
        );
    }

    #[test]
    fn tls_is_forced_on_a_plaintext_config() {
        let plain = ConnectionConfig {
            driver: "planetscale".into(),
            ssl: false,
            ..Default::default()
        };
        assert!(PlanetScaleDriver::require_tls(&plain).ssl);

        let already = ConnectionConfig { ssl: true, ..plain };
        assert!(PlanetScaleDriver::require_tls(&already).ssl);
    }
}

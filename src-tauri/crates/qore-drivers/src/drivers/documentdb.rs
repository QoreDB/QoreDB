// SPDX-License-Identifier: Apache-2.0

//! Amazon DocumentDB Driver
//!
//! Thin wrapper over the MongoDB driver. DocumentDB implements the MongoDB
//! wire protocol, so behaviour is delegated wholesale; only the identity and
//! the TLS requirement differ. DocumentDB clusters are TLS-only and live in a
//! private VPC, so connections usually come through the SSH tunnel the
//! connection form already offers.

use async_trait::async_trait;

use qore_core::error::EngineResult;
use qore_core::traits::{DataEngine, StreamSender};
use qore_core::types::{
    CancelSupport, CollectionList, CollectionListOptions, ConnectionConfig,
    MaintenanceOperationInfo, MaintenanceRequest, MaintenanceResult, Namespace,
    PaginatedQueryResult, PaginationCapability, QueryId, QueryResult, RowData, SessionId,
    TableQueryOptions, TableSchema, TruncateAllResult, Value,
};

use super::mongodb::MongoDriver;

/// Amazon DocumentDB driver — delegates to MongoDriver, over TLS.
pub struct DocumentDbDriver {
    inner: MongoDriver,
}

impl DocumentDbDriver {
    pub fn new() -> Self {
        Self {
            inner: MongoDriver::new(),
        }
    }

    /// DocumentDB clusters reject plaintext connections, and AWS documents two
    /// further departures from MongoDB that a client must carry itself:
    /// retryable writes are unsupported, and a cluster reached through an SSH
    /// tunnel presents a certificate for its AWS hostname, not for the local
    /// end of the tunnel.
    fn documentdb_options(config: &ConnectionConfig) -> ConnectionConfig {
        let mut secured = config.clone();
        secured.ssl = true;

        // Unsupported server-side; leaving it on makes every write fail.
        secured
            .options
            .entry("retryWrites".to_string())
            .or_insert_with(|| "false".to_string());

        // Through a tunnel the host is 127.0.0.1 while the certificate names
        // the cluster: the chain still has to verify against the CA, only the
        // hostname check is dropped.
        if config.ssh_tunnel.is_some() || config.proxy.is_some() {
            secured
                .options
                .entry("tlsAllowInvalidHostnames".to_string())
                .or_insert_with(|| "true".to_string());
        }

        secured
    }
}

impl Default for DocumentDbDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for DocumentDbDriver {
    fn driver_id(&self) -> &'static str {
        "documentdb"
    }

    fn driver_name(&self) -> &'static str {
        "Amazon DocumentDB"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        self.inner
            .test_connection(&Self::documentdb_options(config))
            .await
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        self.inner.connect(&Self::documentdb_options(config)).await
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

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        self.inner.execute(session, query, query_id).await
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

    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        self.inner.cancel(session, query_id).await
    }

    fn pagination_capability(&self) -> PaginationCapability {
        self.inner.pagination_capability()
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

    async fn supports_transactions_for_session(&self, session: SessionId) -> bool {
        self.inner.supports_transactions_for_session(session).await
    }

    fn supports_transactions(&self) -> bool {
        self.inner.supports_transactions()
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
        self.inner.supports_mutations()
    }

    fn supports_streaming(&self) -> bool {
        self.inner.supports_streaming()
    }

    fn supports_maintenance(&self) -> bool {
        self.inner.supports_maintenance()
    }

    async fn list_maintenance_operations(
        &self,
        _session: SessionId,
        _namespace: &Namespace,
        _table: &str,
    ) -> EngineResult<Vec<MaintenanceOperationInfo>> {
        self.inner
            .list_maintenance_operations(_session, _namespace, _table)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use qore_core::types::{SshAuth, SshHostKeyPolicy, SshTunnelConfig};

    /// DocumentDB must behave exactly like MongoDB apart from its identity and
    /// the forced TLS: a divergence here would be a silent capability loss.
    #[test]
    fn only_identity_and_tls_differ_from_mongodb() {
        let mongo = MongoDriver::new();
        let documentdb = DocumentDbDriver::new();

        assert_eq!(documentdb.driver_id(), "documentdb");
        assert_eq!(documentdb.driver_name(), "Amazon DocumentDB");
        assert_eq!(mongo.capabilities(), documentdb.capabilities());
        assert_eq!(mongo.cancel_support(), documentdb.cancel_support());
        assert_eq!(
            mongo.supports_transactions(),
            documentdb.supports_transactions()
        );
        assert_eq!(mongo.supports_mutations(), documentdb.supports_mutations());
        assert_eq!(mongo.supports_schema(), documentdb.supports_schema());
        assert_eq!(mongo.supports_streaming(), documentdb.supports_streaming());
        assert_eq!(
            mongo.supports_maintenance(),
            documentdb.supports_maintenance()
        );
        assert_eq!(
            mongo.supports_truncate_all(),
            documentdb.supports_truncate_all()
        );
        assert_eq!(
            mongo.pagination_capability(),
            documentdb.pagination_capability()
        );
    }

    #[test]
    fn tls_is_forced_on_a_plaintext_config() {
        let plain = ConnectionConfig {
            driver: "documentdb".into(),
            ssl: false,
            ..Default::default()
        };
        assert!(DocumentDbDriver::documentdb_options(&plain).ssl);

        let already = ConnectionConfig { ssl: true, ..plain };
        assert!(DocumentDbDriver::documentdb_options(&already).ssl);
    }

    /// DocumentDB does not implement retryable writes; left on, every write
    /// fails against the cluster.
    #[test]
    fn retryable_writes_are_turned_off() {
        let config = ConnectionConfig {
            driver: "documentdb".into(),
            ..Default::default()
        };
        assert_eq!(
            DocumentDbDriver::documentdb_options(&config)
                .options
                .get("retryWrites")
                .map(String::as_str),
            Some("false")
        );
    }

    /// Through a tunnel the host is the local end while the certificate names
    /// the cluster. The CA is still verified; only the hostname check is
    /// dropped, and only when a tunnel is actually configured.
    #[test]
    fn hostname_verification_is_relaxed_only_behind_a_tunnel() {
        let direct = ConnectionConfig {
            driver: "documentdb".into(),
            ..Default::default()
        };
        assert!(
            !DocumentDbDriver::documentdb_options(&direct)
                .options
                .contains_key("tlsAllowInvalidHostnames"),
            "a direct connection must keep the hostname check"
        );

        let tunnelled = ConnectionConfig {
            driver: "documentdb".into(),
            ssh_tunnel: Some(SshTunnelConfig {
                host: "bastion.internal".into(),
                port: 22,
                username: "ops".into(),
                auth: SshAuth::Password {
                    password: String::new(),
                },
                host_key_policy: SshHostKeyPolicy::AcceptNew,
                known_hosts_path: None,
                proxy_jump: None,
                connect_timeout_secs: 10,
                keepalive_interval_secs: 30,
                keepalive_count_max: 3,
            }),
            ..Default::default()
        };
        assert_eq!(
            DocumentDbDriver::documentdb_options(&tunnelled)
                .options
                .get("tlsAllowInvalidHostnames")
                .map(String::as_str),
            Some("true")
        );
    }

    /// An explicit user setting is never overridden.
    #[test]
    fn explicit_options_win() {
        let mut config = ConnectionConfig {
            driver: "documentdb".into(),
            ..Default::default()
        };
        config
            .options
            .insert("retryWrites".to_string(), "true".to_string());
        assert_eq!(
            DocumentDbDriver::documentdb_options(&config)
                .options
                .get("retryWrites")
                .map(String::as_str),
            Some("true")
        );
    }
}

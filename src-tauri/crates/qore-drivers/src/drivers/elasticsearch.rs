// SPDX-License-Identifier: Apache-2.0

//! Elasticsearch driver — thin wrapper over the shared `search_compat` engine.

use async_trait::async_trait;

use crate::drivers::search_compat::{self, SearchFlavor, SessionMap};
use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::{DataEngine, StreamSender};
use qore_core::types::{
    CancelSupport, CollectionList, CollectionListOptions, ConnectionConfig, Namespace,
    PaginatedQueryResult, QueryId, QueryResult, RowData, SessionId, TableQueryOptions, TableSchema,
    Value,
};

pub struct ElasticsearchDriver {
    sessions: SessionMap,
}

impl ElasticsearchDriver {
    pub fn new() -> Self {
        Self {
            sessions: search_compat::new_session_map(),
        }
    }

    const FLAVOR: SearchFlavor = SearchFlavor::Elasticsearch;
}

impl Default for ElasticsearchDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataEngine for ElasticsearchDriver {
    fn driver_id(&self) -> &'static str {
        "elasticsearch"
    }

    fn driver_name(&self) -> &'static str {
        "Elasticsearch"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        search_compat::test_connection(config, Self::FLAVOR).await
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        search_compat::connect(&self.sessions, config, Self::FLAVOR).await
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        search_compat::disconnect(&self.sessions, session).await
    }

    async fn ping(&self, session: SessionId) -> EngineResult<()> {
        search_compat::ping(&self.sessions, session).await
    }

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        search_compat::list_namespaces(&self.sessions, session).await
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        search_compat::list_collections(&self.sessions, session, namespace, options).await
    }

    async fn describe_table(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        search_compat::describe_table(&self.sessions, session, table).await
    }

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        search_compat::execute(&self.sessions, session, query, query_id).await
    }

    async fn execute_stream(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        search_compat::execute_stream(&self.sessions, session, query, query_id, sender).await
    }

    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        search_compat::cancel(&self.sessions, session, query_id).await
    }

    fn cancel_support(&self) -> CancelSupport {
        CancelSupport::BestEffort
    }

    async fn preview_table(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        search_compat::preview_table(&self.sessions, session, table, limit).await
    }

    async fn query_table(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        search_compat::query_table(&self.sessions, session, table, options).await
    }

    async fn create_database(
        &self,
        _session: SessionId,
        _name: &str,
        _options: Option<Value>,
    ) -> EngineResult<()> {
        Err(EngineError::not_supported(
            "Elasticsearch has no databases; create an index via the console (PUT /index)",
        ))
    }

    async fn drop_database(&self, _session: SessionId, _name: &str) -> EngineResult<()> {
        Err(EngineError::not_supported(
            "Elasticsearch has no databases; delete an index via the console (DELETE /index)",
        ))
    }

    async fn insert_row(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        table: &str,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        search_compat::insert_row(&self.sessions, session, table, data).await
    }

    async fn update_row(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        search_compat::update_row(&self.sessions, session, table, primary_key, data).await
    }

    async fn delete_row(
        &self,
        session: SessionId,
        _namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
    ) -> EngineResult<QueryResult> {
        search_compat::delete_row(&self.sessions, session, table, primary_key).await
    }

    fn supports_mutations(&self) -> bool {
        true
    }

    fn supports_transactions(&self) -> bool {
        false
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_ssh(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elasticsearch_identity() {
        let d = ElasticsearchDriver::new();
        assert_eq!(d.driver_id(), "elasticsearch");
        assert_eq!(d.driver_name(), "Elasticsearch");
        assert!(d.supports_mutations());
        assert!(!d.supports_transactions());
    }
}

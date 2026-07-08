// SPDX-License-Identifier: Apache-2.0
//! Minimal `DataEngine` skeleton. This covers only the identity + connection
//! lifecycle and namespace listing. Copy the closest existing driver
//! (e.g. `sqlite.rs`) for the full trait surface: `execute`, `describe_table`,
//! streaming, and the optional routine/trigger/sequence/event methods.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use qore_core::error::EngineResult;
use qore_core::traits::DataEngine;
use qore_core::types::{
    CollectionList, CollectionListOptions, ConnectionConfig, Namespace, SessionId,
};

/// Replace `Connection` with the backend's real client/connection type.
type Connection = ();

#[derive(Default)]
pub struct NewDriver {
    sessions: Arc<RwLock<HashMap<SessionId, Connection>>>,
}

impl NewDriver {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl DataEngine for NewDriver {
    fn driver_id(&self) -> &'static str {
        "new_driver"
    }

    fn driver_name(&self) -> &'static str {
        "New Driver"
    }

    async fn test_connection(&self, _config: &ConnectionConfig) -> EngineResult<()> {
        todo!("validate credentials without opening a persistent session")
    }

    async fn connect(&self, _config: &ConnectionConfig) -> EngineResult<SessionId> {
        todo!("open a connection, store it in `sessions`, return a fresh SessionId")
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        self.sessions.write().await.remove(&session);
        Ok(())
    }

    async fn ping(&self, _session: SessionId) -> EngineResult<()> {
        todo!("lightweight health check on the stored connection")
    }

    async fn list_namespaces(&self, _session: SessionId) -> EngineResult<Vec<Namespace>> {
        todo!("list databases/schemas visible to this session")
    }

    async fn list_collections(
        &self,
        _session: SessionId,
        _namespace: &Namespace,
        _options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        todo!("list tables/views/collections in the namespace")
    }

    // Remaining methods (`execute`, `describe_table`, streaming, …): copy and
    // adapt from an existing driver in this crate.
}

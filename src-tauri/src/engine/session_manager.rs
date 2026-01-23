//! Session Manager
//!
//! Centralized management of all active database sessions.
//! This is the SINGLE SOURCE OF TRUTH for all connection state.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};
use tracing::instrument;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::ssh_tunnel::SshTunnel;
use crate::engine::traits::DataEngine;
use crate::engine::types::{ConnectionConfig, SessionId};
use crate::engine::DriverRegistry;

/// Active session with its connection pool and optional tunnel
pub struct ActiveSession {
    pub driver_id: String,
    pub config: ConnectionConfig,
    pub display_name: String,
    pub tunnel: Option<SshTunnel>,
}

/// Manages all active database sessions
/// This is the SINGLE SOURCE OF TRUTH - pools are stored here, not in drivers.
pub struct SessionManager {
    registry: Arc<DriverRegistry>,
    sessions: RwLock<HashMap<SessionId, ActiveSession>>,
}

impl SessionManager {
    const CONNECT_TIMEOUT_MS: u64 = 15000;
    const TEST_TIMEOUT_MS: u64 = 10000;
    pub fn new(registry: Arc<DriverRegistry>) -> Self {
        Self {
            registry,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Tests a connection without persisting it
    #[instrument(
        skip(self, config),
        fields(
            driver = %config.driver,
            host = %config.host,
            port = config.port,
            database = ?config.database,
            ssh = config.ssh_tunnel.is_some()
        )
    )]
    pub async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        let driver = self
            .registry
            .get(&config.driver)
            .ok_or_else(|| EngineError::driver_not_found(&config.driver))?;

        let test_future = async {
            // If SSH tunnel is configured, we need to test through it
            if let Some(ref ssh_config) = config.ssh_tunnel {
                let tunnel = SshTunnel::open(ssh_config, &config.host, config.port).await?;
                let mut tunneled_config = config.clone();
                tunneled_config.host = "127.0.0.1".to_string();
                tunneled_config.port = tunnel.local_port();
                // Tunnel will be dropped after test, closing the connection
                return driver.test_connection(&tunneled_config).await;
            }

            driver.test_connection(config).await
        };

        match timeout(Duration::from_millis(Self::TEST_TIMEOUT_MS), test_future).await {
            Ok(result) => result,
            Err(_) => Err(EngineError::Timeout {
                timeout_ms: Self::TEST_TIMEOUT_MS,
            }),
        }
    }

    /// Establishes a new connection and returns its session ID
    #[instrument(
        skip(self, config),
        fields(
            driver = %config.driver,
            host = %config.host,
            port = config.port,
            database = ?config.database,
            ssh = config.ssh_tunnel.is_some()
        )
    )]
    pub async fn connect(&self, config: ConnectionConfig) -> EngineResult<SessionId> {
        let driver = self
            .registry
            .get(&config.driver)
            .ok_or_else(|| EngineError::driver_not_found(&config.driver))?;

        let connect_future = async {
            // Setup SSH tunnel if configured
            let (effective_config, tunnel) = if let Some(ref ssh_config) = config.ssh_tunnel {
                let tunnel = SshTunnel::open(ssh_config, &config.host, config.port).await?;
                let mut tunneled_config = config.clone();
                tunneled_config.host = "127.0.0.1".to_string();
                tunneled_config.port = tunnel.local_port();
                (tunneled_config, Some(tunnel))
            } else {
                (config.clone(), None)
            };

            let session_id = driver.connect(&effective_config).await?;

            let display_name = format!(
                "{}@{}:{}{}",
                config.username,
                config.host,
                config.database.as_deref().unwrap_or("default"),
                if tunnel.is_some() { " (SSH)" } else { "" }
            );

            let session = ActiveSession {
                driver_id: config.driver.clone(),
                config,
                display_name,
                tunnel,
            };

            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id, session);

            Ok(session_id)
        };

        match timeout(Duration::from_millis(Self::CONNECT_TIMEOUT_MS), connect_future).await {
            Ok(result) => result,
            Err(_) => Err(EngineError::Timeout {
                timeout_ms: Self::CONNECT_TIMEOUT_MS,
            }),
        }
    }

    /// Disconnects a session
    #[instrument(skip(self), fields(session_id = %session_id.0))]
    pub async fn disconnect(&self, session_id: SessionId) -> EngineResult<()> {
        let mut session = {
            let mut sessions = self.sessions.write().await;
            sessions
                .remove(&session_id)
                .ok_or_else(|| EngineError::session_not_found(session_id.0.to_string()))?
        };

        let driver = self
            .registry
            .get(&session.driver_id)
            .ok_or_else(|| EngineError::driver_not_found(&session.driver_id))?;

        // Disconnect from database; restore session on failure.
        if let Err(err) = driver.disconnect(session_id).await {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id, session);
            return Err(err);
        }

        // Close SSH tunnel if present
        if let Some(ref mut tunnel) = session.tunnel {
            tunnel.close().await?;
        }

        Ok(())
    }

    /// Gets a driver for an existing session
    pub async fn get_driver(&self, session_id: SessionId) -> EngineResult<Arc<dyn DataEngine>> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| EngineError::session_not_found(session_id.0.to_string()))?;

        self.registry
            .get(&session.driver_id)
            .ok_or_else(|| EngineError::driver_not_found(&session.driver_id))
    }

    /// Lists all active sessions
    pub async fn list_sessions(&self) -> Vec<(SessionId, String)> {
        let sessions = self.sessions.read().await;
        sessions
            .iter()
            .map(|(id, session)| (*id, session.display_name.clone()))
            .collect()
    }

    /// Gets session info
    pub async fn get_session_info(&self, session_id: SessionId) -> Option<String> {
        let sessions = self.sessions.read().await;
        sessions.get(&session_id).map(|s| s.display_name.clone())
    }

    /// Checks if the session is read-only
    pub async fn is_read_only(&self, session_id: SessionId) -> EngineResult<bool> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| EngineError::session_not_found(session_id.0.to_string()))?;

        Ok(session.config.read_only)
    }

    /// Checks if the session is a production environment
    pub async fn is_production(&self, session_id: SessionId) -> EngineResult<bool> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| EngineError::session_not_found(session_id.0.to_string()))?;

        Ok(session.config.environment == "production")
    }

    /// Checks if a session exists
    pub async fn session_exists(&self, session_id: SessionId) -> bool {
        let sessions = self.sessions.read().await;
        sessions.contains_key(&session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::{
        CollectionList, CollectionListOptions, ConnectionConfig, Namespace, QueryId, QueryResult,
        SessionId, TableSchema, Value,
    };
    use async_trait::async_trait;

    #[derive(Debug)]
    struct MockDriver {
        id: &'static str,
    }

    impl MockDriver {
        fn new(id: &'static str) -> Self {
            Self { id }
        }
    }

    #[async_trait]
    impl DataEngine for MockDriver {
        fn driver_id(&self) -> &'static str {
            self.id
        }

        fn driver_name(&self) -> &'static str {
            "Mock Driver"
        }

        async fn test_connection(&self, _config: &ConnectionConfig) -> EngineResult<()> {
            Ok(())
        }

        async fn connect(&self, _config: &ConnectionConfig) -> EngineResult<SessionId> {
            Ok(SessionId::new())
        }

        async fn disconnect(&self, _session: SessionId) -> EngineResult<()> {
            Ok(())
        }

        async fn list_namespaces(&self, _session: SessionId) -> EngineResult<Vec<Namespace>> {
            Ok(vec![])
        }

        async fn list_collections(
            &self,
            _session: SessionId,
            _namespace: &Namespace,
            _options: CollectionListOptions,
        ) -> EngineResult<CollectionList> {
            Ok(CollectionList {
                collections: vec![],
                total_count: 0,
            })
        }

        async fn create_database(
            &self,
            _session: SessionId,
            _name: &str,
            _options: Option<Value>,
        ) -> EngineResult<()> {
            Ok(())
        }

        async fn drop_database(&self, _session: SessionId, _name: &str) -> EngineResult<()> {
            Ok(())
        }

        async fn execute(
            &self,
            _session: SessionId,
            _query: &str,
            _query_id: QueryId,
        ) -> EngineResult<QueryResult> {
            Ok(QueryResult::empty())
        }

        async fn describe_table(
            &self,
            _session: SessionId,
            _namespace: &Namespace,
            _table: &str,
        ) -> EngineResult<TableSchema> {
            Ok(TableSchema {
                columns: vec![],
                primary_key: None,
                foreign_keys: vec![],
                row_count_estimate: None,
            })
        }

        async fn preview_table(
            &self,
            _session: SessionId,
            _namespace: &Namespace,
            _table: &str,
            _limit: u32,
        ) -> EngineResult<QueryResult> {
            Ok(QueryResult::empty())
        }
    }

    fn create_manager() -> SessionManager {
        let mut registry = DriverRegistry::new();
        registry.register(Arc::new(MockDriver::new("mock")));
        SessionManager::new(Arc::new(registry))
    }

    fn create_config() -> ConnectionConfig {
        ConnectionConfig {
            driver: "mock".to_string(),
            host: "localhost".to_string(),
            port: 5432,
            username: "user".to_string(),
            password: "password".to_string(),
            database: Some("test_db".to_string()),
            ssl: false,
            environment: "development".to_string(),
            read_only: false,
            pool_max_connections: None,
            pool_min_connections: None,
            pool_acquire_timeout_secs: None,
            ssh_tunnel: None,
        }
    }

    #[tokio::test]
    async fn test_connect_and_disconnect() {
        let manager = create_manager();
        let config = create_config();

        let session_id = manager.connect(config).await.expect("connect failed");
        assert!(manager.session_exists(session_id).await);

        let sessions = manager.list_sessions().await;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].0, session_id);

        manager.disconnect(session_id).await.expect("disconnect failed");
        assert!(!manager.session_exists(session_id).await);
    }

    #[tokio::test]
    async fn test_test_connection() {
        let manager = create_manager();
        let config = create_config();

        manager.test_connection(&config).await.expect("test connection failed");
    }

    #[tokio::test]
    async fn test_get_driver() {
        let manager = create_manager();
        let config = create_config();

        let session_id = manager.connect(config).await.expect("connect failed");

        let driver = manager.get_driver(session_id).await.expect("get_driver failed");
        assert_eq!(driver.driver_id(), "mock");
    }

    #[tokio::test]
    async fn test_connect_invalid_driver() {
        let manager = create_manager();
        let mut config = create_config();
        config.driver = "nonexistent".to_string();

        let result = manager.connect(config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_properties() {
        let manager = create_manager();
        let config = create_config();

        let session_id = manager.connect(config).await.expect("connect failed");

        assert!(!manager.is_read_only(session_id).await.unwrap());
        assert!(!manager.is_production(session_id).await.unwrap());

        // Test production flag
        let mut prod_config = create_config();
        prod_config.environment = "production".to_string();
        let prod_session = manager.connect(prod_config).await.expect("connect prod failed");
        assert!(manager.is_production(prod_session).await.unwrap());
    }
}

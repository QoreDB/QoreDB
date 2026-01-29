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

    /// Gets the session environment (development/staging/production)
    pub async fn get_environment(&self, session_id: SessionId) -> EngineResult<String> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| EngineError::session_not_found(session_id.0.to_string()))?;

        Ok(session.config.environment.clone())
    }

    /// Checks if a session exists
    pub async fn session_exists(&self, session_id: SessionId) -> bool {
        let sessions = self.sessions.read().await;
        sessions.contains_key(&session_id)
    }
}

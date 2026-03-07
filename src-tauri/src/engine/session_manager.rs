// SPDX-License-Identifier: Apache-2.0

//! Session Manager
//!
//! Centralized management of all active database sessions.
//! This is the SINGLE SOURCE OF TRUTH for all connection state.
//! Includes smart keep-alive with proactive health monitoring and
//! automatic SSH tunnel reconnection.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};
use tracing::instrument;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::ssh_tunnel::SshTunnel;
use crate::engine::traits::DataEngine;
use crate::engine::types::{ConnectionConfig, SessionId};
use crate::engine::DriverRegistry;

/// Connection health status for a single session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionHealth {
    Healthy,
    Unhealthy,
    Reconnecting,
}

/// Event payload emitted to the frontend when health changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionHealthEvent {
    pub session_id: String,
    pub health: ConnectionHealth,
}

/// Active session with its connection pool and optional tunnel
pub struct ActiveSession {
    pub driver_id: String,
    pub config: ConnectionConfig,
    pub display_name: String,
    pub tunnel: Option<SshTunnel>,
    pub health: ConnectionHealth,
    /// Consecutive ping failures (reset on success).
    pub consecutive_failures: u32,
}

/// Manages all active database sessions
/// This is the SINGLE SOURCE OF TRUTH - pools are stored here, not in drivers.
pub struct SessionManager {
    registry: Arc<DriverRegistry>,
    sessions: RwLock<HashMap<SessionId, ActiveSession>>,
}

/// Tauri event name for connection health changes.
pub const EVENT_CONNECTION_HEALTH: &str = "connection_health";

impl SessionManager {
    const CONNECT_TIMEOUT_MS: u64 = 15000;
    const TEST_TIMEOUT_MS: u64 = 10000;
    const PING_TIMEOUT_MS: u64 = 5000;
    /// Interval between health checks (seconds).
    const HEALTH_CHECK_INTERVAL_SECS: u64 = 30;
    /// Consecutive failures before attempting SSH tunnel reconnection.
    const RECONNECT_THRESHOLD: u32 = 2;

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
                health: ConnectionHealth::Healthy,
                consecutive_failures: 0,
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

    /// Updates the display name for an active session.
    pub async fn set_display_name(&self, session_id: SessionId, name: String) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.display_name = name;
        }
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

    /// Returns the current health for a session.
    pub async fn get_health(&self, session_id: SessionId) -> EngineResult<ConnectionHealth> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| EngineError::session_not_found(session_id.0.to_string()))?;
        Ok(session.health)
    }

    /// Pings a session to check connectivity (with timeout).
    pub async fn ping(&self, session_id: SessionId) -> EngineResult<()> {
        let driver = self.get_driver(session_id).await?;
        match timeout(
            Duration::from_millis(Self::PING_TIMEOUT_MS),
            driver.ping(session_id),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(EngineError::Timeout {
                timeout_ms: Self::PING_TIMEOUT_MS,
            }),
        }
    }

    /// Checks if the SSH tunnel for a session is alive by probing the local port.
    async fn is_tunnel_alive(tunnel: &SshTunnel) -> bool {
        tokio::net::TcpStream::connect(("127.0.0.1", tunnel.local_port()))
            .await
            .is_ok()
    }

    /// Attempts to reconnect a broken SSH tunnel for a session.
    /// Returns the new tunnel on success.
    async fn reconnect_tunnel(config: &ConnectionConfig) -> EngineResult<SshTunnel> {
        let ssh_config = config
            .ssh_tunnel
            .as_ref()
            .ok_or_else(|| EngineError::SshError {
                message: "No SSH tunnel configured for this session".into(),
            })?;

        tracing::info!(
            "Reconnecting SSH tunnel to {}:{}",
            ssh_config.host,
            ssh_config.port
        );

        SshTunnel::open(ssh_config, &config.host, config.port).await
    }

    /// Runs one health-check cycle across all sessions.
    /// Returns a list of (session_id, new_health) for sessions whose health changed.
    pub async fn run_health_check(&self) -> Vec<ConnectionHealthEvent> {
        let session_ids: Vec<SessionId> = {
            let sessions = self.sessions.read().await;
            sessions.keys().copied().collect()
        };

        let mut events = Vec::new();

        for sid in session_ids {
            let previous_health = {
                let sessions = self.sessions.read().await;
                match sessions.get(&sid) {
                    Some(s) => s.health,
                    None => continue,
                }
            };

            // 1. Check SSH tunnel first (if applicable)
            let tunnel_ok = {
                let sessions = self.sessions.read().await;
                match sessions.get(&sid) {
                    Some(s) => match &s.tunnel {
                        Some(tunnel) => Self::is_tunnel_alive(tunnel).await,
                        None => true,
                    },
                    None => continue,
                }
            };

            // 2. If tunnel is down and above threshold, try reconnection
            if !tunnel_ok {
                let should_reconnect = {
                    let sessions = self.sessions.read().await;
                    match sessions.get(&sid) {
                        Some(s) => s.consecutive_failures >= Self::RECONNECT_THRESHOLD,
                        None => continue,
                    }
                };

                if should_reconnect {
                    // Set status to reconnecting
                    {
                        let mut sessions = self.sessions.write().await;
                        if let Some(s) = sessions.get_mut(&sid) {
                            s.health = ConnectionHealth::Reconnecting;
                        }
                    }

                    if previous_health != ConnectionHealth::Reconnecting {
                        events.push(ConnectionHealthEvent {
                            session_id: sid.0.to_string(),
                            health: ConnectionHealth::Reconnecting,
                        });
                    }

                    // Attempt reconnection
                    let config = {
                        let sessions = self.sessions.read().await;
                        match sessions.get(&sid) {
                            Some(s) => s.config.clone(),
                            None => continue,
                        }
                    };

                    match Self::reconnect_tunnel(&config).await {
                        Ok(new_tunnel) => {
                            let mut sessions = self.sessions.write().await;
                            if let Some(s) = sessions.get_mut(&sid) {
                                // Close old tunnel (best effort)
                                if let Some(ref mut old_tunnel) = s.tunnel {
                                    let _ = old_tunnel.close().await;
                                }
                                s.tunnel = Some(new_tunnel);
                                s.health = ConnectionHealth::Healthy;
                                s.consecutive_failures = 0;
                                tracing::info!("SSH tunnel reconnected for session {}", sid.0);
                            }
                            events.push(ConnectionHealthEvent {
                                session_id: sid.0.to_string(),
                                health: ConnectionHealth::Healthy,
                            });
                            continue;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "SSH tunnel reconnection failed for session {}: {}",
                                sid.0,
                                e
                            );
                        }
                    }
                }

                // Mark as unhealthy
                let mut sessions = self.sessions.write().await;
                if let Some(s) = sessions.get_mut(&sid) {
                    s.consecutive_failures += 1;
                    s.health = ConnectionHealth::Unhealthy;
                }
                if previous_health != ConnectionHealth::Unhealthy {
                    events.push(ConnectionHealthEvent {
                        session_id: sid.0.to_string(),
                        health: ConnectionHealth::Unhealthy,
                    });
                }
                continue;
            }

            // 3. Ping the database
            let ping_result = self.ping(sid).await;

            let new_health = if ping_result.is_ok() {
                ConnectionHealth::Healthy
            } else {
                ConnectionHealth::Unhealthy
            };

            // Update state
            {
                let mut sessions = self.sessions.write().await;
                if let Some(s) = sessions.get_mut(&sid) {
                    if ping_result.is_ok() {
                        s.consecutive_failures = 0;
                    } else {
                        s.consecutive_failures += 1;
                    }
                    s.health = new_health;
                }
            }

            if new_health != previous_health {
                events.push(ConnectionHealthEvent {
                    session_id: sid.0.to_string(),
                    health: new_health,
                });
            }
        }

        events
    }

    /// Starts the background health monitor.
    /// Spawns a tokio task that periodically checks all sessions
    /// and emits Tauri events when health changes.
    pub fn start_health_monitor(
        self: &Arc<Self>,
        app_handle: tauri::AppHandle,
    ) {
        use tauri::Emitter;

        let manager = Arc::clone(self);
        let interval_secs = Self::HEALTH_CHECK_INTERVAL_SECS;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            // Skip the first immediate tick
            interval.tick().await;

            loop {
                interval.tick().await;
                let events = manager.run_health_check().await;
                for event in events {
                    let _ = app_handle.emit(EVENT_CONNECTION_HEALTH, &event);
                }
            }
        });
    }
}

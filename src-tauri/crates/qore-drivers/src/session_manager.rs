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
use tokio::time::{Duration, timeout};
use tracing::instrument;

use crate::proxy::ProxyTunnel;
use crate::ssh_tunnel::SshTunnel;
use qore_core::DriverRegistry;
use qore_core::error::{EngineError, EngineResult};
use qore_core::traits::DataEngine;
use qore_core::types::{ConnectionConfig, SessionId, SshHostKeyPolicy};

/// Rejects an insecure SSH host-key policy (`StrictHostKeyChecking=no`) when the
/// connection targets a production environment.
fn enforce_ssh_host_key_policy(config: &ConnectionConfig) -> EngineResult<()> {
    if let Some(ssh) = &config.ssh_tunnel {
        if config.environment.eq_ignore_ascii_case("production")
            && matches!(ssh.host_key_policy, SshHostKeyPolicy::InsecureNoCheck)
        {
            return Err(EngineError::SshError {
                message: "Insecure host key checking is not allowed for production connections. \
                          Use 'strict' or 'accept_new' instead."
                    .to_string(),
            });
        }
    }
    Ok(())
}

/// Rejects the proxy + SSH combination: `ssh -L` resolves the forwarding target
/// on the bastion, so chaining them lands on the bastion's own loopback.
fn enforce_supported_topology(config: &ConnectionConfig) -> EngineResult<()> {
    if config.proxy.is_some() && config.ssh_tunnel.is_some() {
        return Err(EngineError::ConnectionFailed {
            message: "Combining a network proxy with an SSH tunnel is not supported. \
                      Keep one of the two on this connection."
                .to_string(),
        });
    }
    Ok(())
}

/// Disconnects a session the driver opened when the connect future is cancelled
/// before registration: nothing else can reach it afterwards.
struct ConnectCleanup {
    driver: Arc<dyn DataEngine>,
    session_id: Option<SessionId>,
}

impl ConnectCleanup {
    fn disarm(&mut self) {
        self.session_id = None;
    }
}

impl Drop for ConnectCleanup {
    fn drop(&mut self) {
        let Some(session_id) = self.session_id.take() else {
            return;
        };
        let driver = Arc::clone(&self.driver);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = driver.disconnect(session_id).await;
            });
        }
    }
}

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
    /// Stable id of the saved connection that opened this session. Direct
    /// debug connections leave this unset.
    pub saved_connection_id: Option<String>,
    pub tunnel: Option<SshTunnel>,
    pub proxy_tunnel: Option<ProxyTunnel>,
    pub health: ConnectionHealth,
    /// Consecutive ping failures (reset on success).
    pub consecutive_failures: u32,
}

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
    #[cfg(feature = "tauri")]
    const HEALTH_CHECK_INTERVAL_SECS: u64 = 30;
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
        enforce_ssh_host_key_policy(config)?;
        enforce_supported_topology(config)?;

        let driver = self
            .registry
            .get(&config.driver)
            .ok_or_else(|| EngineError::driver_not_found(&config.driver))?;

        let test_future = async {
            if let Some(ref proxy_config) = config.proxy {
                let mut proxy_tunnel =
                    ProxyTunnel::open(proxy_config, &config.host, config.port).await?;
                let mut tunneled_config = config.clone();
                tunneled_config.host = "127.0.0.1".to_string();
                tunneled_config.port = proxy_tunnel.local_port();

                let result = driver.test_connection(&tunneled_config).await;
                let _ = proxy_tunnel.close().await;
                return result;
            }

            if let Some(ref ssh_config) = config.ssh_tunnel {
                let mut tunnel = SshTunnel::open(ssh_config, &config.host, config.port).await?;
                let mut tunneled_config = config.clone();
                tunneled_config.host = "127.0.0.1".to_string();
                tunneled_config.port = tunnel.local_port();
                let result = driver.test_connection(&tunneled_config).await;
                let _ = tunnel.close().await;
                return result;
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
        enforce_ssh_host_key_policy(&config)?;
        enforce_supported_topology(&config)?;

        let driver = self
            .registry
            .get(&config.driver)
            .ok_or_else(|| EngineError::driver_not_found(&config.driver))?;

        let connect_future = async {
            let mut effective_config = config.clone();

            let proxy_tunnel = match config.proxy {
                Some(ref proxy_config) => {
                    let proxy_tunnel =
                        ProxyTunnel::open(proxy_config, &config.host, config.port).await?;
                    effective_config.host = "127.0.0.1".to_string();
                    effective_config.port = proxy_tunnel.local_port();
                    Some(proxy_tunnel)
                }
                None => None,
            };

            let tunnel = match config.ssh_tunnel {
                Some(ref ssh_config) => {
                    let tunnel = SshTunnel::open(ssh_config, &config.host, config.port).await?;
                    effective_config.host = "127.0.0.1".to_string();
                    effective_config.port = tunnel.local_port();
                    Some(tunnel)
                }
                None => None,
            };

            let mut cleanup = ConnectCleanup {
                driver: Arc::clone(&driver),
                session_id: None,
            };

            let session_id = driver.connect(&effective_config).await?;
            cleanup.session_id = Some(session_id);

            let suffix = match (proxy_tunnel.is_some(), tunnel.is_some()) {
                (true, _) => " (Proxy)",
                (false, true) => " (SSH)",
                (false, false) => "",
            };

            let display_name = format!(
                "{}@{}:{}{}",
                config.username,
                config.host,
                config.database.as_deref().unwrap_or("default"),
                suffix
            );

            let session = ActiveSession {
                driver_id: config.driver.clone(),
                config,
                display_name,
                saved_connection_id: None,
                tunnel,
                proxy_tunnel,
                health: ConnectionHealth::Healthy,
                consecutive_failures: 0,
            };

            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id, session);
            cleanup.disarm();

            Ok(session_id)
        };

        match timeout(
            Duration::from_millis(Self::CONNECT_TIMEOUT_MS),
            connect_future,
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(EngineError::Timeout {
                timeout_ms: Self::CONNECT_TIMEOUT_MS,
            }),
        }
    }

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

        // The session is already out of the map: a second `disconnect` is impossible.
        let mut first_error = None;

        if let Some(ref mut tunnel) = session.tunnel {
            if let Err(e) = tunnel.close().await {
                first_error = Some(e);
            }
        }

        if let Some(ref mut proxy_tunnel) = session.proxy_tunnel {
            if let Err(e) = proxy_tunnel.close().await {
                first_error = first_error.or(Some(e));
            }
        }

        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    pub async fn get_driver(&self, session_id: SessionId) -> EngineResult<Arc<dyn DataEngine>> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| EngineError::session_not_found(session_id.0.to_string()))?;

        self.registry
            .get(&session.driver_id)
            .ok_or_else(|| EngineError::driver_not_found(&session.driver_id))
    }

    pub async fn list_sessions(&self) -> Vec<(SessionId, String)> {
        let sessions = self.sessions.read().await;
        sessions
            .iter()
            .map(|(id, session)| (*id, session.display_name.clone()))
            .collect()
    }

    pub async fn get_session_info(&self, session_id: SessionId) -> Option<String> {
        let sessions = self.sessions.read().await;
        sessions.get(&session_id).map(|s| s.display_name.clone())
    }

    /// Returns the stable saved-connection id and current display name for a
    /// session. `None` means the session came from a direct/debug connection
    /// and cannot be matched safely to workspace-scoped features.
    pub async fn get_saved_connection_identity(
        &self,
        session_id: SessionId,
    ) -> Option<(String, String)> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(&session_id)?;
        Some((
            session.saved_connection_id.clone()?,
            session.display_name.clone(),
        ))
    }

    /// Returns a stable identifier for the *connection* backing a session.
    pub async fn connection_key(&self, session_id: SessionId) -> Option<String> {
        let sessions = self.sessions.read().await;
        sessions.get(&session_id).map(|s| {
            let c = &s.config;
            format!(
                "{}|{}|{}|{}|{}|{}",
                c.driver,
                c.host,
                c.port,
                c.username,
                c.database.as_deref().unwrap_or(""),
                c.environment,
            )
        })
    }

    pub async fn set_display_name(&self, session_id: SessionId, name: String) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.display_name = name;
        }
    }

    /// Associates a newly-opened session with its saved connection. Keeping
    /// this identity in the session manager prevents callers from spoofing a
    /// connection id when executing workspace-scoped operations.
    pub async fn set_saved_connection_identity(
        &self,
        session_id: SessionId,
        connection_id: String,
        display_name: String,
    ) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.saved_connection_id = Some(connection_id);
            session.display_name = display_name;
        }
    }

    pub async fn is_read_only(&self, session_id: SessionId) -> EngineResult<bool> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| EngineError::session_not_found(session_id.0.to_string()))?;

        Ok(session.config.read_only)
    }

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

    pub async fn session_exists(&self, session_id: SessionId) -> bool {
        let sessions = self.sessions.read().await;
        sessions.contains_key(&session_id)
    }

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

    /// Reopens a broken SSH tunnel on the port the pool already targets. The
    /// caller must close the previous tunnel first so ssh can rebind it.
    async fn reconnect_tunnel(
        config: &ConnectionConfig,
        local_port: u16,
    ) -> EngineResult<SshTunnel> {
        enforce_ssh_host_key_policy(config)?;

        let ssh_config = config
            .ssh_tunnel
            .as_ref()
            .ok_or_else(|| EngineError::SshError {
                message: "No SSH tunnel configured for this session".into(),
            })?;

        tracing::info!(
            "Reconnecting SSH tunnel to {}:{} on local port {}",
            ssh_config.host,
            ssh_config.port,
            local_port
        );

        SshTunnel::open_on_port(ssh_config, &config.host, config.port, local_port).await
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

            if !tunnel_ok {
                // What the frontend was told, so the terminal state is announced.
                let mut announced = previous_health;

                let should_reconnect = {
                    let sessions = self.sessions.read().await;
                    match sessions.get(&sid) {
                        Some(s) => s.consecutive_failures >= Self::RECONNECT_THRESHOLD,
                        None => continue,
                    }
                };

                if should_reconnect {
                    {
                        let mut sessions = self.sessions.write().await;
                        if let Some(s) = sessions.get_mut(&sid) {
                            s.health = ConnectionHealth::Reconnecting;
                        }
                    }

                    if announced != ConnectionHealth::Reconnecting {
                        announced = ConnectionHealth::Reconnecting;
                        events.push(ConnectionHealthEvent {
                            session_id: sid.0.to_string(),
                            health: ConnectionHealth::Reconnecting,
                        });
                    }

                    let reconnect = {
                        let mut sessions = self.sessions.write().await;
                        match sessions.get_mut(&sid) {
                            Some(s) => match s.tunnel.take() {
                                Some(mut old_tunnel) => {
                                    let local_port = old_tunnel.local_port();
                                    let _ = old_tunnel.close().await;
                                    Some((s.config.clone(), local_port))
                                }
                                None => None,
                            },
                            None => continue,
                        }
                    };

                    let Some((config, local_port)) = reconnect else {
                        continue;
                    };

                    match Self::reconnect_tunnel(&config, local_port).await {
                        Ok(new_tunnel) => {
                            {
                                let mut sessions = self.sessions.write().await;
                                match sessions.get_mut(&sid) {
                                    Some(s) => s.tunnel = Some(new_tunnel),
                                    None => continue,
                                }
                            }

                            // A live forwarding says nothing about the pooled
                            // connections behind it.
                            let health = if self.ping(sid).await.is_ok() {
                                ConnectionHealth::Healthy
                            } else {
                                ConnectionHealth::Unhealthy
                            };

                            {
                                let mut sessions = self.sessions.write().await;
                                if let Some(s) = sessions.get_mut(&sid) {
                                    s.health = health;
                                    if health == ConnectionHealth::Healthy {
                                        s.consecutive_failures = 0;
                                    } else {
                                        s.consecutive_failures += 1;
                                    }
                                }
                            }

                            if health == ConnectionHealth::Healthy {
                                tracing::info!("SSH tunnel reconnected for session {}", sid.0);
                            }
                            if health != announced {
                                events.push(ConnectionHealthEvent {
                                    session_id: sid.0.to_string(),
                                    health,
                                });
                            }
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

                let mut sessions = self.sessions.write().await;
                if let Some(s) = sessions.get_mut(&sid) {
                    s.consecutive_failures += 1;
                    s.health = ConnectionHealth::Unhealthy;
                }
                if announced != ConnectionHealth::Unhealthy {
                    events.push(ConnectionHealthEvent {
                        session_id: sid.0.to_string(),
                        health: ConnectionHealth::Unhealthy,
                    });
                }
                continue;
            }

            let ping_result = self.ping(sid).await;

            let new_health = if ping_result.is_ok() {
                ConnectionHealth::Healthy
            } else {
                ConnectionHealth::Unhealthy
            };

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
    #[cfg(feature = "tauri")]
    pub fn start_health_monitor(self: &Arc<Self>, app_handle: tauri::AppHandle) {
        use tauri::Emitter;

        let manager = Arc::clone(self);
        let interval_secs = Self::HEALTH_CHECK_INTERVAL_SECS;

        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            // Skip the immediate first tick that tokio::time::interval fires.
            interval.tick().await;

            loop {
                interval.tick().await;
                let events = manager.run_health_check().await;
                for event in events {
                    let _ = app_handle.emit_to("main", EVENT_CONNECTION_HEALTH, &event);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qore_core::types::{SshAuth, SshTunnelConfig};

    fn config_with(environment: &str, ssh: Option<SshTunnelConfig>) -> ConnectionConfig {
        ConnectionConfig {
            options: Default::default(),
            driver: "postgres".into(),
            host: "db.example.com".into(),
            port: 5432,
            username: "u".into(),
            password: String::new(),
            database: Some("app".into()),
            ssl: false,
            ssl_mode: None,
            environment: environment.into(),
            read_only: false,
            pool_max_connections: None,
            pool_min_connections: None,
            pool_acquire_timeout_secs: None,
            ssh_tunnel: ssh,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
        }
    }

    fn ssh_with(policy: SshHostKeyPolicy) -> SshTunnelConfig {
        SshTunnelConfig {
            host: "bastion.example.com".into(),
            port: 22,
            username: "tunnel".into(),
            auth: SshAuth::Key {
                private_key_path: "id_ed25519".into(),
                passphrase: None,
            },
            host_key_policy: policy,
            known_hosts_path: None,
            proxy_jump: None,
            connect_timeout_secs: 10,
            keepalive_interval_secs: 15,
            keepalive_count_max: 3,
        }
    }

    #[test]
    fn rejects_insecure_host_key_in_production() {
        let config = config_with(
            "production",
            Some(ssh_with(SshHostKeyPolicy::InsecureNoCheck)),
        );
        assert!(enforce_ssh_host_key_policy(&config).is_err());
    }

    #[test]
    fn allows_insecure_host_key_outside_production() {
        let config = config_with(
            "development",
            Some(ssh_with(SshHostKeyPolicy::InsecureNoCheck)),
        );
        assert!(enforce_ssh_host_key_policy(&config).is_ok());
    }

    #[test]
    fn allows_strict_host_key_in_production() {
        let config = config_with("production", Some(ssh_with(SshHostKeyPolicy::Strict)));
        assert!(enforce_ssh_host_key_policy(&config).is_ok());
    }

    #[test]
    fn allows_production_without_ssh_tunnel() {
        let config = config_with("production", None);
        assert!(enforce_ssh_host_key_policy(&config).is_ok());
    }
}

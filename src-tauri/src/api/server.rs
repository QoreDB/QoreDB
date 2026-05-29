// SPDX-License-Identifier: BUSL-1.1

//! HTTP server lifecycle for the Instant Data API.
//!
//! The server binds **strictly** to `127.0.0.1` (never `0.0.0.0`). Lifecycle:
//!   - [`ApiServer::start`] spawns an axum task on the requested port (default
//!     [`DEFAULT_PORT`]) and returns once the listener is bound.
//!   - [`ApiServer::stop`] sends the shutdown signal and drains the cached
//!     sessions (so we don't leak open connections after the user stops the
//!     API or switches workspace).
//!
//! Endpoint state ([`EndpointStore`], [`RateLimiter`]) is shared with the
//! handlers via [`super::handlers::ApiState`] — both server and handlers
//! see the same instances, so a `create_endpoint` after `start` is visible
//! immediately.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::{routing::get, Router};
use axum_server::tls_rustls::RustlsConfig;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};

use qore_drivers::session_manager::SessionManager;

use super::handlers::{handle_endpoint, ApiState};
use super::openapi::{handle_health, handle_openapi};
use super::rate_limit::RateLimiter;
use super::tls::{generate_self_signed, TlsError};
use super::EndpointStore;

/// Default listen port. 4787 spells "QORE" on a phone keypad and lives well
/// above the ephemeral range — easy to remember, unlikely to collide.
pub const DEFAULT_PORT: u16 = 4787;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Instant API server is already running on {0}")]
    AlreadyRunning(SocketAddr),
    #[error("Instant API server is not running")]
    NotRunning,
    #[error("failed to bind {addr}: {source}")]
    Bind {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("TLS setup failed: {0}")]
    Tls(#[from] TlsError),
    #[error("TLS configuration error: {0}")]
    TlsConfig(String),
}

struct RunningServer {
    addr: SocketAddr,
    shutdown: oneshot::Sender<()>,
    started_at: Instant,
    /// True when the server is serving HTTPS (self-signed). Influences the
    /// `base_url` scheme returned in [`InstantApiStatus`].
    tls: bool,
}

pub struct ApiServer {
    inner: Mutex<Option<RunningServer>>,
    store: Arc<EndpointStore>,
    limiter: Arc<RateLimiter>,
    session_manager: Arc<SessionManager>,
    sessions: Arc<Mutex<HashMap<String, qore_core::types::SessionId>>>,
    project_id: String,
    storage_dir: PathBuf,
}

impl ApiServer {
    pub fn new(
        store: Arc<EndpointStore>,
        session_manager: Arc<SessionManager>,
        project_id: String,
        storage_dir: PathBuf,
    ) -> Self {
        Self {
            inner: Mutex::new(None),
            store,
            limiter: Arc::new(RateLimiter::default_capacity()),
            session_manager,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            project_id,
            storage_dir,
        }
    }

    pub async fn is_running(&self) -> bool {
        self.inner.lock().await.is_some()
    }

    pub async fn current_addr(&self) -> Option<SocketAddr> {
        self.inner.lock().await.as_ref().map(|r| r.addr)
    }

    pub async fn uptime_secs(&self) -> Option<u64> {
        self.inner
            .lock()
            .await
            .as_ref()
            .map(|r| r.started_at.elapsed().as_secs())
    }

    /// Returns `true` when the running server is serving HTTPS. `false` when
    /// stopped or running plain HTTP.
    pub async fn is_tls(&self) -> bool {
        self.inner
            .lock()
            .await
            .as_ref()
            .map(|r| r.tls)
            .unwrap_or(false)
    }

    /// Binds the listener and spawns the axum task. Returns the actual
    /// `SocketAddr` (useful when `port == 0` for OS-assigned).
    pub async fn start(&self, port: Option<u16>) -> Result<SocketAddr, ServerError> {
        self.start_with_tls(port, false).await
    }

    /// Same as [`start`] but with optional TLS. When `tls = true`, a fresh
    /// self-signed cert is minted, kept in memory, and used by
    /// `axum_server::bind_rustls`.
    pub async fn start_with_tls(
        &self,
        port: Option<u16>,
        tls: bool,
    ) -> Result<SocketAddr, ServerError> {
        let mut guard = self.inner.lock().await;
        if let Some(existing) = guard.as_ref() {
            return Err(ServerError::AlreadyRunning(existing.addr));
        }

        let requested = port.unwrap_or(DEFAULT_PORT);
        let addr: SocketAddr = ([127, 0, 0, 1], requested).into();

        let started_at = Arc::new(Instant::now());
        let state = ApiState {
            store: Arc::clone(&self.store),
            limiter: Arc::clone(&self.limiter),
            session_manager: Arc::clone(&self.session_manager),
            sessions: Arc::clone(&self.sessions),
            project_id: self.project_id.clone(),
            storage_dir: self.storage_dir.clone(),
            started_at: Arc::clone(&started_at),
        };

        let app: Router = Router::new()
            .route("/api/:name", get(handle_endpoint))
            .route("/openapi.json", get(handle_openapi))
            .route("/health", get(handle_health))
            .with_state(state);

        let (tx, rx) = oneshot::channel::<()>();
        let local_addr = if tls {
            spawn_tls(addr, app, rx).await?
        } else {
            spawn_http(addr, app, rx).await?
        };

        *guard = Some(RunningServer {
            addr: local_addr,
            shutdown: tx,
            started_at: *started_at,
            tls,
        });
        Ok(local_addr)
    }

    /// Sends the shutdown signal, then disconnects all cached sessions.
    pub async fn stop(&self) -> Result<(), ServerError> {
        let running = self
            .inner
            .lock()
            .await
            .take()
            .ok_or(ServerError::NotRunning)?;
        let _ = running.shutdown.send(());

        let mut sessions = self.sessions.lock().await;
        for (_, sid) in sessions.drain() {
            if let Err(e) = self.session_manager.disconnect(sid).await {
                tracing::warn!(error = %e.sanitized_message(), "Failed to disconnect cached Instant API session");
            }
        }
        Ok(())
    }

    /// Forgets the rate-limiter bucket and session cache entry for an
    /// endpoint that was just deleted. Best-effort.
    pub async fn on_endpoint_deleted(&self, endpoint_id: &str, connection_id: &str) {
        self.limiter.forget(endpoint_id);
        // Don't disconnect the cached session — other endpoints on the same
        // connection may still be using it. Sessions are dropped on stop().
        let _ = connection_id;
    }
}

/// Binds a plain-HTTP listener, spawns the axum task, and returns the actual
/// bound address. The graceful-shutdown future awaits `shutdown_rx`.
async fn spawn_http(
    addr: SocketAddr,
    app: Router,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<SocketAddr, ServerError> {
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|source| ServerError::Bind { addr, source })?;
    let local_addr = listener
        .local_addr()
        .map_err(|source| ServerError::Bind { addr, source })?;

    tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        });
        if let Err(e) = server.await {
            tracing::error!(error = %e, "Instant API server stopped with error");
        }
    });
    Ok(local_addr)
}

/// Same as [`spawn_http`] but wraps the listener with Rustls using a freshly
/// minted self-signed certificate. The cert never touches disk — it lives in
/// the spawned task and is dropped when the server stops.
async fn spawn_tls(
    addr: SocketAddr,
    app: Router,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<SocketAddr, ServerError> {
    let bundle = generate_self_signed()?;
    let tls_config =
        RustlsConfig::from_pem(bundle.cert_pem.into_bytes(), bundle.key_pem.into_bytes())
            .await
            .map_err(|e| ServerError::TlsConfig(e.to_string()))?;

    let handle = axum_server::Handle::new();
    let bound = handle.clone();
    let handle_for_shutdown = handle.clone();
    let local_addr = {
        // axum-server doesn't expose `bind()` synchronously the way TcpListener
        // does; we resolve the port lazily through `Handle::listening`.
        let server_handle = tokio::spawn(async move {
            let server = axum_server::bind_rustls(addr, tls_config)
                .handle(bound)
                .serve(app.into_make_service());
            if let Err(e) = server.await {
                tracing::error!(error = %e, "Instant API HTTPS server stopped with error");
            }
        });
        // Wait for the listener to bind (or fail). `listening()` resolves to
        // `Some(addr)` once the bind succeeds, or `None` on bind error.
        let resolved = handle.listening().await;
        match resolved {
            Some(actual) => {
                // Hook up the shutdown receiver so a `stop()` call gracefully
                // shuts down the axum-server.
                tokio::spawn(async move {
                    if shutdown_rx.await.is_ok() {
                        handle_for_shutdown
                            .graceful_shutdown(Some(std::time::Duration::from_secs(2)));
                    }
                });
                actual
            }
            None => {
                // Bind failed — drop the spawn handle and propagate.
                server_handle.abort();
                return Err(ServerError::Bind {
                    addr,
                    source: std::io::Error::new(
                        std::io::ErrorKind::AddrInUse,
                        "axum-server failed to bind TLS listener",
                    ),
                });
            }
        }
    };
    Ok(local_addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn empty_store(tmp: &TempDir) -> Arc<EndpointStore> {
        Arc::new(EndpointStore::new(tmp.path().to_path_buf()).unwrap())
    }

    fn mock_session_manager() -> Arc<SessionManager> {
        use qore_core::registry::DriverRegistry;
        Arc::new(SessionManager::new(Arc::new(DriverRegistry::new())))
    }

    #[tokio::test]
    async fn start_and_stop_bind_ephemeral_port() {
        let tmp = TempDir::new().unwrap();
        let server = ApiServer::new(
            empty_store(&tmp),
            mock_session_manager(),
            "test".into(),
            tmp.path().to_path_buf(),
        );
        let addr = server.start(Some(0)).await.unwrap();
        assert!(addr.ip().is_loopback());
        assert!(server.is_running().await);
        assert!(server.current_addr().await.is_some());
        server.stop().await.unwrap();
        assert!(!server.is_running().await);
    }

    #[tokio::test]
    async fn start_twice_returns_already_running() {
        let tmp = TempDir::new().unwrap();
        let server = ApiServer::new(
            empty_store(&tmp),
            mock_session_manager(),
            "test".into(),
            tmp.path().to_path_buf(),
        );
        server.start(Some(0)).await.unwrap();
        let err = server.start(Some(0)).await.unwrap_err();
        assert!(matches!(err, ServerError::AlreadyRunning(_)));
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn stop_when_not_running_errors() {
        let tmp = TempDir::new().unwrap();
        let server = ApiServer::new(
            empty_store(&tmp),
            mock_session_manager(),
            "test".into(),
            tmp.path().to_path_buf(),
        );
        let err = server.stop().await.unwrap_err();
        assert!(matches!(err, ServerError::NotRunning));
    }
}

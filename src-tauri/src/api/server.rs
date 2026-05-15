// SPDX-License-Identifier: BUSL-1.1

//! HTTP server lifecycle (stub — Phase 2B fills this in).

use std::net::SocketAddr;
use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;

use super::EndpointStore;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("server already running on {0}")]
    AlreadyRunning(SocketAddr),
    #[error("server is not running")]
    NotRunning,
    #[error("bind failed: {0}")]
    Bind(String),
}

pub struct ApiServer {
    inner: Mutex<Option<RunningServer>>,
    #[allow(dead_code)]
    store: Arc<EndpointStore>,
}

#[allow(dead_code)]
struct RunningServer {
    addr: SocketAddr,
    shutdown: tokio::sync::oneshot::Sender<()>,
    started_at: std::time::Instant,
}

impl ApiServer {
    pub fn new(store: Arc<EndpointStore>) -> Self {
        Self {
            inner: Mutex::new(None),
            store,
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
}

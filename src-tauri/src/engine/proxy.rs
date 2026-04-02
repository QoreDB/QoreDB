// SPDX-License-Identifier: Apache-2.0

//! Network Proxy Support
//!
//! Provides HTTP CONNECT and SOCKS5 proxy tunneling for connecting to
//! databases in corporate network environments.
//!
//! Works like the SSH tunnel: binds a local TCP listener and relays
//! connections through the proxy to the target database server.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::time::{timeout, Duration};

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::types::{ProxyConfig, ProxyType};

/// Handle for an active proxy tunnel.
pub struct ProxyTunnel {
    local_port: u16,
    shutdown: Arc<Notify>,
}

impl std::fmt::Debug for ProxyTunnel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxyTunnel")
            .field("local_port", &self.local_port)
            .finish()
    }
}

impl ProxyTunnel {
    /// Opens a proxy tunnel to the remote target.
    ///
    /// Binds a local TCP listener on `127.0.0.1:0` and spawns a background
    /// task that forwards each incoming connection through the configured
    /// proxy to `remote_host:remote_port`.
    pub async fn open(
        config: &ProxyConfig,
        remote_host: &str,
        remote_port: u16,
    ) -> EngineResult<Self> {
        // Validate proxy config
        if config.host.trim().is_empty() {
            return Err(EngineError::ProxyError {
                message: "Proxy host is required".into(),
            });
        }
        if config.port == 0 {
            return Err(EngineError::ProxyError {
                message: "Proxy port must be greater than 0".into(),
            });
        }

        // Test that we can actually reach the proxy
        let proxy_addr = format!("{}:{}", config.host, config.port);
        let connect_timeout = Duration::from_secs(config.connect_timeout_secs as u64);

        let test_stream = timeout(connect_timeout, TcpStream::connect(&proxy_addr))
            .await
            .map_err(|_| EngineError::ProxyError {
                message: format!(
                    "Timed out connecting to proxy {}:{} ({}s)",
                    config.host, config.port, config.connect_timeout_secs
                ),
            })?
            .map_err(|e| EngineError::ProxyError {
                message: format!("Failed to connect to proxy {}:{}: {}", config.host, config.port, e),
            })?;
        drop(test_stream);

        // Bind local listener
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| EngineError::ProxyError {
                message: format!("Failed to bind local port: {}", e),
            })?;

        let local_port = listener
            .local_addr()
            .map_err(|e| EngineError::ProxyError {
                message: format!("Failed to get local address: {}", e),
            })?
            .port();

        let shutdown = Arc::new(Notify::new());
        let shutdown_rx = shutdown.clone();
        let config_clone = config.clone();
        let remote_host_str = remote_host.to_string();

        tracing::info!(
            "Proxy tunnel open: 127.0.0.1:{} → {:?} {}:{} → {}:{}",
            local_port,
            config.proxy_type,
            config.host,
            config.port,
            remote_host,
            remote_port
        );

        // Spawn acceptor loop
        tokio::spawn(async move {
            let config = config_clone;
            let remote_host = remote_host_str;
            loop {
                tokio::select! {
                    _ = shutdown_rx.notified() => break,
                    accept_result = listener.accept() => {
                        match accept_result {
                            Ok((client_stream, _)) => {
                                let config = config.clone();
                                let remote_host = remote_host.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_client(
                                        client_stream,
                                        &config,
                                        &remote_host,
                                        remote_port,
                                    ).await {
                                        tracing::warn!("Proxy relay error: {}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                tracing::warn!("Proxy accept error: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            local_port,
            shutdown,
        })
    }

    /// Returns the local port to connect to.
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    /// Closes the proxy tunnel.
    pub async fn close(&mut self) -> EngineResult<()> {
        self.shutdown.notify_one();
        Ok(())
    }
}

/// Handles a single client connection by tunneling it through the proxy.
async fn handle_client(
    client_stream: TcpStream,
    config: &ProxyConfig,
    remote_host: &str,
    remote_port: u16,
) -> EngineResult<()> {
    let connect_timeout = Duration::from_secs(config.connect_timeout_secs as u64);

    let proxy_stream = match config.proxy_type {
        ProxyType::Socks5 => {
            connect_socks5(config, remote_host, remote_port, connect_timeout).await?
        }
        ProxyType::HttpConnect => {
            connect_http(config, remote_host, remote_port, connect_timeout).await?
        }
    };

    // Bidirectional relay
    let (mut client_read, mut client_write) = tokio::io::split(client_stream);
    let (mut proxy_read, mut proxy_write) = tokio::io::split(proxy_stream);

    tokio::select! {
        r = tokio::io::copy(&mut client_read, &mut proxy_write) => {
            if let Err(e) = r {
                tracing::trace!("Client→proxy relay ended: {}", e);
            }
        }
        r = tokio::io::copy(&mut proxy_read, &mut client_write) => {
            if let Err(e) = r {
                tracing::trace!("Proxy→client relay ended: {}", e);
            }
        }
    }

    Ok(())
}

/// Connects through a SOCKS5 proxy using tokio-socks.
async fn connect_socks5(
    config: &ProxyConfig,
    remote_host: &str,
    remote_port: u16,
    connect_timeout: Duration,
) -> EngineResult<TcpStream> {
    let proxy_addr = format!("{}:{}", config.host, config.port);
    let target = (remote_host, remote_port);

    let stream = timeout(connect_timeout, async {
        match (&config.username, &config.password) {
            (Some(user), Some(pass)) if !user.is_empty() => {
                tokio_socks::tcp::Socks5Stream::connect_with_password(
                    proxy_addr.as_str(),
                    target,
                    user,
                    pass,
                )
                .await
            }
            _ => {
                tokio_socks::tcp::Socks5Stream::connect(proxy_addr.as_str(), target).await
            }
        }
    })
    .await
    .map_err(|_| EngineError::ProxyError {
        message: format!(
            "SOCKS5 connection timed out ({}s)",
            connect_timeout.as_secs()
        ),
    })?
    .map_err(|e| EngineError::ProxyError {
        message: format!("SOCKS5 handshake failed: {}", e),
    })?;

    Ok(stream.into_inner())
}

/// Connects through an HTTP CONNECT proxy.
async fn connect_http(
    config: &ProxyConfig,
    remote_host: &str,
    remote_port: u16,
    connect_timeout: Duration,
) -> EngineResult<TcpStream> {
    let proxy_addr = format!("{}:{}", config.host, config.port);

    let mut stream = timeout(connect_timeout, TcpStream::connect(&proxy_addr))
        .await
        .map_err(|_| EngineError::ProxyError {
            message: format!(
                "HTTP CONNECT timed out connecting to proxy ({}s)",
                connect_timeout.as_secs()
            ),
        })?
        .map_err(|e| EngineError::ProxyError {
            message: format!("Failed to connect to HTTP proxy: {}", e),
        })?;

    // Build CONNECT request
    let mut request = format!("CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n", remote_host, remote_port, remote_host, remote_port);

    // Add proxy authentication if provided
    if let (Some(user), Some(pass)) = (&config.username, &config.password) {
        if !user.is_empty() {
            use base64::Engine;
            let credentials = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", user, pass));
            request.push_str(&format!("Proxy-Authorization: Basic {}\r\n", credentials));
        }
    }

    request.push_str("\r\n");

    // Send CONNECT request
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| EngineError::ProxyError {
            message: format!("Failed to send CONNECT request: {}", e),
        })?;

    // Read response (we only need the status line)
    let mut buf = [0u8; 1024];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| EngineError::ProxyError {
            message: format!("Failed to read CONNECT response: {}", e),
        })?;

    let response = String::from_utf8_lossy(&buf[..n]);
    let status_line = response.lines().next().unwrap_or("");

    // Check for 200 OK
    if !status_line.contains("200") {
        return Err(EngineError::ProxyError {
            message: format!(
                "HTTP CONNECT failed: {}",
                sanitize_proxy_response(status_line)
            ),
        });
    }

    Ok(stream)
}

/// Sanitize proxy response to avoid leaking sensitive information.
fn sanitize_proxy_response(response: &str) -> String {
    let mut s = response.to_string();
    if s.len() > 200 {
        s.truncate(200);
        s.push_str("...");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_long_proxy_response() {
        let long = "x".repeat(300);
        let sanitized = sanitize_proxy_response(&long);
        assert!(sanitized.len() <= 204);
    }

    #[tokio::test]
    async fn rejects_empty_proxy_host() {
        let config = ProxyConfig {
            proxy_type: ProxyType::Socks5,
            host: "".to_string(),
            port: 1080,
            username: None,
            password: None,
            connect_timeout_secs: 5,
        };
        let err = ProxyTunnel::open(&config, "db.example.com", 5432)
            .await
            .expect_err("empty host should fail");
        match err {
            EngineError::ProxyError { message } => assert!(message.contains("host is required")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_zero_proxy_port() {
        let config = ProxyConfig {
            proxy_type: ProxyType::HttpConnect,
            host: "proxy.corp.local".to_string(),
            port: 0,
            username: None,
            password: None,
            connect_timeout_secs: 5,
        };
        let err = ProxyTunnel::open(&config, "db.example.com", 5432)
            .await
            .expect_err("zero port should fail");
        match err {
            EngineError::ProxyError { message } => {
                assert!(message.contains("port must be greater than 0"))
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}

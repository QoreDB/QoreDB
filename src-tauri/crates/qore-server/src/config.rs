// SPDX-License-Identifier: BUSL-1.1

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use base64::Engine as _;
use rand::RngCore;

pub const DEFAULT_PORT: u16 = 8088;
pub use qore_service::paths::{PROJECT_ID, QUERY_TIMEOUT_MS};

pub struct ServerConfig {
    pub addr: SocketAddr,
    pub token: String,
    pub token_generated: bool,
    pub config_dir: PathBuf,
    pub web_dir: Option<PathBuf>,
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let host = std::env::var("QORE_SERVER_HOST")
            .ok()
            .and_then(|h| h.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let port = std::env::var("QORE_SERVER_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(DEFAULT_PORT);
        let (token, token_generated) = match std::env::var("QORE_SERVER_TOKEN") {
            Ok(t) if !t.is_empty() => (t, false),
            _ => (generate_token(), true),
        };
        let web_dir = std::env::var("QORE_SERVER_WEB_DIR")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        let tls_cert = std::env::var("QORE_SERVER_TLS_CERT")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        let tls_key = std::env::var("QORE_SERVER_TLS_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);

        Self {
            addr: SocketAddr::new(host, port),
            token,
            token_generated,
            config_dir: qore_service::paths::config_dir(),
            web_dir,
            tls_cert,
            tls_key,
        }
    }
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    format!("qsrv-{raw}")
}


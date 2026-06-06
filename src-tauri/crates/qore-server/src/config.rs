// SPDX-License-Identifier: BUSL-1.1

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use base64::Engine as _;
use rand::RngCore;

pub const DEFAULT_PORT: u16 = 8088;
pub const PROJECT_ID: &str = "default";
pub const QUERY_TIMEOUT_MS: u64 = 30_000;

pub struct ServerConfig {
    pub addr: SocketAddr,
    pub token: String,
    pub token_generated: bool,
    pub config_dir: PathBuf,
    pub web_dir: Option<PathBuf>,
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

        Self {
            addr: SocketAddr::new(host, port),
            token,
            token_generated,
            config_dir: config_dir(),
            web_dir,
        }
    }
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    format!("qsrv-{raw}")
}

fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("QOREDB_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.rapha.qoredb")
}

//! Saved connection credentials
//!
//! Represents a saved database connection with credentials.

use serde::{Deserialize, Serialize};

use crate::engine::types::{ConnectionConfig, SshTunnelConfig};

/// Environment classification for connections
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    #[default]
    Development,
    Staging,
    Production,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
}

/// A saved connection (credentials stored separately in vault)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedConnection {
    /// Unique identifier for this connection
    pub id: String,
    /// Display name
    pub name: String,
    /// Driver type (postgres, mysql, mongodb)
    pub driver: String,
    /// Environment classification (dev/staging/prod)
    #[serde(default)]
    pub environment: Environment,
    /// Read-only mode (blocks mutations)
    #[serde(default)]
    pub read_only: bool,
    /// Host address
    pub host: String,
    /// Port number
    pub port: u16,
    /// Username
    pub username: String,
    /// Database name (optional)
    pub database: Option<String>,
    /// Use SSL/TLS
    pub ssl: bool,
    /// SSH tunnel configuration (without credentials)
    pub ssh_tunnel: Option<SshTunnelInfo>,
    /// Project ID for isolation
    pub project_id: String,
}

/// SSH tunnel info (credentials stored separately)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshTunnelInfo {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// "password" or "key"
    pub auth_type: String,
    /// Path to private key (if key auth)
    pub key_path: Option<String>,
}

/// Credentials stored in the vault (never serialized to frontend)
#[derive(Debug, Clone)]
pub struct StoredCredentials {
    pub db_password: String,
    pub ssh_password: Option<String>,
    pub ssh_key_passphrase: Option<String>,
}

impl SavedConnection {
    /// Converts to a ConnectionConfig for connecting
    pub fn to_connection_config(&self, creds: &StoredCredentials) -> ConnectionConfig {
        let ssh_tunnel = self.ssh_tunnel.as_ref().map(|ssh| {
            use crate::engine::types::SshAuth;
            
            let auth = if ssh.auth_type == "key" {
                SshAuth::Key {
                    private_key_path: ssh.key_path.clone().unwrap_or_default(),
                    passphrase: creds.ssh_key_passphrase.clone(),
                }
            } else {
                SshAuth::Password {
                    password: creds.ssh_password.clone().unwrap_or_default(),
                }
            };

            SshTunnelConfig {
                host: ssh.host.clone(),
                port: ssh.port,
                username: ssh.username.clone(),
                auth,
            }
        });

        ConnectionConfig {
            driver: self.driver.clone(),
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            password: creds.db_password.clone(),
            database: self.database.clone(),
            ssl: self.ssl,
            environment: Some(self.environment.as_str().to_string()),
            read_only: self.read_only,
            ssh_tunnel,
        }
    }
}

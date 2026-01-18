//! Saved connection credentials
//!
//! Represents a saved database connection with credentials.

use serde::{Deserialize, Serialize};

use crate::engine::types::{ConnectionConfig, SshTunnelConfig};
use crate::engine::error::{EngineError, EngineResult};

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
    /// Driver type
    pub driver: String,
    /// Environment classification (dev/staging/prod)
    pub environment: Environment,
    /// Read-only mode
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

    /// Host key policy (e.g. "accept_new", "strict", "insecure_no_check")
    pub host_key_policy: String,

    /// Optional jump host/bastion (e.g. "user@bastion:22")
    pub proxy_jump: Option<String>,

    /// Connection timeout in seconds for the SSH TCP handshake.
    pub connect_timeout_secs: u32,

    /// SSH keepalive interval in seconds.
    pub keepalive_interval_secs: u32,

    /// Max number of keepalive failures before disconnect.
    pub keepalive_count_max: u32,
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
    pub fn to_connection_config(&self, creds: &StoredCredentials) -> EngineResult<ConnectionConfig> {
        let ssh_tunnel = match self.ssh_tunnel.as_ref() {
            Some(ssh) => {
            use crate::engine::types::SshAuth;
            use crate::engine::types::SshHostKeyPolicy;
            
            let auth = match ssh.auth_type.as_str() {
                "key" => {
                    let key_path = ssh.key_path.clone().ok_or_else(|| {
                        EngineError::internal("key_path must be set when auth_type is 'key'")
                    })?;
                    SshAuth::Key {
                        private_key_path: key_path,
                        passphrase: creds.ssh_key_passphrase.clone(),
                    }
                }
                "password" => SshAuth::Password {
                    password: creds
                        .ssh_password
                        .clone()
                        .ok_or_else(|| EngineError::internal("ssh_password is missing"))?,
                },
                other => {
                    return Err(EngineError::internal(format!(
                        "Invalid ssh auth_type: {}",
                        other
                    )))
                }
            };

            let host_key_policy = match ssh.host_key_policy.as_str() {
                "accept_new" => SshHostKeyPolicy::AcceptNew,
                "strict" => SshHostKeyPolicy::Strict,
                "insecure_no_check" => SshHostKeyPolicy::InsecureNoCheck,
                other => {
                    return Err(EngineError::internal(format!(
                        "Invalid ssh host_key_policy: {}",
                        other
                    )))
                }
            };

            Some(SshTunnelConfig {
                host: ssh.host.clone(),
                port: ssh.port,
                username: ssh.username.clone(),
                auth,

                host_key_policy,
                known_hosts_path: None,
                proxy_jump: ssh.proxy_jump.clone(),
                connect_timeout_secs: ssh.connect_timeout_secs,
                keepalive_interval_secs: ssh.keepalive_interval_secs,
                keepalive_count_max: ssh.keepalive_count_max,
            })
            }
            None => None,
        };

        Ok(ConnectionConfig {
            driver: self.driver.clone(),
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            password: creds.db_password.clone(),
            database: self.database.clone(),
            ssl: self.ssl,
            environment: self.environment.as_str().to_string(),
            read_only: self.read_only,
            ssh_tunnel,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::{SshAuth, SshHostKeyPolicy};

    fn base_connection(auth_type: &str, host_key_policy: &str) -> SavedConnection {
        SavedConnection {
            id: "conn1".to_string(),
            name: "Test".to_string(),
            driver: "postgres".to_string(),
            environment: Environment::Development,
            read_only: false,
            host: "localhost".to_string(),
            port: 5432,
            username: "qoredb".to_string(),
            database: Some("testdb".to_string()),
            ssl: false,
            ssh_tunnel: Some(SshTunnelInfo {
                host: "ssh.local".to_string(),
                port: 22,
                username: "sshuser".to_string(),
                auth_type: auth_type.to_string(),
                key_path: Some("id_ed25519".to_string()),
                host_key_policy: host_key_policy.to_string(),
                proxy_jump: None,
                connect_timeout_secs: 10,
                keepalive_interval_secs: 30,
                keepalive_count_max: 3,
            }),
            project_id: "proj".to_string(),
        }
    }

    #[test]
    fn ssh_password_config_is_built() -> EngineResult<()> {
        let mut connection = base_connection("password", "accept_new");
        if let Some(ref mut ssh) = connection.ssh_tunnel {
            ssh.key_path = None;
        }

        let creds = StoredCredentials {
            db_password: "db".to_string(),
            ssh_password: Some("sshpw".to_string()),
            ssh_key_passphrase: None,
        };

        let config = connection.to_connection_config(&creds)?;
        let ssh = config.ssh_tunnel.expect("ssh config missing");

        match ssh.auth {
            SshAuth::Password { password } => assert_eq!(password, "sshpw"),
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(ssh.host_key_policy, SshHostKeyPolicy::AcceptNew);

        Ok(())
    }

    #[test]
    fn ssh_key_config_is_built() -> EngineResult<()> {
        let connection = base_connection("key", "strict");
        let creds = StoredCredentials {
            db_password: "db".to_string(),
            ssh_password: None,
            ssh_key_passphrase: Some("passphrase".to_string()),
        };

        let config = connection.to_connection_config(&creds)?;
        let ssh = config.ssh_tunnel.expect("ssh config missing");

        match ssh.auth {
            SshAuth::Key {
                private_key_path,
                passphrase,
            } => {
                assert_eq!(private_key_path, "id_ed25519");
                assert_eq!(passphrase.as_deref(), Some("passphrase"));
            }
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(ssh.host_key_policy, SshHostKeyPolicy::Strict);

        Ok(())
    }

    #[test]
    fn rejects_invalid_auth_type() {
        let connection = base_connection("token", "accept_new");
        let creds = StoredCredentials {
            db_password: "db".to_string(),
            ssh_password: Some("sshpw".to_string()),
            ssh_key_passphrase: None,
        };

        let err = connection
            .to_connection_config(&creds)
            .expect_err("invalid auth_type should error");
        assert!(err.to_string().contains("Invalid ssh auth_type"));
    }

    #[test]
    fn rejects_invalid_host_key_policy() {
        let connection = base_connection("password", "unknown");
        let creds = StoredCredentials {
            db_password: "db".to_string(),
            ssh_password: Some("sshpw".to_string()),
            ssh_key_passphrase: None,
        };

        let err = connection
            .to_connection_config(&creds)
            .expect_err("invalid host_key_policy should error");
        assert!(err.to_string().contains("Invalid ssh host_key_policy"));
    }
}

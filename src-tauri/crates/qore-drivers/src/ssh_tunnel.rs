// SPDX-License-Identifier: Apache-2.0

//! SSH Tunnel
//!
//! Provides SSH tunneling for connecting to databases behind firewalls.
//! Uses the native OpenSSH client for maximum compatibility.

use std::process::Stdio;
use std::{fs, path::PathBuf};

use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use qore_core::error::{EngineError, EngineResult};
use qore_core::types::{SshAuth, SshHostKeyPolicy, SshTunnelConfig};

/// Handle for an active SSH tunnel.
#[async_trait]
pub trait SshTunnelHandle: Send {
    fn local_port(&self) -> u16;
    async fn close(&mut self) -> EngineResult<()>;
}

/// Pluggable backend for SSH tunnels.
#[async_trait]
pub trait SshTunnelBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn supports_auth(&self, auth: &SshAuth) -> bool;
    async fn open(
        &self,
        config: &SshTunnelConfig,
        remote_host: &str,
        remote_port: u16,
    ) -> EngineResult<Box<dyn SshTunnelHandle>>;
}

/// Represents an active SSH tunnel, regardless of backend.
pub struct SshTunnel {
    local_port: u16,
    handle: Mutex<Box<dyn SshTunnelHandle + Send>>,
}

impl SshTunnel {
    /// Opens an SSH tunnel to the remote database using the selected backend.
    pub async fn open(
        config: &SshTunnelConfig,
        remote_host: &str,
        remote_port: u16,
    ) -> EngineResult<Self> {
        let backend = select_backend(config)?;
        let handle = backend.open(config, remote_host, remote_port).await?;
        let local_port = handle.local_port();
        Ok(Self {
            local_port,
            handle: Mutex::new(handle),
        })
    }

    /// Returns the local port to connect to
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    /// Returns the local address to use for database connection
    pub fn local_addr(&self) -> String {
        format!("127.0.0.1:{}", self.local_port())
    }

    /// Closes the tunnel
    pub async fn close(&mut self) -> EngineResult<()> {
        let mut handle = self.handle.lock().await;
        handle.close().await
    }
}

fn select_backend(config: &SshTunnelConfig) -> EngineResult<Box<dyn SshTunnelBackend>> {
    let backends: Vec<Box<dyn SshTunnelBackend>> = vec![Box::new(OpenSshBackend)];

    for backend in backends {
        if backend.supports_auth(&config.auth) {
            return Ok(backend);
        }
    }

    let auth_label = match config.auth {
        SshAuth::Password { .. } => "password",
        SshAuth::Key { .. } => "key",
    };

    Err(EngineError::SshError {
        message: format!(
            "No SSH tunnel backend supports {} authentication. Configure key-based auth or enable the embedded backend.",
            auth_label
        ),
    })
}

struct OpenSshTunnel {
    local_port: u16,
    process: Option<Child>,
}

struct OpenSshBackend;

#[async_trait]
impl SshTunnelBackend for OpenSshBackend {
    fn name(&self) -> &'static str {
        "openssh"
    }

    fn supports_auth(&self, auth: &SshAuth) -> bool {
        matches!(auth, SshAuth::Key { .. })
    }

    async fn open(
        &self,
        config: &SshTunnelConfig,
        remote_host: &str,
        remote_port: u16,
    ) -> EngineResult<Box<dyn SshTunnelHandle>> {
        // Find an available local port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| EngineError::SshError {
                message: format!("Failed to bind local port: {}", e),
            })?;

        let local_port = listener
            .local_addr()
            .map_err(|e| EngineError::SshError {
                message: format!("Failed to get local address: {}", e),
            })?
            .port();

        // Drop the listener so ssh can bind to this port
        drop(listener);

        // Validate private key file exists before building the SSH command
        if let SshAuth::Key {
            private_key_path, ..
        } = &config.auth
        {
            validate_private_key_path(private_key_path)?;
        }

        let known_hosts_path = config
            .known_hosts_path
            .clone()
            .unwrap_or_else(default_known_hosts_path);
        ensure_parent_dir_exists(&known_hosts_path)?;

        let mut cmd = build_ssh_command(
            config,
            &known_hosts_path,
            local_port,
            remote_host,
            remote_port,
        )?;

        // Spawn the SSH process
        let mut process = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| EngineError::SshError {
                message: format!("Failed to spawn SSH process: {}. Is OpenSSH installed?", e),
            })?;

        // Wait until ssh is actually listening on the local port, or fail with stderr.
        let startup_deadline = tokio::time::Instant::now()
            + tokio::time::Duration::from_millis(Self::STARTUP_TIMEOUT_MS);

        loop {
            // If the process exited early, surface stderr.
            if let Some(status) = process.try_wait().map_err(|e| EngineError::SshError {
                message: format!("Failed to check SSH process status: {}", e),
            })? {
                let stderr = match process.stderr.take() {
                    Some(mut s) => {
                        let mut buf = Vec::new();
                        let _ = s.read_to_end(&mut buf).await;
                        String::from_utf8_lossy(&buf).trim().to_string()
                    }
                    None => String::new(),
                };

                return Err(EngineError::SshError {
                    message: format!(
                        "SSH tunnel process exited (status: {}). {}",
                        status,
                        if stderr.is_empty() {
                            "No stderr output was captured.".to_string()
                        } else {
                            format!("stderr: {}", sanitize_ssh_stderr(&stderr))
                        }
                    ),
                });
            }

            // Port is open?
            match tokio::net::TcpStream::connect(("127.0.0.1", local_port)).await {
                Ok(stream) => {
                    drop(stream);
                    break;
                }
                Err(_) => {
                    if tokio::time::Instant::now() >= startup_deadline {
                        return Err(EngineError::SshError {
                            message: format!(
                                "SSH tunnel did not become ready within {}ms. Ensure host key is trusted and OpenSSH supports StrictHostKeyChecking=accept-new.",
                                Self::STARTUP_TIMEOUT_MS
                            ),
                        });
                    }

                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        Self::STARTUP_POLL_INTERVAL_MS,
                    ))
                    .await;
                }
            }
        }

        Ok(Box::new(OpenSshTunnel {
            local_port,
            process: Some(process),
        }))
    }
}

impl OpenSshBackend {
    const STARTUP_TIMEOUT_MS: u64 = 5_000;
    const STARTUP_POLL_INTERVAL_MS: u64 = 50;
}

#[async_trait]
impl SshTunnelHandle for OpenSshTunnel {
    fn local_port(&self) -> u16 {
        self.local_port
    }

    async fn close(&mut self) -> EngineResult<()> {
        if let Some(mut process) = self.process.take() {
            process.kill().await.map_err(|e| EngineError::SshError {
                message: format!("Failed to kill SSH process: {}", e),
            })?;
        }
        Ok(())
    }
}

fn build_ssh_command(
    config: &SshTunnelConfig,
    known_hosts_path: &str,
    local_port: u16,
    remote_host: &str,
    remote_port: u16,
) -> EngineResult<Command> {
    // ssh -N -L 127.0.0.1:local_port:remote_host:remote_port user@ssh_host -p ssh_port
    let mut cmd = Command::new("ssh");

    // Use only our app-owned known_hosts file for deterministic behavior.
    let null_device = null_device_path();

    let connect_timeout_secs = config.connect_timeout_secs;
    let keepalive_interval_secs = config.keepalive_interval_secs;
    let keepalive_count_max = config.keepalive_count_max;

    let strict_host_key_checking = match config.host_key_policy {
        SshHostKeyPolicy::AcceptNew => "accept-new",
        SshHostKeyPolicy::Strict => "yes",
        SshHostKeyPolicy::InsecureNoCheck => "no",
    };

    cmd.arg("-N")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ExitOnForwardFailure=yes")
        .arg("-o")
        .arg(format!("ConnectTimeout={}", connect_timeout_secs))
        .arg("-o")
        .arg(format!("ServerAliveInterval={}", keepalive_interval_secs))
        .arg("-o")
        .arg(format!("ServerAliveCountMax={}", keepalive_count_max))
        .arg("-o")
        .arg(format!(
            "StrictHostKeyChecking={}",
            strict_host_key_checking
        ))
        .arg("-o")
        .arg(format!("UserKnownHostsFile={}", known_hosts_path))
        .arg("-o")
        .arg(format!("GlobalKnownHostsFile={}", null_device))
        .arg("-o")
        .arg("IdentitiesOnly=yes")
        .arg("-o")
        .arg("PreferredAuthentications=publickey")
        .arg("-L")
        .arg(format!(
            "127.0.0.1:{}:{}:{}",
            local_port, remote_host, remote_port
        ))
        .arg("-p")
        .arg(config.port.to_string());

    if let Some(proxy_jump) = config.proxy_jump.as_deref() {
        if !proxy_jump.trim().is_empty() {
            validate_proxy_jump(proxy_jump)?;
            cmd.arg("-J").arg(proxy_jump);
        }
    }

    match &config.auth {
        SshAuth::Password { .. } => {
            return Err(EngineError::SshError {
                message: "Password authentication is not supported by the native OpenSSH tunnel backend. Use SSH keys (preferably via ssh-agent).".into(),
            });
        }
        SshAuth::Key {
            private_key_path,
            passphrase,
        } => {
            if passphrase.as_deref().is_some_and(|p| !p.is_empty()) {
                return Err(EngineError::SshError {
                    message: "Key passphrase was provided but is not supported by the native OpenSSH tunnel backend. Load the key into ssh-agent (recommended) or use an unencrypted key.".into(),
                });
            }
            cmd.arg("-i").arg(private_key_path);
        }
    }

    cmd.arg(format!("{}@{}", config.username, config.host));
    Ok(cmd)
}

fn default_known_hosts_path() -> String {
    // Per-user, app-owned file.
    // Windows: %APPDATA%\QoreDB\ssh\known_hosts
    // Others:  $HOME/.qoredb/ssh/known_hosts
    if cfg!(windows) {
        let appdata = std::env::var_os("APPDATA")
            .unwrap_or_else(|| std::env::var_os("USERPROFILE").unwrap_or_default());
        let mut path = PathBuf::from(appdata);
        path.push("QoreDB");
        path.push("ssh");
        path.push("known_hosts");
        path.to_string_lossy().to_string()
    } else {
        let home = std::env::var_os("HOME").unwrap_or_default();
        let mut path = PathBuf::from(home);
        path.push(".qoredb");
        path.push("ssh");
        path.push("known_hosts");
        path.to_string_lossy().to_string()
    }
}

fn null_device_path() -> &'static str {
    if cfg!(windows) {
        "NUL"
    } else {
        "/dev/null"
    }
}

/// Validate proxy_jump format: [user@]host[:port], no spaces or shell-dangerous chars.
fn validate_proxy_jump(proxy_jump: &str) -> EngineResult<()> {
    let re = regex::Regex::new(r"^([a-zA-Z0-9._-]+@)?[a-zA-Z0-9._-]+(:\d{1,5})?$").unwrap();
    if !re.is_match(proxy_jump) {
        return Err(EngineError::SshError {
            message: format!(
                "Invalid proxy jump format: '{}'. Expected [user@]host[:port].",
                proxy_jump
            ),
        });
    }
    Ok(())
}

/// Validate that the private key file exists.
fn validate_private_key_path(path: &str) -> EngineResult<()> {
    let p = std::path::Path::new(path);
    if !p.is_file() {
        return Err(EngineError::SshError {
            message: format!(
                "SSH private key file not found: '{}'. Check the path and permissions.",
                path
            ),
        });
    }
    Ok(())
}

/// Sanitize SSH stderr output to avoid leaking sensitive internal details.
fn sanitize_ssh_stderr(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .filter(|line| !line.contains('@') || line.contains("Permission denied"))
        .collect();

    let mut sanitized = lines.join("\n");

    // Redact IPv4 addresses
    let ip_re = regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap();
    sanitized = ip_re.replace_all(&sanitized, "[redacted-ip]").to_string();

    if sanitized.len() > 200 {
        sanitized.truncate(200);
        sanitized.push_str("...");
    }

    sanitized
}

fn ensure_parent_dir_exists(path: &str) -> EngineResult<()> {
    let path = PathBuf::from(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| EngineError::SshError {
            message: format!(
                "Failed to create SSH config directory {}: {}",
                parent.display(),
                e
            ),
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use qore_core::types::{SshAuth, SshHostKeyPolicy, SshTunnelConfig};

    fn cmd_args(cmd: &Command) -> Vec<String> {
        cmd.as_std()
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn builds_command_with_strict_policy_and_proxyjump() {
        let cfg = SshTunnelConfig {
            host: "ssh.example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            auth: SshAuth::Key {
                private_key_path: "id_ed25519".to_string(),
                passphrase: None,
            },
            host_key_policy: SshHostKeyPolicy::Strict,
            known_hosts_path: Some("/tmp/qoredb_known_hosts".to_string()),
            proxy_jump: Some("jumpuser@jump.example.com:22".to_string()),
            connect_timeout_secs: 7,
            keepalive_interval_secs: 11,
            keepalive_count_max: 2,
        };

        let cmd = build_ssh_command(&cfg, "/tmp/qoredb_known_hosts", 50000, "postgres", 5432)
            .expect("command build should succeed");
        let args = cmd_args(&cmd);

        assert!(args.contains(&"-N".to_string()));
        assert!(args.iter().any(|a| a == "StrictHostKeyChecking=yes"));
        assert!(args
            .iter()
            .any(|a| a == "UserKnownHostsFile=/tmp/qoredb_known_hosts"));
        assert!(args.iter().any(|a| a == "-J"));
        assert!(args.iter().any(|a| a == "jumpuser@jump.example.com:22"));
        assert!(args.iter().any(|a| a == "-L"));
        assert!(args.iter().any(|a| a == "127.0.0.1:50000:postgres:5432"));
    }

    #[test]
    fn rejects_malicious_proxy_jump() {
        let cfg = SshTunnelConfig {
            host: "ssh.example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            auth: SshAuth::Key {
                private_key_path: "id_ed25519".to_string(),
                passphrase: None,
            },
            host_key_policy: SshHostKeyPolicy::AcceptNew,
            known_hosts_path: Some("/tmp/qoredb_known_hosts".to_string()),
            proxy_jump: Some("evil;rm -rf /".to_string()),
            connect_timeout_secs: 10,
            keepalive_interval_secs: 30,
            keepalive_count_max: 3,
        };
        let err = build_ssh_command(&cfg, "/tmp/qoredb_known_hosts", 50000, "postgres", 5432)
            .expect_err("malicious proxy_jump should be rejected");
        match err {
            EngineError::SshError { message } => assert!(message.contains("Invalid proxy jump")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn accepts_valid_proxy_jump_formats() {
        assert!(validate_proxy_jump("jump.example.com").is_ok());
        assert!(validate_proxy_jump("user@jump.example.com").is_ok());
        assert!(validate_proxy_jump("user@jump.example.com:22").is_ok());
    }

    #[test]
    fn rejects_proxy_jump_with_spaces_or_options() {
        assert!(validate_proxy_jump("user@host -o Evil=true").is_err());
        assert!(validate_proxy_jump("host && rm -rf /").is_err());
        assert!(validate_proxy_jump("").is_err());
    }

    #[test]
    fn sanitizes_ssh_stderr_ips() {
        let stderr = "Connection refused by 192.168.1.100 port 22";
        let sanitized = sanitize_ssh_stderr(stderr);
        assert!(!sanitized.contains("192.168.1.100"));
        assert!(sanitized.contains("[redacted-ip]"));
    }

    #[test]
    fn sanitizes_ssh_stderr_truncates_long_output() {
        let stderr = "x".repeat(300);
        let sanitized = sanitize_ssh_stderr(&stderr);
        assert!(sanitized.len() <= 204); // 200 + "..."
    }

    #[test]
    fn sanitizes_ssh_stderr_filters_at_sign() {
        let stderr = "user@internal-host: Permission denied\nsome other info";
        let sanitized = sanitize_ssh_stderr(stderr);
        // "Permission denied" lines with @ are kept
        assert!(sanitized.contains("Permission denied"));
    }

    #[test]
    fn rejects_key_passphrase_for_openssh_backend() {
        let cfg = SshTunnelConfig {
            host: "ssh.example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            auth: SshAuth::Key {
                private_key_path: "id_ed25519".to_string(),
                passphrase: Some("secret".to_string()),
            },
            host_key_policy: SshHostKeyPolicy::AcceptNew,
            known_hosts_path: Some("/tmp/qoredb_known_hosts".to_string()),
            proxy_jump: None,
            connect_timeout_secs: 10,
            keepalive_interval_secs: 30,
            keepalive_count_max: 3,
        };

        let err = build_ssh_command(&cfg, "/tmp/qoredb_known_hosts", 50000, "postgres", 5432)
            .expect_err("passphrase should be rejected");
        match err {
            EngineError::SshError { message } => assert!(message.contains("passphrase")),
            other => panic!("unexpected error: {other:?}"),
        }
    }
}

impl Drop for OpenSshTunnel {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.start_kill();
        }
    }
}

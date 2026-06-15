// SPDX-License-Identifier: Apache-2.0

use qore_core::{ConnectionConfig, MssqlAuthMode, SessionId, SshAuth};
use qore_drivers::session_manager::SessionManager;

use crate::error::ServiceError;
use crate::ratelimit::QueryRateLimiter;

pub async fn test_connection(
    session_manager: &SessionManager,
    config: ConnectionConfig,
) -> Result<(), ServiceError> {
    let config = normalize_config(config).map_err(ServiceError::Message)?;
    session_manager.test_connection(&config).await?;
    Ok(())
}

pub async fn connect(
    session_manager: &SessionManager,
    config: ConnectionConfig,
) -> Result<SessionId, ServiceError> {
    let config = normalize_config(config).map_err(ServiceError::Message)?;
    Ok(session_manager.connect(config).await?)
}

pub async fn disconnect(
    session_manager: &SessionManager,
    query_rate_limiter: &QueryRateLimiter,
    session: SessionId,
) -> Result<(), ServiceError> {
    session_manager.disconnect(session).await?;
    query_rate_limiter.forget(&session.0.to_string());
    Ok(())
}

pub fn normalize_config(mut config: ConnectionConfig) -> Result<ConnectionConfig, String> {
    let driver = config.driver.trim();
    if driver.is_empty() {
        return Err("Driver is required".to_string());
    }

    let normalized_driver = driver.to_ascii_lowercase();
    let normalized_driver = match normalized_driver.as_str() {
        "postgresql" => "postgres",
        "sqlite3" => "sqlite",
        other => other,
    };

    config.driver = normalized_driver.to_string();

    let host = config.host.trim();
    if host.is_empty() {
        return Err("Host is required".to_string());
    }
    config.host = host.to_string();

    let is_mongodb = config.driver == "mongodb";
    let is_sqlite = config.driver == "sqlite";
    let is_duckdb = config.driver == "duckdb";
    let is_redis = config.driver == "redis";
    let is_file_based = is_sqlite || is_duckdb;
    // SQL Server "Windows (Integrated)" uses the current OS/AD session — no username.
    let is_mssql_integrated =
        config.driver == "sqlserver" && config.mssql_auth == Some(MssqlAuthMode::WindowsIntegrated);

    // Search engines (Elasticsearch / OpenSearch) only need a username in
    // basic-auth mode. None / api_key / bearer carry no username.
    let is_search = config.driver == "elasticsearch" || config.driver == "opensearch";
    let search_without_username =
        is_search && config.search_auth_mode.as_deref() != Some("basic");

    // Username is required for SQL databases but optional for MongoDB, file-based DBs, Redis,
    // SQL Server integrated authentication, and non-basic search auth.
    let username = config.username.trim();
    if username.is_empty()
        && !is_mongodb
        && !is_file_based
        && !is_redis
        && !is_mssql_integrated
        && !search_without_username
    {
        return Err("Username is required".to_string());
    }
    config.username = username.to_string();

    if config.port == 0 && !is_file_based {
        return Err("Port must be greater than 0".to_string());
    }

    if let Some(database) = config.database.take() {
        let trimmed = database.trim();
        if !trimmed.is_empty() {
            config.database = Some(trimmed.to_string());
        }
    }

    config.environment = normalize_environment(&config.environment)?;

    if let Some(ref mut ssh) = config.ssh_tunnel {
        let host = ssh.host.trim();
        if host.is_empty() {
            return Err("SSH host is required".to_string());
        }
        ssh.host = host.to_string();

        let username = ssh.username.trim();
        if username.is_empty() {
            return Err("SSH username is required".to_string());
        }
        ssh.username = username.to_string();

        if ssh.port == 0 {
            return Err("SSH port must be greater than 0".to_string());
        }

        match &mut ssh.auth {
            SshAuth::Password { password } => {
                if password.trim().is_empty() {
                    return Err("SSH password is required".to_string());
                }
            }
            SshAuth::Key {
                private_key_path, ..
            } => {
                if private_key_path.trim().is_empty() {
                    return Err("SSH key path is required".to_string());
                }
            }
        }
    }

    if let Some(ref mut proxy) = config.proxy {
        let host = proxy.host.trim();
        if host.is_empty() {
            return Err("Proxy host is required".to_string());
        }
        proxy.host = host.to_string();

        if proxy.port == 0 {
            return Err("Proxy port must be greater than 0".to_string());
        }

        if proxy.connect_timeout_secs < 1 {
            return Err("Proxy connect timeout must be at least 1 second".to_string());
        }

        if let Some(ref mut user) = proxy.username {
            let trimmed = user.trim().to_string();
            if trimmed.is_empty() {
                proxy.username = None;
                proxy.password = None;
            } else {
                *user = trimmed;
            }
        }
    }

    let max_connections = config.pool_max_connections.unwrap_or(10);
    if max_connections == 0 {
        return Err("Pool max connections must be greater than 0".to_string());
    }
    let min_connections = match config.pool_min_connections {
        Some(explicit) => {
            if explicit > max_connections {
                return Err("Pool min connections must be <= max connections".to_string());
            }
            explicit
        }
        None => 2u32.min(max_connections),
    };
    let acquire_timeout = config.pool_acquire_timeout_secs.unwrap_or(15);
    if acquire_timeout < 5 {
        return Err("Pool acquire timeout must be at least 5 seconds".to_string());
    }

    config.pool_max_connections = Some(max_connections);
    config.pool_min_connections = Some(min_connections);
    config.pool_acquire_timeout_secs = Some(acquire_timeout);

    Ok(config)
}

fn normalize_environment(env: &str) -> Result<String, String> {
    let normalized = env.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok("development".to_string());
    }

    match normalized.as_str() {
        "development" | "staging" | "production" => Ok(normalized),
        _ => Err(format!("Invalid environment: {}", env)),
    }
}

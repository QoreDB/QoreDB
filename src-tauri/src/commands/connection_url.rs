//! Connection URL Parsing Commands
//!
//! Tauri commands for parsing database connection URLs into normalized configuration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::instrument;

use crate::engine::connection_url::{parse_connection_url, ParseErrorCode};

/// Response from parsing a connection URL
#[derive(Debug, Serialize, Deserialize)]
pub struct ParseConnectionUrlResponse {
    pub success: bool,
    /// Parsed configuration fields (only present on success)
    pub config: Option<PartialConnectionConfigDto>,
    /// Error message (only present on failure)
    pub error: Option<String>,
    /// Error code for programmatic handling
    pub error_code: Option<ParseErrorCode>,
}

/// DTO for partial connection configuration
/// Matches the frontend's expected format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialConnectionConfigDto {
    pub driver: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
    pub ssl: Option<bool>,
    /// Additional driver-specific options
    pub options: HashMap<String, String>,
}

impl From<crate::engine::connection_url::PartialConnectionConfig> for PartialConnectionConfigDto {
    fn from(config: crate::engine::connection_url::PartialConnectionConfig) -> Self {
        Self {
            driver: config.driver,
            host: config.host,
            port: config.port,
            username: config.username,
            password: config.password,
            database: config.database,
            ssl: config.ssl,
            options: config.options,
        }
    }
}

/// Parse a database connection URL and return the extracted configuration fields.
///
/// Supports:
/// - PostgreSQL: `postgres://` and `postgresql://`
/// - MySQL: `mysql://`
/// - MongoDB: `mongodb://` and `mongodb+srv://`
///
/// The parsed fields can be merged with explicit form values to create a complete
/// ConnectionConfig. URL values are parsed first, then explicit values override them.
#[tauri::command]
#[instrument(skip(url), fields(url_scheme))]
pub fn parse_url(url: String) -> ParseConnectionUrlResponse {
    // Don't log the full URL as it may contain credentials
    let scheme = url
        .split("://")
        .next()
        .unwrap_or("unknown")
        .to_string();
    tracing::Span::current().record("url_scheme", &scheme);

    match parse_connection_url(&url) {
        Ok(config) => {
            tracing::info!(
                driver = ?config.driver,
                host = ?config.host,
                port = ?config.port,
                has_password = config.password.is_some(),
                "URL parsed successfully"
            );

            ParseConnectionUrlResponse {
                success: true,
                config: Some(config.into()),
                error: None,
                error_code: None,
            }
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                error_code = ?err.code,
                "Failed to parse connection URL"
            );

            ParseConnectionUrlResponse {
                success: false,
                config: None,
                error: Some(err.message),
                error_code: Some(err.code),
            }
        }
    }
}

/// Get the list of supported URL schemes
#[tauri::command]
pub fn get_supported_url_schemes() -> Vec<String> {
    vec![
        "postgres".to_string(),
        "postgresql".to_string(),
        "mysql".to_string(),
        "mongodb".to_string(),
        "mongodb+srv".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_url_success() {
        let response = parse_url("postgres://user:pass@localhost:5432/mydb".to_string());
        assert!(response.success);
        assert!(response.config.is_some());

        let config = response.config.unwrap();
        assert_eq!(config.driver.as_deref(), Some("postgres"));
        assert_eq!(config.host.as_deref(), Some("localhost"));
        assert_eq!(config.port, Some(5432));
    }

    #[test]
    fn test_parse_url_invalid() {
        let response = parse_url("not a url".to_string());
        assert!(!response.success);
        assert!(response.error.is_some());
        assert_eq!(response.error_code, Some(ParseErrorCode::InvalidUrl));
    }

    #[test]
    fn test_parse_url_unsupported_scheme() {
        let response = parse_url("redis://localhost:6379".to_string());
        assert!(!response.success);
        assert_eq!(response.error_code, Some(ParseErrorCode::UnsupportedScheme));
    }
}

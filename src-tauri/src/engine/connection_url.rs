//! Connection URL parsing module
//!
//! Provides a unified interface for parsing database connection URLs/DSNs
//! into normalized configuration fields. Supports PostgreSQL, MySQL, and MongoDB.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

/// Partial connection configuration derived from URL parsing.
/// Contains only fields that can be extracted from a connection URL.
/// These are merged with explicit overrides to form the final ConnectionConfig.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartialConnectionConfig {
    pub driver: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
    pub ssl: Option<bool>,
    pub options: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseError {
    pub message: String,
    pub code: ParseErrorCode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParseErrorCode {
    InvalidUrl,
    UnsupportedScheme,
    MissingHost,
    InvalidPort,
    InvalidUtf8,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

impl ParseError {
    pub fn new(code: ParseErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Result type for URL parsing operations
pub type ParseResult<T> = Result<T, ParseError>;

pub trait ConnectionUrlParser: Send + Sync {
    /// Return river identifier
    fn driver_id(&self) -> &str;

    /// Return URL schemes supported by this parser
    fn schemes(&self) -> &[&str];

    /// Return default port for this database
    fn default_port(&self) -> u16;

    /// Parses a URL into a partial configuration
    fn parse(&self, url: &Url) -> ParseResult<PartialConnectionConfig>;
}

// =============================================================================
// PostgreSQL Parser
// =============================================================================

pub struct PostgresUrlParser;

impl ConnectionUrlParser for PostgresUrlParser {
    fn driver_id(&self) -> &str {
        "postgres"
    }

    fn schemes(&self) -> &[&str] {
        &["postgres", "postgresql"]
    }

    fn default_port(&self) -> u16 {
        5432
    }

    fn parse(&self, url: &Url) -> ParseResult<PartialConnectionConfig> {
        let host = url
            .host_str()
            .filter(|h| !h.is_empty())
            .map(String::from);

        if host.is_none() {
            return Err(ParseError::new(
                ParseErrorCode::MissingHost,
                "PostgreSQL URL must specify a host",
            ));
        }

        let port = url.port().or(Some(self.default_port()));

        let username = if url.username().is_empty() {
            None
        } else {
            Some(
                percent_decode(url.username())
                    .map_err(|_| ParseError::new(ParseErrorCode::InvalidUtf8, "Invalid username encoding"))?,
            )
        };

        let password = url
            .password()
            .map(|p| percent_decode(p))
            .transpose()
            .map_err(|_| ParseError::new(ParseErrorCode::InvalidUtf8, "Invalid password encoding"))?;

        // Database is the path without leading slash
        let database = url
            .path()
            .strip_prefix('/')
            .filter(|db| !db.is_empty())
            .map(|db| {
                percent_decode(db)
                    .map_err(|_| ParseError::new(ParseErrorCode::InvalidUtf8, "Invalid database name encoding"))
            })
            .transpose()?;

        // Parse query parameters
        let mut options = HashMap::new();
        let mut ssl_explicit = None;
        let mut ssl_mode_seen = false;
        let mut ssl_implied = false;

        for (key, value) in url.query_pairs() {
            let key_str = key.as_ref();
            let key_lower = key_str.to_ascii_lowercase();
            let value_str = value.as_ref();

            if key_lower == "sslmode" {
                // PostgreSQL sslmode values: disable, allow, prefer, require, verify-ca, verify-full
                ssl_explicit = Some(!value_str.eq_ignore_ascii_case("disable"));
                ssl_mode_seen = true;
            } else if key_lower == "ssl" && !ssl_mode_seen {
                // Simple ssl=true/false
                if let Some(parsed) = parse_bool_param(value_str) {
                    ssl_explicit = Some(parsed);
                }
            }

            if is_ssl_query_key(&key_lower) {
                ssl_implied = true;
            }

            options.insert(key.into_owned(), value.into_owned());
        }

        let ssl = ssl_explicit.or_else(|| if ssl_implied { Some(true) } else { None });

        Ok(PartialConnectionConfig {
            driver: Some(self.driver_id().to_string()),
            host,
            port,
            username,
            password,
            database,
            ssl,
            options,
        })
    }
}

// =============================================================================
// MySQL Parser
// =============================================================================

pub struct MySqlUrlParser;

impl ConnectionUrlParser for MySqlUrlParser {
    fn driver_id(&self) -> &str {
        "mysql"
    }

    fn schemes(&self) -> &[&str] {
        &["mysql"]
    }

    fn default_port(&self) -> u16 {
        3306
    }

    fn parse(&self, url: &Url) -> ParseResult<PartialConnectionConfig> {
        let host = url
            .host_str()
            .filter(|h| !h.is_empty())
            .map(String::from);

        if host.is_none() {
            return Err(ParseError::new(
                ParseErrorCode::MissingHost,
                "MySQL URL must specify a host",
            ));
        }

        let port = url.port().or(Some(self.default_port()));

        let username = if url.username().is_empty() {
            None
        } else {
            Some(
                percent_decode(url.username())
                    .map_err(|_| ParseError::new(ParseErrorCode::InvalidUtf8, "Invalid username encoding"))?,
            )
        };

        let password = url
            .password()
            .map(|p| percent_decode(p))
            .transpose()
            .map_err(|_| ParseError::new(ParseErrorCode::InvalidUtf8, "Invalid password encoding"))?;

        // Database is the path without leading slash
        let database = url
            .path()
            .strip_prefix('/')
            .filter(|db| !db.is_empty())
            .map(|db| {
                percent_decode(db)
                    .map_err(|_| ParseError::new(ParseErrorCode::InvalidUtf8, "Invalid database name encoding"))
            })
            .transpose()?;

        // Parse query parameters
        let mut options = HashMap::new();
        let mut ssl_explicit = None;
        let mut ssl_mode_seen = false;
        let mut ssl_implied = false;

        for (key, value) in url.query_pairs() {
            let key_str = key.as_ref();
            let key_lower = key_str.to_ascii_lowercase();
            let value_str = value.as_ref();

            if key_lower == "ssl-mode" || key_lower == "sslmode" {
                // MySQL ssl-mode values: DISABLED, PREFERRED, REQUIRED, VERIFY_CA, VERIFY_IDENTITY
                ssl_explicit = Some(!value_str.eq_ignore_ascii_case("disabled"));
                ssl_mode_seen = true;
            } else if (key_lower == "ssl" || key_lower == "usessl") && !ssl_mode_seen {
                if let Some(parsed) = parse_bool_param(value_str) {
                    ssl_explicit = Some(parsed);
                }
            }

            if is_ssl_query_key(&key_lower) {
                ssl_implied = true;
            }

            options.insert(key.into_owned(), value.into_owned());
        }

        let ssl = ssl_explicit.or_else(|| if ssl_implied { Some(true) } else { None });

        Ok(PartialConnectionConfig {
            driver: Some(self.driver_id().to_string()),
            host,
            port,
            username,
            password,
            database,
            ssl,
            options,
        })
    }
}

// =============================================================================
// MongoDB Parser
// =============================================================================

pub struct MongoDbUrlParser;

impl ConnectionUrlParser for MongoDbUrlParser {
    fn driver_id(&self) -> &str {
        "mongodb"
    }

    fn schemes(&self) -> &[&str] {
        &["mongodb", "mongodb+srv"]
    }

    fn default_port(&self) -> u16 {
        27017
    }

    fn parse(&self, url: &Url) -> ParseResult<PartialConnectionConfig> {
        let is_srv = url.scheme() == "mongodb+srv";

        let host = url
            .host_str()
            .filter(|h| !h.is_empty())
            .map(String::from);

        if host.is_none() {
            return Err(ParseError::new(
                ParseErrorCode::MissingHost,
                "MongoDB URL must specify a host",
            ));
        }

        // SRV records don't use explicit ports
        let port = if is_srv {
            None
        } else {
            url.port().or(Some(self.default_port()))
        };

        let username = if url.username().is_empty() {
            None
        } else {
            Some(
                percent_decode(url.username())
                    .map_err(|_| ParseError::new(ParseErrorCode::InvalidUtf8, "Invalid username encoding"))?,
            )
        };

        let password = url
            .password()
            .map(|p| percent_decode(p))
            .transpose()
            .map_err(|_| ParseError::new(ParseErrorCode::InvalidUtf8, "Invalid password encoding"))?;

        // MongoDB: database is the path, but authSource can override for auth
        let database = url
            .path()
            .strip_prefix('/')
            .filter(|db| !db.is_empty())
            .map(|db| {
                percent_decode(db)
                    .map_err(|_| ParseError::new(ParseErrorCode::InvalidUtf8, "Invalid database name encoding"))
            })
            .transpose()?;

        // Parse query parameters
        let mut options = HashMap::new();
        let ssl_default = if is_srv { Some(true) } else { None }; // SRV implies TLS
        let mut ssl_explicit = None;
        let mut ssl_implied = false;

        for (key, value) in url.query_pairs() {
            let key_str = key.as_ref();
            let key_lower = key_str.to_ascii_lowercase();
            let value_str = value.as_ref();

            match key_lower.as_str() {
                "tls" | "ssl" => {
                    if let Some(parsed) = parse_bool_param(value_str) {
                        ssl_explicit = Some(parsed);
                    }
                    options.insert(key.into_owned(), value.into_owned());
                }
                "authsource" => {
                    // Store authSource but don't override database
                    options.insert("authSource".to_string(), value.into_owned());
                }
                "replicaset" => {
                    options.insert("replicaSet".to_string(), value.into_owned());
                }
                _ => {
                    options.insert(key.into_owned(), value.into_owned());
                }
            }

            if is_ssl_query_key(&key_lower) {
                ssl_implied = true;
            }
        }

        let ssl = match ssl_explicit {
            Some(value) => Some(value),
            None => match ssl_default {
                Some(value) => Some(value),
                None => {
                    if ssl_implied {
                        Some(true)
                    } else {
                        None
                    }
                }
            },
        };

        Ok(PartialConnectionConfig {
            driver: Some(self.driver_id().to_string()),
            host,
            port,
            username,
            password,
            database,
            ssl,
            options,
        })
    }
}

// =============================================================================
// URL Parser Registry
// =============================================================================

/// Central URL parser that delegates to driver-specific parsers
pub struct ConnectionUrlParserRegistry {
    parsers: Vec<Box<dyn ConnectionUrlParser>>,
}

impl Default for ConnectionUrlParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionUrlParserRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            parsers: Vec::new(),
        };

        // Register built-in parsers
        registry.register(Box::new(PostgresUrlParser));
        registry.register(Box::new(MySqlUrlParser));
        registry.register(Box::new(MongoDbUrlParser));

        registry
    }

    /// Register a new parser
    pub fn register(&mut self, parser: Box<dyn ConnectionUrlParser>) {
        self.parsers.push(parser);
    }

    /// Find a parser for the given URL scheme
    fn find_parser(&self, scheme: &str) -> Option<&dyn ConnectionUrlParser> {
        self.parsers
            .iter()
            .find(|p| p.schemes().iter().any(|s| s.eq_ignore_ascii_case(scheme)))
            .map(|p| p.as_ref())
    }

    /// Parse a connection URL string
    pub fn parse(&self, url_str: &str) -> ParseResult<PartialConnectionConfig> {
        let url = Url::parse(url_str).map_err(|e| {
            ParseError::new(ParseErrorCode::InvalidUrl, format!("Invalid URL: {}", e))
        })?;

        let scheme = url.scheme();
        let parser = self.find_parser(scheme).ok_or_else(|| {
            ParseError::new(
                ParseErrorCode::UnsupportedScheme,
                format!(
                    "Unsupported URL scheme '{}'. Supported schemes: postgres, postgresql, mysql, mongodb, mongodb+srv",
                    scheme
                ),
            )
        })?;

        parser.parse(&url)
    }

    /// Get the list of supported schemes
    pub fn supported_schemes(&self) -> Vec<&str> {
        self.parsers
            .iter()
            .flat_map(|p| p.schemes().iter().copied())
            .collect()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// URL percent-decode a string
fn percent_decode(s: &str) -> Result<String, std::str::Utf8Error> {
    percent_encoding::percent_decode_str(s)
        .decode_utf8()
        .map(|s| s.into_owned())
}

fn parse_bool_param(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "t" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "f" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn is_ssl_query_key(lower_key: &str) -> bool {
    lower_key.starts_with("ssl") || lower_key.starts_with("tls")
}

/// Parse a connection URL and return a partial configuration.
/// This is the main entry point for URL parsing.
pub fn parse_connection_url(url: &str) -> ParseResult<PartialConnectionConfig> {
    let registry = ConnectionUrlParserRegistry::new();
    registry.parse(url)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // PostgreSQL Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_postgres_full_url() {
        let result = parse_connection_url("postgres://user:pass@localhost:5432/mydb").unwrap();
        assert_eq!(result.driver.as_deref(), Some("postgres"));
        assert_eq!(result.host.as_deref(), Some("localhost"));
        assert_eq!(result.port, Some(5432));
        assert_eq!(result.username.as_deref(), Some("user"));
        assert_eq!(result.password.as_deref(), Some("pass"));
        assert_eq!(result.database.as_deref(), Some("mydb"));
    }

    #[test]
    fn test_postgres_default_port() {
        let result = parse_connection_url("postgres://user@localhost/mydb").unwrap();
        assert_eq!(result.port, Some(5432));
    }

    #[test]
    fn test_postgres_sslmode_require() {
        let result = parse_connection_url("postgres://user@localhost/mydb?sslmode=require").unwrap();
        assert_eq!(result.ssl, Some(true));
    }

    #[test]
    fn test_postgres_sslmode_disable() {
        let result = parse_connection_url("postgres://user@localhost/mydb?sslmode=disable").unwrap();
        assert_eq!(result.ssl, Some(false));
    }

    #[test]
    fn test_postgres_ssl_implied_by_sslrootcert() {
        let result = parse_connection_url(
            "postgres://user@localhost/mydb?sslrootcert=%2Fpath%2Fca.pem",
        )
        .unwrap();
        assert_eq!(result.ssl, Some(true));
        assert_eq!(
            result.options.get("sslrootcert"),
            Some(&"/path/ca.pem".to_string())
        );
    }

    #[test]
    fn test_postgres_sslmode_disable_overrides_sslrootcert() {
        let result = parse_connection_url(
            "postgres://user@localhost/mydb?sslmode=disable&sslrootcert=%2Fpath%2Fca.pem",
        )
        .unwrap();
        assert_eq!(result.ssl, Some(false));
    }

    #[test]
    fn test_postgres_postgresql_scheme() {
        let result = parse_connection_url("postgresql://user@localhost/mydb").unwrap();
        assert_eq!(result.driver.as_deref(), Some("postgres"));
    }

    #[test]
    fn test_postgres_encoded_password() {
        let result = parse_connection_url("postgres://user:p%40ss%3Aword@localhost/mydb").unwrap();
        assert_eq!(result.password.as_deref(), Some("p@ss:word"));
    }

    #[test]
    fn test_postgres_no_database() {
        let result = parse_connection_url("postgres://user:pass@localhost:5432").unwrap();
        assert_eq!(result.database, None);
    }

    #[test]
    fn test_postgres_missing_host() {
        let result = parse_connection_url("postgres:///mydb");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ParseErrorCode::MissingHost);
    }

    // -------------------------------------------------------------------------
    // MySQL Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_mysql_full_url() {
        let result = parse_connection_url("mysql://root:secret@db.example.com:3307/app").unwrap();
        assert_eq!(result.driver.as_deref(), Some("mysql"));
        assert_eq!(result.host.as_deref(), Some("db.example.com"));
        assert_eq!(result.port, Some(3307));
        assert_eq!(result.username.as_deref(), Some("root"));
        assert_eq!(result.password.as_deref(), Some("secret"));
        assert_eq!(result.database.as_deref(), Some("app"));
    }

    #[test]
    fn test_mysql_default_port() {
        let result = parse_connection_url("mysql://user@localhost/mydb").unwrap();
        assert_eq!(result.port, Some(3306));
    }

    #[test]
    fn test_mysql_ssl_mode() {
        let result = parse_connection_url("mysql://user@localhost/mydb?ssl-mode=REQUIRED").unwrap();
        assert_eq!(result.ssl, Some(true));
    }

    #[test]
    fn test_mysql_ssl_disabled() {
        let result = parse_connection_url("mysql://user@localhost/mydb?ssl-mode=DISABLED").unwrap();
        assert_eq!(result.ssl, Some(false));
    }

    #[test]
    fn test_mysql_ssl_implied_by_ssl_ca() {
        let result = parse_connection_url("mysql://user@localhost/mydb?ssl-ca=%2Fpath%2Fca.pem").unwrap();
        assert_eq!(result.ssl, Some(true));
        assert_eq!(
            result.options.get("ssl-ca"),
            Some(&"/path/ca.pem".to_string())
        );
    }

    #[test]
    fn test_mysql_ssl_disabled_overrides_ssl_ca() {
        let result = parse_connection_url(
            "mysql://user@localhost/mydb?ssl-mode=DISABLED&ssl-ca=%2Fpath%2Fca.pem",
        )
        .unwrap();
        assert_eq!(result.ssl, Some(false));
    }

    // -------------------------------------------------------------------------
    // MongoDB Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_mongodb_full_url() {
        let result = parse_connection_url("mongodb://admin:pwd@mongo.example.com:27018/admin").unwrap();
        assert_eq!(result.driver.as_deref(), Some("mongodb"));
        assert_eq!(result.host.as_deref(), Some("mongo.example.com"));
        assert_eq!(result.port, Some(27018));
        assert_eq!(result.username.as_deref(), Some("admin"));
        assert_eq!(result.password.as_deref(), Some("pwd"));
        assert_eq!(result.database.as_deref(), Some("admin"));
    }

    #[test]
    fn test_mongodb_default_port() {
        let result = parse_connection_url("mongodb://user@localhost/mydb").unwrap();
        assert_eq!(result.port, Some(27017));
    }

    #[test]
    fn test_mongodb_srv_no_port() {
        let result = parse_connection_url("mongodb+srv://user@cluster.mongodb.net/mydb").unwrap();
        assert_eq!(result.driver.as_deref(), Some("mongodb"));
        assert_eq!(result.port, None); // SRV doesn't use explicit ports
        assert_eq!(result.ssl, Some(true)); // SRV implies TLS
    }

    #[test]
    fn test_mongodb_tls_param() {
        let result = parse_connection_url("mongodb://user@localhost/mydb?tls=true").unwrap();
        assert_eq!(result.ssl, Some(true));
    }

    #[test]
    fn test_mongodb_tls_ca_implies_ssl() {
        let result = parse_connection_url(
            "mongodb://user@localhost/mydb?tlsCAFile=%2Fpath%2Fca.pem",
        )
        .unwrap();
        assert_eq!(result.ssl, Some(true));
        assert_eq!(
            result.options.get("tlsCAFile"),
            Some(&"/path/ca.pem".to_string())
        );
    }

    #[test]
    fn test_mongodb_tls_false_overrides_tls_ca() {
        let result = parse_connection_url(
            "mongodb://user@localhost/mydb?tls=false&tlsCAFile=%2Fpath%2Fca.pem",
        )
        .unwrap();
        assert_eq!(result.ssl, Some(false));
    }

    #[test]
    fn test_mongodb_auth_source() {
        let result = parse_connection_url("mongodb://user@localhost/mydb?authSource=admin").unwrap();
        assert_eq!(result.options.get("authSource"), Some(&"admin".to_string()));
    }

    // -------------------------------------------------------------------------
    // Error Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_unsupported_scheme() {
        let result = parse_connection_url("redis://localhost:6379");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ParseErrorCode::UnsupportedScheme);
    }

    #[test]
    fn test_invalid_url() {
        let result = parse_connection_url("not a valid url");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ParseErrorCode::InvalidUrl);
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_ipv6_host() {
        let result = parse_connection_url("postgres://user@[::1]:5432/mydb").unwrap();
        assert_eq!(result.host.as_deref(), Some("[::1]"));
    }

    #[test]
    fn test_special_chars_in_database() {
        let result = parse_connection_url("postgres://user@localhost/my%2Ddb%5Ftest").unwrap();
        assert_eq!(result.database.as_deref(), Some("my-db_test"));
    }

    #[test]
    fn test_empty_password() {
        // URL library treats empty password (user:@host) as no password
        let result = parse_connection_url("postgres://user:@localhost/mydb").unwrap();
        assert_eq!(result.password, None);
    }

    #[test]
    fn test_no_username() {
        let result = parse_connection_url("postgres://localhost/mydb").unwrap();
        assert_eq!(result.username, None);
    }

    #[test]
    fn test_query_params_preserved() {
        let result = parse_connection_url("postgres://user@localhost/mydb?application_name=qoredb&connect_timeout=10").unwrap();
        assert_eq!(result.options.get("application_name"), Some(&"qoredb".to_string()));
        assert_eq!(result.options.get("connect_timeout"), Some(&"10".to_string()));
    }
}

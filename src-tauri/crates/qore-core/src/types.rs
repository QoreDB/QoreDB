// SPDX-License-Identifier: Apache-2.0

//! Normalized data types shared across SQL and NoSQL engines.

use base64::Engine as _;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use uuid::Uuid;

/// Unique identifier for a database session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a running query
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QueryId(pub Uuid);

impl QueryId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for QueryId {
    fn default() -> Self {
        Self::new()
    }
}

/// Database connection configuration.
///
/// `Debug` is implemented manually: the `password` field is redacted so it
/// cannot leak via `tracing::debug!("{:?}", cfg)`, panic messages, or failed
/// assertions. Use the explicit `password` field accessor when you actually
/// need the value.
#[derive(Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub driver: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(skip_serializing)]
    pub password: String,
    pub database: Option<String>,
    pub ssl: bool,
    #[serde(default)]
    pub ssl_mode: Option<String>,
    pub environment: String,
    pub read_only: bool,
    pub pool_max_connections: Option<u32>,
    pub pool_min_connections: Option<u32>,
    pub pool_acquire_timeout_secs: Option<u32>,
    pub ssh_tunnel: Option<SshTunnelConfig>,
    #[serde(default)]
    pub proxy: Option<ProxyConfig>,
    /// SQL Server authentication mode. `None` means SQL auth (legacy default),
    /// kept optional for JSON back-compat with pre-NTLM saved connections.
    #[serde(default)]
    pub mssql_auth: Option<MssqlAuthMode>,
    /// ClickHouse cluster name for distributed DDL. When set, DDL commands
    /// (CREATE/DROP DATABASE/TABLE) are issued with `ON CLUSTER <name>` so they
    /// propagate to every replica. `None` keeps the single-node behaviour.
    #[serde(default)]
    pub clickhouse_cluster: Option<String>,
    /// Authentication mode for search engines (Elasticsearch / OpenSearch):
    /// `"none" | "basic" | "api_key" | "bearer"`. The secret always transits
    /// via `password` (already vault-encrypted). `None` defaults to `"none"`.
    #[serde(default)]
    pub search_auth_mode: Option<String>,
    /// Path to a custom CA certificate (PEM) used to verify the server's TLS
    /// certificate. Currently honoured by the search drivers; `None` keeps the
    /// system trust store.
    #[serde(default)]
    pub ssl_ca_cert: Option<String>,
    /// Driver options carried verbatim from a parsed connection URL.
    #[serde(default)]
    pub options: HashMap<String, String>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            driver: String::new(),
            host: String::new(),
            port: 0,
            username: String::new(),
            password: String::new(),
            database: None,
            ssl: false,
            ssl_mode: None,
            environment: "development".to_string(),
            read_only: false,
            pool_max_connections: None,
            pool_min_connections: None,
            pool_acquire_timeout_secs: None,
            ssh_tunnel: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
            options: HashMap::new(),
        }
    }
}

/// Wraps a bare IPv6 literal in brackets so it can be embedded in a URL authority.
pub fn host_for_url(host: &str) -> Cow<'_, str> {
    if host.contains(':') && !host.starts_with('[') {
        Cow::Owned(format!("[{}]", host))
    } else {
        Cow::Borrowed(host)
    }
}

impl std::fmt::Debug for ConnectionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionConfig")
            .field("driver", &self.driver)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &redacted_field(&self.password))
            .field("database", &self.database)
            .field("ssl", &self.ssl)
            .field("ssl_mode", &self.ssl_mode)
            .field("environment", &self.environment)
            .field("read_only", &self.read_only)
            .field("pool_max_connections", &self.pool_max_connections)
            .field("pool_min_connections", &self.pool_min_connections)
            .field("pool_acquire_timeout_secs", &self.pool_acquire_timeout_secs)
            .field("ssh_tunnel", &self.ssh_tunnel)
            .field("proxy", &self.proxy)
            .field("mssql_auth", &self.mssql_auth)
            .field("clickhouse_cluster", &self.clickhouse_cluster)
            .field("search_auth_mode", &self.search_auth_mode)
            .field("ssl_ca_cert", &self.ssl_ca_cert)
            .field("options", &self.options)
            .finish()
    }
}

/// Sentinel used by manual `Debug` impls in this module: prints `"[REDACTED]"`
/// when the secret is set, `"<empty>"` otherwise, so logs still convey shape
/// without leaking the value.
fn redacted_field(value: &str) -> &'static str {
    if value.is_empty() {
        "<empty>"
    } else {
        "[REDACTED]"
    }
}

/// Authentication mode for SQL Server connections.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MssqlAuthMode {
    #[default]
    SqlPassword,
    WindowsNtlm,
    WindowsIntegrated,
}

/// Network proxy configuration for corporate environments. `Debug` redacts
/// the password.
#[derive(Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Proxy type (HTTP CONNECT or SOCKS5)
    pub proxy_type: ProxyType,
    /// Proxy server hostname
    pub host: String,
    /// Proxy server port
    pub port: u16,
    /// Optional username for proxy authentication
    pub username: Option<String>,
    /// Optional password for proxy authentication
    #[serde(skip_serializing)]
    pub password: Option<String>,
    /// Connection timeout in seconds
    pub connect_timeout_secs: u32,
}

impl std::fmt::Debug for ProxyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let password = match self.password.as_deref() {
            Some(s) => Some(redacted_field(s)),
            None => None,
        };
        f.debug_struct("ProxyConfig")
            .field("proxy_type", &self.proxy_type)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &password)
            .field("connect_timeout_secs", &self.connect_timeout_secs)
            .finish()
    }
}

/// Supported proxy protocol types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProxyType {
    /// HTTP CONNECT tunnel (RFC 7231)
    HttpConnect,
    /// SOCKS5 proxy (RFC 1928)
    Socks5,
}

/// SSH tunnel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshTunnelConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,

    /// Host key verification policy.
    pub host_key_policy: SshHostKeyPolicy,

    /// Optional path to an app-owned known_hosts file.
    /// If not provided, a per-user default is used.
    pub known_hosts_path: Option<String>,

    /// Optional SSH jump host/bastion, formatted like `user@host:port`.
    pub proxy_jump: Option<String>,

    /// Connection timeout in seconds for the SSH TCP handshake.
    pub connect_timeout_secs: u32,

    /// SSH keepalive interval in seconds.
    pub keepalive_interval_secs: u32,

    /// Max number of keepalive failures before disconnect.
    pub keepalive_count_max: u32,
}

/// Host key verification policy for SSH.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SshHostKeyPolicy {
    /// Trust on first use: auto-add new hosts to an app-owned known_hosts file.
    AcceptNew,
    /// Strict: require the host key to already be present in known_hosts.
    Strict,
    /// Insecure: disable host key checking (dev-only).
    InsecureNoCheck,
}

/// SSH authentication method. `Debug` redacts the password / passphrase, and
/// these fields are also `skip_serializing` so they never leak into a saved
/// connection JSON dump (the vault stores them separately in the keyring).
#[derive(Clone, Serialize, Deserialize)]
pub enum SshAuth {
    Password {
        #[serde(skip_serializing)]
        password: String,
    },
    Key {
        private_key_path: String,
        #[serde(skip_serializing)]
        passphrase: Option<String>,
    },
}

impl std::fmt::Debug for SshAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SshAuth::Password { password } => f
                .debug_struct("SshAuth::Password")
                .field("password", &redacted_field(password))
                .finish(),
            SshAuth::Key {
                private_key_path,
                passphrase,
            } => {
                let passphrase = match passphrase.as_deref() {
                    Some(s) => Some(redacted_field(s)),
                    None => None,
                };
                f.debug_struct("SshAuth::Key")
                    .field("private_key_path", private_key_path)
                    .field("passphrase", &passphrase)
                    .finish()
            }
        }
    }
}

/// Query cancellation support level for a driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelSupport {
    None,
    BestEffort,
    Driver,
}

/// Reported capabilities for a driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverCapabilities {
    pub transactions: bool,
    pub mutations: bool,
    pub cancel: CancelSupport,
    pub supports_ssh: bool,
    pub schema: bool,
    pub streaming: bool,
    pub explain: bool,
    pub maintenance: bool,
    #[serde(default)]
    pub pagination: PaginationCapability,
}

/// Snapshot a driver can hold across pages of the same browsing session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotSupport {
    /// Each page is an independent read; concurrent writes are visible.
    #[default]
    None,
    /// Point-in-time reader, as offered by the search engines.
    Pit,
    /// An open transaction pins the view, at the cost of holding a connection.
    Transaction,
}

/// What a driver can promise about walking a result set.
///
/// Declared rather than inferred: strategy selection has to read this from one
/// stable place, otherwise each driver improvises its own answer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaginationCapability {
    /// Keyset (cursor) pagination is implemented by this driver.
    pub keyset: bool,
    /// Keyset needs a unique key; without one the driver falls back to offset.
    pub requires_unique_key: bool,
    pub supports_backward: bool,
    pub snapshot: SnapshotSupport,
    /// Highest `offset + limit` the engine will serve, when it caps it at all.
    /// Lets the caller clamp instead of walking into the engine's error.
    pub max_offset_window: Option<u64>,
}

impl Default for PaginationCapability {
    /// Offset-only, no snapshot, no window cap: what every driver could always
    /// do. A driver that promises more says so explicitly.
    fn default() -> Self {
        Self {
            keyset: false,
            requires_unique_key: true,
            supports_backward: false,
            snapshot: SnapshotSupport::None,
            max_offset_window: None,
        }
    }
}

/// Driver metadata exposed to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverInfo {
    pub id: String,
    pub name: String,
    pub capabilities: DriverCapabilities,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_auth_deserializes_from_ts_style_externally_tagged_enum() {
        let json = r#"{"Key":{"private_key_path":"/tmp/id_ed25519","passphrase":"p"}}"#;
        let auth: SshAuth = serde_json::from_str(json).expect("should parse");

        match auth {
            SshAuth::Key {
                private_key_path,
                passphrase,
            } => {
                assert_eq!(private_key_path, "/tmp/id_ed25519");
                assert_eq!(passphrase.as_deref(), Some("p"));
            }
            other => panic!("unexpected auth variant: {other:?}"),
        }
    }

    #[test]
    fn mssql_auth_mode_serialises_snake_case() {
        let json = serde_json::to_string(&MssqlAuthMode::WindowsNtlm).unwrap();
        assert_eq!(json, "\"windows_ntlm\"");
        let json = serde_json::to_string(&MssqlAuthMode::SqlPassword).unwrap();
        assert_eq!(json, "\"sql_password\"");
        let json = serde_json::to_string(&MssqlAuthMode::WindowsIntegrated).unwrap();
        assert_eq!(json, "\"windows_integrated\"");
    }

    #[test]
    fn connection_config_roundtrips_windows_integrated() {
        let json = r#"{
            "driver":"sqlserver","host":"localhost","port":1433,
            "username":"","password":"","database":null,"ssl":false,
            "environment":"development","read_only":false,
            "pool_max_connections":null,"pool_min_connections":null,
            "pool_acquire_timeout_secs":null,"ssh_tunnel":null,
            "mssql_auth":"windows_integrated"
        }"#;
        let cfg: ConnectionConfig = serde_json::from_str(json).expect("must parse");
        assert_eq!(cfg.mssql_auth, Some(MssqlAuthMode::WindowsIntegrated));
    }

    #[test]
    fn connection_config_accepts_legacy_json_without_mssql_auth() {
        let legacy = r#"{
            "driver":"sqlserver","host":"localhost","port":1433,
            "username":"sa","password":"x","database":null,"ssl":false,
            "environment":"development","read_only":false,
            "pool_max_connections":null,"pool_min_connections":null,
            "pool_acquire_timeout_secs":null,"ssh_tunnel":null
        }"#;
        let cfg: ConnectionConfig = serde_json::from_str(legacy).expect("legacy config must parse");
        assert!(cfg.mssql_auth.is_none());
    }

    #[test]
    fn connection_config_debug_redacts_password() {
        let cfg = ConnectionConfig {
            options: Default::default(),
            driver: "postgres".into(),
            host: "localhost".into(),
            port: 5432,
            username: "alice".into(),
            password: "s3cret".into(),
            database: None,
            ssl: false,
            ssl_mode: None,
            environment: "development".into(),
            read_only: false,
            pool_max_connections: None,
            pool_min_connections: None,
            pool_acquire_timeout_secs: None,
            ssh_tunnel: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
        };
        let dbg = format!("{:?}", cfg);
        assert!(dbg.contains("[REDACTED]"), "expected redaction in {dbg}");
        assert!(!dbg.contains("s3cret"), "password leaked: {dbg}");
    }

    #[test]
    fn ssh_auth_debug_redacts_password_and_passphrase() {
        let pwd = SshAuth::Password {
            password: "leakme".into(),
        };
        let key = SshAuth::Key {
            private_key_path: "/path/id_ed25519".into(),
            passphrase: Some("ZZZQQQ".into()),
        };
        assert!(!format!("{:?}", pwd).contains("leakme"));
        assert!(!format!("{:?}", key).contains("ZZZQQQ"));
        assert!(format!("{:?}", key).contains("id_ed25519"));
    }

    #[test]
    fn proxy_config_debug_redacts_password() {
        let cfg = ProxyConfig {
            proxy_type: ProxyType::HttpConnect,
            host: "proxy.local".into(),
            port: 8080,
            username: Some("alice".into()),
            password: Some("hideme".into()),
            connect_timeout_secs: 10,
        };
        assert!(!format!("{:?}", cfg).contains("hideme"));
    }

    #[test]
    fn ssh_auth_passphrase_not_serialized() {
        let key = SshAuth::Key {
            private_key_path: "/p".into(),
            passphrase: Some("secret".into()),
        };
        let json = serde_json::to_string(&key).unwrap();
        assert!(!json.contains("secret"), "passphrase leaked: {json}");
        assert!(json.contains("/p"));
    }

    #[test]
    fn connection_config_roundtrips_windows_ntlm() {
        let json = r#"{
            "driver":"sqlserver","host":"localhost","port":1433,
            "username":"CORP\\jdoe","password":"x","database":null,"ssl":false,
            "environment":"development","read_only":false,
            "pool_max_connections":null,"pool_min_connections":null,
            "pool_acquire_timeout_secs":null,"ssh_tunnel":null,
            "mssql_auth":"windows_ntlm"
        }"#;
        let cfg: ConnectionConfig = serde_json::from_str(json).expect("must parse");
        assert_eq!(cfg.mssql_auth, Some(MssqlAuthMode::WindowsNtlm));
    }
}

/// Namespace represents the hierarchy level above collections
/// - For PostgreSQL: database + schema
/// - For MySQL: database
/// - For MongoDB: database
/// - For SQLite: N/A (uses default namespace)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Namespace {
    pub database: String,
    pub schema: Option<String>,
}

impl Namespace {
    pub fn new(database: impl Into<String>) -> Self {
        Self {
            database: database.into(),
            schema: None,
        }
    }

    pub fn with_schema(database: impl Into<String>, schema: impl Into<String>) -> Self {
        Self {
            database: database.into(),
            schema: Some(schema.into()),
        }
    }
}

/// Collection represents a table (SQL) or collection (NoSQL)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub namespace: Namespace,
    pub name: String,
    pub collection_type: CollectionType,
}

/// Type of collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollectionType {
    Table,
    View,
    MaterializedView,
    Collection, // NoSQL
}

/// Largest integer a JSON number survives intact.
///
/// The receiving end reads every JSON number as a double, so anything past this
/// is rounded before a single line of consuming code runs. Snowflake
/// identifiers and nanosecond timestamps both sit well beyond it.
pub const MAX_SAFE_JSON_INT: i64 = 9_007_199_254_740_991;

/// Sole key of the envelope carrying an integer a JSON number cannot hold.
///
/// Distinctive on purpose: `Int` is tried before `Json` in this untagged enum,
/// so a document that happened to carry this exact shape would be read as an
/// integer. One key, a reserved name, and a digit string make that collision
/// something no engine produces by accident.
pub const EXACT_INT_KEY: &str = "$qoreInt";

/// Universal value representation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Int(#[serde(with = "exact_int")] i64),
    Float(f64),
    Text(String),
    Bytes(#[serde(with = "base64_bytes")] Vec<u8>),
    Json(serde_json::Value),
    Array(Vec<Value>),
}

impl Value {
    /// Returns the inner string if the value is `Value::Text`, otherwise
    /// `None`. Prefer this over ad-hoc `match` when a callsite needs a
    /// string-only contract (regex pattern, full-text query, …).
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Representation of an exact decimal, from the engine's own rendering of it.
    ///
    /// A `NUMERIC` or `DECIMAL` carries more digits than a double: converting it
    /// unconditionally rounds the value before any caller sees it, and nothing
    /// downstream can tell a rounded value from a stored one. Values a double
    /// holds exactly stay numbers, so the ordinary column keeps its type; only
    /// the ones a double would alter travel as text.
    pub fn from_decimal_text(text: String) -> Value {
        match text.parse::<f64>() {
            Ok(f) if f.is_finite() && same_decimal_digits(&text, f) => Value::Float(f),
            _ => Value::Text(text),
        }
    }

    /// Canonical conversion to ordinary JSON: bytes become base64 strings,
    /// non-finite floats become `null`, nested `Json`/`Array` pass through. Use
    /// this instead of re-implementing the match at each call site (cf. dédup
    /// D8). Contexts that need a different shape (e.g. a `<binary N bytes>`
    /// placeholder) keep their own mapping.
    ///
    /// Deliberately not `serde_json::to_value(self)`. That renders the wire
    /// form, where a large integer travels in an envelope — right for the
    /// interface, wrong everywhere this method is used: binding an array
    /// parameter, feeding a model, writing an export. Those want the number.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Int(i) => serde_json::Value::Number((*i).into()),
            Value::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Value::Text(s) => serde_json::Value::String(s.clone()),
            Value::Bytes(b) => {
                serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(b))
            }
            Value::Json(j) => j.clone(),
            Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(Value::to_json).collect())
            }
        }
    }
}

/// Plain decimal digits, stripped of the notations that mean the same number.
/// Returns `None` for anything that is not a plain decimal, which then counts as
/// not comparable rather than as equal.
fn canonical_decimal(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let (sign, rest) = match trimmed.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    let (int_part, frac_part) = match rest.split_once('.') {
        Some((int_part, frac_part)) => (int_part, frac_part),
        None => (rest, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }

    let int_digits = int_part.trim_start_matches('0');
    let int_digits = if int_digits.is_empty() {
        "0"
    } else {
        int_digits
    };
    let frac_digits = frac_part.trim_end_matches('0');
    let sign = if sign == "-" && !(int_digits == "0" && frac_digits.is_empty()) {
        "-"
    } else {
        ""
    };
    Some(if frac_digits.is_empty() {
        format!("{sign}{int_digits}")
    } else {
        format!("{sign}{int_digits}.{frac_digits}")
    })
}

/// Compares digit strings rather than values: re-parsing the rendered double
/// would compare it with itself and never see what was lost.
fn same_decimal_digits(text: &str, value: f64) -> bool {
    match (
        canonical_decimal(text),
        canonical_decimal(&value.to_string()),
    ) {
        (Some(before), Some(after)) => before == after,
        _ => false,
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<i8> for Value {
    fn from(v: i8) -> Self {
        Value::Int(v as i64)
    }
}
impl From<i16> for Value {
    fn from(v: i16) -> Self {
        Value::Int(v as i64)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Int(v as i64)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}
impl From<u8> for Value {
    fn from(v: u8) -> Self {
        Value::Int(v as i64)
    }
}
impl From<u16> for Value {
    fn from(v: u16) -> Self {
        Value::Int(v as i64)
    }
}
impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::Int(v as i64)
    }
}

impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::Float(v as f64)
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Text(v.to_string())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Text(v)
    }
}
impl From<&String> for Value {
    fn from(v: &String) -> Self {
        Value::Text(v.clone())
    }
}

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Value::Bytes(v)
    }
}

impl<T> From<Option<T>> for Value
where
    T: Into<Value>,
{
    fn from(v: Option<T>) -> Self {
        match v {
            Some(x) => x.into(),
            None => Value::Null,
        }
    }
}

// Borrowed-value conversions for callers passing slices like `&[1i64, 2, 3]`
// whose iterator yields `&T`. Derived from the owned impls via `Copy`.
macro_rules! impl_from_ref_copy {
    ($($t:ty),*) => {
        $(
            impl From<&$t> for Value {
                fn from(v: &$t) -> Self { Value::from(*v) }
            }
        )*
    };
}
impl_from_ref_copy!(bool, i8, i16, i32, i64, u8, u16, u32, f32, f64);

/// Wire form for `Value::Int`.
///
/// Below the safe range an integer stays a plain JSON number, which is what
/// every consumer already expects. Beyond it, the number would arrive rounded —
/// and a rounded primary key does not merely display wrong, it retargets the
/// `WHERE` clause of a delete. Such values travel in a marked envelope instead,
/// and come back as `Value::Int`, so parameter binding stays typed: a digit
/// string alone could not be told apart from a text column holding digits, and
/// that ambiguity is what blocked every simpler attempt.
mod exact_int {
    use super::{EXACT_INT_KEY, MAX_SAFE_JSON_INT};
    use serde::de::{Error as DeError, MapAccess, Visitor};
    use serde::ser::SerializeMap;
    use serde::{Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<S>(value: &i64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if (-MAX_SAFE_JSON_INT..=MAX_SAFE_JSON_INT).contains(value) {
            return serializer.serialize_i64(*value);
        }
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(EXACT_INT_KEY, &value.to_string())?;
        map.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ExactInt)
    }

    struct ExactInt;

    impl<'de> Visitor<'de> for ExactInt {
        type Value = i64;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("an integer, or the envelope carrying an exact one")
        }

        fn visit_i64<E: DeError>(self, value: i64) -> Result<i64, E> {
            Ok(value)
        }

        fn visit_u64<E: DeError>(self, value: u64) -> Result<i64, E> {
            i64::try_from(value).map_err(DeError::custom)
        }

        // Anything that is not this exact shape has to fail, so the untagged
        // enum moves on to the variant that does match.
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<i64, A::Error> {
            let key: String = match map.next_key()? {
                Some(key) => key,
                None => return Err(DeError::custom("not an exact integer")),
            };
            if key != EXACT_INT_KEY {
                return Err(DeError::custom("not an exact integer"));
            }
            let text: String = map.next_value()?;
            if map.next_key::<String>()?.is_some() {
                return Err(DeError::custom("not an exact integer"));
            }
            text.parse::<i64>().map_err(DeError::custom)
        }
    }
}

mod base64_bytes {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        STANDARD.decode(&s).map_err(serde::de::Error::custom)
    }
}

/// Column metadata.
///
/// `name` and `data_type` are stored as [`CompactString`]: most identifiers
/// fit inline (≤ 24 bytes on 64-bit) and avoid a heap allocation per column
/// per result. Serde wire format is identical to `String` — the change is
/// transparent to MessagePack / JSON consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: CompactString,
    pub data_type: CompactString,
    pub nullable: bool,
}

/// A single row of data (indexed by column order)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub values: Vec<Value>,
}

/// Row data for mutation operations (indexed by column name)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowData {
    /// Map of column name to value
    pub columns: std::collections::HashMap<String, Value>,
}

impl RowData {
    pub fn new() -> Self {
        Self {
            columns: std::collections::HashMap::new(),
        }
    }

    pub fn with_column(mut self, name: impl Into<String>, value: Value) -> Self {
        self.columns.insert(name.into(), value);
        self
    }
}

impl Default for RowData {
    fn default() -> Self {
        Self::new()
    }
}

/// Query execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Column information
    pub columns: Vec<ColumnInfo>,
    /// Result rows
    pub rows: Vec<Row>,
    /// Number of affected rows (for INSERT/UPDATE/DELETE)
    pub affected_rows: Option<u64>,
    /// Execution time in milliseconds
    pub execution_time_ms: f64,
}

impl QueryResult {
    pub fn empty() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }

    pub fn with_affected_rows(affected: u64, time_ms: f64) -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            affected_rows: Some(affected),
            execution_time_ms: time_ms,
        }
    }
}

/// Foreign Key definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKey {
    /// The column in this table
    pub column: String,
    /// The referenced table
    pub referenced_table: String,
    /// The referenced column
    pub referenced_column: String,
    /// The referenced schema (if available)
    pub referenced_schema: Option<String>,
    /// The referenced database (if available)
    pub referenced_database: Option<String>,
    /// The constraint name (optional)
    pub constraint_name: Option<String>,
    /// Whether this is a virtual relation (user-defined, not in the database)
    #[serde(default)]
    pub is_virtual: bool,
}

/// Table index definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableIndex {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    /// Engine-specific index type, when known (e.g. `btree`, `hash`, `gin`,
    /// `fulltext`, `text`, `2dsphere`). `None` means unspecified/default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_type: Option<String>,
}

/// Table schema metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    /// Column definitions
    pub columns: Vec<TableColumn>,
    /// Primary key columns (if any)
    pub primary_key: Option<Vec<String>>,
    /// Foreign keys
    pub foreign_keys: Vec<ForeignKey>,
    /// Estimated row count (if available)
    pub row_count_estimate: Option<u64>,
    /// Table indexes
    pub indexes: Vec<TableIndex>,
}

/// Column metadata for table schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableColumn {
    /// Column name
    pub name: String,
    /// Data type (database-specific)
    pub data_type: String,
    /// Whether the column allows NULL values
    pub nullable: bool,
    /// Default value expression (if any)
    pub default_value: Option<String>,
    /// Whether this column is part of the primary key
    pub is_primary_key: bool,
    /// Whether the database fills this column itself (auto_increment / IDENTITY /
    /// serial / SQLite rowid). Such columns must not receive a generated value.
    #[serde(default)]
    pub is_auto_increment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CollectionListOptions {
    pub search: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionList {
    pub collections: Vec<Collection>,
    pub total_count: u32,
}

/// Type of database routine
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RoutineType {
    Function,
    Procedure,
}

/// Database routine metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub namespace: Namespace,
    pub name: String,
    pub routine_type: RoutineType,
    pub arguments: String,
    pub return_type: Option<String>,
    pub language: Option<String>,
}

/// Options for listing routines
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutineListOptions {
    pub search: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub routine_type: Option<RoutineType>,
}

/// Paginated routine list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineList {
    pub routines: Vec<Routine>,
    pub total_count: u32,
}

/// Full routine definition (CREATE statement) for viewing/editing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineDefinition {
    pub name: String,
    pub namespace: Namespace,
    pub routine_type: RoutineType,
    /// Full CREATE OR REPLACE statement
    pub definition: String,
    pub language: Option<String>,
    pub arguments: String,
    pub return_type: Option<String>,
}

/// Result of a routine drop operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineOperationResult {
    pub success: bool,
    /// The SQL command that was executed
    pub executed_command: String,
    pub message: Option<String>,
    pub execution_time_ms: f64,
}

/// Timing of a database trigger
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TriggerTiming {
    Before,
    After,
    InsteadOf,
}

/// Event that fires a trigger
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TriggerEvent {
    Insert,
    Update,
    Delete,
    Truncate,
}

/// Database trigger metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub namespace: Namespace,
    pub name: String,
    pub table_name: String,
    pub timing: TriggerTiming,
    pub events: Vec<TriggerEvent>,
    pub enabled: bool,
    /// For PostgreSQL: the function called by the trigger
    pub function_name: Option<String>,
}

/// Options for listing triggers
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TriggerListOptions {
    pub search: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Paginated trigger list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerList {
    pub triggers: Vec<Trigger>,
    pub total_count: u32,
}

/// Status of a MySQL scheduled event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventStatus {
    Enabled,
    Disabled,
    SlavesideDisabled,
}

/// MySQL scheduled event metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseEvent {
    pub namespace: Namespace,
    pub name: String,
    pub event_type: String,
    pub interval_value: Option<String>,
    pub interval_field: Option<String>,
    pub status: EventStatus,
}

/// Options for listing events
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventListOptions {
    pub search: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Paginated event list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventList {
    pub events: Vec<DatabaseEvent>,
    pub total_count: u32,
}

/// Full trigger definition (CREATE statement) for viewing/editing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerDefinition {
    pub name: String,
    pub namespace: Namespace,
    pub table_name: String,
    pub timing: TriggerTiming,
    pub events: Vec<TriggerEvent>,
    /// Full CREATE TRIGGER statement
    pub definition: String,
    pub enabled: bool,
    pub function_name: Option<String>,
}

/// Result of a trigger operation (drop, enable, disable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerOperationResult {
    pub success: bool,
    /// The SQL command that was executed
    pub executed_command: String,
    pub message: Option<String>,
    pub execution_time_ms: f64,
}

/// Full event definition (CREATE statement) for viewing/editing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDefinition {
    pub name: String,
    pub namespace: Namespace,
    /// Full CREATE EVENT statement
    pub definition: String,
    pub status: EventStatus,
}

/// Result of an event operation (drop, enable, disable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventOperationResult {
    pub success: bool,
    /// The SQL command that was executed
    pub executed_command: String,
    pub message: Option<String>,
    pub execution_time_ms: f64,
}

/// Database sequence metadata (MariaDB 10.3+)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sequence {
    pub namespace: Namespace,
    pub name: String,
    pub data_type: String,
    pub start_value: i64,
    pub min_value: i64,
    pub max_value: i64,
    pub increment: i64,
    pub cycle: bool,
    pub cache_size: i64,
}

/// Options for listing sequences
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SequenceListOptions {
    pub search: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Paginated sequence list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceList {
    pub sequences: Vec<Sequence>,
    pub total_count: u32,
}

/// Full sequence definition (CREATE statement)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceDefinition {
    pub name: String,
    pub namespace: Namespace,
    /// Full CREATE SEQUENCE statement
    pub definition: String,
}

/// Result of a sequence operation (drop)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceOperationResult {
    pub success: bool,
    /// The SQL command that was executed
    pub executed_command: String,
    pub message: Option<String>,
    pub execution_time_ms: f64,
}

/// Information about a character set available for database creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharsetInfo {
    pub name: String,
    pub description: String,
    pub default_collation: String,
    pub collations: Vec<CollationInfo>,
}

/// Information about a collation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollationInfo {
    pub name: String,
    pub is_default: bool,
}

/// Options available when creating a database (charsets, collations, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreationOptions {
    pub charsets: Vec<CharsetInfo>,
}

/// Sort direction for query results
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

/// Filter operator for WHERE clauses
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    #[default]
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    IsNull,
    IsNotNull,
    /// Regular-expression match. Pattern is in `ColumnFilter::value` (string);
    /// flags (`i`, `m`, `x`, `s`) are in `ColumnFilter::options.regex_flags`.
    Regex,
    /// Engine-native full-text search. Query is in `ColumnFilter::value`;
    /// optional language is in `ColumnFilter::options.text_language`.
    Text,
}

/// Per-filter tuning options. Kept separate from `FilterOperator` so that
/// the operator stays `Copy` and the existing on-wire representation of
/// unit variants (plain snake_case strings) is preserved.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilterOptions {
    /// Regex flags string for `FilterOperator::Regex` (subset of `imxs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regex_flags: Option<String>,
    /// Language tag for `FilterOperator::Text` (e.g. `"english"`, `"french"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_language: Option<String>,
}

impl FilterOptions {
    pub fn is_empty(&self) -> bool {
        self.regex_flags.is_none() && self.text_language.is_none()
    }

    /// Returns only the valid regex flags (`i`, `m`, `x`, `s`) — defense in
    /// depth against backends that interpolate flags into SQL literals or
    /// protocol documents. The UI is expected to sanitize on entry, but this
    /// guarantees it regardless of caller (including raw API consumers).
    pub fn sanitized_regex_flags(&self) -> String {
        self.regex_flags
            .as_deref()
            .unwrap_or("")
            .chars()
            .filter(|c| matches!(c, 'i' | 'm' | 'x' | 's'))
            .collect()
    }

    /// Returns the requested text-search language if it passes a strict
    /// identifier check (`[a-z_]+`, 1..=32 chars), otherwise returns
    /// `fallback`. Used by drivers that must interpolate the language into a
    /// server-side function call (e.g. PostgreSQL's `to_tsvector(lang, …)`).
    pub fn sanitized_text_language(&self, fallback: &str) -> String {
        match self.text_language.as_deref() {
            Some(lang)
                if !lang.is_empty()
                    && lang.len() <= 32
                    && lang.chars().all(|c| c.is_ascii_lowercase() || c == '_') =>
            {
                lang.to_string()
            }
            _ => fallback.to_string(),
        }
    }
}

/// Column filter for WHERE clauses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnFilter {
    pub column: String,
    pub operator: FilterOperator,
    pub value: Value,
    #[serde(default, skip_serializing_if = "FilterOptions::is_empty")]
    pub options: FilterOptions,
}

/// Options for querying table data with pagination, sorting, and filtering
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TableQueryOptions {
    /// Page number (1-indexed; `0` also maps to the first page for compatibility)
    pub page: Option<u32>,
    /// Page size (default: 50, max: 10000)
    pub page_size: Option<u32>,
    /// Column to sort by
    pub sort_column: Option<String>,
    /// Sort direction (default: Asc)
    pub sort_direction: Option<SortDirection>,
    /// Column filters
    pub filters: Option<Vec<ColumnFilter>>,
    /// Full-text search term (searches all string columns)
    pub search: Option<String>,
    /// Columns the search applies to. When absent the driver falls back to
    /// reading the catalog and searching everything, which costs a round-trip
    /// per call — callers that already hold the schema should send the scope
    /// rather than make the driver rediscover it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_columns: Option<Vec<String>>,
    /// How the search term matches. Defaults to the substring behaviour the
    /// product has always had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_mode: Option<SearchMode>,
    /// Whether the driver should compute an exact total row count.
    ///
    /// Defaults to `Exact` for backwards compatibility. Interactive table
    /// browsing can request `None` and use `has_more` instead, avoiding an
    /// expensive count on every page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count_mode: Option<CountMode>,
    /// Unique key the driver may use as a keyset tie-breaker. Supplied by the
    /// caller, which already holds the schema; without it the driver has no
    /// total order to rely on and stays on `OFFSET`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyset_columns: Option<Vec<String>>,
    /// Opaque keyset cursor from a previous page. When present the driver
    /// walks from that boundary and ignores `page`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Handle a caller can later pass to `cancel` to interrupt an exact count.
    ///
    /// Never serialized: the query cache keys on the serialized options, and a
    /// per-call identifier there would turn every lookup into a miss.
    #[serde(default, skip_serializing)]
    pub query_id: Option<QueryId>,
}

impl TableQueryOptions {
    /// Effective page number
    pub fn effective_page(&self) -> u32 {
        self.page.unwrap_or(0)
    }

    /// Effective page size
    pub fn effective_page_size(&self) -> u32 {
        self.page_size.unwrap_or(50).clamp(1, 10000)
    }

    /// SQL OFFSET for pagination
    pub fn offset(&self) -> u64 {
        let page = self.effective_page();
        let zero_indexed_page = if page > 0 { page - 1 } else { 0 };
        zero_indexed_page as u64 * self.effective_page_size() as u64
    }

    pub fn effective_count_mode(&self) -> CountMode {
        self.count_mode.unwrap_or_default()
    }

    pub fn wants_exact_total(&self) -> bool {
        matches!(self.effective_count_mode(), CountMode::Exact)
    }

    /// True when the caller wants a total at all, exact or approximate.
    ///
    /// Drivers whose cheap total happens to be exact (columnar engines, Redis
    /// cardinality commands) answer `Estimated` with an exact number rather
    /// than degrading it — an honest total is never worse than an estimate.
    pub fn wants_any_total(&self) -> bool {
        !matches!(self.effective_count_mode(), CountMode::None)
    }

    /// Non-empty search term, if any.
    pub fn effective_search(&self) -> Option<&str> {
        self.search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Caller-supplied search scope, ignoring an empty list — which would mean
    /// "search nothing" and silently return no rows.
    pub fn effective_search_columns(&self) -> Option<&[String]> {
        self.search_columns
            .as_deref()
            .filter(|cols| !cols.is_empty())
    }

    /// Whether a keyset walk applies to this request.
    ///
    /// A cursor means the caller is walking a chain; the first page starts one.
    /// A later page *without* a cursor means the caller is paging by offset,
    /// and answering it with a keyset would drop the offset and silently serve
    /// the first page again.
    pub fn keyset_applies(&self) -> bool {
        self.cursor.is_some() || self.effective_page() <= 1
    }

    /// Caller-supplied unique key, ignoring an empty list.
    pub fn effective_keyset_columns(&self) -> Option<&[String]> {
        self.keyset_columns
            .as_deref()
            .filter(|cols| !cols.is_empty())
    }

    pub fn effective_search_mode(&self) -> SearchMode {
        self.search_mode.unwrap_or_default()
    }

    /// `LIKE` pattern for `term` under the requested mode. The anchored form
    /// omits the leading wildcard, which is the whole reason an index can be
    /// considered at all.
    pub fn search_pattern(&self, term: &str) -> String {
        match self.effective_search_mode() {
            SearchMode::StartsWith => format!("{term}%"),
            SearchMode::Contains => format!("%{term}%"),
        }
    }

    /// Whether a table-level engine estimate would describe the rows actually
    /// returned. Catalog statistics count the whole table, so they must not be
    /// presented as the total of a filtered or searched view.
    pub fn estimate_matches_scope(&self) -> bool {
        self.search.as_deref().unwrap_or("").is_empty()
            && self.filters.as_deref().unwrap_or(&[]).is_empty()
    }

    /// Number of rows requested from the engine.
    ///
    /// Without an exact total, one extra row is fetched to derive `has_more`
    /// without a separate count query.
    pub fn fetch_size(&self) -> u32 {
        let page_size = self.effective_page_size();
        if self.wants_exact_total() {
            page_size
        } else {
            page_size.saturating_add(1)
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CountMode {
    None,
    /// Engine metadata rather than a scan. Cheap, and approximate by nature:
    /// a driver with no trustworthy source answers `None` instead of guessing.
    Estimated,
    #[default]
    Exact,
}

/// How a search term is matched against a column.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    /// Substring match. Never uses an index: the pattern starts with a
    /// wildcard, so the engine has to read every row.
    #[default]
    Contains,
    /// Prefix match. Index-eligible on a text column, which is the point.
    StartsWith,
}

/// How a page was walked.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaginationStrategy {
    /// `OFFSET`: cost grows with depth, and concurrent writes shift rows
    /// between pages.
    #[default]
    Offset,
    /// Keyset on a unique ordering: constant cost, stable under writes.
    Keyset,
}

/// What the ordering of a page can be relied on for.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrderingGuarantee {
    /// No total order: rows may repeat or be skipped across pages. The common
    /// case for views and tables without a unique key.
    #[default]
    None,
    /// Total order via a unique tie-breaker, so every row appears exactly once.
    Stable,
}

/// Provenance of a known `total_rows`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TotalRowsSource {
    Exact,
    Estimated,
}

/// Paginated query result with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedQueryResult {
    /// The data rows for the current page
    pub result: QueryResult,
    /// Total row count, or `None` when it is unknown. Never a lower bound: a
    /// consumer that ignores the provenance still cannot mistake it for a
    /// total.
    #[serde(default)]
    pub total_rows: Option<u64>,
    /// Provenance of `total_rows`. `Some` if and only if `total_rows` is.
    #[serde(default)]
    pub total_rows_source: Option<TotalRowsSource>,
    /// When the engine last refreshed the statistics behind an estimate, as a
    /// Unix timestamp in milliseconds. Only engines that expose it fill it in.
    #[serde(default)]
    pub total_rows_as_of: Option<i64>,
    /// Current page number
    pub page: u32,
    /// Page size used
    pub page_size: u32,
    /// Whether another page is available.
    #[serde(default)]
    pub has_more: bool,
    /// Cursor for the page after this one. `None` when the driver paged by
    /// offset, or when this is the last page.
    #[serde(default)]
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub pagination_strategy: PaginationStrategy,
    #[serde(default)]
    pub ordering_guarantee: OrderingGuarantee,
}

impl PaginatedQueryResult {
    pub fn new(result: QueryResult, total_rows: u64, page: u32, page_size: u32) -> Self {
        Self::from_optional_total(result, Some(total_rows), page, page_size)
    }

    /// Builds a page from either an exact total or an over-fetched result.
    ///
    /// When `total_rows` is `None`, callers must have requested
    /// `page_size + 1` rows. The extra row is removed before returning.
    pub fn from_optional_total(
        mut result: QueryResult,
        total_rows: Option<u64>,
        page: u32,
        page_size: u32,
    ) -> Self {
        let offset = page.saturating_sub(1) as u64 * page_size as u64;
        let over_fetched = result.rows.len() > page_size as usize;
        if over_fetched {
            result.rows.truncate(page_size as usize);
        }

        let returned_rows = result.rows.len() as u64;
        let has_more = match total_rows {
            Some(total) => offset.saturating_add(returned_rows) < total,
            None => over_fetched,
        };

        Self {
            result,
            total_rows,
            total_rows_source: total_rows.map(|_| TotalRowsSource::Exact),
            total_rows_as_of: None,
            page,
            page_size,
            has_more,
            next_cursor: None,
            pagination_strategy: PaginationStrategy::Offset,
            ordering_guarantee: OrderingGuarantee::None,
        }
    }

    /// Marks the page as keyset-walked. `next_cursor` is `None` on the last
    /// page, which is what `has_more` already says.
    pub fn with_keyset(mut self, next_cursor: Option<String>) -> Self {
        self.next_cursor = next_cursor;
        self.pagination_strategy = PaginationStrategy::Keyset;
        self.ordering_guarantee = OrderingGuarantee::Stable;
        self
    }

    /// Attaches an engine estimate. Never overrides an exact total: an
    /// approximation must not replace a number the caller paid a scan for.
    pub fn with_estimate(mut self, estimate: Option<u64>, as_of: Option<i64>) -> Self {
        if self.total_rows_source == Some(TotalRowsSource::Exact) {
            return self;
        }
        if let Some(value) = estimate {
            self.total_rows = Some(value);
            self.total_rows_source = Some(TotalRowsSource::Estimated);
            self.total_rows_as_of = as_of;
        }
        self
    }
}

/// Type of maintenance operation available for a table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceOperationType {
    Vacuum,
    Analyze,
    Reindex,
    Optimize,
    Repair,
    Check,
    Cluster,
    RebuildIndexes,
    UpdateStatistics,
    Compact,
    Validate,
    IntegrityCheck,
    ChangeEngine,
}

/// Options for a specific maintenance operation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MaintenanceOptions {
    /// PostgreSQL: VACUUM FULL (rewrites entire table, exclusive lock)
    pub full: Option<bool>,
    /// PostgreSQL: VACUUM ANALYZE (combine vacuum with analyze)
    pub with_analyze: Option<bool>,
    /// PostgreSQL: VACUUM VERBOSE / MySQL: extended check
    pub verbose: Option<bool>,
    /// PostgreSQL CLUSTER: index name to cluster by
    pub index_name: Option<String>,
    /// MySQL: target engine for ALTER TABLE ... ENGINE=
    pub target_engine: Option<String>,
}

/// Request to run a maintenance operation on a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceRequest {
    pub operation: MaintenanceOperationType,
    #[serde(default)]
    pub options: MaintenanceOptions,
}

/// Description of an available maintenance operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceOperationInfo {
    pub operation: MaintenanceOperationType,
    /// Whether this operation can be heavy/slow on large tables
    pub is_heavy: bool,
    /// Whether this operation requires extra options
    pub has_options: bool,
}

/// Severity level of a maintenance message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceMessageLevel {
    Info,
    Warning,
    Error,
    Status,
}

/// A single status message from a maintenance operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceMessage {
    pub level: MaintenanceMessageLevel,
    pub text: String,
}

/// Result of a maintenance operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceResult {
    /// The SQL/command that was executed
    pub executed_command: String,
    /// Status messages returned by the database
    pub messages: Vec<MaintenanceMessage>,
    /// Execution time in milliseconds
    pub execution_time_ms: f64,
    /// Whether the operation succeeded
    pub success: bool,
}

/// Result of a "truncate all tables" operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncateAllResult {
    /// The command(s) that were executed
    pub executed_command: String,
    /// Names of the tables that were truncated
    pub truncated_tables: Vec<String>,
    /// Status messages returned by the database
    pub messages: Vec<MaintenanceMessage>,
    /// Execution time in milliseconds
    pub execution_time_ms: f64,
    /// Whether the operation succeeded
    pub success: bool,
}

#[cfg(test)]
mod pagination_tests {
    use super::*;

    fn result_with_rows(count: usize) -> QueryResult {
        QueryResult {
            columns: Vec::new(),
            rows: (0..count)
                .map(|index| Row {
                    values: vec![Value::Int(index as i64)],
                })
                .collect(),
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }

    #[test]
    fn exact_count_remains_the_backwards_compatible_default() {
        let options = TableQueryOptions::default();
        assert_eq!(options.effective_count_mode(), CountMode::Exact);
        assert_eq!(options.fetch_size(), 50);
    }

    #[test]
    fn count_free_pages_overfetch_one_row() {
        let options = TableQueryOptions {
            page_size: Some(100),
            count_mode: Some(CountMode::None),
            ..Default::default()
        };
        assert_eq!(options.fetch_size(), 101);
        assert_eq!(
            serde_json::to_value(&options).unwrap()["count_mode"],
            serde_json::json!("none")
        );
    }

    #[test]
    fn omitted_count_mode_deserializes_to_the_compatible_exact_behavior() {
        let options: TableQueryOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(options.effective_count_mode(), CountMode::Exact);
    }

    #[test]
    fn query_id_is_accepted_but_kept_out_of_the_cache_key() {
        let raw = r#"{"query_id":"7c9e6679-7425-40de-944b-e07fc1f90ae7"}"#;
        let options: TableQueryOptions = serde_json::from_str(raw).unwrap();
        assert!(options.query_id.is_some());
        assert!(
            !serde_json::to_string(&options)
                .unwrap()
                .contains("query_id")
        );
    }

    #[test]
    fn overfetched_page_reports_has_more_and_hides_the_extra_row() {
        let page = PaginatedQueryResult::from_optional_total(result_with_rows(101), None, 2, 100);
        assert_eq!(page.result.rows.len(), 100);
        assert_eq!(page.total_rows, None);
        assert_eq!(page.total_rows_source, None);
        assert!(page.has_more);
    }

    #[test]
    fn final_count_free_page_reports_no_total() {
        let page = PaginatedQueryResult::from_optional_total(result_with_rows(50), None, 3, 100);
        assert_eq!(page.result.rows.len(), 50);
        assert_eq!(page.total_rows, None);
        assert!(!page.has_more);
    }

    #[test]
    fn exact_page_keeps_total_metadata() {
        let page = PaginatedQueryResult::new(result_with_rows(100), 250, 2, 100);
        assert_eq!(page.result.rows.len(), 100);
        assert_eq!(page.total_rows, Some(250));
        assert_eq!(page.total_rows_source, Some(TotalRowsSource::Exact));
        assert!(page.has_more);
    }

    #[test]
    fn an_estimate_fills_an_unknown_total_but_never_replaces_an_exact_one() {
        let unknown = PaginatedQueryResult::from_optional_total(result_with_rows(10), None, 1, 100)
            .with_estimate(Some(2_400_000), Some(1_700_000_000_000));
        assert_eq!(unknown.total_rows, Some(2_400_000));
        assert_eq!(unknown.total_rows_source, Some(TotalRowsSource::Estimated));
        assert_eq!(unknown.total_rows_as_of, Some(1_700_000_000_000));

        let exact = PaginatedQueryResult::new(result_with_rows(10), 42, 1, 100)
            .with_estimate(Some(2_400_000), None);
        assert_eq!(exact.total_rows, Some(42));
        assert_eq!(exact.total_rows_source, Some(TotalRowsSource::Exact));
    }

    #[test]
    fn a_later_page_without_a_cursor_is_not_a_keyset_walk() {
        let unique = Some(vec!["id".to_string()]);

        // First page: starts the chain.
        let first = TableQueryOptions {
            page: Some(1),
            keyset_columns: unique.clone(),
            ..Default::default()
        };
        assert!(first.keyset_applies());

        // Later page carrying a cursor: continues the chain.
        let walking = TableQueryOptions {
            page: Some(4),
            cursor: Some("opaque".into()),
            keyset_columns: unique.clone(),
            ..Default::default()
        };
        assert!(walking.keyset_applies());

        // Later page with no cursor: the caller is paging by offset. Serving a
        // keyset here drops the offset and repeats the first page.
        let offset_paging = TableQueryOptions {
            page: Some(4),
            keyset_columns: unique,
            ..Default::default()
        };
        assert!(!offset_paging.keyset_applies());
    }

    #[test]
    fn an_empty_search_scope_falls_back_to_catalog_discovery() {
        let none = TableQueryOptions {
            search: Some("  ".into()),
            ..Default::default()
        };
        assert_eq!(none.effective_search(), None);
        assert_eq!(none.effective_search_columns(), None);

        // An empty list would otherwise mean "search no column at all", which
        // silently returns nothing instead of falling back.
        let empty_scope = TableQueryOptions {
            search: Some("alpha".into()),
            search_columns: Some(Vec::new()),
            ..Default::default()
        };
        assert_eq!(empty_scope.effective_search(), Some("alpha"));
        assert_eq!(empty_scope.effective_search_columns(), None);

        let scoped = TableQueryOptions {
            search: Some("alpha".into()),
            search_columns: Some(vec!["name".into()]),
            ..Default::default()
        };
        assert_eq!(
            scoped.effective_search_columns().map(<[String]>::len),
            Some(1)
        );
    }

    #[test]
    fn a_table_estimate_is_not_offered_for_a_filtered_or_searched_view() {
        let plain = TableQueryOptions {
            count_mode: Some(CountMode::Estimated),
            ..Default::default()
        };
        assert!(plain.wants_any_total());
        assert!(!plain.wants_exact_total());
        assert!(plain.estimate_matches_scope());

        let searched = TableQueryOptions {
            search: Some("alpha".into()),
            ..plain.clone()
        };
        assert!(!searched.estimate_matches_scope());
    }

    #[test]
    fn unknown_total_serializes_as_null_rather_than_a_missing_field() {
        let page = PaginatedQueryResult::from_optional_total(result_with_rows(10), None, 1, 100);
        let json = serde_json::to_value(&page).unwrap();
        assert!(json["total_rows"].is_null());
        assert!(json["total_rows_source"].is_null());
        assert_eq!(json["has_more"], serde_json::json!(false));
    }
    #[test]
    fn a_decimal_a_double_holds_stays_a_number() {
        for text in ["0", "1", "-42", "1.5", "0.25", "123456.789"] {
            assert!(
                matches!(Value::from_decimal_text(text.to_string()), Value::Float(_)),
                "{text} should stay a number"
            );
        }
    }

    #[test]
    fn a_decimal_a_double_would_round_travels_as_text() {
        for text in [
            "123456789012345678901.1234567890",
            "0.12345678901234567890123",
            "9007199254740993",
        ] {
            match Value::from_decimal_text(text.to_string()) {
                Value::Text(kept) => assert_eq!(kept, text),
                other => panic!("{text} should stay exact, got {other:?}"),
            }
        }
    }

    // The engine renders the column's scale; that is not a loss and must not
    // push an ordinary value onto the text path.
    #[test]
    fn trailing_zeros_are_not_a_loss() {
        assert!(matches!(
            Value::from_decimal_text("1.50".to_string()),
            Value::Float(f) if f == 1.5
        ));
        assert!(matches!(
            Value::from_decimal_text("-0.0".to_string()),
            Value::Float(_)
        ));
    }

    #[test]
    fn an_integer_in_the_safe_range_stays_a_plain_json_number() {
        for value in [0i64, 1, -1, 42, MAX_SAFE_JSON_INT, -MAX_SAFE_JSON_INT] {
            let json = serde_json::to_string(&Value::Int(value)).unwrap();
            assert_eq!(json, value.to_string(), "{value} should stay a number");
        }
    }

    // A rounded primary key does not merely display wrong: it retargets the
    // WHERE clause of a delete.
    #[test]
    fn an_integer_past_the_safe_range_travels_in_its_envelope() {
        let value = 9_007_199_254_740_993i64;
        let json = serde_json::to_string(&Value::Int(value)).unwrap();
        assert_eq!(json, r#"{"$qoreInt":"9007199254740993"}"#);

        let back: Value = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, Value::Int(v) if v == value));
    }

    #[test]
    fn every_integer_round_trips_unchanged() {
        for value in [
            0i64,
            -1,
            MAX_SAFE_JSON_INT,
            MAX_SAFE_JSON_INT + 1,
            -MAX_SAFE_JSON_INT - 1,
            1_409_876_543_210_987_654,
            i64::MAX,
            i64::MIN,
        ] {
            let json = serde_json::to_string(&Value::Int(value)).unwrap();
            let back: Value = serde_json::from_str(&json).unwrap();
            assert!(
                matches!(back, Value::Int(v) if v == value),
                "{value} did not survive, got {back:?} from {json}"
            );
        }
    }

    // `Int` is tried before `Json` in the untagged enum, so a document must not
    // be swallowed by a shape that merely resembles the envelope.
    #[test]
    fn a_document_is_never_mistaken_for_an_envelope() {
        for json in [
            r#"{"id":1}"#,
            r#"{"$qoreInt":"12","extra":1}"#,
            r#"{"$qoreInt":12}"#,
            r#"{"$qoreIntish":"12"}"#,
            r#"{}"#,
        ] {
            let value: Value = serde_json::from_str(json).unwrap();
            assert!(
                matches!(value, Value::Json(_)),
                "{json} should stay a document, got {value:?}"
            );
        }
    }

    // A text column holding digits must not become an integer on the way back,
    // or its parameter would be bound with the wrong type.
    #[test]
    fn a_digit_string_stays_text() {
        let value: Value = serde_json::from_str(r#""9007199254740993""#).unwrap();
        assert!(matches!(value, Value::Text(t) if t == "9007199254740993"));
    }

    // `Array` is unreachable on the wire — `Json` precedes it and accepts any
    // JSON — but that predates the envelope and is asserted here so a change to
    // the variant order shows up.
    #[test]
    fn the_other_variants_are_untouched() {
        for (json, ok) in [
            (
                "null",
                matches!(serde_json::from_str::<Value>("null").unwrap(), Value::Null),
            ),
            (
                "true",
                matches!(
                    serde_json::from_str::<Value>("true").unwrap(),
                    Value::Bool(true)
                ),
            ),
            (
                "1.5",
                matches!(
                    serde_json::from_str::<Value>("1.5").unwrap(),
                    Value::Float(_)
                ),
            ),
            (
                "[1,2]",
                matches!(
                    serde_json::from_str::<Value>("[1,2]").unwrap(),
                    Value::Json(_)
                ),
            ),
        ] {
            assert!(ok, "{json} changed shape");
        }
    }

    #[test]
    fn anything_that_is_not_a_plain_decimal_stays_text() {
        for text in ["NaN", "1e5", "", "abc"] {
            assert!(
                matches!(Value::from_decimal_text(text.to_string()), Value::Text(_)),
                "{text} should stay text"
            );
        }
    }
}

// SPDX-License-Identifier: Apache-2.0

//! Universal data types for the QoreDB Data Engine
//!
//! These types provide a normalized representation of database concepts
//! across SQL and NoSQL engines.

use serde::{Deserialize, Serialize};
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

/// Database connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub driver: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(skip_serializing)]
    pub password: String,
    pub database: Option<String>,
    pub ssl: bool,
    /// Optional SSL mode override (e.g. "verify-full", "verify-ca", "require", "prefer", "disable").
    /// When set, takes precedence over the `ssl` boolean for drivers that support it.
    #[serde(default)]
    pub ssl_mode: Option<String>,
    pub environment: String,
    pub read_only: bool,
    pub pool_max_connections: Option<u32>,
    pub pool_min_connections: Option<u32>,
    pub pool_acquire_timeout_secs: Option<u32>,
    pub ssh_tunnel: Option<SshTunnelConfig>,
    /// Network proxy configuration (HTTP CONNECT or SOCKS5)
    #[serde(default)]
    pub proxy: Option<ProxyConfig>,
}

/// Network proxy configuration for corporate environments
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Host key verification policy (security-critical).
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

/// SSH authentication method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SshAuth {
    Password {
        password: String,
    },
    Key {
        private_key_path: String,
        passphrase: Option<String>,
    },
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DriverCapabilities {
    pub transactions: bool,
    pub mutations: bool,
    pub cancel: CancelSupport,
    pub supports_ssh: bool,
    pub schema: bool,
    pub streaming: bool,
    pub explain: bool,
    pub maintenance: bool,
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

/// Universal value representation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(#[serde(with = "base64_bytes")] Vec<u8>),
    Json(serde_json::Value),
    Array(Vec<Value>),
}

mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
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

/// Column metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
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
}

// ==================== Collection List Types ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionListOptions {
    pub search: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

impl Default for CollectionListOptions {
    fn default() -> Self {
        Self {
            search: None,
            page: None,
            page_size: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionList {
    pub collections: Vec<Collection>,
    pub total_count: u32,
}

// ==================== Routine Types ====================

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

// ==================== Trigger Types ====================

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

// ==================== Event Types (MySQL) ====================

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

// ==================== Trigger Definition & Operation ====================

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

// ==================== Sequence Types (MariaDB) ====================

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

// ==================== Database Creation Options ====================

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

// ==================== Table Query Types (Pagination) ====================

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
}

/// Column filter for WHERE clauses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnFilter {
    pub column: String,
    pub operator: FilterOperator,
    pub value: Value,
}

/// Options for querying table data with pagination, sorting, and filtering
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TableQueryOptions {
    /// Page number (0-indexed)
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
}

/// Paginated query result with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedQueryResult {
    /// The data rows for the current page
    pub result: QueryResult,
    /// Total number of rows matching the query
    pub total_rows: u64,
    /// Current page (0-indexed)
    pub page: u32,
    /// Page size used
    pub page_size: u32,
    /// Total number of pages
    pub total_pages: u32,
}

impl PaginatedQueryResult {
    pub fn new(result: QueryResult, total_rows: u64, page: u32, page_size: u32) -> Self {
        let total_pages = if page_size == 0 {
            0
        } else {
            ((total_rows + page_size as u64 - 1) / page_size as u64) as u32
        };
        Self {
            result,
            total_rows,
            page,
            page_size,
            total_pages,
        }
    }
}

// ==================== Maintenance Types ====================

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

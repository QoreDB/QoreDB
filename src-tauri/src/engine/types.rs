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
    pub ssh_tunnel: Option<SshTunnelConfig>,
}

/// SSH tunnel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshTunnelConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
}

/// SSH authentication method
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SshAuth {
    Password { password: String },
    Key { private_key_path: String, passphrase: Option<String> },
}

/// Namespace represents the hierarchy level above collections
/// - For PostgreSQL: database + schema
/// - For MySQL: database
/// - For MongoDB: database
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
    use serde::{Deserialize, Deserializer, Serializer};
    use base64::{Engine, engine::general_purpose::STANDARD};

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
///
/// Used for INSERT and UPDATE operations where values are specified by column name.
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

/// Table schema metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    /// Column definitions
    pub columns: Vec<TableColumn>,
    /// Primary key columns (if any)
    pub primary_key: Option<Vec<String>>,
    /// Estimated row count (if available)
    pub row_count_estimate: Option<u64>,
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

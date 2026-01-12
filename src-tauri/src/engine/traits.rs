//! DataEngine trait definition
//!
//! This is the core abstraction that all database drivers must implement.
//! It provides a unified interface for connecting, querying, and managing
//! database sessions across SQL and NoSQL engines.

use async_trait::async_trait;

use crate::engine::error::EngineResult;
use crate::engine::types::{
    Collection, ConnectionConfig, Namespace, QueryResult, SessionId,
};

/// Core trait that all database drivers must implement
///
/// This trait defines the universal interface for database operations.
/// Each driver (PostgreSQL, MySQL, MongoDB, etc.) implements this trait
/// to provide consistent behavior across different database engines.
#[async_trait]
pub trait DataEngine: Send + Sync {
    /// Returns the unique identifier for this driver (e.g., "postgres", "mysql", "mongodb")
    fn driver_id(&self) -> &'static str;

    /// Returns a human-readable name for this driver
    fn driver_name(&self) -> &'static str;

    /// Tests the connection without establishing a persistent session
    ///
    /// Use this to validate credentials before saving a connection.
    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()>;

    /// Establishes a connection and returns a session identifier
    ///
    /// The session ID is used for all subsequent operations on this connection.
    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId>;

    /// Closes a session and releases associated resources
    async fn disconnect(&self, session: SessionId) -> EngineResult<()>;

    /// Lists all namespaces (databases/schemas) accessible in this session
    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>>;

    /// Lists all collections (tables/views/collections) in a namespace
    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
    ) -> EngineResult<Vec<Collection>>;

    /// Executes a query and returns the result
    ///
    /// For SQL engines: executes SQL statements
    /// For MongoDB: expects JSON query format
    async fn execute(&self, session: SessionId, query: &str) -> EngineResult<QueryResult>;

    /// Cancels a running query for the given session
    async fn cancel(&self, session: SessionId) -> EngineResult<()>;
}

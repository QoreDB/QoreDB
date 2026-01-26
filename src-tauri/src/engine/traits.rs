//! DataEngine trait definition
//!
//! This is the core abstraction that all database drivers must implement.
//! It provides a unified interface for connecting, querying, and managing
//! database sessions across SQL and NoSQL engines.

use async_trait::async_trait;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::types::{
    CancelSupport, CollectionList, CollectionListOptions, ConnectionConfig, DriverCapabilities, Namespace,
    QueryId, QueryResult, Row, RowData, SessionId, TableSchema, ColumnInfo, Value, ForeignKey
};

/// Events emitted during query streaming
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Column definitions (emitted once at the start)
    Columns(Vec<ColumnInfo>),
    /// A single data row
    Row(Row),
    /// Error occurred during streaming
    Error(String),
    /// Streaming complete. Contains affected rows count if applicable.
    Done(u64),
}

/// Sender for streaming events
pub type StreamSender = tokio::sync::mpsc::Sender<StreamEvent>;

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
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList>;

    /// Creates a new database (or schema in PostgreSQL)
    /// 
    /// For MongoDB, 'options' can contain {"collection": "name"} to create the initial collection.
    async fn create_database(&self, session: SessionId, name: &str, options: Option<Value>) -> EngineResult<()>;

    /// Drops an existing database (or schema in PostgreSQL)
    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()>;

    /// Executes a query and returns the result
    ///
    /// For SQL engines: executes SQL statements
    /// For MongoDB: expects JSON query format
    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult>;

    /// Executes a query with an optional namespace context.
    ///
    /// Default implementation ignores the namespace and delegates to `execute()`.
    /// Drivers that need per-query database/schema selection (e.g. MySQL `USE db`,
    /// PostgreSQL `SET LOCAL search_path`) can override this.
    async fn execute_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let _ = namespace;
        self.execute(session, query, query_id).await
    }

    /// Executes a query and streams results via the provided sender
    async fn execute_stream(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        let _ = (session, query, query_id, sender);
        Err(crate::engine::error::EngineError::not_supported(
            "Streaming is not supported by this driver",
        ))
    }

    /// Streams query results with an optional namespace context.
    ///
    /// Default implementation ignores the namespace and delegates to `execute_stream()`.
    async fn execute_stream_in_namespace(
        &self,
        session: SessionId,
        namespace: Option<Namespace>,
        query: &str,
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
        let _ = namespace;
        self.execute_stream(session, query, query_id, sender).await
    }

    /// Returns the schema of a table/collection
    ///
    /// Includes column types, nullability, default values, and primary key info.
    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema>;

    /// Returns a preview of the table data (first N rows)
    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult>;

    /// Fetches rows from a referenced table for a given foreign key value.
    ///
    /// Default implementation returns NotSupported. SQL drivers should override.
    async fn peek_foreign_key(
        &self,
        session: SessionId,
        namespace: &Namespace,
        foreign_key: &ForeignKey,
        value: &Value,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let _ = (session, namespace, foreign_key, value, limit);
        Err(EngineError::not_supported(
            "Foreign key peek is not supported by this driver",
        ))
    }

    /// Cancels a running query for the given session
    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        let _ = (session, query_id);
        Err(crate::engine::error::EngineError::not_supported(
            "Query cancellation is not supported by this driver"
        ))
    }

    /// Reports cancellation support level for this driver.
    fn cancel_support(&self) -> CancelSupport {
        CancelSupport::None
    }

    /// Reports whether the driver supports SSH tunneling.
    fn supports_ssh(&self) -> bool {
        true
    }

    /// Aggregated driver capabilities.
    fn capabilities(&self) -> DriverCapabilities {
        DriverCapabilities {
            transactions: self.supports_transactions(),
            mutations: self.supports_mutations(),
            cancel: self.cancel_support(),
            supports_ssh: self.supports_ssh(),
            schema: self.supports_schema(),
            streaming: self.supports_streaming(),
            explain: self.supports_explain(),
        }
    }

    // ==================== Transaction Methods ====================
    // These have default implementations that return NotSupported.
    // Drivers that support transactions should override these.

    /// Begin a transaction for the session.
    /// 
    /// After calling this, all subsequent queries will be part of the transaction
    /// until commit() or rollback() is called.
    /// 
    /// Note: For connection-pooled drivers (SQLx), this acquires a dedicated connection.
    async fn begin_transaction(&self, session: SessionId) -> EngineResult<()> {
        let _ = session;
        Err(crate::engine::error::EngineError::not_supported(
            "Transactions are not supported by this driver"
        ))
    }

    /// Commit the current transaction.
    /// 
    /// All changes made since begin_transaction() will be persisted.
    async fn commit(&self, session: SessionId) -> EngineResult<()> {
        let _ = session;
        Err(crate::engine::error::EngineError::not_supported(
            "Transactions are not supported by this driver"
        ))
    }

    /// Rollback the current transaction.
    /// 
    /// All changes made since begin_transaction() will be discarded.
    async fn rollback(&self, session: SessionId) -> EngineResult<()> {
        let _ = session;
        Err(crate::engine::error::EngineError::not_supported(
            "Transactions are not supported by this driver"
        ))
    }

    /// Check if the driver supports transactions.
    fn supports_transactions(&self) -> bool {
        false
    }

    /// Check if the driver supports schema inspection (describe, list, etc).
    fn supports_schema(&self) -> bool {
        true
    }

    /// Check if the driver supports streaming results.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Check if the driver supports explain plans.
    fn supports_explain(&self) -> bool {
        false
    }

    // ==================== Mutation Methods ====================
    // These have default implementations that return NotSupported.
    // Drivers should override these to provide CRUD functionality.

    /// Insert a new row into a table.
    ///
    /// # Arguments
    /// * `session` - The session ID
    /// * `namespace` - The namespace (database/schema) containing the table
    /// * `table` - The table name
    /// * `data` - The row data to insert (column name -> value mapping)
    ///
    /// # Returns
    /// QueryResult with affected_rows = 1 on success
    async fn insert_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let _ = (session, namespace, table, data);
        Err(crate::engine::error::EngineError::not_supported(
            "Insert operations are not supported by this driver"
        ))
    }

    /// Update a row identified by primary key.
    ///
    /// # Arguments
    /// * `session` - The session ID
    /// * `namespace` - The namespace (database/schema) containing the table
    /// * `table` - The table name
    /// * `primary_key` - The primary key columns and their values
    /// * `data` - The columns to update (column name -> new value mapping)
    ///
    /// # Returns
    /// QueryResult with affected_rows indicating how many rows were updated
    async fn update_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
        data: &RowData,
    ) -> EngineResult<QueryResult> {
        let _ = (session, namespace, table, primary_key, data);
        Err(crate::engine::error::EngineError::not_supported(
            "Update operations are not supported by this driver"
        ))
    }

    /// Delete a row identified by primary key.
    ///
    /// # Arguments
    /// * `session` - The session ID
    /// * `namespace` - The namespace (database/schema) containing the table
    /// * `table` - The table name
    /// * `primary_key` - The primary key columns and their values
    ///
    /// # Returns
    /// QueryResult with affected_rows indicating how many rows were deleted
    async fn delete_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &RowData,
    ) -> EngineResult<QueryResult> {
        let _ = (session, namespace, table, primary_key);
        Err(crate::engine::error::EngineError::not_supported(
            "Delete operations are not supported by this driver"
        ))
    }

    /// Check if the driver supports CRUD mutations.
    fn supports_mutations(&self) -> bool {
        false
    }
}

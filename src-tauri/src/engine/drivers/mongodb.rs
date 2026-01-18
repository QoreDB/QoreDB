//! MongoDB Driver
//!
//! Implements the DataEngine trait for MongoDB using the official MongoDB driver.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use futures::future::{AbortHandle, Abortable};
use mongodb::bson::{doc, Document};
use mongodb::{Client, options::ClientOptions};
use tokio::sync::{Mutex, RwLock};

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::traits::DataEngine;
use crate::engine::types::{
    CancelSupport, Collection, CollectionType, ColumnInfo, ConnectionConfig, Namespace, QueryId,
    QueryResult, Row as QRow, SessionId, TableColumn, TableSchema, Value,
};

/// MongoDB driver implementation
pub struct MongoDriver {
    sessions: Arc<RwLock<HashMap<SessionId, Client>>>,
    active_queries: Arc<Mutex<HashMap<QueryId, (SessionId, AbortHandle)>>>,
}

impl MongoDriver {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            active_queries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Builds a connection string from config
    fn build_connection_string(config: &ConnectionConfig) -> String {
        let db = config.database.as_deref().unwrap_or("admin");
        let tls = if config.ssl { "true" } else { "false" };

        format!(
            "mongodb://{}:{}@{}:{}/{}?authSource=admin&tls={}",
            config.username, config.password, config.host, config.port, db, tls
        )
    }

    /// Converts a BSON document to a single JSON value for document-centric rendering.
    fn document_to_row(doc: &Document) -> QRow {
        let json = serde_json::to_value(doc).unwrap_or(serde_json::Value::Null);
        QRow {
            values: vec![Value::Json(json)],
        }
    }

    /// Column info for document-centric output.
    fn document_column_info() -> Vec<ColumnInfo> {
        vec![ColumnInfo {
            name: "document".to_string(),
            data_type: "json".to_string(),
            nullable: true,
        }]
    }

    /// Parses a MongoDB query string (JSON format)
    fn parse_query(query: &str) -> EngineResult<(String, String, Document)> {
        // Expected format: db.collection.method({...})
        // or JSON: {"database": "db", "collection": "col", "operation": "find", "query": {...}}

        let trimmed = query.trim();

        // Try JSON format first
        if trimmed.starts_with('{') {
            let parsed: serde_json::Value = serde_json::from_str(trimmed)
                .map_err(|e| EngineError::syntax_error(format!("Invalid JSON: {}", e)))?;

            let database = parsed["database"]
                .as_str()
                .ok_or_else(|| EngineError::syntax_error("Missing 'database' field"))?
                .to_string();

            let collection = parsed["collection"]
                .as_str()
                .ok_or_else(|| EngineError::syntax_error("Missing 'collection' field"))?
                .to_string();

            let filter = if let Some(q) = parsed.get("query") {
                mongodb::bson::to_document(q)
                    .map_err(|e| EngineError::syntax_error(format!("Invalid query: {}", e)))?
            } else {
                doc! {}
            };

            return Ok((database, collection, filter));
        }

        // Fallback: simple format "database.collection"
        let parts: Vec<&str> = trimmed.split('.').collect();
        if parts.len() >= 2 {
            return Ok((
                parts[0].to_string(),
                parts[1].to_string(),
                doc! {},
            ));
        }

        Err(EngineError::syntax_error(
            "Invalid query format. Use JSON: {\"database\": \"db\", \"collection\": \"col\", \"query\": {...}}",
        ))
    }

    // Helper to convert universal Value back to BSON
    fn value_to_bson(value: &Value) -> mongodb::bson::Bson {
        use mongodb::bson::Bson;
        match value {
            Value::Null => Bson::Null,
            Value::Bool(b) => Bson::Boolean(*b),
            Value::Int(i) => Bson::Int64(*i),
            Value::Float(f) => Bson::Double(*f),
            Value::Text(s) => {
                // Try to parse as ObjectId if it looks like one
                if let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(s) {
                    Bson::ObjectId(oid)
                } else {
                    Bson::String(s.clone())
                }
            },
            Value::Bytes(b) => Bson::Binary(mongodb::bson::Binary {
                subtype: mongodb::bson::spec::BinarySubtype::Generic,
                bytes: b.clone(),
            }),
            Value::Json(j) => mongodb::bson::to_bson(j).unwrap_or(Bson::Null),
            Value::Array(arr) => {
                Bson::Array(arr.iter().map(Self::value_to_bson).collect())
            }
        }
    }

    // Helper to convert RowData to Document
    fn row_data_to_document(data: &QRowData) -> Document {
        let mut doc = Document::new();
        for (key, value) in &data.columns {
            // Skip _id if it's null (let MongoDB generate it on insert)
            if key == "_id" {
                if let Value::Null = value {
                    continue;
                }
                // Handle empty string _id as null/skip for inserts 
                if let Value::Text(s) = value {
                    if s.is_empty() {
                        continue;
                    }
                }
            }
            doc.insert(key, Self::value_to_bson(value));
        }
        doc
    }
}

impl Default for MongoDriver {
    fn default() -> Self {
        Self::new()
    }
}

// Need to import QRowData alias if not already present or use crate::engine::types::RowData
use crate::engine::types::RowData as QRowData;

#[async_trait]
impl DataEngine for MongoDriver {
    fn driver_id(&self) -> &'static str {
        "mongodb"
    }

    fn driver_name(&self) -> &'static str {
        "MongoDB"
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()> {
        let conn_str = Self::build_connection_string(config);

        let options = ClientOptions::parse(&conn_str)
            .await
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        let client = Client::with_options(options)
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        // Ping to verify connection
        client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("Authentication failed") {
                    EngineError::auth_failed(msg)
                } else {
                    EngineError::connection_failed(msg)
                }
            })?;

        Ok(())
    }

    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId> {
        let conn_str = Self::build_connection_string(config);

        let options = ClientOptions::parse(&conn_str)
            .await
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        let client = Client::with_options(options)
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        // Verify connection with ping
        client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .map_err(|e| EngineError::connection_failed(e.to_string()))?;

        let session_id = SessionId::new();

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id, client);

        Ok(session_id)
    }

    async fn disconnect(&self, session: SessionId) -> EngineResult<()> {
        let mut sessions = self.sessions.write().await;

        if sessions.remove(&session).is_some() {
            Ok(())
        } else {
            Err(EngineError::session_not_found(session.0.to_string()))
        }
    }

    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>> {
        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let databases = client
            .list_database_names()
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let namespaces = databases
            .into_iter()
            .filter(|db| db != "admin" && db != "config" && db != "local")
            .map(Namespace::new)
            .collect();

        Ok(namespaces)
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
    ) -> EngineResult<Vec<Collection>> {
        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let db = client.database(&namespace.database);
        let collection_names = db
            .list_collection_names()
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let collections = collection_names
            .into_iter()
            .map(|name| Collection {
                namespace: namespace.clone(),
                name,
                collection_type: CollectionType::Collection,
            })
            .collect();

        Ok(collections)
    }

    async fn execute(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
    ) -> EngineResult<QueryResult> {
        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?
            .clone();
        drop(sessions);

        let (abort_handle, abort_reg) = AbortHandle::new_pair();
        {
            let mut active = self.active_queries.lock().await;
            active.insert(query_id, (session, abort_handle));
        }

        let query = query.to_string();
        let result = Abortable::new(
            async move {
                let start = Instant::now();
                let trimmed = query.trim();

                if trimmed.starts_with('{') {
                    let parsed: serde_json::Value = serde_json::from_str(trimmed)
                        .map_err(|e| EngineError::syntax_error(format!("Invalid JSON: {}", e)))?;

                    if let Some(operation) = parsed.get("operation").and_then(|v| v.as_str()) {
                        if operation == "create_collection" {
                            let database = parsed["database"]
                                .as_str()
                                .ok_or_else(|| EngineError::syntax_error("Missing 'database' field"))?;
                            let collection = parsed["collection"]
                                .as_str()
                                .ok_or_else(|| EngineError::syntax_error("Missing 'collection' field"))?;

                            client
                                .database(database)
                                .run_command(doc! { "create": collection })
                                .await
                                .map_err(|e| EngineError::execution_error(e.to_string()))?;

                            let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;
                            return Ok(QueryResult {
                                columns: Vec::new(),
                                rows: Vec::new(),
                                affected_rows: None,
                                execution_time_ms,
                            });
                        }
                    }
                }

                let (database, collection_name, filter) = Self::parse_query(&query)?;

                let collection = client.database(&database).collection::<Document>(&collection_name);

                let mut cursor = collection
                    .find(filter)
                    .await
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;

                let mut documents: Vec<Document> = Vec::new();
                use futures::TryStreamExt;
                while let Some(doc) = cursor
                    .try_next()
                    .await
                    .map_err(|e| EngineError::execution_error(e.to_string()))?
                {
                    documents.push(doc);
                    // Limit for POC
                    if documents.len() >= 1000 {
                        break;
                    }
                }

                let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

                if documents.is_empty() {
                    return Ok(QueryResult {
                        columns: Vec::new(),
                        rows: Vec::new(),
                        affected_rows: None,
                        execution_time_ms,
                    });
                }

                let columns = Self::document_column_info();
                let rows: Vec<QRow> = documents.iter().map(Self::document_to_row).collect();

                Ok(QueryResult {
                    columns,
                    rows,
                    affected_rows: None,
                    execution_time_ms,
                })
            },
            abort_reg,
        )
        .await;

        {
            let mut active = self.active_queries.lock().await;
            active.remove(&query_id);
        }

        match result {
            Ok(inner) => inner,
            Err(_) => Err(EngineError::Cancelled),
        }
    }

    async fn describe_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
    ) -> EngineResult<TableSchema> {
        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let collection = client
            .database(&namespace.database)
            .collection::<Document>(table);

        // Sample documents to infer schema (MongoDB is schemaless)
        use futures::TryStreamExt;
        let cursor = collection
            .find(doc! {})
            .limit(100)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let documents: Vec<Document> = cursor
            .try_collect()
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        // Collect all unique field names and their types
        let mut fields: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for doc in &documents {
            for (key, value) in doc.iter() {
                if !fields.contains_key(key) {
                    let type_name = match value {
                        mongodb::bson::Bson::Null => "null",
                        mongodb::bson::Bson::Boolean(_) => "boolean",
                        mongodb::bson::Bson::Int32(_) => "int32",
                        mongodb::bson::Bson::Int64(_) => "int64",
                        mongodb::bson::Bson::Double(_) => "double",
                        mongodb::bson::Bson::String(_) => "string",
                        mongodb::bson::Bson::ObjectId(_) => "ObjectId",
                        mongodb::bson::Bson::DateTime(_) => "datetime",
                        mongodb::bson::Bson::Array(_) => "array",
                        mongodb::bson::Bson::Document(_) => "document",
                        mongodb::bson::Bson::Binary(_) => "binary",
                        _ => "mixed",
                    };
                    fields.insert(key.clone(), type_name.to_string());
                }
            }
        }

        // Build columns (sorted, with _id first if present)
        let mut columns: Vec<TableColumn> = fields
            .into_iter()
            .map(|(name, data_type)| TableColumn {
                is_primary_key: name == "_id",
                name,
                data_type,
                nullable: true, // MongoDB fields are always nullable
                default_value: None,
            })
            .collect();

        // Sort with _id first
        columns.sort_by(|a, b| {
            if a.name == "_id" {
                std::cmp::Ordering::Less
            } else if b.name == "_id" {
                std::cmp::Ordering::Greater
            } else {
                a.name.cmp(&b.name)
            }
        });

        // Get estimated document count
        let count = collection
            .estimated_document_count()
            .await
            .ok();

        Ok(TableSchema {
            columns,
            primary_key: Some(vec!["_id".to_string()]),
            row_count_estimate: count,
        })
    }

    async fn preview_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        limit: u32,
    ) -> EngineResult<QueryResult> {
        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let start = Instant::now();

        let collection = client
            .database(&namespace.database)
            .collection::<Document>(table);

        use futures::TryStreamExt;
        let cursor = collection
            .find(doc! {})
            .limit(limit as i64)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let documents: Vec<Document> = cursor
            .try_collect()
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

        if documents.is_empty() {
            return Ok(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: None,
                execution_time_ms,
            });
        }

        let columns = Self::document_column_info();
        let rows: Vec<QRow> = documents.iter().map(Self::document_to_row).collect();

        Ok(QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms,
        })
    }

    async fn cancel(&self, session: SessionId, query_id: Option<QueryId>) -> EngineResult<()> {
        let sessions = self.sessions.read().await;
        if !sessions.contains_key(&session) {
            return Err(EngineError::session_not_found(session.0.to_string()));
        }
        drop(sessions);

        let mut active = self.active_queries.lock().await;

        if let Some(qid) = query_id {
            if let Some((sid, handle)) = active.get(&qid) {
                if *sid == session {
                    handle.abort();
                    active.remove(&qid);
                    return Ok(());
                }
            }
            return Err(EngineError::execution_error("Query not found"));
        }

        let to_cancel: Vec<QueryId> = active
            .iter()
            .filter_map(|(qid, (sid, _))| if *sid == session { Some(*qid) } else { None })
            .collect();

        for qid in to_cancel {
            if let Some((_, handle)) = active.remove(&qid) {
                handle.abort();
            }
        }

        Ok(())
    }

    fn cancel_support(&self) -> CancelSupport {
        CancelSupport::BestEffort
    }

    // ==================== Transaction Methods ====================
    // MongoDB transactions require a replica set configuration.
    // Standalone MongoDB instances do not support multi-document transactions.

    async fn begin_transaction(&self, _session: SessionId) -> EngineResult<()> {
        Err(EngineError::not_supported(
            "MongoDB transactions require a replica set. Standalone instances do not support transactions."
        ))
    }

    async fn commit(&self, _session: SessionId) -> EngineResult<()> {
        Err(EngineError::not_supported(
            "MongoDB transactions require a replica set. Standalone instances do not support transactions."
        ))
    }

    async fn rollback(&self, _session: SessionId) -> EngineResult<()> {
        Err(EngineError::not_supported(
            "MongoDB transactions require a replica set. Standalone instances do not support transactions."
        ))
    }

    fn supports_transactions(&self) -> bool {
        // Returns false because we can't know at this point if the server is a replica set
        false
    }
    
    // ==================== Mutation Methods ====================

    async fn insert_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        data: &QRowData,
    ) -> EngineResult<QueryResult> {
        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let start = Instant::now();

        let collection = client
            .database(&namespace.database)
            .collection::<Document>(table);

        let doc = Self::row_data_to_document(data);

        collection
            .insert_one(doc)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

        Ok(QueryResult::with_affected_rows(1, execution_time_ms))
    }

    async fn update_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &QRowData,
        data: &QRowData,
    ) -> EngineResult<QueryResult> {
        if primary_key.columns.is_empty() {
            return Err(EngineError::execution_error(
                "Primary key required for update operations".to_string(),
            ));
        }

        if data.columns.is_empty() {
            return Ok(QueryResult::with_affected_rows(0, 0.0));
        }

        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let start = Instant::now();

        let collection = client
            .database(&namespace.database)
            .collection::<Document>(table);

        // Construct filter from primary key (usually _id)
        let mut filter = Document::new();
        for (key, value) in &primary_key.columns {
            filter.insert(key, Self::value_to_bson(value));
        }

        // Construct update document
        let update_doc = Self::row_data_to_document(data);
        let update = doc! { "$set": update_doc };

        let result = collection
            .update_one(filter, update)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

        Ok(QueryResult::with_affected_rows(result.modified_count, execution_time_ms))
    }

    async fn delete_row(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        primary_key: &QRowData,
    ) -> EngineResult<QueryResult> {
        if primary_key.columns.is_empty() {
            return Err(EngineError::execution_error(
                "Primary key required for delete operations".to_string(),
            ));
        }

        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let start = Instant::now();

        let collection = client
            .database(&namespace.database)
            .collection::<Document>(table);

        // Construct filter from primary key (usually _id)
        let mut filter = Document::new();
        for (key, value) in &primary_key.columns {
            filter.insert(key, Self::value_to_bson(value));
        }

        let result = collection
            .delete_one(filter)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

        Ok(QueryResult::with_affected_rows(result.deleted_count, execution_time_ms))
    }

    fn supports_mutations(&self) -> bool {
        true
    }
}

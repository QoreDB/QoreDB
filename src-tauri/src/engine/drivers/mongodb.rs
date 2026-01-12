//! MongoDB Driver
//!
//! Implements the DataEngine trait for MongoDB using the official MongoDB driver.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use mongodb::bson::{doc, Document};
use mongodb::{Client, options::ClientOptions};
use tokio::sync::RwLock;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::traits::DataEngine;
use crate::engine::types::{
    Collection, CollectionType, ColumnInfo, ConnectionConfig, Namespace, QueryResult,
    Row as QRow, SessionId, Value,
};

/// MongoDB driver implementation
pub struct MongoDriver {
    sessions: Arc<RwLock<HashMap<SessionId, Client>>>,
}

impl MongoDriver {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
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

    /// Converts a BSON document to our universal Row type
    fn document_to_row(doc: &Document) -> QRow {
        let values: Vec<Value> = doc.values().map(Self::bson_to_value).collect();
        QRow { values }
    }

    /// Converts a BSON value to our universal Value type
    fn bson_to_value(bson: &mongodb::bson::Bson) -> Value {
        use mongodb::bson::Bson;

        match bson {
            Bson::Null => Value::Null,
            Bson::Boolean(b) => Value::Bool(*b),
            Bson::Int32(i) => Value::Int(*i as i64),
            Bson::Int64(i) => Value::Int(*i),
            Bson::Double(f) => Value::Float(*f),
            Bson::String(s) => Value::Text(s.clone()),
            Bson::Binary(b) => Value::Bytes(b.bytes.clone()),
            Bson::ObjectId(oid) => Value::Text(oid.to_hex()),
            Bson::DateTime(dt) => Value::Text(dt.to_string()),
            Bson::Array(arr) => {
                Value::Array(arr.iter().map(Self::bson_to_value).collect())
            }
            Bson::Document(doc) => {
                Value::Json(serde_json::to_value(doc).unwrap_or(serde_json::Value::Null))
            }
            _ => Value::Text(bson.to_string()),
        }
    }

    /// Gets column info from a document
    fn get_column_info(doc: &Document) -> Vec<ColumnInfo> {
        doc.keys()
            .map(|key| ColumnInfo {
                name: key.clone(),
                data_type: "mixed".to_string(), // MongoDB is schemaless
                nullable: true,
            })
            .collect()
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
}

impl Default for MongoDriver {
    fn default() -> Self {
        Self::new()
    }
}

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

    async fn execute(&self, session: SessionId, query: &str) -> EngineResult<QueryResult> {
        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let start = Instant::now();

        let (database, collection_name, filter) = Self::parse_query(query)?;

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

        let execution_time_ms = start.elapsed().as_millis() as u64;

        if documents.is_empty() {
            return Ok(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: None,
                execution_time_ms,
            });
        }

        let columns = Self::get_column_info(&documents[0]);
        let rows: Vec<QRow> = documents.iter().map(Self::document_to_row).collect();

        Ok(QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms,
        })
    }

    async fn cancel(&self, session: SessionId) -> EngineResult<()> {
        let sessions = self.sessions.read().await;
        if sessions.contains_key(&session) {
            Ok(())
        } else {
            Err(EngineError::session_not_found(session.0.to_string()))
        }
    }
}

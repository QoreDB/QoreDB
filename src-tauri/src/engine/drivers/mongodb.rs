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
use crate::engine::traits::{StreamEvent, StreamSender};
use crate::engine::types::{
    CancelSupport, Collection, CollectionList, CollectionListOptions, CollectionType, ColumnInfo,
    ConnectionConfig, Namespace, QueryId, QueryResult, Row as QRow, SessionId, TableColumn,
    TableSchema, Value,
    TableQueryOptions, PaginatedQueryResult, SortDirection, FilterOperator,
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

    async fn create_database(&self, session: SessionId, name: &str, options: Option<Value>) -> EngineResult<()> {
        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        // In MongoDB, a database is created when the first collection is created.
        // We require a collection name in the options for explicit creation.
        let collection_name = if let Some(opts) = options {
             match opts {
                 Value::Json(json) => {
                     json.get("collection")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                 },
                 _ => None,
             }
        } else {
            None
        };

        let collection_name = collection_name.ok_or_else(|| 
            EngineError::validation("Collection name is required to create a MongoDB database")
        )?;

        client
            .database(name)
            .run_command(doc! { "create": collection_name })
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        Ok(())
    }

    async fn drop_database(&self, session: SessionId, name: &str) -> EngineResult<()> {
        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        client
            .database(name)
            .drop()
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        tracing::info!("MongoDB: Successfully dropped database '{}'", name);
        Ok(())
    }

    async fn list_collections(
        &self,
        session: SessionId,
        namespace: &Namespace,
        options: CollectionListOptions,
    ) -> EngineResult<CollectionList> {
        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let db = client.database(&namespace.database);
        let collection_names = db
            .list_collection_names()
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        // In-memory filtering and pagination
        let mut filtered: Vec<String> = if let Some(search) = &options.search {
            let search = search.to_lowercase();
            collection_names
                .into_iter()
                .filter(|name| name.to_lowercase().contains(&search))
                .collect()
        } else {
            collection_names
        };

        filtered.sort();
        
        let total_count = filtered.len();

        let paginated = if let Some(limit) = options.page_size {
            let page = options.page.unwrap_or(1).max(1);
            let offset = ((page - 1) * limit) as usize;
            let limit = limit as usize;
            
            if offset >= filtered.len() {
                Vec::new()
            } else {
                filtered
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .collect()
            }
        } else {
            filtered
        };

        let collections = paginated
            .into_iter()
            .map(|name| Collection {
                namespace: namespace.clone(),
                name,
                collection_type: CollectionType::Collection,
            })
            .collect();

        Ok(CollectionList {
            collections,
            total_count: total_count as u32,
        })
    }

    async fn execute_stream(
        &self,
        session: SessionId,
        query: &str,
        query_id: QueryId,
        sender: StreamSender,
    ) -> EngineResult<()> {
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
        let sender_inner = sender.clone();
        let result = Abortable::new(
            async move {
                let sender = sender_inner;
                let trimmed = query.trim();

                // Handle special commands that don't stream (Create Collection, etc)
                if trimmed.starts_with('{') {
                     // Parse partially to check operation
                     if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
                        if let Some(op) = parsed.get("operation").and_then(|v| v.as_str()) {
                            if op == "create_collection" {
                                 // Execute standard execute for non-streaming ops
                                 // We can't reuse self.execute easily here due to ownership/async
                                 // So we just return early and let the caller handle it or 
                                 // better yet, we implement the logic here.
                                 
                                 // Re-use logic from execute
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
                                
                                let _ = sender.send(StreamEvent::Done(0)).await;
                                return Ok(());
                            }
                        }
                     }
                }

                let (database, collection_name, filter) = Self::parse_query(&query)?;

                let collection = client.database(&database).collection::<Document>(&collection_name);
                
                let mut cursor = collection
                    .find(filter)
                    .await
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;

                // Send columns info first
                let columns = Self::document_column_info();
                if sender.send(StreamEvent::Columns(columns)).await.is_err() {
                    return Ok(());
                }

                let mut row_count = 0;
                use futures::TryStreamExt;
                
                while let Some(doc) = cursor
                    .try_next()
                    .await
                    .map_err(|e| EngineError::execution_error(e.to_string()))?
                {
                    let row = Self::document_to_row(&doc);
                    if sender.send(StreamEvent::Row(row)).await.is_err() {
                        break;
                    }
                    row_count += 1;
                    
                    // Limit for POC safety
                    if row_count >= 1000 {
                        break;
                    }
                }

                let _ = sender.send(StreamEvent::Done(row_count)).await;
                Ok(())
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
            Err(_) => {
                let _ = sender.send(StreamEvent::Error("Query cancelled".to_string())).await;
                Err(EngineError::Cancelled)
            },
        }
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
                nullable: true,
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
            foreign_keys: Vec::new(),
            row_count_estimate: count,
            indexes: Vec::new(),
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

    async fn query_table(
        &self,
        session: SessionId,
        namespace: &Namespace,
        table: &str,
        options: TableQueryOptions,
    ) -> EngineResult<PaginatedQueryResult> {
        let sessions = self.sessions.read().await;
        let client = sessions
            .get(&session)
            .ok_or_else(|| EngineError::session_not_found(session.0.to_string()))?;

        let start = Instant::now();

        let collection = client
            .database(&namespace.database)
            .collection::<Document>(table);

        let page = options.effective_page();
        let page_size = options.effective_page_size();
        let offset = options.offset();

        // Build $match filter document
        let mut filter_doc = Document::new();

        if let Some(filters) = &options.filters {
            for filter in filters {
                let bson_value = Self::value_to_bson(&filter.value);

                let condition = match filter.operator {
                    FilterOperator::Eq => bson_value,
                    FilterOperator::Neq => mongodb::bson::Bson::Document(doc! { "$ne": bson_value }),
                    FilterOperator::Gt => mongodb::bson::Bson::Document(doc! { "$gt": bson_value }),
                    FilterOperator::Gte => mongodb::bson::Bson::Document(doc! { "$gte": bson_value }),
                    FilterOperator::Lt => mongodb::bson::Bson::Document(doc! { "$lt": bson_value }),
                    FilterOperator::Lte => mongodb::bson::Bson::Document(doc! { "$lte": bson_value }),
                    FilterOperator::Like => {
                        // Convert LIKE pattern to regex
                        if let mongodb::bson::Bson::String(s) = &bson_value {
                            let pattern = s.replace('%', ".*").replace('_', ".");
                            mongodb::bson::Bson::Document(doc! { "$regex": pattern, "$options": "i" })
                        } else {
                            bson_value
                        }
                    }
                    FilterOperator::IsNull => mongodb::bson::Bson::Document(doc! { "$eq": mongodb::bson::Bson::Null }),
                    FilterOperator::IsNotNull => mongodb::bson::Bson::Document(doc! { "$ne": mongodb::bson::Bson::Null }),
                };

                filter_doc.insert(&filter.column, condition);
            }
        }

        // Handle search across string fields
        if let Some(ref search_term) = options.search {
            if !search_term.trim().is_empty() {
                // Sample one document to discover string fields
                let sample_doc = collection.find_one(doc! {}).await
                    .map_err(|e| EngineError::execution_error(e.to_string()))?;

                let mut search_conditions: Vec<Document> = Vec::new();
                
                if let Some(doc) = sample_doc {
                    for (key, value) in doc.iter() {
                        // Only search string fields
                        if matches!(value, mongodb::bson::Bson::String(_)) {
                            search_conditions.push(doc! {
                                key: { "$regex": search_term.as_str(), "$options": "i" }
                            });
                        }
                    }
                }

                if !search_conditions.is_empty() {
                    filter_doc.insert("$or", search_conditions);
                }
            }
        }

        // Get total count with filters
        let total_rows = collection
            .count_documents(filter_doc.clone())
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        // Build find options with sort, skip, limit
        use mongodb::options::FindOptions;
        let mut find_options = FindOptions::builder()
            .skip(Some(offset))
            .limit(Some(page_size as i64))
            .build();

        if let Some(sort_col) = &options.sort_column {
            let sort_direction = match options.sort_direction.unwrap_or_default() {
                SortDirection::Asc => 1,
                SortDirection::Desc => -1,
            };
            find_options.sort = Some(doc! { sort_col: sort_direction });
        }

        use futures::TryStreamExt;
        let cursor = collection
            .find(filter_doc)
            .with_options(find_options)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let documents: Vec<Document> = cursor
            .try_collect()
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;

        let execution_time_ms = start.elapsed().as_micros() as f64 / 1000.0;

        let result = if documents.is_empty() {
            QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: None,
                execution_time_ms,
            }
        } else {
            let columns = Self::document_column_info();
            let rows: Vec<QRow> = documents.iter().map(Self::document_to_row).collect();
            QueryResult {
                columns,
                rows,
                affected_rows: None,
                execution_time_ms,
            }
        };

        Ok(PaginatedQueryResult::new(result, total_rows, page, page_size))
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
        //TODO : Implement a check to see if the connected MongoDB instance is a replica set
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

    fn supports_streaming(&self) -> bool {
        true
    }
}

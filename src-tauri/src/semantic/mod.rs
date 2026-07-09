// SPDX-License-Identifier: Apache-2.0

//! Local semantic schema search: Ollama embeddings persisted in DuckDB.

pub mod indexer;
pub mod ollama;
pub mod store;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::engine::types::{Namespace, SessionId};
use crate::engine::SessionManager;
use ollama::OllamaEmbedder;
use store::{IndexedObject, SemanticStore};

pub const DEFAULT_MODEL: &str = "nomic-embed-text";
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";

const REFRESH_DEBOUNCE: Duration = Duration::from_secs(3);
const RANK_SEARCH_LIMIT: u32 = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

impl Default for SemanticConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: None,
            model: default_model(),
        }
    }
}

impl SemanticConfig {
    pub fn effective_base_url(&self) -> &str {
        self.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexSummary {
    pub total: usize,
    pub embedded: usize,
    pub deleted: usize,
    pub duration_ms: u64,
}

pub struct SemanticService {
    data_dir: PathBuf,
    config_path: PathBuf,
    config: RwLock<SemanticConfig>,
    stores: Mutex<HashMap<String, Arc<SemanticStore>>>,
    refresh_gen: Mutex<HashMap<String, u64>>,
    building: Mutex<HashSet<String>>,
}

struct BuildingGuard<'a> {
    service: &'a SemanticService,
    key: String,
}

impl Drop for BuildingGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut building) = self.service.building.lock() {
            building.remove(&self.key);
        }
    }
}

impl SemanticService {
    pub fn new() -> Self {
        Self::with_dir(crate::paths::app_data_dir().join("semantic"))
    }

    fn with_dir(data_dir: PathBuf) -> Self {
        let config_path = data_dir.join("config.json");
        let config = std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();
        Self {
            data_dir,
            config_path,
            config: RwLock::new(config),
            stores: Mutex::new(HashMap::new()),
            refresh_gen: Mutex::new(HashMap::new()),
            building: Mutex::new(HashSet::new()),
        }
    }

    pub fn config(&self) -> SemanticConfig {
        self.config.read().clone()
    }

    pub fn set_config(&self, config: SemanticConfig) {
        match serde_json::to_string_pretty(&config) {
            Ok(json) => {
                if let Err(e) = std::fs::create_dir_all(&self.data_dir) {
                    warn!("Failed to create semantic directory: {e}");
                }
                if let Err(e) =
                    crate::atomic_write::write_atomic(&self.config_path, json.as_bytes())
                {
                    warn!("Failed to write semantic config: {e}");
                }
            }
            Err(e) => warn!("Failed to serialize semantic config: {e}"),
        }
        *self.config.write() = config;
    }

    pub fn store_for(&self, project_id: &str) -> Result<Arc<SemanticStore>, String> {
        let mut stores = self
            .stores
            .lock()
            .map_err(|e| format!("Semantic store cache poisoned: {e}"))?;
        if let Some(store) = stores.get(project_id) {
            return Ok(Arc::clone(store));
        }
        let safe_id: String = project_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let store = Arc::new(SemanticStore::open(
            &self.data_dir.join(format!("{safe_id}.duckdb")),
        )?);
        stores.insert(project_id.to_string(), Arc::clone(&store));
        Ok(store)
    }

    pub fn is_building(&self, connection_key: &str) -> bool {
        self.building
            .lock()
            .map(|b| b.contains(connection_key))
            .unwrap_or(false)
    }

    pub async fn refresh(
        &self,
        session_manager: &Arc<SessionManager>,
        session: SessionId,
        connection_key: &str,
        project_id: &str,
    ) -> Result<IndexSummary, String> {
        let config = self.config();
        if !config.enabled {
            return Err("Semantic search is disabled".to_string());
        }
        {
            let mut building = self
                .building
                .lock()
                .map_err(|e| format!("Semantic state poisoned: {e}"))?;
            if !building.insert(connection_key.to_string()) {
                return Err("Semantic index refresh already in progress".to_string());
            }
        }
        let _guard = BuildingGuard {
            service: self,
            key: connection_key.to_string(),
        };

        let start = Instant::now();
        let store = self.store_for(project_id)?;
        let corpus = indexer::build_corpus(session_manager, session).await?;
        let existing = store.fingerprints(connection_key).await?;

        let mut corpus_ids = HashSet::with_capacity(corpus.len());
        let mut to_embed = Vec::new();
        for doc in corpus {
            let fp = indexer::fingerprint(&config.model, &doc.document);
            corpus_ids.insert(doc.object_id.clone());
            if existing.get(&doc.object_id) != Some(&fp) {
                to_embed.push((doc, fp));
            }
        }
        let deleted_ids: Vec<String> = existing
            .keys()
            .filter(|id| !corpus_ids.contains(*id))
            .cloned()
            .collect();

        let embedded = to_embed.len();
        let deleted = deleted_ids.len();
        let total = corpus_ids.len();

        let upserts = if to_embed.is_empty() {
            Vec::new()
        } else {
            let embedder = OllamaEmbedder::new(&config);
            let documents: Vec<String> = to_embed.iter().map(|(d, _)| d.document.clone()).collect();
            let embeddings = embedder.embed_documents(&documents).await?;
            let dim = embeddings.first().map(Vec::len).unwrap_or(0);
            if dim == 0 || embeddings.iter().any(|e| e.len() != dim) {
                return Err("Ollama returned inconsistent embedding dimensions".to_string());
            }
            to_embed
                .into_iter()
                .zip(embeddings)
                .map(|((doc, fingerprint), embedding)| IndexedObject {
                    object_id: doc.object_id,
                    kind: doc.kind,
                    database: doc.database,
                    schema: doc.schema,
                    table: doc.table,
                    column: doc.column,
                    document: doc.document,
                    fingerprint,
                    sensitive: doc.sensitive,
                    embedding,
                })
                .collect()
        };

        store
            .apply(connection_key, &config.model, upserts, deleted_ids)
            .await?;

        Ok(IndexSummary {
            total,
            embedded,
            deleted,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    pub async fn search(
        &self,
        connection_key: &str,
        project_id: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<store::SemanticHit>, String> {
        let config = self.config();
        let store = self.store_for(project_id)?;
        let embedder = OllamaEmbedder::new(&config);
        let query_vec = embedder.embed_query(query).await?;
        store
            .search(connection_key, &config.model, &query_vec, limit)
            .await
    }

    /// Tables of `namespace` ranked by semantic relevance to `prompt`, for
    /// AI schema-context prioritization. Fail-open: any unavailability
    /// (disabled, empty index, Ollama down) yields an empty list so the
    /// caller falls back to its non-semantic ordering.
    pub async fn rank_tables_for_prompt(
        &self,
        connection_key: &str,
        project_id: &str,
        namespace: &Namespace,
        prompt: &str,
    ) -> Vec<String> {
        match self
            .try_rank_tables(connection_key, project_id, namespace, prompt)
            .await
        {
            Ok(tables) => tables,
            Err(e) => {
                debug!("Semantic table ranking skipped: {e}");
                Vec::new()
            }
        }
    }

    async fn try_rank_tables(
        &self,
        connection_key: &str,
        project_id: &str,
        namespace: &Namespace,
        prompt: &str,
    ) -> Result<Vec<String>, String> {
        let config = self.config();
        if !config.enabled || prompt.trim().is_empty() || self.is_building(connection_key) {
            return Ok(Vec::new());
        }
        let store = self.store_for(project_id)?;
        if store.count(connection_key, &config.model).await? == 0 {
            return Ok(Vec::new());
        }
        let hits = self
            .search(connection_key, project_id, prompt, RANK_SEARCH_LIMIT)
            .await?;
        let mut tables = Vec::new();
        for hit in hits {
            if hit.database != namespace.database
                || hit.schema.as_deref() != namespace.schema.as_deref()
            {
                continue;
            }
            if !tables.contains(&hit.table) {
                tables.push(hit.table);
            }
        }
        Ok(tables)
    }

    pub fn schedule_refresh(
        self: &Arc<Self>,
        session_manager: Arc<SessionManager>,
        session: SessionId,
        connection_key: String,
        project_id: String,
    ) {
        if !self.config.read().enabled {
            return;
        }
        let generation = {
            let Ok(mut gens) = self.refresh_gen.lock() else {
                return;
            };
            let counter = gens.entry(connection_key.clone()).or_insert(0);
            *counter += 1;
            *counter
        };
        let service = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(REFRESH_DEBOUNCE).await;
            let current = service
                .refresh_gen
                .lock()
                .ok()
                .and_then(|gens| gens.get(&connection_key).copied());
            if current != Some(generation) {
                return;
            }
            let indexed = match service.store_for(&project_id) {
                Ok(store) => !store
                    .fingerprints(&connection_key)
                    .await
                    .unwrap_or_default()
                    .is_empty(),
                Err(_) => false,
            };
            if !indexed {
                return;
            }
            match service
                .refresh(&session_manager, session, &connection_key, &project_id)
                .await
            {
                Ok(summary) => debug!(
                    "Semantic index refreshed after DDL: {} embedded, {} deleted ({} ms)",
                    summary.embedded, summary.deleted, summary.duration_ms
                ),
                Err(e) => debug!("Semantic DDL refresh skipped: {e}"),
            }
        });
    }
}

impl Default for SemanticService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod e2e_tests {
    use super::*;
    use crate::engine::drivers::postgres::PostgresDriver;
    use crate::engine::types::{ConnectionConfig, QueryId};
    use crate::engine::DriverRegistry;

    fn pg_config() -> ConnectionConfig {
        let env = |key: &str, default: &str| std::env::var(key).unwrap_or_else(|_| default.into());
        ConnectionConfig {
            driver: "postgres".to_string(),
            host: env("QOREDB_TEST_PG_HOST", "127.0.0.1"),
            port: env("QOREDB_TEST_PG_PORT", "54321").parse().unwrap_or(54321),
            username: env("QOREDB_TEST_PG_USER", "qoredb"),
            password: env("QOREDB_TEST_PG_PASSWORD", "qoredb_test"),
            database: Some(env("QOREDB_TEST_PG_DB", "testdb")),
            ssl: false,
            ssl_mode: None,
            environment: "development".to_string(),
            read_only: false,
            ssh_tunnel: None,
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
        }
    }

    async fn exec(sm: &Arc<SessionManager>, session: SessionId, sql: &str) {
        let driver = sm.get_driver(session).await.expect("driver");
        driver
            .execute(session, sql, QueryId::new())
            .await
            .map_err(|e| e.sanitized_message())
            .expect(sql);
    }

    #[tokio::test]
    #[ignore = "requires docker postgres (127.0.0.1:54321) and a local Ollama with nomic-embed-text pulled"]
    async fn full_pipeline_indexes_searches_refreshes_and_persists() {
        let mut registry = DriverRegistry::new();
        registry.register(Arc::new(PostgresDriver::new()));
        let sm = Arc::new(SessionManager::new(Arc::new(registry)));
        let session = sm.connect(pg_config()).await.expect("postgres connect");
        let key = sm.connection_key(session).await.expect("connection key");

        exec(&sm, session, "DROP TABLE IF EXISTS sem_orders").await;
        exec(&sm, session, "DROP TABLE IF EXISTS sem_customers").await;
        exec(
            &sm,
            session,
            "CREATE TABLE sem_customers (id SERIAL PRIMARY KEY, email VARCHAR(255) NOT NULL, full_name VARCHAR(255))",
        )
        .await;
        exec(
            &sm,
            session,
            "CREATE TABLE sem_orders (id SERIAL PRIMARY KEY, customer_id INTEGER NOT NULL REFERENCES sem_customers(id), total NUMERIC(10,2))",
        )
        .await;

        let dir = tempfile::tempdir().expect("tempdir");
        let service = SemanticService::with_dir(dir.path().join("semantic"));
        service.set_config(SemanticConfig {
            enabled: true,
            base_url: None,
            model: DEFAULT_MODEL.to_string(),
        });

        let summary = service
            .refresh(&sm, session, &key, "e2e")
            .await
            .expect("initial refresh");
        assert!(summary.total > 0);
        assert_eq!(summary.embedded, summary.total);

        let hits = service
            .search(&key, "e2e", "où est stocké l'email client ?", 5)
            .await
            .expect("semantic search");
        assert!(
            hits.iter()
                .take(3)
                .any(|h| h.column.as_deref() == Some("email")),
            "expected an email column in top hits: {hits:?}"
        );

        let ns = Namespace {
            database: pg_config().database.expect("test db"),
            schema: Some("public".to_string()),
        };
        let ranked = service
            .rank_tables_for_prompt(&key, "e2e", &ns, "où est stocké l'email client ?")
            .await;
        assert!(
            ranked
                .iter()
                .take(3)
                .any(|t| t == "sem_customers"),
            "expected sem_customers in top ranked tables: {ranked:?}"
        );
        let wrong_ns = Namespace::new("no_such_database");
        assert!(service
            .rank_tables_for_prompt(&key, "e2e", &wrong_ns, "email")
            .await
            .is_empty());

        exec(
            &sm,
            session,
            "ALTER TABLE sem_customers ADD COLUMN loyalty_tier TEXT",
        )
        .await;
        let summary = service
            .refresh(&sm, session, &key, "e2e")
            .await
            .expect("refresh after DDL");
        assert!(
            summary.embedded >= 1 && summary.embedded < summary.total,
            "expected an incremental refresh: {summary:?}"
        );
        let hits = service
            .search(&key, "e2e", "customer loyalty level", 5)
            .await
            .expect("search after DDL");
        assert!(
            hits.iter()
                .take(3)
                .any(|h| h.column.as_deref() == Some("loyalty_tier")),
            "expected loyalty_tier in top hits: {hits:?}"
        );

        exec(&sm, session, "DROP TABLE sem_orders").await;
        exec(&sm, session, "DROP TABLE sem_customers").await;

        drop(service);
        let reopened = SemanticService::with_dir(dir.path().join("semantic"));
        assert!(reopened.config().enabled);
        let hits = reopened
            .search(&key, "e2e", "where is the customer email stored?", 5)
            .await
            .expect("search after reopen");
        assert!(
            hits.iter()
                .take(3)
                .any(|h| h.column.as_deref() == Some("email")),
            "expected the persisted index to answer after reopen: {hits:?}"
        );
    }
}

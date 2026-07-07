// SPDX-License-Identifier: BUSL-1.1

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

use crate::engine::types::SessionId;
use crate::engine::SessionManager;
use ollama::OllamaEmbedder;
use store::{IndexedObject, SemanticStore};

pub const DEFAULT_MODEL: &str = "nomic-embed-text";
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";

const REFRESH_DEBOUNCE: Duration = Duration::from_secs(3);

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
        let data_dir = crate::paths::app_data_dir().join("semantic");
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
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
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

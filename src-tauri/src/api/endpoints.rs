// SPDX-License-Identifier: BUSL-1.1

//! JSON-backed registry of Instant Data API endpoints.
//!
//! On-disk shape: `<data_dir>/instant_api/endpoints.json`, written atomically
//! (tmp + rename). Token hashes live inside this file; raw tokens are never
//! persisted.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::types::{Endpoint, EndpointMeta, EndpointParam, QueryShape};

const STORE_FILE: &str = "endpoints.json";
const NAME_MAX_LEN: usize = 64;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("endpoint name must match [A-Za-z0-9_-]{{1,64}} (got {0:?})")]
    InvalidName(String),
    #[error("endpoint name {0:?} already exists")]
    DuplicateName(String),
    #[error("endpoint not found: {0}")]
    NotFound(String),
    #[error("serialization error: {0}")]
    Serialize(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoreFile {
    #[serde(default)]
    endpoints: Vec<Endpoint>,
}

pub struct EndpointStore {
    path: PathBuf,
    state: RwLock<HashMap<String, Endpoint>>,
}

impl EndpointStore {
    pub fn new(data_dir: PathBuf) -> Result<Self, StoreError> {
        let dir = data_dir.join("instant_api");
        fs::create_dir_all(&dir).map_err(|e| StoreError::Io {
            path: dir.clone(),
            source: e,
        })?;
        let path = dir.join(STORE_FILE);
        let state = if path.exists() {
            let bytes = fs::read(&path).map_err(|e| StoreError::Io {
                path: path.clone(),
                source: e,
            })?;
            let parsed: StoreFile = serde_json::from_slice(&bytes)
                .map_err(|e| StoreError::Serialize(e.to_string()))?;
            parsed
                .endpoints
                .into_iter()
                .map(|e| (e.id.clone(), e))
                .collect()
        } else {
            HashMap::new()
        };
        Ok(Self {
            path,
            state: RwLock::new(state),
        })
    }

    pub fn list(&self) -> Vec<EndpointMeta> {
        let state = self.state.read().unwrap();
        let mut out: Vec<EndpointMeta> = state.values().map(EndpointMeta::from).collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn get_by_name(&self, name: &str) -> Option<Endpoint> {
        let state = self.state.read().unwrap();
        state.values().find(|e| e.name == name).cloned()
    }

    pub fn count(&self) -> u32 {
        self.state.read().unwrap().len() as u32
    }

    /// Inserts a new endpoint. `token_hash` is the Argon2 hash of the
    /// already-issued raw token (caller handles the one-shot display).
    pub fn create(
        &self,
        name: String,
        connection_id: String,
        query_source: String,
        params: Vec<EndpointParam>,
        shape: QueryShape,
        page_size: u32,
        token_hash: String,
    ) -> Result<Endpoint, StoreError> {
        validate_name(&name)?;
        let mut state = self.state.write().unwrap();
        if state.values().any(|e| e.name == name) {
            return Err(StoreError::DuplicateName(name));
        }
        let now = Utc::now().to_rfc3339();
        let endpoint = Endpoint {
            id: Uuid::new_v4().to_string(),
            name,
            connection_id,
            query_source,
            params,
            shape,
            token_hash,
            page_size,
            created_at: now.clone(),
            updated_at: now,
        };
        state.insert(endpoint.id.clone(), endpoint.clone());
        self.flush(&state)?;
        Ok(endpoint)
    }

    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        let mut state = self.state.write().unwrap();
        if state.remove(id).is_none() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.flush(&state)
    }

    fn flush(&self, state: &HashMap<String, Endpoint>) -> Result<(), StoreError> {
        let mut endpoints: Vec<Endpoint> = state.values().cloned().collect();
        endpoints.sort_by(|a, b| a.name.cmp(&b.name));
        let bytes = serde_json::to_vec_pretty(&StoreFile { endpoints })
            .map_err(|e| StoreError::Serialize(e.to_string()))?;
        let tmp = self.path.with_extension("json.tmp");
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|e| StoreError::Io {
                path: tmp.clone(),
                source: e,
            })?;
        f.write_all(&bytes).map_err(|e| StoreError::Io {
            path: tmp.clone(),
            source: e,
        })?;
        f.sync_all().map_err(|e| StoreError::Io {
            path: tmp.clone(),
            source: e,
        })?;
        drop(f);
        fs::rename(&tmp, &self.path).map_err(|e| StoreError::Io {
            path: self.path.clone(),
            source: e,
        })
    }
}

fn validate_name(name: &str) -> Result<(), StoreError> {
    if name.is_empty() || name.len() > NAME_MAX_LEN {
        return Err(StoreError::InvalidName(name.to_string()));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(StoreError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// Loads the [`EndpointStore`] anchored at the standard app data dir (used
/// when constructing from the Tauri state).
pub fn open_default(data_dir: &Path) -> Result<EndpointStore, StoreError> {
    EndpointStore::new(data_dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_store(tmp: &TempDir) -> EndpointStore {
        EndpointStore::new(tmp.path().to_path_buf()).unwrap()
    }

    #[test]
    fn validate_name_accepts_alnum_underscore_dash() {
        assert!(validate_name("orders_top").is_ok());
        assert!(validate_name("orders-2024").is_ok());
        assert!(validate_name("ABC123").is_ok());
    }

    #[test]
    fn validate_name_rejects_bad_chars() {
        assert!(validate_name("").is_err());
        assert!(validate_name("with space").is_err());
        assert!(validate_name("with/slash").is_err());
        assert!(validate_name("../traversal").is_err());
        assert!(validate_name(&"x".repeat(65)).is_err());
    }

    #[test]
    fn create_and_list_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = make_store(&tmp);
        let ep = store
            .create(
                "orders_top".into(),
                "conn-1".into(),
                "SELECT * FROM orders LIMIT {{limit}}".into(),
                Vec::new(),
                QueryShape::Rows,
                100,
                "hash".into(),
            )
            .unwrap();
        assert_eq!(ep.name, "orders_top");
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "orders_top");
    }

    #[test]
    fn rejects_duplicate_name() {
        let tmp = TempDir::new().unwrap();
        let store = make_store(&tmp);
        store
            .create(
                "a".into(),
                "c".into(),
                "SELECT 1".into(),
                vec![],
                QueryShape::Rows,
                100,
                "h".into(),
            )
            .unwrap();
        let err = store
            .create(
                "a".into(),
                "c".into(),
                "SELECT 1".into(),
                vec![],
                QueryShape::Rows,
                100,
                "h".into(),
            )
            .unwrap_err();
        assert!(matches!(err, StoreError::DuplicateName(_)));
    }

    #[test]
    fn delete_removes_endpoint() {
        let tmp = TempDir::new().unwrap();
        let store = make_store(&tmp);
        let ep = store
            .create(
                "a".into(),
                "c".into(),
                "SELECT 1".into(),
                vec![],
                QueryShape::Rows,
                100,
                "h".into(),
            )
            .unwrap();
        store.delete(&ep.id).unwrap();
        assert!(store.list().is_empty());
    }

    #[test]
    fn persists_across_reopens() {
        let tmp = TempDir::new().unwrap();
        {
            let store = make_store(&tmp);
            store
                .create(
                    "a".into(),
                    "c".into(),
                    "SELECT 1".into(),
                    vec![],
                    QueryShape::Rows,
                    100,
                    "h".into(),
                )
                .unwrap();
        }
        let store = make_store(&tmp);
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "a");
    }
}

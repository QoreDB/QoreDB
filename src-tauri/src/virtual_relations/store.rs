// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use crate::engine::types::ForeignKey;

use super::types::{VirtualRelation, VirtualRelationsConfig};

/// In-memory store for virtual relations, with JSON persistence per connection
pub struct VirtualRelationStore {
    data_dir: PathBuf,
    cache: RwLock<HashMap<String, VirtualRelationsConfig>>,
}

impl VirtualRelationStore {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);
        Self {
            data_dir,
            cache: RwLock::new(HashMap::new()),
        }
    }

    fn file_path(&self, connection_id: &str) -> PathBuf {
        self.data_dir.join(format!("{}.json", connection_id))
    }

    fn ensure_loaded(&self, connection_id: &str) -> VirtualRelationsConfig {
        {
            let cache = self.cache.read().unwrap();
            if let Some(config) = cache.get(connection_id) {
                return config.clone();
            }
        }
        let path = self.file_path(connection_id);
        let config = if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            VirtualRelationsConfig::default()
        };
        let mut cache = self.cache.write().unwrap();
        cache.insert(connection_id.to_string(), config.clone());
        config
    }

    fn save(&self, connection_id: &str, config: &VirtualRelationsConfig) -> Result<(), String> {
        let path = self.file_path(connection_id);
        let content = serde_json::to_string_pretty(config)
            .map_err(|e| format!("Failed to serialize virtual relations: {}", e))?;
        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write virtual relations: {}", e))?;
        let mut cache = self.cache.write().unwrap();
        cache.insert(connection_id.to_string(), config.clone());
        Ok(())
    }

    /// Get all virtual relations for a connection
    pub fn list(&self, connection_id: &str) -> Vec<VirtualRelation> {
        self.ensure_loaded(connection_id).relations
    }

    /// Get virtual FKs for a specific table, returned as ForeignKey structs with is_virtual=true
    pub fn get_foreign_keys_for_table(
        &self,
        connection_id: &str,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Vec<ForeignKey> {
        self.ensure_loaded(connection_id)
            .relations
            .iter()
            .filter(|r| {
                r.source_database == database
                    && r.source_schema.as_deref() == schema
                    && r.source_table == table
            })
            .map(|r| ForeignKey {
                column: r.source_column.clone(),
                referenced_table: r.referenced_table.clone(),
                referenced_column: r.referenced_column.clone(),
                referenced_schema: r.referenced_schema.clone(),
                referenced_database: r.referenced_database.clone(),
                constraint_name: r
                    .label
                    .clone()
                    .or_else(|| Some(format!("virtual_{}", r.id))),
                is_virtual: true,
            })
            .collect()
    }

    /// Add a new virtual relation
    pub fn add(&self, connection_id: &str, relation: VirtualRelation) -> Result<(), String> {
        let mut config = self.ensure_loaded(connection_id);
        config.relations.push(relation);
        self.save(connection_id, &config)
    }

    /// Update an existing virtual relation
    pub fn update(&self, connection_id: &str, relation: VirtualRelation) -> Result<(), String> {
        let mut config = self.ensure_loaded(connection_id);
        if let Some(pos) = config.relations.iter().position(|r| r.id == relation.id) {
            config.relations[pos] = relation;
            self.save(connection_id, &config)
        } else {
            Err("Virtual relation not found".to_string())
        }
    }

    /// Delete a virtual relation by ID
    pub fn delete(&self, connection_id: &str, relation_id: &str) -> Result<(), String> {
        let mut config = self.ensure_loaded(connection_id);
        let original_len = config.relations.len();
        config.relations.retain(|r| r.id != relation_id);
        if config.relations.len() == original_len {
            return Err("Virtual relation not found".to_string());
        }
        self.save(connection_id, &config)
    }
}

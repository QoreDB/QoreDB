// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::engine::types::QueryResult;

use super::types::{Snapshot, SnapshotMeta};

/// File-based snapshot store, persisting each snapshot as a separate JSON file
pub struct SnapshotStore {
    data_dir: PathBuf,
}

impl SnapshotStore {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);
        Self { data_dir }
    }

    /// Validate that a snapshot ID is a legitimate UUID (prevents path traversal).
    fn validate_snapshot_id(snapshot_id: &str) -> Result<(), String> {
        uuid::Uuid::parse_str(snapshot_id)
            .map_err(|_| "Invalid snapshot ID".to_string())?;
        Ok(())
    }

    fn file_path(&self, snapshot_id: &str) -> Result<PathBuf, String> {
        Self::validate_snapshot_id(snapshot_id)?;
        let path = self.data_dir.join(format!("{}.json", snapshot_id));
        // Belt-and-suspenders: verify resolved path stays within data_dir
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        let canonical_dir = self
            .data_dir
            .canonicalize()
            .unwrap_or_else(|_| self.data_dir.clone());
        if !canonical.starts_with(&canonical_dir) {
            return Err("Invalid snapshot path".to_string());
        }
        Ok(path)
    }

    /// Save a new snapshot from a query result
    pub fn save(
        &self,
        name: String,
        description: Option<String>,
        source: String,
        source_type: String,
        connection_name: Option<String>,
        driver: Option<String>,
        namespace: Option<crate::engine::types::Namespace>,
        result: &QueryResult,
    ) -> Result<SnapshotMeta, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();

        let meta = SnapshotMeta {
            id: id.clone(),
            name,
            description,
            source,
            source_type,
            connection_name,
            driver,
            namespace,
            columns: result.columns.clone(),
            row_count: result.rows.len(),
            created_at,
            file_size: 0,
        };

        let snapshot = Snapshot {
            meta: meta.clone(),
            rows: result.rows.clone(),
        };

        let content = serde_json::to_string(&snapshot)
            .map_err(|e| format!("Failed to serialize snapshot: {}", e))?;

        let path = self.file_path(&id)?;
        std::fs::write(&path, &content).map_err(|e| format!("Failed to write snapshot: {}", e))?;

        let mut meta = meta;
        meta.file_size = content.len() as u64;
        Ok(meta)
    }

    /// List all snapshots (metadata only, sorted by creation date desc)
    pub fn list(&self) -> Result<Vec<SnapshotMeta>, String> {
        let mut metas = Vec::new();

        let entries = std::fs::read_dir(&self.data_dir)
            .map_err(|e| format!("Failed to read snapshots directory: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Parse only the meta field to avoid loading all row data
            let snapshot: Result<Snapshot, _> = serde_json::from_str(&content);
            if let Ok(snapshot) = snapshot {
                let mut meta = snapshot.meta;
                meta.file_size = content.len() as u64;
                metas.push(meta);
            }
        }

        // Sort by created_at descending (most recent first)
        metas.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(metas)
    }

    /// Get a full snapshot by ID (including row data)
    pub fn get(&self, snapshot_id: &str) -> Result<Snapshot, String> {
        let path = self.file_path(snapshot_id)?;
        if !path.exists() {
            return Err("Snapshot not found".to_string());
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read snapshot: {}", e))?;

        let mut snapshot: Snapshot = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse snapshot: {}", e))?;

        snapshot.meta.file_size = content.len() as u64;
        Ok(snapshot)
    }

    /// Delete a snapshot by ID
    pub fn delete(&self, snapshot_id: &str) -> Result<(), String> {
        let path = self.file_path(snapshot_id)?;
        if !path.exists() {
            return Err("Snapshot not found".to_string());
        }
        std::fs::remove_file(&path).map_err(|e| format!("Failed to delete snapshot: {}", e))
    }

    /// Rename a snapshot
    pub fn rename(&self, snapshot_id: &str, new_name: String) -> Result<SnapshotMeta, String> {
        let path = self.file_path(snapshot_id)?;
        if !path.exists() {
            return Err("Snapshot not found".to_string());
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read snapshot: {}", e))?;

        let mut snapshot: Snapshot = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse snapshot: {}", e))?;

        snapshot.meta.name = new_name;

        let updated = serde_json::to_string(&snapshot)
            .map_err(|e| format!("Failed to serialize snapshot: {}", e))?;

        std::fs::write(&path, &updated).map_err(|e| format!("Failed to write snapshot: {}", e))?;

        snapshot.meta.file_size = updated.len() as u64;
        Ok(snapshot.meta)
    }

    /// Update description of a snapshot
    pub fn update_description(
        &self,
        snapshot_id: &str,
        description: Option<String>,
    ) -> Result<SnapshotMeta, String> {
        let path = self.file_path(snapshot_id)?;
        if !path.exists() {
            return Err("Snapshot not found".to_string());
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read snapshot: {}", e))?;

        let mut snapshot: Snapshot = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse snapshot: {}", e))?;

        snapshot.meta.description = description;

        let updated = serde_json::to_string(&snapshot)
            .map_err(|e| format!("Failed to serialize snapshot: {}", e))?;

        std::fs::write(&path, &updated).map_err(|e| format!("Failed to write snapshot: {}", e))?;

        snapshot.meta.file_size = updated.len() as u64;
        Ok(snapshot.meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_snapshot_id() {
        let store = SnapshotStore::new(PathBuf::from("/tmp/qoredb_test_snapshots"));
        assert!(store.get("../../../etc/passwd").is_err());
        assert!(store.delete("../../../etc/passwd").is_err());
        assert!(
            store
                .rename("../../../etc/passwd", "evil".into())
                .is_err()
        );
    }

    #[test]
    fn rejects_non_uuid_snapshot_id() {
        let store = SnapshotStore::new(PathBuf::from("/tmp/qoredb_test_snapshots"));
        assert!(store.get("not-a-uuid").is_err());
        assert!(store.get("").is_err());
        assert!(store.get("foo/bar").is_err());
    }

    #[test]
    fn accepts_valid_uuid_snapshot_id() {
        assert!(
            SnapshotStore::validate_snapshot_id("550e8400-e29b-41d4-a716-446655440000").is_ok()
        );
        assert!(SnapshotStore::validate_snapshot_id("not-a-uuid").is_err());
    }
}

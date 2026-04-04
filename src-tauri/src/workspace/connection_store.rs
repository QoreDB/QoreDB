// SPDX-License-Identifier: Apache-2.0

//! Workspace Connection Store
//!
//! Stores connection metadata as individual JSON files in `.qoredb/connections/`.
//! Credentials (passwords) are stored in the OS keyring, never on disk.
//! One file per connection to minimize Git merge conflicts.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::engine::error::{EngineError, EngineResult};
use crate::observability::Sensitive;
use crate::vault::backend::CredentialProvider;
use crate::vault::credentials::{SavedConnection, StoredCredentials};
use crate::workspace::write_registry::WriteRegistry;

/// Credentials JSON shape (same as vault storage — shared format).
#[derive(Serialize, Deserialize)]
struct CredsJson {
    db_password: String,
    ssh_password: Option<String>,
    ssh_key_passphrase: Option<String>,
    #[serde(default)]
    proxy_password: Option<String>,
}

/// Connection store that persists metadata in a workspace directory.
pub struct WorkspaceConnectionStore {
    /// `.qoredb/connections/` directory
    connections_dir: PathBuf,
    /// Keyring service name (unique per workspace)
    service_name: String,
    /// Keyring provider
    provider: Box<dyn CredentialProvider>,
    /// Optional write registry for file watcher exclusion
    write_registry: Option<WriteRegistry>,
}

impl WorkspaceConnectionStore {
    pub fn new(
        connections_dir: PathBuf,
        service_name: String,
        provider: Box<dyn CredentialProvider>,
    ) -> Self {
        Self {
            connections_dir,
            service_name,
            provider,
            write_registry: None,
        }
    }

    /// Sets the write registry for file watcher self-write exclusion.
    pub fn with_write_registry(mut self, registry: WriteRegistry) -> Self {
        self.write_registry = Some(registry);
        self
    }

    /// Register a file write with the write registry (if present).
    fn register_write(&self, path: &PathBuf) {
        if let Some(ref reg) = self.write_registry {
            reg.register_with_auto_unregister(path.clone());
        }
    }

    /// Validates that a connection_id is safe to use as a filename.
    /// Rejects path traversal attempts, empty IDs, and dangerous characters.
    fn validate_connection_id(connection_id: &str) -> EngineResult<()> {
        if connection_id.is_empty() {
            return Err(EngineError::internal("Connection ID cannot be empty"));
        }
        if connection_id.contains("..")
            || connection_id.contains('/')
            || connection_id.contains('\\')
            || connection_id.contains('\0')
        {
            return Err(EngineError::internal(
                "Connection ID contains invalid characters",
            ));
        }
        // Only allow alphanumeric, underscore, hyphen
        if !connection_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(EngineError::internal(
                "Connection ID must only contain alphanumeric characters, underscores, or hyphens",
            ));
        }
        Ok(())
    }

    fn connection_file(&self, connection_id: &str) -> EngineResult<PathBuf> {
        Self::validate_connection_id(connection_id)?;
        Ok(self.connections_dir.join(format!("{}.json", connection_id)))
    }

    fn credentials_key(connection_id: &str) -> String {
        format!("creds_{}", connection_id)
    }

    /// Lists all connections from the workspace directory.
    pub fn list_connections(&self) -> EngineResult<Vec<SavedConnection>> {
        if !self.connections_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&self.connections_dir).map_err(|e| {
            EngineError::internal(format!("Failed to read connections dir: {}", e))
        })?;

        let mut connections = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| {
                EngineError::internal(format!("Failed to read dir entry: {}", e))
            })?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<SavedConnection>(&content) {
                    Ok(conn) => connections.push(conn),
                    Err(e) => {
                        tracing::warn!("Skipping invalid connection file {}: {}", path.display(), e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read {}: {}", path.display(), e);
                }
            }
        }

        connections.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(connections)
    }

    /// Gets a specific connection by ID.
    pub fn get_connection(&self, connection_id: &str) -> EngineResult<SavedConnection> {
        let path = self.connection_file(connection_id)?;
        let content = fs::read_to_string(&path).map_err(|e| {
            EngineError::internal(format!("Connection not found: {}", e))
        })?;

        serde_json::from_str(&content)
            .map_err(|e| EngineError::internal(format!("Invalid connection file: {}", e)))
    }

    /// Saves a connection (metadata to file, credentials to keyring).
    pub fn save_connection(
        &self,
        connection: &SavedConnection,
        credentials: &StoredCredentials,
    ) -> EngineResult<()> {
        // Ensure directory exists
        fs::create_dir_all(&self.connections_dir).map_err(|e| {
            EngineError::internal(format!("Failed to create connections dir: {}", e))
        })?;

        // Write metadata to file
        let content = serde_json::to_string_pretty(connection)
            .map_err(|e| EngineError::internal(format!("Serialization error: {}", e)))?;

        let file_path = self.connection_file(&connection.id)?;
        self.register_write(&file_path);
        fs::write(&file_path, content)
            .map_err(|e| EngineError::internal(format!("Failed to write connection file: {}", e)))?;

        // Save credentials to keyring
        let creds_json = serde_json::to_string(&CredsJson {
            db_password: credentials.db_password.expose().clone(),
            ssh_password: credentials.ssh_password.as_ref().map(|s| s.expose().clone()),
            ssh_key_passphrase: credentials.ssh_key_passphrase.as_ref().map(|s| s.expose().clone()),
            proxy_password: credentials.proxy_password.as_ref().map(|s| s.expose().clone()),
        })
        .map_err(|e| EngineError::internal(format!("Serialization error: {}", e)))?;

        self.provider
            .set_password(
                &self.service_name,
                &Self::credentials_key(&connection.id),
                &creds_json,
            )
            .map_err(|e| EngineError::internal(format!("Failed to save credentials: {}", e)))?;

        Ok(())
    }

    /// Gets credentials for a connection from the keyring.
    pub fn get_credentials(&self, connection_id: &str) -> EngineResult<StoredCredentials> {
        let creds_json = self
            .provider
            .get_password(&self.service_name, &Self::credentials_key(connection_id))?;

        let creds: CredsJson = serde_json::from_str(&creds_json)
            .map_err(|e| EngineError::internal(format!("Deserialization error: {}", e)))?;

        Ok(StoredCredentials {
            db_password: Sensitive::new(creds.db_password),
            ssh_password: creds.ssh_password.map(Sensitive::new),
            ssh_key_passphrase: creds.ssh_key_passphrase.map(Sensitive::new),
            proxy_password: creds.proxy_password.map(Sensitive::new),
        })
    }

    /// Deletes a connection (file + keyring entry).
    pub fn delete_connection(&self, connection_id: &str) -> EngineResult<()> {
        let path = self.connection_file(connection_id)?;
        if path.exists() {
            self.register_write(&path);
            fs::remove_file(&path).map_err(|e| {
                EngineError::internal(format!("Failed to delete connection file: {}", e))
            })?;
        }

        let _ = self.provider.delete_password(
            &self.service_name,
            &Self::credentials_key(connection_id),
        );

        Ok(())
    }

    /// Duplicates a connection under a new ID.
    pub fn duplicate_connection(&self, source_id: &str) -> EngineResult<SavedConnection> {
        let mut source = self.get_connection(source_id)?;
        let creds = self.get_credentials(source_id)?;

        let existing_names: HashSet<String> = self
            .list_connections()?
            .into_iter()
            .map(|c| c.name)
            .collect();

        let new_id = format!("conn_{}", Uuid::new_v4().simple());
        let new_name = make_copy_name(&source.name, &existing_names);

        source.id = new_id;
        source.name = new_name;

        self.save_connection(&source, &creds)?;
        Ok(source)
    }
}

fn make_copy_name(base_name: &str, existing_names: &HashSet<String>) -> String {
    let candidate = format!("{} (copy)", base_name);
    if !existing_names.contains(&candidate) {
        return candidate;
    }
    let mut index = 2;
    loop {
        let candidate = format!("{} (copy {})", base_name, index);
        if !existing_names.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::backend::MockProvider;
    use crate::vault::credentials::{Environment, SavedConnection, StoredCredentials};
    use tempfile::TempDir;

    fn make_connection(id: &str, name: &str) -> SavedConnection {
        SavedConnection {
            id: id.to_string(),
            name: name.to_string(),
            driver: "postgres".to_string(),
            environment: Environment::Development,
            read_only: false,
            host: "localhost".to_string(),
            port: 5432,
            username: "user".to_string(),
            database: Some("db".to_string()),
            ssl: false,
            ssl_mode: None,
            pool_max_connections: None,
            pool_min_connections: None,
            pool_acquire_timeout_secs: None,
            ssh_tunnel: None,
            proxy: None,
            project_id: "ws_test".to_string(),
        }
    }

    fn make_creds() -> StoredCredentials {
        StoredCredentials {
            db_password: Sensitive::new("secret".to_string()),
            ssh_password: None,
            ssh_key_passphrase: None,
            proxy_password: None,
        }
    }

    #[test]
    fn save_list_delete_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = WorkspaceConnectionStore::new(
            tmp.path().join("connections"),
            "qoredb_test".to_string(),
            Box::new(MockProvider::new()),
        );

        let conn = make_connection("conn_1", "Test DB");
        let creds = make_creds();

        store.save_connection(&conn, &creds).unwrap();

        let list = store.list_connections().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Test DB");

        let loaded = store.get_connection("conn_1").unwrap();
        assert_eq!(loaded.host, "localhost");

        let loaded_creds = store.get_credentials("conn_1").unwrap();
        assert_eq!(loaded_creds.db_password.expose(), "secret");

        store.delete_connection("conn_1").unwrap();
        let list = store.list_connections().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn duplicate_connection() {
        let tmp = TempDir::new().unwrap();
        let store = WorkspaceConnectionStore::new(
            tmp.path().join("connections"),
            "qoredb_test".to_string(),
            Box::new(MockProvider::new()),
        );

        let conn = make_connection("conn_orig", "My DB");
        store.save_connection(&conn, &make_creds()).unwrap();

        let dup = store.duplicate_connection("conn_orig").unwrap();
        assert_eq!(dup.name, "My DB (copy)");
        assert_ne!(dup.id, "conn_orig");

        let list = store.list_connections().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn rejects_path_traversal_in_connection_id() {
        assert!(WorkspaceConnectionStore::validate_connection_id("../etc/passwd").is_err());
        assert!(WorkspaceConnectionStore::validate_connection_id("foo/bar").is_err());
        assert!(WorkspaceConnectionStore::validate_connection_id("foo\\bar").is_err());
        assert!(WorkspaceConnectionStore::validate_connection_id("foo\0bar").is_err());
        assert!(WorkspaceConnectionStore::validate_connection_id("").is_err());
        assert!(WorkspaceConnectionStore::validate_connection_id("hello world").is_err());
        assert!(WorkspaceConnectionStore::validate_connection_id("conn_abc123").is_ok());
        assert!(WorkspaceConnectionStore::validate_connection_id("conn-abc-123").is_ok());
    }
}

//! Vault Storage
//!
//! Secure storage for database credentials using OS keychain.
//! Connection metadata is stored in a local JSON file to avoid excessive keychain prompts.

use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::engine::error::{EngineError, EngineResult};
use crate::vault::credentials::{SavedConnection, StoredCredentials};

const SERVICE_PREFIX: &str = "qoredb";
const CONNECTIONS_FILE: &str = "connections.json";

/// Storage for saved connections and their credentials
pub struct VaultStorage {
    project_id: String,
    storage_dir: PathBuf,
}

impl VaultStorage {
    /// Creates a new vault storage with project isolation
    pub fn new(project_id: &str, storage_dir: PathBuf) -> Self {
        Self {
            project_id: project_id.to_string(),
            storage_dir,
        }
    }

    /// Gets the keyring service name for this project
    fn service_name(&self) -> String {
        format!("{}_{}", SERVICE_PREFIX, self.project_id)
    }

    /// Gets the keyring key for connection credentials
    fn credentials_key(&self, connection_id: &str) -> String {
        format!("creds_{}", connection_id)
    }

    fn connections_file_path(&self) -> PathBuf {
        self.storage_dir.join(CONNECTIONS_FILE)
    }

    fn load_connections_file(&self) -> EngineResult<Vec<SavedConnection>> {
        let path = self.connections_file_path();
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| EngineError::internal(format!("Failed to read connections file: {}", e)))?;

        let connections: Vec<SavedConnection> = serde_json::from_str(&content)
            .map_err(|e| EngineError::internal(format!("Failed to parse connections file: {}", e)))?;

        Ok(connections)
    }

    fn save_connections_file(&self, connections: &[SavedConnection]) -> EngineResult<()> {
        let path = self.connections_file_path();
        
        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| EngineError::internal(format!("Failed to create storage directory: {}", e)))?;
        }

        let content = serde_json::to_string_pretty(connections)
            .map_err(|e| EngineError::internal(format!("Failed to serialize connections: {}", e)))?;

        fs::write(&path, content)
            .map_err(|e| EngineError::internal(format!("Failed to write connections file: {}", e)))?;

        Ok(())
    }

    /// Saves a connection with its credentials
    pub fn save_connection(
        &self,
        connection: &SavedConnection,
        credentials: &StoredCredentials,
    ) -> EngineResult<()> {
        // 1. Update metadata in JSON file
        let mut connections = self.load_connections_file()?;
        
        // Remove existing if present (update)
        connections.retain(|c| c.id != connection.id);
        connections.push(connection.clone());

        self.save_connections_file(&connections)?;

        // 2. Save credentials to Keychain (secrets only)
        let service = self.service_name();
        let creds_entry = Entry::new(&service, &self.credentials_key(&connection.id))
            .map_err(|e| EngineError::internal(format!("Keyring error: {}", e)))?;

        let creds_json = serde_json::to_string(&CredsJson {
            db_password: credentials.db_password.clone(),
            ssh_password: credentials.ssh_password.clone(),
            ssh_key_passphrase: credentials.ssh_key_passphrase.clone(),
        })
        .map_err(|e| EngineError::internal(format!("Serialization error: {}", e)))?;

        creds_entry
            .set_password(&creds_json)
            .map_err(|e| EngineError::internal(format!("Failed to save credentials: {}", e)))?;

        Ok(())
    }

    /// Retrieves a saved connection (metadata only, no credentials)
    pub fn get_connection(&self, connection_id: &str) -> EngineResult<SavedConnection> {
        let connections = self.load_connections_file()?;
        
        connections
            .into_iter()
            .find(|c| c.id == connection_id)
            .ok_or_else(|| EngineError::internal("Connection not found"))
    }

    /// Retrieves credentials for a connection
    pub fn get_credentials(&self, connection_id: &str) -> EngineResult<StoredCredentials> {
        let service = self.service_name();
        let entry = Entry::new(&service, &self.credentials_key(connection_id))
            .map_err(|e| EngineError::internal(format!("Keyring error: {}", e)))?;

        let creds_json = entry
            .get_password()
            .map_err(|_| EngineError::internal("Credentials not found"))?;

        let creds: CredsJson = serde_json::from_str(&creds_json)
            .map_err(|e| EngineError::internal(format!("Deserialization error: {}", e)))?;

        Ok(StoredCredentials {
            db_password: creds.db_password,
            ssh_password: creds.ssh_password,
            ssh_key_passphrase: creds.ssh_key_passphrase,
        })
    }

    /// Deletes a saved connection
    pub fn delete_connection(&self, connection_id: &str) -> EngineResult<()> {
        // 1. Remove from JSON file
        let mut connections = self.load_connections_file()?;
        let original_len = connections.len();
        connections.retain(|c| c.id != connection_id);
        
        if connections.len() != original_len {
             self.save_connections_file(&connections)?;
        }

        // 2. Remove credentials from Keychain
        let service = self.service_name();
        if let Ok(entry) = Entry::new(&service, &self.credentials_key(connection_id)) {
            let _ = entry.delete_credential();
        }

        // Try to clean up old metadata from keychain if it exists (migration cleanup)
        // We don't error if this fails, just best effort
        if let Ok(entry) = Entry::new(&service, &format!("meta_{}", connection_id)) {
            let _ = entry.delete_credential();
        }

        Ok(())
    }

    /// Duplicates a saved connection (metadata + credentials) under a new ID.
    pub fn duplicate_connection(&self, source_connection_id: &str) -> EngineResult<SavedConnection> {
        let mut source = self.get_connection(source_connection_id)?;
        let creds = self.get_credentials(source_connection_id)?;

        let existing_names: HashSet<String> = self
            .list_connections_full()?
            .into_iter()
            .map(|c| c.name)
            .collect();

        let new_id = format!("conn_{}", Uuid::new_v4().simple());
        let new_name = make_copy_name(&source.name, &existing_names);

        source.id = new_id;
        source.name = new_name;
        source.project_id = self.project_id.clone();

        self.save_connection(&source, &creds)?;

        Ok(source)
    }

    /// Lists all saved connections with metadata
    pub fn list_connections_full(&self) -> EngineResult<Vec<SavedConnection>> {
        self.load_connections_file()
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

/// Internal struct for serializing credentials
#[derive(Serialize, Deserialize)]
struct CredsJson {
    db_password: String,
    ssh_password: Option<String>,
    ssh_key_passphrase: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::credentials::{Environment, SavedConnection, SshTunnelInfo, StoredCredentials};
    use uuid::Uuid;
    use tempfile::TempDir;

    #[test]
    fn save_list_delete_roundtrip() -> EngineResult<()> {
        let temp_dir = TempDir::new().unwrap();
        let project_id = format!("qoredb_test_{}", Uuid::new_v4().simple());
        let connection_id = Uuid::new_v4().simple().to_string();
        let storage = VaultStorage::new(&project_id, temp_dir.path().to_path_buf());

        let connection = SavedConnection {
            id: connection_id.clone(),
            name: "test-connection".to_string(),
            driver: "postgres".to_string(),
            environment: Environment::Development,
            read_only: false,
            host: "localhost".to_string(),
            port: 5432,
            username: "qoredb".to_string(),
            database: Some("testdb".to_string()),
            ssl: false,
            ssh_tunnel: Some(SshTunnelInfo {
                host: "ssh.local".to_string(),
                port: 22,
                username: "sshuser".to_string(),
                auth_type: "password".to_string(),
                key_path: None,
                host_key_policy: "accept_new".to_string(),
                proxy_jump: None,
                connect_timeout_secs: 10,
                keepalive_interval_secs: 30,
                keepalive_count_max: 3,
            }),
            project_id: project_id.clone(),
            pool_acquire_timeout_secs: None,
            pool_max_connections: None,
            pool_min_connections: None
        };

        let credentials = StoredCredentials {
            db_password: "db_secret".to_string(),
            ssh_password: Some("ssh_secret".to_string()),
            ssh_key_passphrase: None,
        };

        storage.save_connection(&connection, &credentials)?;

        let full = storage.list_connections_full()?;
        assert_eq!(full.len(), 1);
        assert_eq!(full[0].id, connection_id);

        let loaded = storage.get_connection(&connection_id)?;
        assert_eq!(loaded.name, connection.name);

        let loaded_creds = storage.get_credentials(&connection_id)?;
        assert_eq!(loaded_creds.db_password, credentials.db_password);
        assert_eq!(loaded_creds.ssh_password, credentials.ssh_password);

        storage.delete_connection(&connection_id)?;
        let full = storage.list_connections_full()?;
        assert!(full.is_empty());

        Ok(())
    }
}

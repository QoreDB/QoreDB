//! Vault Storage
//!
//! Secure storage for database credentials using OS keychain.

use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

use crate::engine::error::{EngineError, EngineResult};
use crate::vault::credentials::{SavedConnection, StoredCredentials};

const SERVICE_PREFIX: &str = "qoredb";

/// Storage for saved connections and their credentials
pub struct VaultStorage {
    project_id: String,
}

impl VaultStorage {
    /// Creates a new vault storage with project isolation
    pub fn new(project_id: &str) -> Self {
        Self {
            project_id: project_id.to_string(),
        }
    }

    /// Gets the keyring service name for this project
    fn service_name(&self) -> String {
        format!("{}_{}", SERVICE_PREFIX, self.project_id)
    }

    /// Gets the keyring key for connection metadata
    fn metadata_key(&self, connection_id: &str) -> String {
        format!("meta_{}", connection_id)
    }

    /// Gets the keyring key for connection credentials
    fn credentials_key(&self, connection_id: &str) -> String {
        format!("creds_{}", connection_id)
    }

    /// Gets the keyring key for the connection list
    fn list_key(&self) -> String {
        "__connection_list__".to_string()
    }

    /// Saves a connection with its credentials
    pub fn save_connection(
        &self,
        connection: &SavedConnection,
        credentials: &StoredCredentials,
    ) -> EngineResult<()> {
        let service = self.service_name();

        // Save metadata (safe to expose)
        let meta_entry = Entry::new(&service, &self.metadata_key(&connection.id))
            .map_err(|e| EngineError::internal(format!("Keyring error: {}", e)))?;

        let meta_json = serde_json::to_string(connection)
            .map_err(|e| EngineError::internal(format!("Serialization error: {}", e)))?;

        meta_entry
            .set_password(&meta_json)
            .map_err(|e| EngineError::internal(format!("Failed to save metadata: {}", e)))?;

        // Save credentials (secrets)
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

        // Update connection list
        self.add_to_list(&connection.id)?;

        Ok(())
    }

    /// Retrieves a saved connection (metadata only, no credentials)
    pub fn get_connection(&self, connection_id: &str) -> EngineResult<SavedConnection> {
        let service = self.service_name();

        let entry = Entry::new(&service, &self.metadata_key(connection_id))
            .map_err(|e| EngineError::internal(format!("Keyring error: {}", e)))?;

        let meta_json = entry
            .get_password()
            .map_err(|_| EngineError::internal("Connection not found"))?;

        let connection: SavedConnection = serde_json::from_str(&meta_json)
            .map_err(|e| EngineError::internal(format!("Deserialization error: {}", e)))?;

        Ok(connection)
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
        let service = self.service_name();

        // Delete metadata
        if let Ok(entry) = Entry::new(&service, &self.metadata_key(connection_id)) {
            let _ = entry.delete_credential();
        }

        // Delete credentials
        if let Ok(entry) = Entry::new(&service, &self.credentials_key(connection_id)) {
            let _ = entry.delete_credential();
        }

        // Remove from list
        self.remove_from_list(connection_id)?;

        Ok(())
    }

    /// Duplicates a saved connection (metadata + credentials) under a new ID.
    ///
    /// Secrets never leave the vault: duplication is performed fully on the backend.
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

    /// Lists all saved connection IDs
    pub fn list_connections(&self) -> EngineResult<Vec<String>> {
        let service = self.service_name();

        let entry = Entry::new(&service, &self.list_key())
            .map_err(|e| EngineError::internal(format!("Keyring error: {}", e)))?;

        match entry.get_password() {
            Ok(list_json) => {
                let list: Vec<String> = serde_json::from_str(&list_json).map_err(|e| {
                    EngineError::internal(format!(
                        "Invalid connection list JSON in keyring: {}",
                        e
                    ))
                })?;
                Ok(list)
            }
            Err(keyring::Error::NoEntry) => Ok(Vec::new()),
            Err(e) => Err(EngineError::internal(format!("Failed to get list: {}", e))),
        }
    }

    /// Lists all saved connections with metadata
    pub fn list_connections_full(&self) -> EngineResult<Vec<SavedConnection>> {
        let ids = self.list_connections()?;
        let mut connections = Vec::new();

        for id in ids {
            if let Ok(conn) = self.get_connection(&id) {
                connections.push(conn);
            }
        }

        Ok(connections)
    }

    fn add_to_list(&self, connection_id: &str) -> EngineResult<()> {
        let mut list = self.list_connections()?;
        
        if !list.contains(&connection_id.to_string()) {
            list.push(connection_id.to_string());
            self.save_list(&list)?;
        }

        Ok(())
    }

    fn remove_from_list(&self, connection_id: &str) -> EngineResult<()> {
        let mut list = self.list_connections()?;
        list.retain(|id| id != connection_id);
        self.save_list(&list)
    }

    fn save_list(&self, list: &[String]) -> EngineResult<()> {
        let service = self.service_name();

        let entry = Entry::new(&service, &self.list_key())
            .map_err(|e| EngineError::internal(format!("Keyring error: {}", e)))?;

        let list_json = serde_json::to_string(list)
            .map_err(|e| EngineError::internal(format!("Serialization error: {}", e)))?;

        entry
            .set_password(&list_json)
            .map_err(|e| EngineError::internal(format!("Failed to save list: {}", e)))?;

        Ok(())
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

    #[test]
    fn save_list_delete_roundtrip() -> EngineResult<()> {
        let project_id = format!("qoredb_test_{}", Uuid::new_v4().simple());
        let connection_id = Uuid::new_v4().simple().to_string();
        let storage = VaultStorage::new(&project_id);

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

        let ids = storage.list_connections()?;
        assert!(ids.contains(&connection_id));

        let loaded = storage.get_connection(&connection_id)?;
        assert_eq!(loaded.name, connection.name);

        let loaded_creds = storage.get_credentials(&connection_id)?;
        assert_eq!(loaded_creds.db_password, credentials.db_password);
        assert_eq!(loaded_creds.ssh_password, credentials.ssh_password);

        let full = storage.list_connections_full()?;
        assert_eq!(full.len(), 1);
        assert_eq!(full[0].id, connection_id);

        storage.delete_connection(&connection_id)?;
        let ids = storage.list_connections()?;
        assert!(!ids.contains(&connection_id));

        Ok(())
    }
}

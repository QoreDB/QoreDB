// SPDX-License-Identifier: Apache-2.0

//! Gate between the vault and the headless agent surfaces (MCP server, CLI).
//! A connection reaches an agent only when the user opted in, and always in
//! read-only mode.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use qore_core::{ConnectionConfig, SessionId};
use qore_drivers::session_manager::SessionManager;

use crate::vault::VaultStorage;
use crate::vault::backend::KeyringProvider;
use crate::vault::credentials::{SavedConnection, StoredCredentials};
use crate::workspace::connection_store::WorkspaceConnectionStore;
use crate::workspace::{DEFAULT_PROJECT_ID, discovery, keyring_service, workspace_project_id};

pub const IDLE_SESSION_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub const WORKSPACE_ENV: &str = "QOREDB_WORKSPACE";

/// The connection store an agent surface reads: the default vault, or the
/// store of a file-based workspace when one is given or detected from the
/// working directory, exactly as the desktop app does.
pub enum AgentVault {
    Default(VaultStorage),
    Workspace {
        path: PathBuf,
        store: WorkspaceConnectionStore,
    },
}

impl AgentVault {
    pub fn open(config_dir: PathBuf, workspace: Option<&Path>) -> Self {
        match resolve_workspace(workspace) {
            Some(path) => {
                let store = WorkspaceConnectionStore::new(
                    path.join("connections"),
                    keyring_service(&workspace_project_id(&path)),
                    Box::new(KeyringProvider::new()),
                );
                Self::Workspace { path, store }
            }
            None => Self::Default(VaultStorage::new(
                DEFAULT_PROJECT_ID,
                config_dir,
                Box::new(KeyringProvider::new()),
            )),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Default(_) => "default vault".to_string(),
            Self::Workspace { path, .. } => format!("workspace {}", path.display()),
        }
    }

    pub fn workspace_path(&self) -> Option<&Path> {
        match self {
            Self::Default(_) => None,
            Self::Workspace { path, .. } => Some(path),
        }
    }

    pub fn list(&self) -> Result<Vec<SavedConnection>, String> {
        match self {
            Self::Default(storage) => storage.list_connections_full(),
            Self::Workspace { store, .. } => store.list_connections(),
        }
        .map_err(|e| e.sanitized_message())
    }

    pub fn exposed(&self) -> Result<Vec<SavedConnection>, String> {
        self.list().map(exposed_connections)
    }

    pub fn get(&self, connection_id: &str) -> Result<SavedConnection, String> {
        match self {
            Self::Default(storage) => storage.get_connection(connection_id),
            Self::Workspace { store, .. } => store.get_connection(connection_id),
        }
        .map_err(|e| e.sanitized_message())
    }

    pub fn credentials(&self, connection_id: &str) -> Result<StoredCredentials, String> {
        match self {
            Self::Default(storage) => storage.get_credentials(connection_id),
            Self::Workspace { store, .. } => store.get_credentials(connection_id),
        }
        .map_err(|e| e.sanitized_message())
    }
}

/// Explicit path first (the `.qoredb/` directory or its parent), then the
/// `QOREDB_WORKSPACE` variable, then discovery from the working directory.
pub fn resolve_workspace(explicit: Option<&Path>) -> Option<PathBuf> {
    let from_env = std::env::var_os(WORKSPACE_ENV).map(PathBuf::from);
    let candidate = explicit.map(Path::to_path_buf).or(from_env)?;
    let qoredb = if candidate.file_name().is_some_and(|n| n == ".qoredb") {
        candidate
    } else {
        candidate.join(".qoredb")
    };
    qoredb.join("workspace.json").is_file().then_some(qoredb)
}

pub fn detect_workspace(explicit: Option<&Path>) -> Option<PathBuf> {
    if explicit.is_some() || std::env::var_os(WORKSPACE_ENV).is_some() {
        return resolve_workspace(explicit);
    }
    discovery::detect_workspace_from_cwd()
}

/// The one way an agent surface opens a session: exposure is checked before
/// any secret is read, and the session is always read-only.
pub async fn open_session(
    vault: &AgentVault,
    session_manager: &SessionManager,
    connection_id: &str,
) -> Result<SessionId, String> {
    let saved = vault.get(connection_id)?;
    require_exposed(&saved)?;
    let creds = vault.credentials(connection_id)?;
    let config = agent_connection_config(&saved, &creds)?;
    let session = crate::connection::connect(session_manager, config)
        .await
        .map_err(|e| e.sanitized())?;
    session_manager
        .set_saved_connection_identity(session, saved.id.clone(), saved.name.clone())
        .await;
    session_manager.set_masking(session, &saved.masking).await;
    Ok(session)
}

pub fn exposed_connections(all: Vec<SavedConnection>) -> Vec<SavedConnection> {
    all.into_iter().filter(|c| c.expose_to_agents).collect()
}

pub fn require_exposed(connection: &SavedConnection) -> Result<(), String> {
    if connection.expose_to_agents {
        return Ok(());
    }
    Err(format!(
        "Connection '{}' is not exposed to AI agents. Enable it in QoreDB under \
         Settings > AI agents to allow access.",
        connection.id
    ))
}

/// Builds the config an agent may connect with: exposure is checked and the
/// session is forced read-only whatever the saved flag says.
pub fn agent_connection_config(
    connection: &SavedConnection,
    creds: &StoredCredentials,
) -> Result<ConnectionConfig, String> {
    require_exposed(connection)?;
    let mut config = connection
        .to_connection_config(creds)
        .map_err(|e| e.sanitized_message())?;
    config.read_only = true;
    Ok(config)
}

pub fn connection_summary(connection: &SavedConnection) -> serde_json::Value {
    serde_json::json!({
        "id": connection.id,
        "name": connection.name,
        "driver": connection.driver,
        "host": connection.host,
        "database": connection.database,
        "environment": connection.environment.as_str(),
        "read_only": true,
    })
}

/// Sessions opened on behalf of an agent, keyed by connection id. Sessions
/// idle for longer than [`IDLE_SESSION_TIMEOUT`] are handed back by
/// [`AgentSessions::take_idle`] so the caller can close them; there is no
/// background task.
#[derive(Default)]
pub struct AgentSessions {
    entries: HashMap<String, (SessionId, Instant)>,
}

impl AgentSessions {
    pub fn get(&mut self, connection_id: &str) -> Option<SessionId> {
        let (session, last_used) = self.entries.get_mut(connection_id)?;
        *last_used = Instant::now();
        Some(*session)
    }

    pub fn insert(&mut self, connection_id: String, session: SessionId) {
        self.entries
            .insert(connection_id, (session, Instant::now()));
    }

    pub fn take_idle(&mut self) -> Vec<SessionId> {
        self.take_idle_at(Instant::now())
    }

    fn take_idle_at(&mut self, now: Instant) -> Vec<SessionId> {
        let idle: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, (_, last))| now.duration_since(*last) >= IDLE_SESSION_TIMEOUT)
            .map(|(id, _)| id.clone())
            .collect();
        idle.iter()
            .filter_map(|id| self.entries.remove(id).map(|(session, _)| session))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensitive::Sensitive;
    use crate::vault::credentials::Environment;

    fn connection(id: &str, expose: bool) -> SavedConnection {
        SavedConnection {
            options: Default::default(),
            id: id.to_string(),
            name: id.to_string(),
            driver: "postgres".to_string(),
            environment: Environment::Production,
            read_only: false,
            expose_to_agents: expose,
            host: "localhost".to_string(),
            port: 5432,
            username: "qoredb".to_string(),
            database: None,
            ssl: false,
            ssl_mode: None,
            pool_max_connections: None,
            pool_min_connections: None,
            pool_acquire_timeout_secs: None,
            ssh_tunnel: None,
            proxy: None,
            mssql_auth: None,
            clickhouse_cluster: None,
            search_auth_mode: None,
            ssl_ca_cert: None,
            masking: Default::default(),
            project_id: "default".to_string(),
        }
    }

    fn creds() -> StoredCredentials {
        StoredCredentials {
            db_password: Sensitive::new("pw".to_string()),
            ssh_password: None,
            ssh_key_passphrase: None,
            proxy_password: None,
        }
    }

    #[test]
    fn only_exposed_connections_are_listed() {
        let listed = exposed_connections(vec![
            connection("hidden", false),
            connection("visible", true),
        ]);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "visible");
    }

    #[test]
    fn legacy_connections_are_hidden_by_default() {
        let legacy = r#"{
            "id":"c1","name":"legacy","driver":"postgres",
            "environment":"development","read_only":false,
            "host":"localhost","port":5432,"username":"u","database":null,
            "ssl":false,"ssh_tunnel":null,"project_id":"default"
        }"#;
        let parsed: SavedConnection = serde_json::from_str(legacy).unwrap();
        assert!(!parsed.expose_to_agents);
        assert!(require_exposed(&parsed).is_err());
    }

    #[test]
    fn unexposed_connection_is_refused_even_by_id() {
        let err = agent_connection_config(&connection("secret", false), &creds()).unwrap_err();
        assert!(err.contains("not exposed"), "{err}");
        assert!(err.contains("secret"));
    }

    #[test]
    fn exposed_connection_is_forced_read_only() {
        let config = agent_connection_config(&connection("prod", true), &creds()).unwrap();
        assert!(config.read_only);
        assert_eq!(config.environment, "production");
    }

    #[test]
    fn idle_sessions_are_evicted_after_timeout() {
        let mut sessions = AgentSessions::default();
        let fresh = SessionId(uuid::Uuid::new_v4());
        let stale = SessionId(uuid::Uuid::new_v4());
        sessions.insert("fresh".to_string(), fresh);
        sessions.insert("stale".to_string(), stale);
        sessions.entries.get_mut("stale").unwrap().1 =
            Instant::now() - IDLE_SESSION_TIMEOUT - Duration::from_secs(1);

        let evicted = sessions.take_idle_at(Instant::now());
        assert_eq!(evicted, vec![stale]);
        assert_eq!(sessions.get("fresh"), Some(fresh));
        assert_eq!(sessions.get("stale"), None);
    }

    #[test]
    fn workspace_resolution_accepts_the_qoredb_dir_or_its_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let qoredb = tmp.path().join(".qoredb");
        std::fs::create_dir_all(&qoredb).unwrap();
        assert_eq!(resolve_workspace(Some(tmp.path())), None);

        std::fs::write(qoredb.join("workspace.json"), "{}").unwrap();
        assert_eq!(resolve_workspace(Some(tmp.path())), Some(qoredb.clone()));
        assert_eq!(resolve_workspace(Some(&qoredb)), Some(qoredb.clone()));

        let vault = AgentVault::open(tmp.path().to_path_buf(), Some(tmp.path()));
        assert!(matches!(vault, AgentVault::Workspace { .. }));
        assert_eq!(vault.list().unwrap().len(), 0);
    }

    #[test]
    fn touching_a_session_keeps_it_alive() {
        let mut sessions = AgentSessions::default();
        let id = SessionId(uuid::Uuid::new_v4());
        sessions.insert("c".to_string(), id);
        sessions.entries.get_mut("c").unwrap().1 = Instant::now() - IDLE_SESSION_TIMEOUT;
        assert_eq!(sessions.get("c"), Some(id));
        assert!(sessions.take_idle().is_empty());
    }
}

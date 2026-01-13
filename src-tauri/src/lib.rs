// QoreDB - Modern local-first database client
// Core library

pub mod commands;
pub mod engine;
pub mod vault;

use std::sync::Arc;
use tokio::sync::Mutex;

use engine::drivers::mongodb::MongoDriver;
use engine::drivers::mysql::MySqlDriver;
use engine::drivers::postgres::PostgresDriver;
use engine::{DriverRegistry, SessionManager};
use vault::VaultLock;

pub type SharedState = Arc<Mutex<AppState>>;
pub struct AppState {
    pub registry: Arc<DriverRegistry>,
    pub session_manager: SessionManager,
    pub vault_lock: VaultLock,
}

impl AppState {
    pub fn new() -> Self {
        let mut registry = DriverRegistry::new();

        registry.register(Arc::new(PostgresDriver::new()));
        registry.register(Arc::new(MySqlDriver::new()));
        registry.register(Arc::new(MongoDriver::new()));

        let registry = Arc::new(registry);
        let session_manager = SessionManager::new(Arc::clone(&registry));
        let mut vault_lock = VaultLock::new();

        let _ = vault_lock.auto_unlock_if_no_password();

        Self {
            registry,
            session_manager,
            vault_lock,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state: SharedState = Arc::new(Mutex::new(AppState::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            // Connection commands
            commands::connection::test_connection,
            commands::connection::connect,
            commands::connection::disconnect,
            commands::connection::list_sessions,
            // Query commands
            commands::query::execute_query,
            commands::query::cancel_query,
            commands::query::list_namespaces,
            commands::query::list_collections,
            commands::query::describe_table,
            commands::query::preview_table,
            // Vault commands
            commands::vault::get_vault_status,
            commands::vault::setup_master_password,
            commands::vault::unlock_vault,
            commands::vault::lock_vault,
            commands::vault::save_connection,
            commands::vault::list_saved_connections,
            commands::vault::delete_saved_connection,
            commands::vault::get_connection_credentials,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

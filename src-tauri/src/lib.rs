// QoreDB - Modern local-first database client
// Core library

pub mod commands;
pub mod engine;
pub mod metrics;
pub mod observability;
pub mod policy;
pub mod vault;

use std::sync::Arc;
use tokio::sync::Mutex;

use engine::drivers::mongodb::MongoDriver;
use engine::drivers::mysql::MySqlDriver;
use engine::drivers::postgres::PostgresDriver;
use engine::{DriverRegistry, QueryManager, SessionManager};
use policy::SafetyPolicy;
use vault::{VaultLock, backend::KeyringProvider};

pub type SharedState = Arc<Mutex<AppState>>;
pub struct AppState {
    pub registry: Arc<DriverRegistry>,
    pub session_manager: Arc<SessionManager>,
    pub vault_lock: VaultLock,
    pub policy: SafetyPolicy,
    pub query_manager: Arc<QueryManager>,
}

impl AppState {
    pub fn new() -> Self {
        let mut registry = DriverRegistry::new();

        registry.register(Arc::new(PostgresDriver::new()));
        registry.register(Arc::new(MySqlDriver::new()));
        registry.register(Arc::new(MongoDriver::new()));

        let registry = Arc::new(registry);
        let session_manager = Arc::new(SessionManager::new(Arc::clone(&registry)));
        let mut vault_lock = VaultLock::new(Box::new(KeyringProvider::new()));
        let policy = SafetyPolicy::load();
        let query_manager = Arc::new(QueryManager::new());

        let _ = vault_lock.auto_unlock_if_no_password();

        Self {
            registry,
            session_manager,
            vault_lock,
            policy,
            query_manager,
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
    observability::init_tracing();
    let state: SharedState = Arc::new(Mutex::new(AppState::new()));

    tauri::Builder::default()
        .setup(|app| {
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            // Connection commands
            commands::connection::test_connection,
            commands::connection::test_saved_connection,
            commands::connection::connect,
            commands::connection::connect_saved_connection,
            commands::connection::disconnect,
            commands::connection::list_sessions,
            // Driver commands
            commands::driver::get_driver_info,
            commands::driver::list_drivers,
            // Query commands
            commands::query::execute_query,
            commands::query::cancel_query,
            commands::query::list_namespaces,
            commands::query::list_collections,
            commands::query::describe_table,
            commands::query::preview_table,
            commands::query::peek_foreign_key,
            commands::query::create_database,
            commands::query::drop_database,
            // Transaction commands
            commands::query::begin_transaction,
            commands::query::commit_transaction,
            commands::query::rollback_transaction,
            commands::query::supports_transactions,
            // Mutation commands
            commands::mutation::insert_row,
            commands::mutation::update_row,
            commands::mutation::delete_row,
            commands::mutation::supports_mutations,
            // Logs
            commands::logs::export_logs,
            commands::logs::log_frontend_message,
            // Metrics (dev-only)
            commands::metrics::get_metrics,
            // Vault commands
            commands::vault::get_vault_status,
            commands::vault::setup_master_password,
            commands::vault::unlock_vault,
            commands::vault::lock_vault,
            commands::vault::save_connection,
            commands::vault::list_saved_connections,
            commands::vault::delete_saved_connection,
            commands::vault::duplicate_saved_connection,
            commands::vault::get_connection_credentials,
            // Policy commands
            commands::policy::get_safety_policy,
            commands::policy::set_safety_policy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

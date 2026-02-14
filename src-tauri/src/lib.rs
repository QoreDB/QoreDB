// QoreDB - Modern local-first database client
// Core library

pub mod commands;
pub mod engine;
pub mod export;
pub mod interceptor;
pub mod metrics;
pub mod observability;
pub mod policy;
pub mod vault;
pub mod virtual_relations;

use std::sync::Arc;
use tokio::sync::Mutex;

use engine::drivers::mongodb::MongoDriver;
use engine::drivers::mysql::MySqlDriver;
use engine::drivers::postgres::PostgresDriver;
use engine::drivers::redis::RedisDriver;
use engine::drivers::sqlite::SqliteDriver;
use engine::{DriverRegistry, QueryManager, SessionManager};
use interceptor::InterceptorPipeline;
use policy::SafetyPolicy;
use vault::{VaultLock, backend::KeyringProvider};
use export::ExportPipeline;
use virtual_relations::VirtualRelationStore;

pub type SharedState = Arc<Mutex<AppState>>;
pub struct AppState {
    pub registry: Arc<DriverRegistry>,
    pub session_manager: Arc<SessionManager>,
    pub vault_lock: VaultLock,
    pub policy: SafetyPolicy,
    pub query_manager: Arc<QueryManager>,
    pub interceptor: Arc<InterceptorPipeline>,
    pub export_pipeline: Arc<ExportPipeline>,
    pub virtual_relations: Arc<VirtualRelationStore>,
}

impl AppState {
    pub fn new() -> Self {
        let mut registry = DriverRegistry::new();

        registry.register(Arc::new(PostgresDriver::new()));
        registry.register(Arc::new(MySqlDriver::new()));
        registry.register(Arc::new(MongoDriver::new()));
        registry.register(Arc::new(RedisDriver::new()));
        registry.register(Arc::new(SqliteDriver::new()));

        let registry = Arc::new(registry);
        let session_manager = Arc::new(SessionManager::new(Arc::clone(&registry)));
        let mut vault_lock = VaultLock::new(Box::new(KeyringProvider::new()));
        let policy = SafetyPolicy::load();
        let query_manager = Arc::new(QueryManager::new());
        let export_pipeline = Arc::new(ExportPipeline::new());

        // Initialize interceptor with data directory
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("com.qoredb.app");
        let interceptor = Arc::new(InterceptorPipeline::new(data_dir.join("interceptor")));
        let _ = interceptor.load_config();

        // Initialize virtual relations store
        let virtual_relations = Arc::new(VirtualRelationStore::new(
            data_dir.join("virtual_relations"),
        ));

        let _ = vault_lock.auto_unlock_if_no_password();

        Self {
            registry,
            session_manager,
            vault_lock,
            policy,
            query_manager,
            interceptor,
            export_pipeline,
            virtual_relations,
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
            // Connection URL parsing
            commands::connection_url::parse_url,
            commands::connection_url::get_supported_url_schemes,
            // Driver commands
            commands::driver::get_driver_info,
            commands::driver::list_drivers,
            // Query commands
            commands::query::execute_query,
            commands::query::cancel_query,
            commands::query::list_namespaces,
            commands::query::list_collections,
            commands::query::list_routines,
            commands::query::list_triggers,
            commands::query::list_events,
            commands::query::describe_table,
            commands::query::preview_table,
            commands::query::query_table,
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
            // Export
            commands::export::start_export,
            commands::export::cancel_export,
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
            // Sandbox commands
            commands::sandbox::generate_migration_sql,
            commands::sandbox::apply_sandbox_changes,
            // Full-text search
            commands::fulltext_search::fulltext_search,
            // Interceptor commands
            commands::interceptor::get_interceptor_config,
            commands::interceptor::update_interceptor_config,
            commands::interceptor::get_audit_entries,
            commands::interceptor::get_audit_stats,
            commands::interceptor::clear_audit_log,
            commands::interceptor::export_audit_log,
            commands::interceptor::get_profiling_metrics,
            commands::interceptor::get_slow_queries,
            commands::interceptor::clear_slow_queries,
            commands::interceptor::reset_profiling,
            commands::interceptor::export_profiling,
            commands::interceptor::get_safety_rules,
            commands::interceptor::add_safety_rule,
            commands::interceptor::update_safety_rule,
            commands::interceptor::remove_safety_rule,
            // Virtual relations commands
            commands::virtual_relations::list_virtual_relations,
            commands::virtual_relations::add_virtual_relation,
            commands::virtual_relations::update_virtual_relation,
            commands::virtual_relations::delete_virtual_relation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

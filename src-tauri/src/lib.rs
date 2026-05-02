// SPDX-License-Identifier: Apache-2.0

// QoreDB - Modern local-first database client
// Core library

#[cfg(feature = "pro")]
pub mod ai;
pub mod commands;
pub mod engine;
pub mod export;
#[cfg(feature = "pro")]
pub mod federation;
pub mod interceptor;
pub mod license;
pub mod time_travel;
pub mod metrics;
pub mod observability;
pub mod policy;
pub mod share;
pub mod snapshots;
pub mod vault;
pub mod virtual_relations;
pub mod workspace;

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

use commands::workspace::SharedWorkspaceManager;
use engine::drivers::cockroachdb::CockroachDbDriver;
use engine::drivers::duckdb::DuckDbDriver;
use engine::drivers::mariadb::MariaDbDriver;
use engine::drivers::mongodb::MongoDriver;
use engine::drivers::mysql::MySqlDriver;
use engine::drivers::neon::NeonDriver;
use engine::drivers::postgres::PostgresDriver;
use engine::drivers::redis::RedisDriver;
use engine::drivers::sqlite::SqliteDriver;
use engine::drivers::sqlserver::SqlServerDriver;
use engine::drivers::supabase::SupabaseDriver;
use engine::drivers::timescaledb::TimescaleDbDriver;
use engine::{DriverRegistry, QueryManager, SessionManager};
use export::ExportPipeline;
use interceptor::InterceptorPipeline;
use license::LicenseManager;
use policy::SafetyPolicy;
use share::ShareManager;
use snapshots::SnapshotStore;
use vault::{backend::KeyringProvider, VaultLock};
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
    pub share_manager: Arc<ShareManager>,
    pub virtual_relations: Arc<VirtualRelationStore>,
    pub license_manager: LicenseManager,
    #[cfg(feature = "pro")]
    pub ai_manager: Arc<ai::manager::AiManager>,
    pub changelog_store: Arc<time_travel::ChangelogStore>,
}

impl AppState {
    pub fn new() -> Self {
        let mut registry = DriverRegistry::new();

        registry.register(Arc::new(PostgresDriver::new()));
        registry.register(Arc::new(MySqlDriver::new()));
        registry.register(Arc::new(MongoDriver::new()));
        registry.register(Arc::new(RedisDriver::new()));
        registry.register(Arc::new(SqliteDriver::new()));
        registry.register(Arc::new(DuckDbDriver::new()));
        registry.register(Arc::new(CockroachDbDriver::new()));
        registry.register(Arc::new(SqlServerDriver::new()));
        registry.register(Arc::new(MariaDbDriver::new()));
        registry.register(Arc::new(SupabaseDriver::new()));
        registry.register(Arc::new(NeonDriver::new()));
        registry.register(Arc::new(TimescaleDbDriver::new()));

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
        let share_manager = Arc::new(ShareManager::new(
            data_dir.join("share"),
            Box::new(KeyringProvider::new()),
        ));

        let _ = vault_lock.auto_unlock_if_no_password();

        // Initialize license manager (loads stored key from keyring)
        let license_manager = LicenseManager::new(Box::new(KeyringProvider::new()));

        // Initialize AI manager (Pro only)
        #[cfg(feature = "pro")]
        let ai_manager = Arc::new(ai::manager::AiManager::new(
            Box::new(KeyringProvider::new()),
        ));

        // Initialize changelog store for Data Time-Travel
        let changelog_store = Arc::new(time_travel::ChangelogStore::new(
            data_dir.join("time-travel"),
        ));

        Self {
            registry,
            session_manager,
            vault_lock,
            policy,
            query_manager,
            interceptor,
            export_pipeline,
            share_manager,
            virtual_relations,
            license_manager,
            #[cfg(feature = "pro")]
            ai_manager,
            changelog_store,
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

    // Initialize snapshot store (managed separately — no mutex needed)
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.qoredb.app");
    let snapshot_store: commands::snapshots::SharedSnapshotStore =
        Arc::new(SnapshotStore::new(data_dir.join("snapshots")));

    // Initialize workspace manager
    let app_config_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.qoredb.app");
    let workspace_manager: SharedWorkspaceManager =
        Arc::new(tokio::sync::Mutex::new(workspace::WorkspaceManager::new(app_config_dir)));

    // Initialize workspace file watcher infrastructure
    let write_registry = workspace::write_registry::WriteRegistry::new();
    let (ws_path_tx, ws_path_rx) =
        tokio::sync::watch::channel::<Option<std::path::PathBuf>>(None);
    let watcher_path_sender: commands::workspace::WatcherPathSender = Arc::new(ws_path_tx);

    tauri::Builder::default()
        .setup(|app| {
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            #[cfg(target_os = "linux")]
            {
                use tauri::image::Image;
                if let Some(window) = app.get_webview_window("main") {
                    let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))
                        .expect("failed to load app icon");
                    let _ = window.set_icon(icon);
                }
            }

            // Start the connection health monitor
            let state: tauri::State<SharedState> = app.state();
            let session_manager = {
                let app_state = state.blocking_lock();
                Arc::clone(&app_state.session_manager)
            };
            session_manager.start_health_monitor(app.handle().clone());

            // Start workspace file watcher
            let wr: tauri::State<workspace::write_registry::WriteRegistry> = app.state();
            workspace::watcher::start_workspace_watcher(
                app.handle().clone(),
                ws_path_rx,
                wr.inner().clone(),
            );

            Ok(())
        })
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(state)
        .manage(snapshot_store)
        .manage(workspace_manager)
        .manage(write_registry)
        .manage(watcher_path_sender)
        .invoke_handler(tauri::generate_handler![
            // Connection commands
            commands::connection::test_connection,
            commands::connection::test_saved_connection,
            commands::connection::connect,
            commands::connection::connect_saved_connection,
            commands::connection::disconnect,
            commands::connection::list_sessions,
            commands::connection::check_connection_health,
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
            commands::query::list_sequences,
            commands::query::describe_table,
            commands::query::preview_table,
            commands::query::query_table,
            commands::query::peek_foreign_key,
            commands::query::get_creation_options,
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
            // Maintenance commands
            commands::maintenance::list_maintenance_operations,
            commands::maintenance::run_maintenance,
            // Routine management commands
            commands::routines::get_routine_definition,
            commands::routines::drop_routine,
            // Trigger & Event management commands
            commands::triggers::get_trigger_definition,
            commands::triggers::drop_trigger,
            commands::triggers::toggle_trigger,
            commands::triggers::get_event_definition,
            commands::triggers::drop_event,
            // Sequence management commands
            commands::sequences::get_sequence_definition,
            commands::sequences::drop_sequence,
            // Logs
            commands::logs::export_logs,
            commands::logs::log_frontend_message,
            // Export
            commands::export::start_export,
            commands::export::cancel_export,
            // Share
            commands::share::share_prepare_export,
            commands::share::share_cleanup_export,
            commands::share::share_upload_prepared_export,
            commands::share::share_save_provider_token,
            commands::share::share_delete_provider_token,
            commands::share::share_get_provider_status,
            commands::share::share_snapshot,
            // Import
            commands::import::preview_csv,
            commands::import::import_csv,
            // Schema export
            commands::schema_export::export_schema,
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
            // Governance commands
            commands::query::get_governance_limits,
            commands::query::update_governance_limits,
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
            // Snapshot commands
            commands::snapshots::save_snapshot,
            commands::snapshots::list_snapshots,
            commands::snapshots::get_snapshot,
            commands::snapshots::delete_snapshot,
            commands::snapshots::rename_snapshot,
            // Virtual relations commands
            commands::virtual_relations::list_virtual_relations,
            commands::virtual_relations::add_virtual_relation,
            commands::virtual_relations::update_virtual_relation,
            commands::virtual_relations::delete_virtual_relation,
            // License commands
            commands::license::activate_license,
            commands::license::get_license_status,
            commands::license::deactivate_license,
            commands::license::dev_set_license_tier,
            // Federation commands
            commands::federation::execute_federation_query,
            commands::federation::list_federation_sources,
            // AI commands
            commands::ai::ai_generate_query,
            commands::ai::ai_explain_result,
            commands::ai::ai_summarize_schema,
            commands::ai::ai_fix_error,
            commands::ai::ai_save_api_key,
            commands::ai::ai_delete_api_key,
            commands::ai::ai_get_provider_status,
            // Workspace commands
            commands::workspace::detect_workspace,
            commands::workspace::get_active_workspace,
            commands::workspace::get_workspace_project_id,
            commands::workspace::create_workspace,
            commands::workspace::open_workspace,
            commands::workspace::switch_workspace,
            commands::workspace::switch_to_default_workspace,
            commands::workspace::rename_workspace,
            commands::workspace::list_recent_workspaces,
            commands::workspace::import_default_connections,
            // Workspace query library commands
            commands::workspace_queries::ws_get_query_library,
            commands::workspace_queries::ws_save_query_library,
            // Time-Travel commands
            commands::time_travel::get_table_timeline,
            commands::time_travel::get_row_history,
            commands::time_travel::compute_temporal_diff,
            commands::time_travel::get_row_state_at,
            commands::time_travel::generate_rollback_sql,
            commands::time_travel::generate_entry_rollback_sql,
            commands::time_travel::get_time_travel_config,
            commands::time_travel::update_time_travel_config,
            commands::time_travel::clear_table_changelog,
            commands::time_travel::clear_all_changelog,
            commands::time_travel::export_changelog,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

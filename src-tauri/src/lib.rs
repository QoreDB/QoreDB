// SPDX-License-Identifier: Apache-2.0

//! QoreDB core library — modern local-first database client.

#[cfg(feature = "pro")]
pub mod ai;
#[cfg(feature = "pro")]
pub mod api;
pub mod backup;
pub mod commands;
#[cfg(feature = "pro")]
pub mod contracts;
pub mod engine;
pub mod export;
#[cfg(feature = "pro")]
pub mod federation;
pub mod observability;
pub mod plugins;
pub mod share;
pub mod snapshots;
pub mod time_travel;
pub mod workspace;

pub use qore_service::{
    cache, interceptor, license, metrics, paths, policy, ratelimit, vault, virtual_relations,
};

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

use commands::workspace::SharedWorkspaceManager;
use export::ExportPipeline;
use plugins::runtime::PluginHost;
use qore_service::ServiceContext;
use share::ShareManager;
use snapshots::SnapshotStore;
use vault::backend::KeyringProvider;

pub type SharedState = Arc<Mutex<AppState>>;

pub struct AppState {
    pub service: ServiceContext,
    pub plugin_host: Arc<PluginHost>,
    pub export_pipeline: Arc<ExportPipeline>,
    pub share_manager: Arc<ShareManager>,
    #[cfg(feature = "pro")]
    pub ai_manager: Arc<ai::manager::AiManager>,
    pub changelog_store: Arc<time_travel::ChangelogStore>,
    pub backup_tool_paths: Arc<backup::BackupToolPaths>,
    pub active_backups: Arc<backup::runner::ActiveBackups>,
    pub confirmation_tokens: Arc<commands::confirmation::ConfirmationTokenStore>,
}

impl AppState {
    pub fn new() -> Self {
        let service = ServiceContext::new();

        let data_dir = paths::app_data_dir();
        let export_pipeline = Arc::new(ExportPipeline::new());
        let share_manager = Arc::new(ShareManager::new(
            data_dir.join("share"),
            Box::new(KeyringProvider::new()),
        ));

        #[cfg(feature = "pro")]
        let ai_manager = Arc::new(ai::manager::AiManager::new(
            Box::new(KeyringProvider::new()),
        ));

        let changelog_store = Arc::new(time_travel::ChangelogStore::new(
            data_dir.join("time-travel"),
        ));

        // Load executable plugins once at startup.
        let plugin_host = Arc::new(PluginHost::new());
        plugin_host.reload();

        Self {
            service,
            plugin_host,
            export_pipeline,
            share_manager,
            #[cfg(feature = "pro")]
            ai_manager,
            changelog_store,
            backup_tool_paths: Arc::new(backup::BackupToolPaths::new()),
            active_backups: Arc::new(backup::runner::ActiveBackups::new()),
            confirmation_tokens: Arc::new(commands::confirmation::ConfirmationTokenStore::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for AppState {
    type Target = ServiceContext;
    fn deref(&self) -> &ServiceContext {
        &self.service
    }
}

impl std::ops::DerefMut for AppState {
    fn deref_mut(&mut self) -> &mut ServiceContext {
        &mut self.service
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    observability::init_tracing();
    let state: SharedState = Arc::new(Mutex::new(AppState::new()));

    let data_dir = paths::app_data_dir();
    let snapshot_store: commands::snapshots::SharedSnapshotStore =
        Arc::new(SnapshotStore::new(data_dir.join("snapshots")));

    let write_registry = workspace::write_registry::WriteRegistry::new();
    let (ws_path_tx, ws_path_rx) = tokio::sync::watch::channel::<Option<std::path::PathBuf>>(None);
    let watcher_path_sender: commands::workspace::WatcherPathSender = Arc::new(ws_path_tx);

    #[cfg(feature = "pro")]
    let instant_api: commands::instant_api::SharedInstantApi = Arc::new(tokio::sync::Mutex::new(
        commands::instant_api::InstantApiState::new(data_dir.clone())
            .expect("failed to initialize Instant API endpoint store"),
    ));

    let builder = tauri::Builder::default()
        .setup(|app| {
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            let app_config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let workspace_manager: SharedWorkspaceManager = Arc::new(tokio::sync::Mutex::new(
                workspace::WorkspaceManager::new(app_config_dir),
            ));
            app.manage(workspace_manager);

            #[cfg(target_os = "linux")]
            {
                use tauri::image::Image;
                if let Some(window) = app.get_webview_window("main") {
                    let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))
                        .expect("failed to load app icon");
                    let _ = window.set_icon(icon);
                }
            }

            let state: tauri::State<SharedState> = app.state();
            let (session_manager, plugin_host) = {
                let app_state = state.blocking_lock();
                (
                    Arc::clone(&app_state.session_manager),
                    Arc::clone(&app_state.plugin_host),
                )
            };
            session_manager.start_health_monitor(app.handle().clone());

            {
                use tauri::Emitter;
                let (tx, mut rx) =
                    tokio::sync::mpsc::unbounded_channel::<plugins::runtime::NotifyEvent>();
                plugin_host.set_notify_sender(tx);
                let (log_tx, mut log_rx) =
                    tokio::sync::mpsc::unbounded_channel::<plugins::runtime::LogEvent>();
                plugin_host.set_log_sender(log_tx);
                // Sender wired before the reload so plugins loaded at startup
                // carry the log channel into their host services.
                plugin_host.reload();
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    while let Some(event) = rx.recv().await {
                        if let Err(e) = app_handle.emit("plugin-notify", &event) {
                            tracing::warn!(
                                error = %e,
                                "failed to emit plugin notify event"
                            );
                        }
                    }
                });
                let log_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    while let Some(event) = log_rx.recv().await {
                        if let Err(e) = log_handle.emit("plugin-log", &event) {
                            tracing::warn!(
                                error = %e,
                                "failed to emit plugin log event"
                            );
                        }
                    }
                });
            }

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
        .manage(write_registry)
        .manage(watcher_path_sender);

    #[cfg(feature = "pro")]
    let builder = builder.manage(instant_api);

    builder
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
            // Query result cache commands
            commands::cache::get_cache_config,
            commands::cache::set_cache_config,
            commands::cache::clear_query_cache,
            commands::cache::get_cache_stats,
            // Sandbox commands
            commands::sandbox::generate_migration_sql,
            commands::sandbox::apply_sandbox_changes,
            // Full-text search
            commands::fulltext_search::fulltext_search,
            // Confirmation tokens for destructive commands
            commands::confirmation::request_confirmation_token,
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
            // Backup / Restore commands
            commands::backup::detect_backup_tools,
            commands::backup::set_backup_tool_path,
            commands::backup::start_backup,
            commands::backup::start_restore,
            commands::backup::cancel_backup,
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
            // Data Contracts commands (Pro)
            #[cfg(feature = "pro")]
            commands::contracts::list_contracts,
            #[cfg(feature = "pro")]
            commands::contracts::load_contract,
            #[cfg(feature = "pro")]
            commands::contracts::save_contract,
            #[cfg(feature = "pro")]
            commands::contracts::delete_contract,
            #[cfg(feature = "pro")]
            commands::contracts::run_contract,
            #[cfg(feature = "pro")]
            commands::contracts::get_contract_history,
            // Instant Data API commands (Pro)
            #[cfg(feature = "pro")]
            commands::instant_api::start_instant_api,
            #[cfg(feature = "pro")]
            commands::instant_api::stop_instant_api,
            #[cfg(feature = "pro")]
            commands::instant_api::get_instant_api_status,
            #[cfg(feature = "pro")]
            commands::instant_api::list_endpoints,
            #[cfg(feature = "pro")]
            commands::instant_api::get_openapi_document,
            #[cfg(feature = "pro")]
            commands::instant_api::create_endpoint,
            #[cfg(feature = "pro")]
            commands::instant_api::regenerate_endpoint_token,
            #[cfg(feature = "pro")]
            commands::instant_api::delete_endpoint,
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
            // Plugin system commands
            commands::plugins::list_plugins,
            commands::plugins::install_plugin,
            commands::plugins::install_plugin_from_url,
            commands::plugins::fetch_marketplace_index,
            commands::plugins::remove_plugin,
            commands::plugins::set_plugin_enabled,
            commands::plugins::get_plugin_contributions,
            commands::plugins::get_plugin_consent,
            commands::plugins::get_plugin_statuses,
            commands::plugins::set_plugin_consent,
            commands::plugins::run_plugin_command,
            commands::plugins::list_provisioned_secrets,
            commands::plugins::set_plugin_secret,
            commands::plugins::delete_plugin_secret,
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

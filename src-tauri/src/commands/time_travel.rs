// SPDX-License-Identifier: BUSL-1.1

//! API surface for the Time-Travel feature: timeline, diff, rollback, config.

const PRO_REQUIRED: &str = "Data Time-Travel requires a Pro license.";

#[cfg(not(feature = "pro"))]
pub mod stubs {
    use super::PRO_REQUIRED;
    use tauri::State;

    #[tauri::command]
    pub async fn get_table_timeline(
        _state: State<'_, crate::SharedState>,
        _session_id: String,
        _database: String,
        _schema: Option<String>,
        _table_name: String,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn get_row_history(
        _state: State<'_, crate::SharedState>,
        _session_id: String,
        _database: String,
        _schema: Option<String>,
        _table_name: String,
        _primary_key: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn compute_temporal_diff(
        _state: State<'_, crate::SharedState>,
        _session_id: String,
        _database: String,
        _schema: Option<String>,
        _table_name: String,
        _timestamp_from: String,
        _timestamp_to: String,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn get_row_state_at(
        _state: State<'_, crate::SharedState>,
        _session_id: String,
        _database: String,
        _schema: Option<String>,
        _table_name: String,
        _primary_key: serde_json::Value,
        _timestamp: String,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn generate_rollback_sql(
        _state: State<'_, crate::SharedState>,
        _session_id: String,
        _database: String,
        _schema: Option<String>,
        _table_name: String,
        _target_timestamp: String,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn generate_entry_rollback_sql(
        _state: State<'_, crate::SharedState>,
        _session_id: String,
        _entry_id: String,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn get_time_travel_config(
        _state: State<'_, crate::SharedState>,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn update_time_travel_config(
        _state: State<'_, crate::SharedState>,
        _config: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn clear_table_changelog(
        _state: State<'_, crate::SharedState>,
        _session_id: String,
        _database: String,
        _schema: Option<String>,
        _table_name: String,
        _confirmation_token: String,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn clear_all_changelog(
        _state: State<'_, crate::SharedState>,
        _confirmation_token: String,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn export_changelog(
        _state: State<'_, crate::SharedState>,
        _session_id: String,
        _filter: serde_json::Value,
    ) -> Result<String, String> {
        Err(PRO_REQUIRED.to_string())
    }
}

#[cfg(feature = "pro")]
pub mod pro {
    use std::collections::HashMap;
    use std::sync::Arc;

    use chrono::DateTime;
    use serde::Serialize;
    use tauri::State;
    use tracing::instrument;
    use uuid::Uuid;

    use crate::commands::workspace::SharedWorkspaceManager;
    use crate::engine::types::Namespace;
    use crate::time_travel::rollback::generate_rollback_statements;
    use crate::time_travel::types::{
        ChangelogEntry, ChangelogFilter, ChangelogScope, TemporalDiff, TimeTravelConfig,
        TimelineEvent,
    };

    async fn run_history_io<T: Send + 'static>(
        operation: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        tokio::task::spawn_blocking(operation)
            .await
            .map_err(|error| format!("History operation failed: {error}"))?
    }

    async fn ensure_pro(state: &State<'_, crate::SharedState>) -> Result<(), String> {
        let state = state.lock().await;
        if state
            .license_manager
            .effective_status()
            .tier
            .includes(crate::license::status::LicenseTier::Pro)
        {
            Ok(())
        } else {
            Err(super::PRO_REQUIRED.to_string())
        }
    }

    fn ensure_session_workspace(origin: Option<&str>, active: &str) -> Result<(), String> {
        if origin == Some(active) {
            Ok(())
        } else {
            Err("Session does not belong to the active workspace".into())
        }
    }

    async fn resolve_scope(
        state: &State<'_, crate::SharedState>,
        ws_manager: &State<'_, SharedWorkspaceManager>,
        session_id: &str,
    ) -> Result<ChangelogScope, String> {
        let session = crate::commands::parse_session_id(session_id)?;
        let session_manager = {
            let state = state.lock().await;
            Arc::clone(&state.session_manager)
        };
        let driver = session_manager
            .get_driver(session)
            .await
            .map_err(|e| e.sanitized_message())?;
        let workspace_id = session_manager.workspace_id(session).await;
        let active_workspace = ws_manager.lock().await.project_id();
        ensure_session_workspace(workspace_id.as_deref(), &active_workspace)?;
        let connection_id = session_manager
            .get_saved_connection_identity(session)
            .await
            .map(|(id, _)| id);
        Ok(ChangelogScope {
            masking: session_manager
                .masking(session)
                .await
                .map(|masking| masking.config.clone()),
            session_id: session_id.to_string(),
            connection_id,
            workspace_id,
            driver_id: driver.driver_id().to_string(),
        })
    }

    #[derive(Serialize)]
    pub struct TimelineResponse {
        pub success: bool,
        pub events: Vec<TimelineEvent>,
        pub total_count: usize,
        pub error: Option<String>,
    }

    #[derive(Serialize)]
    pub struct RowHistoryResponse {
        pub success: bool,
        pub entries: Vec<ChangelogEntry>,
        pub error: Option<String>,
    }

    #[derive(Serialize)]
    pub struct TemporalDiffResponse {
        pub success: bool,
        pub diff: Option<TemporalDiff>,
        pub error: Option<String>,
    }

    #[derive(Serialize)]
    pub struct RowStateResponse {
        pub success: bool,
        pub state: Option<HashMap<String, serde_json::Value>>,
        pub exists: bool,
        pub error: Option<String>,
    }

    #[derive(Serialize)]
    pub struct RollbackSqlResponse {
        pub success: bool,
        pub sql: Option<String>,
        pub statements_count: usize,
        pub warnings: Vec<String>,
        pub error: Option<String>,
    }

    #[derive(Serialize)]
    pub struct TimeTravelConfigResponse {
        pub success: bool,
        pub config: TimeTravelConfig,
        pub error: Option<String>,
    }

    #[derive(Serialize)]
    pub struct GenericResponse {
        pub success: bool,
        pub error: Option<String>,
    }

    #[tauri::command]
    #[instrument(skip(state, ws_manager, primary_key_search))]
    pub async fn get_table_timeline(
        state: State<'_, crate::SharedState>,
        ws_manager: State<'_, SharedWorkspaceManager>,
        session_id: String,
        database: String,
        schema: Option<String>,
        table_name: String,
        from_timestamp: Option<String>,
        to_timestamp: Option<String>,
        operation: Option<String>,
        connection_name: Option<String>,
        environment: Option<String>,
        primary_key_search: Option<String>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<TimelineResponse, String> {
        ensure_pro(&state).await?;
        let scope = resolve_scope(&state, &ws_manager, &session_id).await?;
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        let namespace = Namespace { database, schema };
        let filter = ChangelogFilter {
            from_timestamp: from_timestamp
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc)),
            to_timestamp: to_timestamp
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc)),
            operation: operation.and_then(|s| serde_json::from_str(&format!("\"{}\"", s)).ok()),
            connection_name,
            environment,
            primary_key_search,
            limit,
            offset,
            ..Default::default()
        };

        run_history_io(move || {
            let (events, total_count) =
                changelog_store.get_timeline_page(&scope, &namespace, &table_name, &filter)?;

            Ok(TimelineResponse {
                success: true,
                events,
                total_count,
                error: None,
            })
        })
        .await
    }

    #[tauri::command]
    #[instrument(skip(state, ws_manager, primary_key))]
    pub async fn get_row_history(
        state: State<'_, crate::SharedState>,
        ws_manager: State<'_, SharedWorkspaceManager>,
        session_id: String,
        database: String,
        schema: Option<String>,
        table_name: String,
        primary_key: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
    ) -> Result<RowHistoryResponse, String> {
        ensure_pro(&state).await?;
        let scope = resolve_scope(&state, &ws_manager, &session_id).await?;
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        let namespace = Namespace { database, schema };
        run_history_io(move || {
            if !changelog_store.can_identify_row(&scope, &table_name, &primary_key) {
                return Err("Row history requires an available, unmasked primary key".into());
            }
            let entries = changelog_store.get_row_history(
                &scope,
                &namespace,
                &table_name,
                &primary_key,
                limit,
            )?;

            Ok(RowHistoryResponse {
                success: true,
                entries,
                error: None,
            })
        })
        .await
    }

    #[tauri::command]
    #[instrument(skip(state, ws_manager))]
    pub async fn compute_temporal_diff(
        state: State<'_, crate::SharedState>,
        ws_manager: State<'_, SharedWorkspaceManager>,
        session_id: String,
        database: String,
        schema: Option<String>,
        table_name: String,
        timestamp_from: String,
        timestamp_to: String,
        limit: Option<usize>,
    ) -> Result<TemporalDiffResponse, String> {
        ensure_pro(&state).await?;
        let scope = resolve_scope(&state, &ws_manager, &session_id).await?;
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        let namespace = Namespace { database, schema };
        let t1 = DateTime::parse_from_rfc3339(&timestamp_from)
            .map_err(|e| format!("Invalid timestamp_from: {}", e))?
            .with_timezone(&chrono::Utc);
        let t2 = DateTime::parse_from_rfc3339(&timestamp_to)
            .map_err(|e| format!("Invalid timestamp_to: {}", e))?
            .with_timezone(&chrono::Utc);

        run_history_io(move || {
            let diff = changelog_store.compute_temporal_diff(
                &scope,
                &namespace,
                &table_name,
                t1,
                t2,
                limit,
            )?;

            Ok(TemporalDiffResponse {
                success: true,
                diff: Some(diff),
                error: None,
            })
        })
        .await
    }

    #[tauri::command]
    #[instrument(skip(state, ws_manager, primary_key))]
    pub async fn get_row_state_at(
        state: State<'_, crate::SharedState>,
        ws_manager: State<'_, SharedWorkspaceManager>,
        session_id: String,
        database: String,
        schema: Option<String>,
        table_name: String,
        primary_key: HashMap<String, serde_json::Value>,
        timestamp: String,
    ) -> Result<RowStateResponse, String> {
        ensure_pro(&state).await?;
        let scope = resolve_scope(&state, &ws_manager, &session_id).await?;
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        let namespace = Namespace { database, schema };
        let ts = DateTime::parse_from_rfc3339(&timestamp)
            .map_err(|e| format!("Invalid timestamp: {}", e))?
            .with_timezone(&chrono::Utc);

        run_history_io(move || {
            if !changelog_store.can_identify_row(&scope, &table_name, &primary_key) {
                return Err("Row state requires an available, unmasked primary key".into());
            }
            let row_state = changelog_store.get_row_state_at(
                &scope,
                &namespace,
                &table_name,
                &primary_key,
                ts,
            )?;
            let exists = row_state.is_some();

            Ok(RowStateResponse {
                success: true,
                state: row_state,
                exists,
                error: None,
            })
        })
        .await
    }

    #[tauri::command]
    #[instrument(skip(state, ws_manager))]
    pub async fn generate_rollback_sql(
        state: State<'_, crate::SharedState>,
        ws_manager: State<'_, SharedWorkspaceManager>,
        session_id: String,
        database: String,
        schema: Option<String>,
        table_name: String,
        target_timestamp: String,
    ) -> Result<RollbackSqlResponse, String> {
        ensure_pro(&state).await?;
        let scope = resolve_scope(&state, &ws_manager, &session_id).await?;
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        let namespace = Namespace { database, schema };
        let target = DateTime::parse_from_rfc3339(&target_timestamp)
            .map_err(|e| format!("Invalid target_timestamp: {}", e))?
            .with_timezone(&chrono::Utc);

        run_history_io(move || {
            let entries =
                changelog_store.get_rollback_entries(&scope, &namespace, &table_name, target)?;

            if entries.is_empty() {
                return Ok(RollbackSqlResponse {
                    success: true,
                    sql: Some("-- No changes to rollback".to_string()),
                    statements_count: 0,
                    warnings: vec![],
                    error: None,
                });
            }

            let result = generate_rollback_statements(&entries, &scope.driver_id);

            Ok(RollbackSqlResponse {
                success: true,
                sql: Some(result.sql),
                statements_count: result.statements_count,
                warnings: result.warnings,
                error: None,
            })
        })
        .await
    }

    #[tauri::command]
    #[instrument(skip(state, ws_manager))]
    pub async fn generate_entry_rollback_sql(
        state: State<'_, crate::SharedState>,
        ws_manager: State<'_, SharedWorkspaceManager>,
        session_id: String,
        entry_id: String,
    ) -> Result<RollbackSqlResponse, String> {
        ensure_pro(&state).await?;
        let scope = resolve_scope(&state, &ws_manager, &session_id).await?;
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        let uuid = Uuid::parse_str(&entry_id).map_err(|e| format!("Invalid entry_id: {}", e))?;

        run_history_io(move || {
            let entry = changelog_store
                .get_entry(&scope, &uuid)?
                .ok_or_else(|| "Entry not found".to_string())?;

            let result = generate_rollback_statements(&[entry], &scope.driver_id);

            Ok(RollbackSqlResponse {
                success: true,
                sql: Some(result.sql),
                statements_count: result.statements_count,
                warnings: result.warnings,
                error: None,
            })
        })
        .await
    }

    #[tauri::command]
    pub async fn get_time_travel_config(
        state: State<'_, crate::SharedState>,
    ) -> Result<TimeTravelConfigResponse, String> {
        ensure_pro(&state).await?;
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        Ok(TimeTravelConfigResponse {
            success: true,
            config: changelog_store.get_config(),
            error: None,
        })
    }

    #[tauri::command]
    pub async fn update_time_travel_config(
        state: State<'_, crate::SharedState>,
        config: TimeTravelConfig,
    ) -> Result<TimeTravelConfigResponse, String> {
        ensure_pro(&state).await?;
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        run_history_io(move || {
            changelog_store.update_config(config)?;

            Ok(TimeTravelConfigResponse {
                success: true,
                config: changelog_store.get_config(),
                error: None,
            })
        })
        .await
    }

    /// Requires a one-shot confirmation token from
    /// `request_confirmation_token("clear_table_changelog")`.
    #[tauri::command]
    pub async fn clear_table_changelog(
        state: State<'_, crate::SharedState>,
        ws_manager: State<'_, SharedWorkspaceManager>,
        session_id: String,
        database: String,
        schema: Option<String>,
        table_name: String,
        confirmation_token: String,
    ) -> Result<GenericResponse, String> {
        ensure_pro(&state).await?;
        let scope = resolve_scope(&state, &ws_manager, &session_id).await?;
        let (changelog_store, confirmation_tokens) = {
            let state = state.lock().await;
            (
                Arc::clone(&state.changelog_store),
                Arc::clone(&state.confirmation_tokens),
            )
        };

        confirmation_tokens.consume_for_workspace(
            "clear_table_changelog",
            scope
                .workspace_id
                .as_deref()
                .ok_or("Missing workspace identity")?,
            &confirmation_token,
        )?;

        let namespace = Namespace { database, schema };
        run_history_io(move || {
            changelog_store.clear_table(&scope, &namespace, &table_name)?;
            tracing::warn!(
                database = %namespace.database,
                schema = ?namespace.schema,
                table = %table_name,
                "time-travel changelog cleared for table"
            );

            Ok(GenericResponse {
                success: true,
                error: None,
            })
        })
        .await
    }

    /// Requires a one-shot confirmation token from
    /// `request_confirmation_token("clear_all_changelog")`.
    #[tauri::command]
    pub async fn clear_all_changelog(
        state: State<'_, crate::SharedState>,
        ws_manager: State<'_, SharedWorkspaceManager>,
        confirmation_token: String,
    ) -> Result<GenericResponse, String> {
        ensure_pro(&state).await?;
        let (changelog_store, confirmation_tokens) = {
            let state = state.lock().await;
            (
                Arc::clone(&state.changelog_store),
                Arc::clone(&state.confirmation_tokens),
            )
        };

        let workspace_id = ws_manager.lock().await.project_id();
        confirmation_tokens.consume_for_workspace(
            "clear_all_changelog",
            &workspace_id,
            &confirmation_token,
        )?;
        run_history_io(move || {
            changelog_store.clear_workspace(&workspace_id)?;
            tracing::warn!("time-travel changelog cleared (active workspace)");

            Ok(GenericResponse {
                success: true,
                error: None,
            })
        })
        .await
    }

    #[tauri::command]
    pub async fn export_changelog(
        state: State<'_, crate::SharedState>,
        ws_manager: State<'_, SharedWorkspaceManager>,
        session_id: String,
        filter: ChangelogFilter,
    ) -> Result<String, String> {
        ensure_pro(&state).await?;
        let scope = resolve_scope(&state, &ws_manager, &session_id).await?;
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        run_history_io(move || changelog_store.export(&scope, &filter)).await
    }

    #[cfg(test)]
    mod workspace_tests {
        use super::ensure_session_workspace;

        #[test]
        fn history_requires_a_known_session_origin_equal_to_the_active_workspace() {
            assert!(ensure_session_workspace(Some("workspace-a"), "workspace-a").is_ok());
            assert!(ensure_session_workspace(Some("workspace-a"), "workspace-b").is_err());
            assert!(ensure_session_workspace(None, "workspace-a").is_err());
        }
    }
}

#[cfg(feature = "pro")]
pub use pro::*;

#[cfg(not(feature = "pro"))]
pub use stubs::*;

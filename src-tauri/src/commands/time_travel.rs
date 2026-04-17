// SPDX-License-Identifier: BUSL-1.1

//! Data Time-Travel Tauri Commands
//!
//! API surface for the Time-Travel feature: timeline, diff, rollback, config.

#[cfg(not(feature = "pro"))]
const PRO_REQUIRED: &str = "Data Time-Travel requires a Pro license.";

// ─── Pro stubs (non-pro builds) ────────────────────────────────────────────

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
        _database: String,
        _schema: Option<String>,
        _table_name: String,
        _target_timestamp: String,
        _driver_id: String,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn generate_entry_rollback_sql(
        _state: State<'_, crate::SharedState>,
        _entry_id: String,
        _driver_id: String,
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
        _database: String,
        _schema: Option<String>,
        _table_name: String,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn clear_all_changelog(
        _state: State<'_, crate::SharedState>,
    ) -> Result<serde_json::Value, String> {
        Err(PRO_REQUIRED.to_string())
    }

    #[tauri::command]
    pub async fn export_changelog(
        _state: State<'_, crate::SharedState>,
        _filter: serde_json::Value,
    ) -> Result<String, String> {
        Err(PRO_REQUIRED.to_string())
    }
}

// ─── Pro implementation ────────────────────────────────────────────────────

#[cfg(feature = "pro")]
pub mod pro {
    use std::collections::HashMap;
    use std::sync::Arc;

    use chrono::DateTime;
    use serde::Serialize;
    use tauri::State;
    use tracing::instrument;
    use uuid::Uuid;

    use crate::engine::types::Namespace;
    use crate::time_travel::rollback::generate_rollback_statements;
    use crate::time_travel::types::{
        ChangelogEntry, ChangelogFilter, TemporalDiff, TimelineEvent, TimeTravelConfig,
    };

    // ─── Response types ────────────────────────────────────────────────

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

    // ─── Timeline commands ─────────────────────────────────────────────

    #[tauri::command]
    #[instrument(skip(state))]
    pub async fn get_table_timeline(
        state: State<'_, crate::SharedState>,
        _session_id: String,
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

        let total_count = changelog_store.get_timeline_count(&namespace, &table_name);
        let events = changelog_store.get_timeline(&namespace, &table_name, &filter);

        Ok(TimelineResponse {
            success: true,
            events,
            total_count,
            error: None,
        })
    }

    #[tauri::command]
    #[instrument(skip(state))]
    pub async fn get_row_history(
        state: State<'_, crate::SharedState>,
        database: String,
        schema: Option<String>,
        table_name: String,
        primary_key: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
    ) -> Result<RowHistoryResponse, String> {
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        let namespace = Namespace { database, schema };
        let entries = changelog_store.get_row_history(&namespace, &table_name, &primary_key, limit);

        Ok(RowHistoryResponse {
            success: true,
            entries,
            error: None,
        })
    }

    // ─── Diff commands ─────────────────────────────────────────────────

    #[tauri::command]
    #[instrument(skip(state))]
    pub async fn compute_temporal_diff(
        state: State<'_, crate::SharedState>,
        database: String,
        schema: Option<String>,
        table_name: String,
        timestamp_from: String,
        timestamp_to: String,
        limit: Option<usize>,
    ) -> Result<TemporalDiffResponse, String> {
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

        let diff = changelog_store.compute_temporal_diff(&namespace, &table_name, t1, t2, limit);

        Ok(TemporalDiffResponse {
            success: true,
            diff: Some(diff),
            error: None,
        })
    }

    #[tauri::command]
    #[instrument(skip(state))]
    pub async fn get_row_state_at(
        state: State<'_, crate::SharedState>,
        database: String,
        schema: Option<String>,
        table_name: String,
        primary_key: HashMap<String, serde_json::Value>,
        timestamp: String,
    ) -> Result<RowStateResponse, String> {
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        let namespace = Namespace { database, schema };
        let ts = DateTime::parse_from_rfc3339(&timestamp)
            .map_err(|e| format!("Invalid timestamp: {}", e))?
            .with_timezone(&chrono::Utc);

        let row_state =
            changelog_store.get_row_state_at(&namespace, &table_name, &primary_key, ts);
        let exists = row_state.is_some();

        Ok(RowStateResponse {
            success: true,
            state: row_state,
            exists,
            error: None,
        })
    }

    // ─── Rollback commands ─────────────────────────────────────────────

    #[tauri::command]
    #[instrument(skip(state))]
    pub async fn generate_rollback_sql(
        state: State<'_, crate::SharedState>,
        database: String,
        schema: Option<String>,
        table_name: String,
        target_timestamp: String,
        driver_id: String,
    ) -> Result<RollbackSqlResponse, String> {
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        let namespace = Namespace { database, schema };
        let target = DateTime::parse_from_rfc3339(&target_timestamp)
            .map_err(|e| format!("Invalid target_timestamp: {}", e))?
            .with_timezone(&chrono::Utc);

        // Get all entries after the target timestamp for this table
        let filter = ChangelogFilter {
            table_name: Some(table_name.clone()),
            namespace: Some(namespace.clone()),
            from_timestamp: Some(target),
            limit: Some(10_000),
            ..Default::default()
        };
        let entries = changelog_store.get_entries(&filter);

        if entries.is_empty() {
            return Ok(RollbackSqlResponse {
                success: true,
                sql: Some("-- No changes to rollback".to_string()),
                statements_count: 0,
                warnings: vec![],
                error: None,
            });
        }

        let result = generate_rollback_statements(&entries, &driver_id);

        Ok(RollbackSqlResponse {
            success: true,
            sql: Some(result.sql),
            statements_count: result.statements_count,
            warnings: result.warnings,
            error: None,
        })
    }

    #[tauri::command]
    #[instrument(skip(state))]
    pub async fn generate_entry_rollback_sql(
        state: State<'_, crate::SharedState>,
        entry_id: String,
        driver_id: String,
    ) -> Result<RollbackSqlResponse, String> {
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        let uuid = Uuid::parse_str(&entry_id).map_err(|e| format!("Invalid entry_id: {}", e))?;

        let entry = changelog_store
            .get_entry(&uuid)
            .ok_or_else(|| "Entry not found".to_string())?;

        let result = generate_rollback_statements(&[entry], &driver_id);

        Ok(RollbackSqlResponse {
            success: true,
            sql: Some(result.sql),
            statements_count: result.statements_count,
            warnings: result.warnings,
            error: None,
        })
    }

    // ─── Config commands ───────────────────────────────────────────────

    #[tauri::command]
    pub async fn get_time_travel_config(
        state: State<'_, crate::SharedState>,
    ) -> Result<TimeTravelConfigResponse, String> {
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
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        changelog_store.update_config(config);

        Ok(TimeTravelConfigResponse {
            success: true,
            config: changelog_store.get_config(),
            error: None,
        })
    }

    // ─── Maintenance commands ──────────────────────────────────────────

    #[tauri::command]
    pub async fn clear_table_changelog(
        state: State<'_, crate::SharedState>,
        database: String,
        schema: Option<String>,
        table_name: String,
    ) -> Result<GenericResponse, String> {
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        let namespace = Namespace { database, schema };
        changelog_store.clear_table(&namespace, &table_name);

        Ok(GenericResponse {
            success: true,
            error: None,
        })
    }

    #[tauri::command]
    pub async fn clear_all_changelog(
        state: State<'_, crate::SharedState>,
    ) -> Result<GenericResponse, String> {
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        changelog_store.clear_all();

        Ok(GenericResponse {
            success: true,
            error: None,
        })
    }

    #[tauri::command]
    pub async fn export_changelog(
        state: State<'_, crate::SharedState>,
        filter: ChangelogFilter,
    ) -> Result<String, String> {
        let changelog_store = {
            let state = state.lock().await;
            Arc::clone(&state.changelog_store)
        };

        Ok(changelog_store.export(&filter))
    }
}

// ─── Re-exports (so lib.rs can reference commands uniformly) ───────────────

#[cfg(feature = "pro")]
pub use pro::*;

#[cfg(not(feature = "pro"))]
pub use stubs::*;

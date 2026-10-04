// SPDX-License-Identifier: Apache-2.0

//! Commands for generating migration SQL and applying sandbox changes.
//! Core batches are limited to five changes; the Sandbox UI requires Pro.

use serde::Serialize;
use tauri::State;

use crate::engine::sql_generator::SandboxChangeDto;

#[derive(Debug, Serialize)]
pub struct MigrationScriptResponse {
    pub success: bool,
    pub script: Option<crate::engine::sql_generator::MigrationScript>,
    pub error: Option<String>,
}

pub use qore_service::mutation::batch::ApplyBatchResult as ApplySandboxResponse;

// Always compiled. Core mode enforces a 5-change batch limit (covers
// Bulk Edit and other batched mutation flows). The Sandbox UI itself is
// still Pro-gated, so Core users only reach this limit through Bulk Edit.
//
// The check is **runtime**, not `#[cfg(feature = "pro")]`: a Pro build whose
// licence has expired or downgraded to Core must still enforce the cap, so
// we ask the in-memory `LicenseManager` rather than the compile-time feature
// flag (cf. audit B6-C5).

const CORE_SANDBOX_LIMIT: usize = 5;

/// True iff the in-memory licence currently grants Pro-tier features. Pulled
/// out as a helper because every entry point in this module needs the same
/// check before applying the Core batch cap.
async fn license_allows_unlimited_sandbox(state: &State<'_, crate::SharedState>) -> bool {
    let tier = {
        let guard = state.lock().await;
        guard.license_manager.effective_status().tier
    };
    tier.includes(crate::license::status::LicenseTier::Pro)
}

mod sandbox_impl {
    use super::*;
    use crate::engine::sql_generator::generate_migration_script;
    use crate::time_travel::capture::capture_confirmed_batch;
    use std::sync::Arc;
    use tracing::instrument;

    use crate::commands::parse_session_id;

    #[tauri::command]
    #[instrument(skip(state, changes), fields(session_id = %session_id))]
    pub async fn generate_migration_sql(
        state: State<'_, crate::SharedState>,
        session_id: String,
        changes: Vec<SandboxChangeDto>,
    ) -> Result<MigrationScriptResponse, String> {
        if changes.len() > CORE_SANDBOX_LIMIT
            && !super::license_allows_unlimited_sandbox(&state).await
        {
            return Ok(MigrationScriptResponse {
                success: false,
                script: None,
                error: Some(format!(
                    "Core edition is limited to {} changes per batch. Upgrade to QoreDB Pro for unlimited.",
                    CORE_SANDBOX_LIMIT
                )),
            });
        }

        let driver_id = {
            let state = state.lock().await;
            let session = parse_session_id(&session_id)?;
            match state.session_manager.get_driver(session).await {
                Ok(driver) => driver.driver_id().to_string(),
                Err(e) => {
                    return Ok(MigrationScriptResponse {
                        success: false,
                        script: None,
                        error: Some(format!("Failed to get driver: {}", e.sanitized_message())),
                    });
                }
            }
        };

        let script = generate_migration_script(&driver_id, &changes);
        Ok(MigrationScriptResponse {
            success: true,
            script: Some(script),
            error: None,
        })
    }

    #[tauri::command]
    #[instrument(skip(state, changes), fields(session_id = %session_id))]
    pub async fn apply_sandbox_changes(
        state: State<'_, crate::SharedState>,
        session_id: String,
        changes: Vec<SandboxChangeDto>,
        use_transaction: bool,
        acknowledged_dangerous: Option<bool>,
    ) -> Result<ApplySandboxResponse, String> {
        if changes.len() > CORE_SANDBOX_LIMIT
            && !super::license_allows_unlimited_sandbox(&state).await
        {
            return Ok(ApplySandboxResponse::rejected(format!(
                "Core edition is limited to {} changes per batch. Upgrade to QoreDB Pro for unlimited.",
                CORE_SANDBOX_LIMIT
            )));
        }

        let state_guard = state.lock().await;
        let session_manager = Arc::clone(&state_guard.session_manager);
        let changelog_store = Arc::clone(&state_guard.changelog_store);
        let interceptor = Arc::clone(&state_guard.interceptor);
        let query_cache = Arc::clone(&state_guard.query_cache);
        drop(state_guard);

        let session = parse_session_id(&session_id)?;
        let connection_identity = session_manager.get_saved_connection_identity(session).await;
        let workspace_id = session_manager.workspace_id(session).await;
        let driver = session_manager
            .get_driver(session)
            .await
            .map_err(|e| e.sanitized_message())?;
        let environment = session_manager
            .get_environment(session)
            .await
            .map_err(|e| e.sanitized_message())?;
        let capture_indices: Vec<_> = changes
            .iter()
            .enumerate()
            .filter(|(_, change)| changelog_store.should_capture(&change.table_name, &environment))
            .map(|(index, _)| index)
            .collect();
        let result = qore_service::mutation::batch::apply_batch_with_capture(
            &session_manager,
            &interceptor,
            &query_cache,
            session,
            &changes,
            use_transaction,
            acknowledged_dangerous.unwrap_or(false),
            &capture_indices,
        )
        .await;
        capture_confirmed_batch(
            &changelog_store,
            &session_id,
            driver.driver_id(),
            workspace_id.as_deref(),
            connection_identity.as_ref().map(|(id, _)| id.as_str()),
            connection_identity.as_ref().map(|(_, name)| name.as_str()),
            &environment,
            &changes,
            &result,
            session_manager
                .masking(session)
                .await
                .as_ref()
                .map(|masking| &masking.config),
        );
        Ok(result)
    }
}

pub use sandbox_impl::*;

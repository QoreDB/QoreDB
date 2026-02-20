// SPDX-License-Identifier: Apache-2.0

//! Sandbox Tauri Commands
//!
//! Commands for generating migration SQL and applying sandbox changes.
//! Sandbox is a Pro feature — Core builds return an explicit error.

use serde::Serialize;
use tauri::State;

use crate::engine::sql_generator::SandboxChangeDto;

/// Response for migration script generation
#[derive(Debug, Serialize)]
pub struct MigrationScriptResponse {
    pub success: bool,
    pub script: Option<crate::engine::sql_generator::MigrationScript>,
    pub error: Option<String>,
}

/// Response for applying sandbox changes
#[derive(Debug, Serialize)]
pub struct ApplySandboxResponse {
    pub success: bool,
    pub applied_count: usize,
    pub error: Option<String>,
    pub failed_changes: Vec<FailedChange>,
}

/// Information about a failed change
#[derive(Debug, Serialize)]
pub struct FailedChange {
    pub index: usize,
    pub error: String,
}

// ==================== Implementation ====================
// Always compiled. Core mode enforces a 3-change limit.

#[cfg(not(feature = "pro"))]
const CORE_SANDBOX_LIMIT: usize = 3;

mod sandbox_impl {
    use super::*;
    use crate::engine::sql_generator::{generate_migration_script, SandboxChangeType};
    use crate::engine::types::{RowData, SessionId};
    use std::sync::Arc;
    use tracing::instrument;
    use uuid::Uuid;

    fn parse_session_id(id: &str) -> Result<SessionId, String> {
        let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
        Ok(SessionId(uuid))
    }

    #[tauri::command]
    #[instrument(skip(state, changes), fields(session_id = %session_id))]
    pub async fn generate_migration_sql(
        state: State<'_, crate::SharedState>,
        session_id: String,
        changes: Vec<SandboxChangeDto>,
    ) -> Result<MigrationScriptResponse, String> {
        #[cfg(not(feature = "pro"))]
        if changes.len() > CORE_SANDBOX_LIMIT {
            return Ok(MigrationScriptResponse {
                success: false,
                script: None,
                error: Some(format!(
                    "Core edition is limited to {} sandbox changes. Upgrade to QoreDB Pro for unlimited.",
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
                        error: Some(format!("Failed to get driver: {}", e)),
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
    ) -> Result<ApplySandboxResponse, String> {
        #[cfg(not(feature = "pro"))]
        if changes.len() > CORE_SANDBOX_LIMIT {
            return Ok(ApplySandboxResponse {
                success: false,
                applied_count: 0,
                error: Some(format!(
                    "Core edition is limited to {} sandbox changes. Upgrade to QoreDB Pro for unlimited.",
                    CORE_SANDBOX_LIMIT
                )),
                failed_changes: vec![],
            });
        }

        let session_manager = {
            let state = state.lock().await;
            Arc::clone(&state.session_manager)
        };

        let session = parse_session_id(&session_id)?;

        if session_manager
            .is_read_only(session)
            .await
            .map_err(|e| e.to_string())?
        {
            return Ok(ApplySandboxResponse {
                success: false,
                applied_count: 0,
                error: Some("Operation blocked: read-only mode".to_string()),
                failed_changes: vec![],
            });
        }

        let driver = session_manager
            .get_driver(session)
            .await
            .map_err(|e| e.to_string())?;

        if !driver.capabilities().mutations {
            return Ok(ApplySandboxResponse {
                success: false,
                applied_count: 0,
                error: Some("Mutations are not supported by this driver".to_string()),
                failed_changes: vec![],
            });
        }

        let mut applied_count = 0;
        let mut failed_changes: Vec<super::FailedChange> = Vec::new();

        let supports_tx = driver.supports_transactions_for_session(session).await;
        if use_transaction && supports_tx {
            if let Err(e) = driver.begin_transaction(session).await {
                return Ok(ApplySandboxResponse {
                    success: false,
                    applied_count: 0,
                    error: Some(format!("Failed to begin transaction: {}", e)),
                    failed_changes: vec![],
                });
            }
        }

        for (idx, change) in changes.iter().enumerate() {
            let result = apply_single_change(&driver, session, change).await;
            match result {
                Ok(_) => applied_count += 1,
                Err(e) => {
                    failed_changes.push(super::FailedChange {
                        index: idx,
                        error: e.clone(),
                    });
                    if use_transaction && supports_tx {
                        if let Err(rb_err) = driver.rollback(session).await {
                            return Ok(ApplySandboxResponse {
                                success: false,
                                applied_count,
                                error: Some(format!(
                                    "Change {} failed: {}. Rollback also failed: {}",
                                    idx + 1,
                                    e,
                                    rb_err
                                )),
                                failed_changes,
                            });
                        }
                        return Ok(ApplySandboxResponse {
                            success: false,
                            applied_count,
                            error: Some(format!(
                                "Change {} failed: {}. Transaction rolled back.",
                                idx + 1,
                                e
                            )),
                            failed_changes,
                        });
                    }
                }
            }
        }

        if use_transaction && supports_tx && failed_changes.is_empty() {
            if let Err(e) = driver.commit(session).await {
                return Ok(ApplySandboxResponse {
                    success: false,
                    applied_count,
                    error: Some(format!("Failed to commit transaction: {}", e)),
                    failed_changes,
                });
            }
        }

        Ok(ApplySandboxResponse {
            success: failed_changes.is_empty(),
            applied_count,
            error: if failed_changes.is_empty() {
                None
            } else {
                Some(format!("{} change(s) failed", failed_changes.len()))
            },
            failed_changes,
        })
    }

    async fn apply_single_change(
        driver: &Arc<dyn crate::engine::traits::DataEngine>,
        session: SessionId,
        change: &SandboxChangeDto,
    ) -> Result<(), String> {
        match change.change_type {
            SandboxChangeType::Insert => {
                let new_values = change
                    .new_values
                    .as_ref()
                    .ok_or_else(|| "INSERT missing new_values".to_string())?;
                let data = RowData {
                    columns: new_values.clone(),
                };
                let result = driver
                    .insert_row(session, &change.namespace, &change.table_name, &data)
                    .await
                    .map_err(|e| e.to_string())?;
                if matches!(result.affected_rows, Some(0)) {
                    return Err("Insert affected 0 rows (possible conflict)".to_string());
                }
                Ok(())
            }
            SandboxChangeType::Update => {
                let pk = change
                    .primary_key
                    .as_ref()
                    .ok_or_else(|| "UPDATE missing primary_key".to_string())?;
                let new_values = change
                    .new_values
                    .as_ref()
                    .ok_or_else(|| "UPDATE missing new_values".to_string())?;
                let data = RowData {
                    columns: new_values.clone(),
                };
                let result = driver
                    .update_row(session, &change.namespace, &change.table_name, pk, &data)
                    .await
                    .map_err(|e| e.to_string())?;
                if matches!(result.affected_rows, Some(0)) {
                    return Err("Update affected 0 rows (possible conflict)".to_string());
                }
                Ok(())
            }
            SandboxChangeType::Delete => {
                let pk = change
                    .primary_key
                    .as_ref()
                    .ok_or_else(|| "DELETE missing primary_key".to_string())?;
                let result = driver
                    .delete_row(session, &change.namespace, &change.table_name, pk)
                    .await
                    .map_err(|e| e.to_string())?;
                if matches!(result.affected_rows, Some(0)) {
                    return Err("Delete affected 0 rows (possible conflict)".to_string());
                }
                Ok(())
            }
        }
    }
}

pub use sandbox_impl::*;

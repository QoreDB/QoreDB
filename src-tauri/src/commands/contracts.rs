// SPDX-License-Identifier: BUSL-1.1

//! Tauri commands for Data Contracts (Pro).
//!
//! Frontend usage:
//! ```ts
//! const contracts = await invoke('list_contracts');
//! const source = await invoke('load_contract', { name });
//! await invoke('save_contract', { name, source });
//! await invoke('delete_contract', { name });
//! const run = await invoke('run_contract', { sessionId, source });
//! window.listen('contract.run', (e) => …);
//! const history = await invoke('get_contract_history', { name, limit: 20 });
//! ```

#![cfg(feature = "pro")]

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::commands::workspace::SharedWorkspaceManager;
use crate::contracts::events::{CONTRACT_RUN_EVENT, ContractEventSink, ContractRunEvent};
use crate::contracts::parser::{Format, parse_contract};
use crate::contracts::runner::{RunOptions, RunnerError, run_contract as run_contract_inner};
use crate::contracts::storage;
use crate::contracts::{ContractMeta, ContractRun};

use super::parse_session_id;

/// Lock the workspace manager and clone the active `.qoredb/` path.
async fn active_workspace_path(
    ws_manager: &State<'_, SharedWorkspaceManager>,
) -> std::path::PathBuf {
    let mgr = ws_manager.lock().await;
    mgr.active().path.clone()
}

#[tauri::command]
pub async fn list_contracts(
    ws_manager: State<'_, SharedWorkspaceManager>,
) -> Result<Vec<ContractMeta>, String> {
    let root = active_workspace_path(&ws_manager).await;
    storage::list_contracts(&root).map_err(|e| e.to_string())
}

/// Reads the raw YAML source for a contract by canonical name. The frontend
/// editor uses this so it can round-trip the on-disk content.
#[tauri::command]
pub async fn load_contract(
    ws_manager: State<'_, SharedWorkspaceManager>,
    name: String,
) -> Result<String, String> {
    let root = active_workspace_path(&ws_manager).await;
    storage::load_contract_source(&root, &name).map_err(|e| e.to_string())
}

/// Validates and writes a contract YAML to disk. The embedded `name:` must
/// match `name` (the filename) or the call is rejected.
#[tauri::command]
pub async fn save_contract(
    ws_manager: State<'_, SharedWorkspaceManager>,
    name: String,
    source: String,
) -> Result<(), String> {
    let root = active_workspace_path(&ws_manager).await;
    storage::save_contract_source(&root, &name, &source)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Deletes a contract YAML and its persisted run history.
#[tauri::command]
pub async fn delete_contract(
    ws_manager: State<'_, SharedWorkspaceManager>,
    name: String,
) -> Result<(), String> {
    let root = active_workspace_path(&ws_manager).await;
    storage::delete_contract(&root, &name).map_err(|e| e.to_string())
}

/// Streams progress over the `contract.run` Tauri topic while it runs, then
/// returns the aggregated [`ContractRun`]. The run is also appended to the
/// contract's history JSONL.
#[tauri::command]
pub async fn run_contract(
    app: AppHandle,
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    session_id: String,
    source: String,
    connection_id: Option<String>,
) -> Result<ContractRun, String> {
    let contract = parse_contract(&source, Format::Auto).map_err(|e| e.to_string())?;
    let session = parse_session_id(&session_id)?;

    let session_manager = {
        let st = state.lock().await;
        Arc::clone(&st.session_manager)
    };
    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;

    let identity = session_manager.get_saved_connection_identity(session).await;
    let session_display_name = if identity.is_none() {
        session_manager.get_session_info(session).await
    } else {
        None
    };
    let connection_id = resolve_target_connection(
        &contract.target.connection,
        identity.as_ref(),
        connection_id.as_deref(),
        session_display_name.as_deref(),
    )?;

    let sink = TauriContractSink { app: app.clone() };
    let root = active_workspace_path(&ws_manager).await;

    let run = run_contract_inner(
        driver,
        session,
        connection_id,
        &contract,
        RunOptions::default(),
        &sink,
    )
    .await
    .map_err(|e| match e {
        RunnerError::UnknownDialect(d) => {
            format!("Driver '{d}' is not supported by Data Contracts")
        }
    })?;

    if let Err(err) = storage::append_run(&root, &contract.name, &run) {
        tracing::warn!("failed to append contract run history: {err}");
    }

    Ok(run)
}

/// Resolves the authoritative saved-connection id and verifies that the
/// contract target names that exact connection. For compatibility with early
/// contract files, the saved connection's display name is accepted too.
fn resolve_target_connection(
    target: &str,
    identity: Option<&(String, String)>,
    supplied_connection_id: Option<&str>,
    session_display_name: Option<&str>,
) -> Result<String, String> {
    if let Some((connection_id, display_name)) = identity {
        if target == connection_id || target == display_name {
            return Ok(connection_id.clone());
        }
        return Err(format!(
            "Contract targets connection '{target}', but the active session is '{display_name}' (id '{connection_id}')"
        ));
    }

    // Direct connections only exist in debug builds. They do not carry a
    // persisted identity, so use the explicit frontend id as a compatibility
    // fallback while still checking it against the contract target.
    if let Some(connection_id) = supplied_connection_id {
        if target == connection_id || session_display_name == Some(target) {
            return Ok(connection_id.to_string());
        }
        return Err(format!(
            "Contract targets connection '{target}', but the active connection id is '{connection_id}'"
        ));
    }

    Err(format!(
        "Cannot verify contract target '{target}': the active session is not associated with a saved connection"
    ))
}

/// Returns the most recent runs for a contract (oldest → newest). Pass `None`
/// for `limit` to get everything (capped by the rotation policy at ~200).
#[tauri::command]
pub async fn get_contract_history(
    ws_manager: State<'_, SharedWorkspaceManager>,
    name: String,
    limit: Option<u32>,
) -> Result<Vec<ContractRun>, String> {
    let root = active_workspace_path(&ws_manager).await;
    let limit = limit.map(|n| n as usize);
    storage::read_history(&root, &name, limit).map_err(|e| e.to_string())
}

/// Sink that fans `ContractRunEvent`s out to the renderer via Tauri.
struct TauriContractSink {
    app: AppHandle,
}

impl ContractEventSink for TauriContractSink {
    fn emit(&self, event: ContractRunEvent) {
        let _ = self.app.emit(CONTRACT_RUN_EVENT, event);
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_target_connection;

    #[test]
    fn accepts_authoritative_saved_connection_id() {
        let identity = ("conn_123".to_string(), "Production".to_string());
        let resolved = resolve_target_connection("conn_123", Some(&identity), None, None).unwrap();
        assert_eq!(resolved, "conn_123");
    }

    #[test]
    fn accepts_legacy_display_name_but_returns_stable_id() {
        let identity = ("conn_123".to_string(), "Production".to_string());
        let resolved =
            resolve_target_connection("Production", Some(&identity), None, None).unwrap();
        assert_eq!(resolved, "conn_123");
    }

    #[test]
    fn rejects_mismatched_authoritative_connection_even_if_supplied_id_is_spoofed() {
        let identity = ("conn_prod".to_string(), "Production".to_string());
        let err =
            resolve_target_connection("conn_staging", Some(&identity), Some("conn_staging"), None)
                .unwrap_err();
        assert!(err.contains("active session"));
    }

    #[test]
    fn rejects_unidentified_session_without_explicit_fallback() {
        let err =
            resolve_target_connection("conn_prod", None, None, Some("user@host:db")).unwrap_err();
        assert!(err.contains("not associated with a saved connection"));
    }
}

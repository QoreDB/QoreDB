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
use uuid::Uuid;

use crate::commands::workspace::SharedWorkspaceManager;
use crate::contracts::events::{ContractEventSink, ContractRunEvent, CONTRACT_RUN_EVENT};
use crate::contracts::parser::{parse_contract, Format};
use crate::contracts::runner::{run_contract as run_contract_inner, RunOptions, RunnerError};
use crate::contracts::storage;
use crate::contracts::{ContractMeta, ContractRun};
use qore_core::types::SessionId;

/// Resolves a session id string into a typed [`SessionId`].
fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

/// Lock the workspace manager and clone the active `.qoredb/` path.
async fn active_workspace_path(
    ws_manager: &State<'_, SharedWorkspaceManager>,
) -> std::path::PathBuf {
    let mgr = ws_manager.lock().await;
    mgr.active().path.clone()
}

/// Index every contract YAML under the active workspace.
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

    let connection_id = match connection_id {
        Some(id) => id,
        None => session_manager
            .get_session_info(session)
            .await
            .unwrap_or_else(|| session_id.clone()),
    };

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

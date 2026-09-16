// SPDX-License-Identifier: BUSL-1.1

//! Tauri surface of the Query Replay Lab.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::instrument;

use super::parse_session_id;
use crate::commands::workspace::SharedWorkspaceManager;
use crate::engine::types::QueryResult;
use crate::replay::compare::rebase_expectations;
use crate::replay::digest::compute_digest;
use crate::replay::runner::{ReplayServices, run_ab, run_set};
use crate::replay::types::{
    CaptureMode, DEFAULT_MAX_CAPTURED_ROWS, ReplayAbReport, ReplayReport, ReplayRunOptions,
    ReplaySet, ReplaySetSummary, RunMeta,
};
use crate::replay::{
    CaptureStore, Recorder, RecordingOptions, RecordingStatus, ReplaySetStore, SecretPolicy,
    slugify,
};
use crate::workspace::write_registry::WriteRegistry;

const REQUIRES_PRO: &str = "The Query Replay Lab requires a Pro license.";

pub struct ReplayState {
    pub recorder: Arc<Recorder>,
    data_dir: std::path::PathBuf,
    cancel: AtomicBool,
    running: AtomicBool,
}

impl ReplayState {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        Self {
            recorder: Arc::new(Recorder::new()),
            data_dir,
            cancel: AtomicBool::new(false),
            running: AtomicBool::new(false),
        }
    }

    /// Captures live under the active workspace: two projects holding a set
    /// under the same slug must not see — or delete — each other's runs.
    pub fn captures(&self, project_id: &str) -> Result<CaptureStore, String> {
        CaptureStore::scoped(&self.data_dir, project_id)
    }

    /// The store the live recording writes to, or `None` when nothing is
    /// recording. Resolved from the recording's own workspace, so switching
    /// workspaces mid-recording cannot scatter its captures.
    pub fn captures_for_recording(&self) -> Option<CaptureStore> {
        let project = self.recorder.project_id()?;
        self.captures(&project).ok()
    }
}

pub type SharedReplayState = Arc<ReplayState>;

async fn replay_state(state: &State<'_, crate::SharedState>) -> SharedReplayState {
    Arc::clone(&state.lock().await.replay)
}

#[cfg(feature = "pro")]
async fn license_allows_pro(state: &State<'_, crate::SharedState>) -> bool {
    let tier = {
        let guard = state.lock().await;
        guard.license_manager.effective_status().tier
    };
    tier.includes(crate::license::status::LicenseTier::Pro)
}

#[cfg(not(feature = "pro"))]
async fn license_allows_pro(_state: &State<'_, crate::SharedState>) -> bool {
    false
}

async fn set_store(ws_manager: &State<'_, SharedWorkspaceManager>) -> ReplaySetStore {
    let mgr = ws_manager.lock().await;
    ReplaySetStore::new(&mgr.active().path)
}

async fn project_id(ws_manager: &State<'_, SharedWorkspaceManager>) -> String {
    ws_manager.lock().await.project_id()
}

/// The captures of the active workspace.
async fn captures(
    state: &State<'_, crate::SharedState>,
    ws_manager: &State<'_, SharedWorkspaceManager>,
) -> Result<CaptureStore, String> {
    let project = project_id(ws_manager).await;
    replay_state(state).await.captures(&project)
}

#[derive(Debug, Deserialize)]
pub struct StartRecordingRequest {
    pub session_id: String,
    pub name: String,
    #[serde(default)]
    pub ignored_columns: Vec<String>,
    /// Off by default: a migration run while the recording is live would
    /// otherwise be captured as part of the set.
    #[serde(default)]
    pub record_mutations: bool,
    #[serde(default)]
    pub capture_mode: CaptureMode,
    /// Capturing values from a production connection is an explicit choice.
    #[serde(default)]
    pub allow_production_capture: bool,
    #[serde(default)]
    pub max_captured_rows: Option<usize>,
    #[serde(default)]
    pub capture_budget_bytes: Option<u64>,
    /// How the recording treats query text that looks like a credential.
    #[serde(default)]
    pub secret_policy: SecretPolicy,
}

#[derive(Debug, Serialize)]
pub struct RecordedPreview {
    pub order: u32,
    pub query_preview: String,
    pub is_mutation: bool,
    /// The query text looks like it carries a credential, and the set is
    /// versioned: worth dropping before sharing.
    pub looks_like_secret: bool,
}

#[tauri::command]
#[instrument(skip(state, ws_manager, request), fields(name = %request.name))]
pub async fn replay_start_recording(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    request: StartRecordingRequest,
) -> Result<RecordingStatus, String> {
    if !license_allows_pro(&state).await {
        return Err(REQUIRES_PRO.to_string());
    }

    let session = parse_session_id(&request.session_id)?;
    let (session_manager, replay) = {
        let guard = state.lock().await;
        (
            Arc::clone(&guard.session_manager),
            Arc::clone(&guard.replay),
        )
    };

    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;
    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let connection_label = session_manager.connection_key(session).await;
    let (project, workspace_path) = {
        let mgr = ws_manager.lock().await;
        (mgr.project_id(), mgr.active().path.clone())
    };
    // One place governs both the audit log and what a set flags.
    let secret_patterns = {
        let guard = state.lock().await;
        guard.interceptor.get_config().redaction_patterns
    };

    let defaults = ReplayRunOptions::default();
    replay.recorder.start(
        RecordingOptions {
            name: request.name,
            session_id: request.session_id.clone(),
            project_id: project,
            workspace_path,
            ignored_columns: request.ignored_columns,
            record_mutations: request.record_mutations,
            capture_mode: request.capture_mode,
            max_captured_rows: request
                .max_captured_rows
                .unwrap_or(defaults.max_captured_rows),
            capture_budget_bytes: request
                .capture_budget_bytes
                .unwrap_or(defaults.capture_budget_bytes),
            secret_policy: request.secret_policy,
            secret_patterns,
        },
        driver.driver_id().to_string(),
        connection_label,
        environment,
        request.allow_production_capture,
    )
}

#[tauri::command]
pub async fn replay_recording_status(
    state: State<'_, crate::SharedState>,
) -> Result<Option<RecordingStatus>, String> {
    Ok(replay_state(&state).await.recorder.status())
}

#[tauri::command]
pub async fn replay_recorded_previews(
    state: State<'_, crate::SharedState>,
) -> Result<Vec<RecordedPreview>, String> {
    Ok(replay_state(&state)
        .await
        .recorder
        .recorded_previews()
        .into_iter()
        .map(
            |(order, query_preview, is_mutation, looks_like_secret)| RecordedPreview {
                order,
                query_preview,
                is_mutation,
                looks_like_secret,
            },
        )
        .collect())
}

#[tauri::command]
pub async fn replay_discard_recorded(
    state: State<'_, crate::SharedState>,
    index: usize,
) -> Result<(), String> {
    let replay = replay_state(&state).await;
    let captures = replay
        .captures_for_recording()
        .ok_or_else(|| "No recording in progress".to_string())?;
    replay.recorder.discard_preview(index, &captures)
}

/// Drops every mutation recorded so far, and the rows captured for them.
#[tauri::command]
pub async fn replay_discard_mutations(
    state: State<'_, crate::SharedState>,
) -> Result<usize, String> {
    let replay = replay_state(&state).await;
    let captures = replay
        .captures_for_recording()
        .ok_or_else(|| "No recording in progress".to_string())?;
    replay.recorder.discard_mutations(&captures)
}

#[tauri::command]
pub async fn replay_cancel_recording(state: State<'_, crate::SharedState>) -> Result<(), String> {
    let replay = replay_state(&state).await;
    // The recording's own workspace, not the active one: the user may have
    // switched in the meantime.
    let project = replay.recorder.project_id();
    if let Some(run_id) = replay.recorder.cancel()
        && let Some(captures) = project.and_then(|p| replay.captures(&p).ok())
    {
        let _ = captures.delete_run(&run_id);
    }
    Ok(())
}

/// Ends the recording, writes the set to `.qoredb/replays/` and keeps its
/// captured rows as the baseline run.
#[tauri::command]
#[instrument(skip(state, write_registry))]
pub async fn replay_stop_recording(
    state: State<'_, crate::SharedState>,
    write_registry: State<'_, WriteRegistry>,
    slug: Option<String>,
) -> Result<ReplaySetSummary, String> {
    let replay = replay_state(&state).await;
    let captures = replay
        .captures_for_recording()
        .ok_or_else(|| "No recording in progress".to_string())?;

    // Everything that can refuse the save is checked before the recording is
    // consumed: taking the entries out of the recorder is irreversible, and a
    // name clash must not cost the user their session.
    let (workspace_path, name) = replay
        .recorder
        .destination()
        .ok_or_else(|| "No recording in progress".to_string())?;

    if replay.recorder.entry_count() == 0 {
        return Err("Nothing was recorded".to_string());
    }

    let slug = match slug {
        Some(slug) => {
            crate::replay::validate_slug(&slug)?;
            slug
        }
        None => slugify(&name)?,
    };

    // The set belongs to the workspace the recording started in, next to the
    // captures it was recorded with — not to whichever one is active now.
    let store = ReplaySetStore::new(&workspace_path);
    let path = store.path_for(&slug)?;
    if path.exists() {
        return Err(format!("A replay set named '{slug}' already exists"));
    }

    let (set, mut run) = replay
        .recorder
        .stop()
        .ok_or_else(|| "No recording in progress".to_string())?;

    write_registry.register_with_auto_unregister(path);
    if let Err(e) = store.create(&slug, &set) {
        let _ = captures.delete_run(&run.run_id);
        return Err(e);
    }

    run.set_slug = slug.clone();
    captures.save_run_meta(&run)?;

    Ok(ReplaySetSummary {
        slug,
        name: set.name,
        created_at: set.created_at,
        driver_id: set.source.driver_id,
        environment: set.source.environment,
        entry_count: set.entries.len(),
        redacted: set.redacted,
    })
}

#[tauri::command]
pub async fn replay_list_sets(
    ws_manager: State<'_, SharedWorkspaceManager>,
) -> Result<Vec<ReplaySetSummary>, String> {
    set_store(&ws_manager).await.list()
}

#[tauri::command]
pub async fn replay_load_set(
    ws_manager: State<'_, SharedWorkspaceManager>,
    slug: String,
) -> Result<ReplaySet, String> {
    set_store(&ws_manager).await.load(&slug)
}

#[tauri::command]
pub async fn replay_delete_set(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    slug: String,
) -> Result<(), String> {
    let captures = captures(&state, &ws_manager).await?;
    for run in captures.list_runs(&slug)? {
        let _ = captures.delete_run(&run.run_id);
    }
    set_store(&ws_manager).await.delete(&slug)?;
    Ok(())
}

/// Ignored columns live on the set, so the exclusion travels with it in Git.
#[tauri::command]
pub async fn replay_set_ignored_columns(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    write_registry: State<'_, WriteRegistry>,
    slug: String,
    columns: Vec<String>,
) -> Result<ReplaySet, String> {
    let store = set_store(&ws_manager).await;
    let mut set = store.load(&slug)?;
    set.ignored_columns = columns;

    // The recorded digests were computed with the old column list. Left as
    // they are, an unchanged query would come back as a content difference —
    // so they are recomputed from the baseline capture, or dropped when there
    // is none to recompute from.
    let captures = captures(&state, &ws_manager).await?;
    let baseline = captures
        .list_runs(&slug)?
        .into_iter()
        .find(|run| run.is_baseline);
    let ignored = set.ignored_columns.clone();
    for entry in &mut set.entries {
        entry.expected.result_digest = baseline.as_ref().and_then(|run| {
            let snapshot = captures.load_entry(&run.run_id, &entry.id).ok()?;

            // A bounded capture holds the first N rows of the engine's output,
            // while the digest hashes the first N rows *after* sorting. The two
            // sets differ as soon as the result was larger than the bound, so
            // the original digest cannot be rebuilt from what was kept —
            // pretending otherwise would report a regression on an unchanged
            // query. Drop the digest instead and fall back to metadata.
            let captured_all = entry
                .expected
                .row_count
                .is_none_or(|total| total as usize <= snapshot.rows.len());
            if !captured_all {
                return None;
            }

            Some(
                compute_digest(
                    &snapshot.to_query_result(),
                    &ignored,
                    DEFAULT_MAX_CAPTURED_ROWS,
                )
                .digest,
            )
        });
    }

    let path = store.path_for(&slug)?;
    write_registry.register_with_auto_unregister(path);
    store.save(&slug, &set)?;
    Ok(set)
}

#[derive(Debug, Deserialize)]
pub struct AcceptRunRequest {
    pub slug: String,
    pub run_id: String,
    /// Entries to accept. `None` accepts every entry the run executed.
    #[serde(default)]
    pub entry_ids: Option<Vec<String>>,
}

/// Promotes what a run observed to the set's expectation, so a deliberate
/// migration stops being reported as a regression on every later replay.
///
/// The rows captured by the run replace the baseline's for those entries: a
/// report that says "identical" must not sit next to a diff of the old ones.
#[tauri::command]
#[instrument(skip(state, ws_manager, write_registry, request), fields(slug = %request.slug))]
pub async fn replay_accept_run(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    write_registry: State<'_, WriteRegistry>,
    request: AcceptRunRequest,
) -> Result<ReplaySet, String> {
    if !license_allows_pro(&state).await {
        return Err(REQUIRES_PRO.to_string());
    }

    let captures = captures(&state, &ws_manager).await?;
    let report = captures.load_report(&request.run_id)?;
    if report.run.set_slug != request.slug {
        return Err("That run belongs to another replay set".to_string());
    }

    let store = set_store(&ws_manager).await;
    let mut set = store.load(&request.slug)?;

    let observed: std::collections::HashMap<&str, &crate::replay::types::ReplayEntryResult> =
        report.results.iter().map(|r| (r.entry_id.as_str(), r)).collect();
    let wanted = request.entry_ids.as_ref();
    let baseline = captures
        .list_runs(&request.slug)?
        .into_iter()
        .find(|run| run.is_baseline)
        .map(|run| run.run_id);

    let mut accepted = 0usize;
    for entry in &mut set.entries {
        if wanted.is_some_and(|ids| !ids.contains(&entry.id)) {
            continue;
        }
        let Some(result) = observed.get(entry.id.as_str()) else {
            continue;
        };
        // An entry the run never executed has nothing to promote.
        if result.verdict == crate::replay::types::ReplayVerdict::Skipped {
            continue;
        }
        entry.expected.execution_time_ms = result.execution_time_ms;
        entry.expected.row_count = result.row_count;
        entry.expected.success = result.success;
        entry.expected.result_digest = result.digest.clone();
        if let Some(baseline_run) = baseline.as_deref()
            && baseline_run != request.run_id
        {
            let _ = captures.adopt_entry(&request.run_id, baseline_run, &entry.id);
        }
        accepted += 1;
    }

    if accepted == 0 {
        return Err("That run has nothing to accept for this set".to_string());
    }

    let path = store.path_for(&request.slug)?;
    write_registry.register_with_auto_unregister(path);
    store.save(&request.slug, &set)?;
    Ok(set)
}

#[tauri::command]
pub async fn replay_list_runs(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    slug: String,
) -> Result<Vec<RunMeta>, String> {
    captures(&state, &ws_manager).await?.list_runs(&slug)
}

/// Rows captured for one entry of one run, in the shape `DataDiffViewer` reads.
#[tauri::command]
pub async fn replay_load_capture(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    run_id: String,
    entry_id: String,
) -> Result<QueryResult, String> {
    Ok(captures(&state, &ws_manager)
        .await?
        .load_entry(&run_id, &entry_id)?
        .to_query_result())
}

/// The most recent report for a set, so reopening the tab shows the last run
/// instead of an empty panel. An A/B comparison is returned as such: it is not
/// a run against the recording and must not be shown as one.
#[derive(Debug, Serialize)]
pub struct LastReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<ReplayReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ab: Option<ReplayAbReport>,
}

#[tauri::command]
pub async fn replay_last_report(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    slug: String,
) -> Result<LastReport, String> {
    let captures = captures(&state, &ws_manager).await?;
    let ab = captures.latest_ab_report(&slug)?;
    Ok(LastReport {
        report: if ab.is_some() {
            None
        } else {
            captures.latest_report(&slug)?
        },
        ab,
    })
}

#[derive(Debug, Deserialize)]
pub struct RunReplayRequest {
    pub session_id: String,
    pub slug: String,
    #[serde(default)]
    pub options: ReplayRunOptions,
    /// Run whose captured rows the report diffs against; defaults to the
    /// set's baseline.
    #[serde(default)]
    pub baseline_run_id: Option<String>,
}

#[tauri::command]
#[instrument(skip(state, ws_manager, app, request), fields(slug = %request.slug))]
pub async fn replay_run(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    app: tauri::AppHandle,
    request: RunReplayRequest,
) -> Result<ReplayReport, String> {
    if !license_allows_pro(&state).await {
        return Err(REQUIRES_PRO.to_string());
    }

    let replay = replay_state(&state).await;
    if replay
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("A replay is already running".to_string());
    }
    replay.cancel.store(false, Ordering::SeqCst);

    let outcome = run_replay_inner(&state, &replay, &ws_manager, &app, request).await;
    replay.running.store(false, Ordering::SeqCst);
    outcome
}

async fn run_replay_inner(
    state: &State<'_, crate::SharedState>,
    replay: &SharedReplayState,
    ws_manager: &State<'_, SharedWorkspaceManager>,
    app: &tauri::AppHandle,
    request: RunReplayRequest,
) -> Result<ReplayReport, String> {
    let set = set_store(ws_manager).await.load(&request.slug)?;
    let project = project_id(ws_manager).await;
    let captures = replay.captures(&project)?;

    let runs = captures.list_runs(&request.slug)?;
    let recording_run = runs
        .iter()
        .find(|run| run.is_baseline)
        .map(|r| r.run_id.clone());
    let baseline_run_id = request.baseline_run_id.clone().or(recording_run.clone());

    // Comparing against a previous run means classifying against what that run
    // observed, not against the original recording — otherwise the table would
    // say "identical" while the diff shows two different runs.
    let set = match request.baseline_run_id.as_deref() {
        Some(chosen) if Some(chosen) != recording_run.as_deref() => {
            match captures.load_report(chosen) {
                Ok(report) => rebase_expectations(&set, &report),
                Err(e) => return Err(format!("Cannot compare to that run: {e}")),
            }
        }
        _ => set,
    };

    let session = parse_session_id(&request.session_id)?;
    let (session_manager, query_manager, query_rate_limiter, query_cache, interceptor, policy) = {
        let guard = state.lock().await;
        (
            Arc::clone(&guard.session_manager),
            Arc::clone(&guard.query_manager),
            Arc::clone(&guard.query_rate_limiter),
            Arc::clone(&guard.query_cache),
            Arc::clone(&guard.interceptor),
            guard.policy.clone(),
        )
    };

    let app_handle = app.clone();
    run_set(
        ReplayServices {
            session_manager: &session_manager,
            query_manager: &query_manager,
            query_rate_limiter: &query_rate_limiter,
            query_cache: &query_cache,
            interceptor: &interceptor,
            policy: &policy,
        },
        session,
        &request.session_id,
        &set,
        &request.slug,
        &project,
        &request.options,
        &captures,
        baseline_run_id,
        &Default::default(),
        &replay.cancel,
        |progress| crate::emit_gate::emit_gated(&app_handle, "replay-progress", &progress),
    )
    .await
}

#[derive(Debug, Deserialize)]
pub struct RunAbRequest {
    pub left_session_id: String,
    pub right_session_id: String,
    pub slug: String,
    #[serde(default)]
    pub options: ReplayRunOptions,
}

/// Replays one set on two connections and compares the two live results.
#[tauri::command]
#[instrument(skip(state, ws_manager, app, request), fields(slug = %request.slug))]
pub async fn replay_run_ab(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    app: tauri::AppHandle,
    request: RunAbRequest,
) -> Result<ReplayAbReport, String> {
    if !license_allows_pro(&state).await {
        return Err(REQUIRES_PRO.to_string());
    }

    let replay = replay_state(&state).await;
    if replay
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("A replay is already running".to_string());
    }
    replay.cancel.store(false, Ordering::SeqCst);

    let outcome = run_ab_inner(&state, &replay, &ws_manager, &app, request).await;
    replay.running.store(false, Ordering::SeqCst);
    outcome
}

async fn run_ab_inner(
    state: &State<'_, crate::SharedState>,
    replay: &SharedReplayState,
    ws_manager: &State<'_, SharedWorkspaceManager>,
    app: &tauri::AppHandle,
    request: RunAbRequest,
) -> Result<ReplayAbReport, String> {
    let set = set_store(ws_manager).await.load(&request.slug)?;
    let project = project_id(ws_manager).await;
    let captures = replay.captures(&project)?;
    let left = parse_session_id(&request.left_session_id)?;
    let right = parse_session_id(&request.right_session_id)?;

    let (session_manager, query_manager, query_rate_limiter, query_cache, interceptor, policy) = {
        let guard = state.lock().await;
        (
            Arc::clone(&guard.session_manager),
            Arc::clone(&guard.query_manager),
            Arc::clone(&guard.query_rate_limiter),
            Arc::clone(&guard.query_cache),
            Arc::clone(&guard.interceptor),
            guard.policy.clone(),
        )
    };

    let app_handle = app.clone();
    run_ab(
        ReplayServices {
            session_manager: &session_manager,
            query_manager: &query_manager,
            query_rate_limiter: &query_rate_limiter,
            query_cache: &query_cache,
            interceptor: &interceptor,
            policy: &policy,
        },
        left,
        &request.left_session_id,
        right,
        &request.right_session_id,
        &set,
        &request.slug,
        &project,
        &request.options,
        &captures,
        &replay.cancel,
        |progress| crate::emit_gate::emit_gated(&app_handle, "replay-progress", &progress),
    )
    .await
}

#[tauri::command]
pub async fn replay_cancel_run(state: State<'_, crate::SharedState>) -> Result<(), String> {
    replay_state(&state)
        .await
        .cancel
        .store(true, Ordering::SeqCst);
    Ok(())
}

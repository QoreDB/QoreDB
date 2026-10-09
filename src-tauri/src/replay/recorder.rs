// SPDX-License-Identifier: BUSL-1.1

//! Recording side of the Replay Lab.
//!
//! The recorder observes what the interceptor already sees, at the point where
//! a query completes. Only editor, notebook and query-library executions reach
//! it — tree navigation (`preview_table`, `query_table`) goes through other
//! commands and would otherwise flood a set with clicks.

use parking_lot::RwLock;

use crate::engine::types::{Namespace, QueryResult};
use crate::interceptor::{QueryContext, QueryExecutionResult, fingerprint_query};

use super::capture::CaptureStore;
use super::digest::compute_digest;
use super::secrets::{SecretPolicy, looks_like_secret};
use super::types::{
    CaptureMode, CaptureStopReason, ExpectedOutcome, REPLAY_SET_VERSION, ReplayEntry, ReplaySet,
    ReplaySource, RunMeta, query_preview,
};

pub struct RecordingOptions {
    pub name: String,
    /// The connection being recorded. Executions from any other session are
    /// ignored: a recording started on dev must never capture production rows
    /// because another tab happened to run a query.
    pub session_id: String,
    /// Workspace the captures belong to.
    pub project_id: String,
    /// Where the set will be written. Held from the start so a workspace
    /// switch mid-recording cannot separate a set from its baseline.
    pub workspace_path: std::path::PathBuf,
    pub ignored_columns: Vec<String>,
    /// Off by default. A migration run while a recording is live would land in
    /// the set, and a set that carries mutations is one nobody can replay
    /// without thinking twice.
    pub record_mutations: bool,
    pub capture_mode: CaptureMode,
    pub max_captured_rows: usize,
    pub capture_budget_bytes: u64,
    /// How the set treats query text that looks like it carries a credential.
    pub secret_policy: SecretPolicy,
    /// The interceptor's user-defined redaction patterns, so one place governs
    /// both the audit log and what a set flags.
    pub secret_patterns: Vec<String>,
}

#[derive(Clone)]
struct RecordingSession {
    run_id: String,
    name: String,
    session_id: String,
    project_id: String,
    workspace_path: std::path::PathBuf,
    started_at: String,
    driver_id: String,
    connection_label: Option<String>,
    environment: String,
    ignored_columns: Vec<String>,
    record_mutations: bool,
    capture_mode: CaptureMode,
    max_captured_rows: usize,
    capture_budget_bytes: u64,
    entries: Vec<ReplayEntry>,
    captured_bytes: u64,
    stop_reason: Option<CaptureStopReason>,
    ignored_other_session: usize,
    excluded_mutations: usize,
    secret_policy: SecretPolicy,
    secret_patterns: Vec<String>,
    /// Entry ids whose query looks like it carries a credential.
    flagged: Vec<String>,
}

/// What the UI shows while a recording is live.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RecordingStatus {
    pub run_id: String,
    pub name: String,
    pub started_at: String,
    pub entry_count: usize,
    pub captured_bytes: u64,
    pub capture_mode: CaptureMode,
    pub capture_stopped_reason: Option<CaptureStopReason>,
    /// Executions seen from another connection and left out.
    pub ignored_other_session: usize,
    pub record_mutations: bool,
    /// Mutations left out because `record_mutations` is off.
    pub excluded_mutations: usize,
    /// Recorded entries that write. Zero unless `record_mutations` is on.
    pub mutation_count: usize,
    /// Recorded queries that look like they carry a credential.
    pub secrets_detected: usize,
    pub secret_policy: SecretPolicy,
}

#[derive(Default)]
pub struct Recorder {
    session: RwLock<Option<RecordingSession>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_recording(&self) -> bool {
        self.session.read().is_some()
    }

    /// Whether the live recording is the one watching this connection. The
    /// query path asks before deciding to stream: a streamed result reaches the
    /// recorder without rows, and the entry it produces can never be compared.
    pub fn records_session(&self, session_id: &str) -> bool {
        self.session
            .read()
            .as_ref()
            .is_some_and(|s| s.session_id == session_id)
    }

    /// Workspace the live recording belongs to — not necessarily the active
    /// one, since the user can switch workspaces mid-recording.
    pub fn project_id(&self) -> Option<String> {
        self.session.read().as_ref().map(|s| s.project_id.clone())
    }

    /// Where the set will be written, and under what name — readable without
    /// consuming the recording, so a caller can validate the destination
    /// before taking the entries out of the recorder.
    pub fn destination(&self) -> Option<(std::path::PathBuf, String)> {
        self.session
            .read()
            .as_ref()
            .map(|s| (s.workspace_path.clone(), s.name.clone()))
    }

    pub fn entry_count(&self) -> usize {
        self.session
            .read()
            .as_ref()
            .map(|s| s.entries.len())
            .unwrap_or(0)
    }

    pub fn status(&self, project: &str) -> Option<RecordingStatus> {
        self.session
            .read()
            .as_ref()
            .filter(|s| s.project_id == project)
            .map(Self::session_status)
    }

    fn require_target(s: &RecordingSession, project: &str, run_id: &str) -> Result<(), String> {
        if s.project_id != project || s.run_id != run_id {
            return Err("Recording is no longer available in this workspace".into());
        }
        Ok(())
    }

    fn session_status(s: &RecordingSession) -> RecordingStatus {
        RecordingStatus {
            run_id: s.run_id.clone(),
            name: s.name.clone(),
            started_at: s.started_at.clone(),
            entry_count: s.entries.len(),
            captured_bytes: s.captured_bytes,
            capture_mode: s.capture_mode,
            capture_stopped_reason: s.stop_reason,
            ignored_other_session: s.ignored_other_session,
            record_mutations: s.record_mutations,
            excluded_mutations: s.excluded_mutations,
            mutation_count: s.entries.iter().filter(|e| e.is_mutation).count(),
            secrets_detected: s.flagged.len(),
            secret_policy: s.secret_policy,
        }
    }

    /// Starts a recording. Value capture is refused in production unless the
    /// caller passes `allow_production_capture`, so a set recorded against
    /// prod defaults to digests without rows.
    pub fn start(
        &self,
        options: RecordingOptions,
        driver_id: String,
        connection_label: Option<String>,
        environment: String,
        allow_production_capture: bool,
    ) -> Result<RecordingStatus, String> {
        let mut guard = self.session.write();
        if guard.is_some() {
            return Err("A recording is already in progress".to_string());
        }

        let is_production = environment == "production";
        let (capture_mode, stop_reason) = match options.capture_mode {
            CaptureMode::MetadataOnly => (
                CaptureMode::MetadataOnly,
                Some(CaptureStopReason::MetadataOnly),
            ),
            CaptureMode::Full if is_production && !allow_production_capture => (
                CaptureMode::MetadataOnly,
                Some(CaptureStopReason::ProductionPolicy),
            ),
            CaptureMode::Full => (CaptureMode::Full, None),
        };

        let session = RecordingSession {
            run_id: uuid::Uuid::new_v4().to_string(),
            name: options.name,
            session_id: options.session_id,
            project_id: options.project_id,
            workspace_path: options.workspace_path,
            started_at: chrono::Utc::now().to_rfc3339(),
            driver_id,
            connection_label,
            environment,
            ignored_columns: options.ignored_columns,
            record_mutations: options.record_mutations,
            capture_mode,
            max_captured_rows: options.max_captured_rows,
            capture_budget_bytes: options.capture_budget_bytes,
            entries: Vec::new(),
            captured_bytes: 0,
            stop_reason,
            ignored_other_session: 0,
            excluded_mutations: 0,
            secret_policy: options.secret_policy,
            secret_patterns: options.secret_patterns,
            flagged: Vec::new(),
        };

        let status = Self::session_status(&session);
        *guard = Some(session);
        Ok(status)
    }

    /// Records one completed query. Silently returns when no recording is live,
    /// so the call site stays a single unconditional line.
    pub fn record(
        &self,
        context: &QueryContext,
        namespace: Option<&Namespace>,
        exec: &QueryExecutionResult,
        result: Option<&QueryResult>,
        capture_store: &CaptureStore,
    ) {
        let mut guard = self.session.write();
        let Some(session) = guard.as_mut() else {
            return;
        };

        // Only the connection the recording was started on. Without this, a
        // query run in another tab — production included — would be captured
        // under this recording's policy and connection label.
        if context.session_id != session.session_id {
            session.ignored_other_session += 1;
            return;
        }

        if context.is_mutation && !session.record_mutations {
            session.excluded_mutations += 1;
            return;
        }

        let entry_id = uuid::Uuid::new_v4().to_string();
        let order = session.entries.len() as u32 + 1;

        let digest = result
            .map(|r| compute_digest(r, &session.ignored_columns, session.max_captured_rows).digest);

        // Row count from the result when there is one: `affected_rows` is None
        // for a plain SELECT on most drivers.
        let row_count = result.map(|r| r.rows.len() as i64).or(exec.row_count);

        if session.capture_mode == CaptureMode::Full {
            let budget_left = session
                .capture_budget_bytes
                .saturating_sub(session.captured_bytes);
            if budget_left == 0 {
                session.stop_reason = Some(CaptureStopReason::BudgetExceeded);
            } else if let Some(result) = result {
                match capture_store.save_entry(
                    &session.run_id,
                    &entry_id,
                    &context.query,
                    &context.driver_id,
                    session.connection_label.as_deref(),
                    namespace.cloned(),
                    result,
                    session.max_captured_rows,
                    budget_left,
                ) {
                    Ok(Some(written)) => {
                        session.captured_bytes += written;
                        if session.captured_bytes >= session.capture_budget_bytes {
                            session.stop_reason = Some(CaptureStopReason::BudgetExceeded);
                        }
                    }
                    // Did not fit: nothing was written, and the run says so.
                    Ok(None) => {
                        session.stop_reason = Some(CaptureStopReason::BudgetExceeded);
                    }
                    Err(e) => tracing::warn!(error = %e, "replay capture failed"),
                }
            }
        }

        if session.secret_policy != SecretPolicy::Off
            && looks_like_secret(&context.query, &session.secret_patterns)
        {
            session.flagged.push(entry_id.clone());
        }

        session.entries.push(ReplayEntry {
            id: entry_id,
            order,
            query: context.query.clone(),
            driver_id: context.driver_id.clone(),
            namespace: namespace.cloned(),
            operation_type: format!("{:?}", context.operation_type).to_lowercase(),
            is_mutation: context.is_mutation,
            expected: ExpectedOutcome {
                execution_time_ms: exec.execution_time_ms,
                row_count,
                success: exec.success,
                fingerprint: Some(fingerprint_query(&context.query, &context.driver_id)),
                result_digest: digest,
            },
        });
    }

    /// Ends the recording and hands back the set plus the baseline run it
    /// captured. `None` when nothing was recording.
    pub fn stop(&self) -> Option<(ReplaySet, RunMeta)> {
        self.session.write().take().map(Self::snapshot)
    }

    fn snapshot(mut session: RecordingSession) -> (ReplaySet, RunMeta) {
        // Redacting is the caller's explicit choice: it makes the set
        // shareable without reservation and unreplayable in the same move,
        // which is why the set carries the fact.
        let redacted = session.secret_policy == SecretPolicy::Redact;
        if redacted {
            for entry in &mut session.entries {
                entry.query =
                    crate::interceptor::redact_query_forced(&entry.query, &entry.driver_id);
            }
        }

        let entry_count = session.entries.len();
        let set = ReplaySet {
            version: REPLAY_SET_VERSION,
            baseline_run_id: Some(session.run_id.clone()),
            name: session.name.clone(),
            created_at: session.started_at.clone(),
            source: ReplaySource {
                driver_id: session.driver_id.clone(),
                connection_label: session.connection_label.clone(),
                environment: session.environment.clone(),
            },
            ignored_columns: session.ignored_columns.clone(),
            redacted,
            entries: session.entries,
        };

        let run = RunMeta {
            run_id: session.run_id,
            project_id: session.project_id,
            set_slug: String::new(),
            set_name: session.name,
            started_at: session.started_at,
            finished_at: Some(chrono::Utc::now().to_rfc3339()),
            cancelled: false,
            connection_label: session.connection_label,
            driver_id: session.driver_id,
            environment: session.environment,
            capture_mode: session.capture_mode,
            capture_stopped_reason: session.stop_reason,
            is_baseline: true,
            reference_generation: false,
            captured_bytes: session.captured_bytes,
            entry_count,
        };

        (set, run)
    }

    /// Consumes a recording only once its destination has been persisted.
    pub fn finish<T>(
        &self,
        project: &str,
        run_id: &str,
        persist: impl FnOnce(&std::path::Path, &ReplaySet, RunMeta) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self.session.write();
        let session = guard.as_ref().ok_or("No recording in progress")?;
        Self::require_target(session, project, run_id)?;
        let path = session.workspace_path.clone();
        let (set, run) = Self::snapshot(session.clone());
        let saved = persist(&path, &set, run)?;
        *guard = None;
        Ok(saved)
    }

    /// Drops a recording without producing a set.
    pub fn cancel(
        &self,
        project: &str,
        run_id: &str,
        cleanup: impl FnOnce(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut guard = self.session.write();
        let session = guard.as_ref().ok_or("No recording in progress")?;
        Self::require_target(session, project, run_id)?;
        cleanup(&session.run_id)?;
        *guard = None;
        Ok(())
    }

    /// Drops a recorded entry, and the rows captured for it: leaving them on
    /// disk would keep values the user explicitly removed from the set.
    pub fn discard_preview(
        &self,
        project: &str,
        run_id: &str,
        index: usize,
        capture_store: &CaptureStore,
    ) -> Result<(), String> {
        let mut guard = self.session.write();
        let session = guard
            .as_mut()
            .ok_or_else(|| "No recording in progress".to_string())?;
        Self::require_target(session, project, run_id)?;
        if index >= session.entries.len() {
            return Err("No such recorded entry".to_string());
        }
        capture_store.delete_entry(&session.run_id, &session.entries[index].id)?;
        let removed = session.entries.remove(index);
        session.flagged.retain(|id| id != &removed.id);
        for (position, entry) in session.entries.iter_mut().enumerate() {
            entry.order = position as u32 + 1;
        }
        Ok(())
    }

    /// Drops every recorded mutation and its captured rows, and returns how
    /// many were removed.
    pub fn discard_mutations(
        &self,
        project: &str,
        run_id: &str,
        capture_store: &CaptureStore,
    ) -> Result<usize, String> {
        let mut guard = self.session.write();
        let session = guard
            .as_mut()
            .ok_or_else(|| "No recording in progress".to_string())?;
        Self::require_target(session, project, run_id)?;

        let mut removed = 0;
        let mut index = 0;
        while index < session.entries.len() {
            if !session.entries[index].is_mutation {
                index += 1;
                continue;
            }
            capture_store.delete_entry(&session.run_id, &session.entries[index].id)?;
            let entry = session.entries.remove(index);
            session.flagged.retain(|id| id != &entry.id);
            for (position, entry) in session.entries.iter_mut().enumerate() {
                entry.order = position as u32 + 1;
            }
            removed += 1;
        }
        Ok(removed)
    }

    /// `(order, preview, is_mutation, looks_like_secret)` per recorded entry.
    pub fn recorded_previews(&self, project: &str, run_id: &str) -> Vec<(u32, String, bool, bool)> {
        self.session
            .read()
            .as_ref()
            .filter(|s| s.project_id == project && s.run_id == run_id)
            .map(|s| {
                s.entries
                    .iter()
                    .map(|e| {
                        (
                            e.order,
                            query_preview(&e.query),
                            e.is_mutation,
                            s.flagged.contains(&e.id),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::{ColumnInfo, Row, Value};
    use crate::interceptor::{Environment, QueryOperationType, QuerySource};

    fn current_id(recorder: &Recorder) -> String {
        recorder.status("default").unwrap().run_id
    }

    #[test]
    fn foreign_workspace_cannot_read_or_modify_recording() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        let status = recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "staging".into(),
                false,
            )
            .unwrap();
        recorder.record(
            &context("SELECT 1"),
            None,
            &exec(true, 1.0),
            Some(&sample_result(1)),
            &capture,
        );
        assert!(recorder.status("other").is_none());
        assert!(
            recorder
                .recorded_previews("other", &status.run_id)
                .is_empty()
        );
        assert!(
            recorder
                .discard_preview("other", &status.run_id, 0, &capture)
                .is_err()
        );
        assert!(
            recorder
                .discard_mutations("other", &status.run_id, &capture)
                .is_err()
        );
        assert!(
            recorder
                .finish::<()>("other", &status.run_id, |_, _, _| panic!(
                    "must not publish"
                ))
                .is_err()
        );
        assert!(
            recorder
                .cancel("other", &status.run_id, |_| panic!("must not delete"))
                .is_err()
        );
        assert_eq!(recorder.status("default").unwrap().entry_count, 1);
        assert_eq!(
            recorder.recorded_previews("default", &status.run_id).len(),
            1
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stale_recording_id_cannot_modify_a_new_recording_in_the_same_workspace() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        let old = recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "staging".into(),
                false,
            )
            .unwrap();
        recorder.cancel("default", &old.run_id, |_| Ok(())).unwrap();
        let new = recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "staging".into(),
                false,
            )
            .unwrap();
        recorder.record(
            &context("SELECT 2"),
            None,
            &exec(true, 1.0),
            Some(&sample_result(1)),
            &capture,
        );
        assert!(
            recorder
                .recorded_previews("default", &old.run_id)
                .is_empty()
        );
        assert!(
            recorder
                .discard_preview("default", &old.run_id, 0, &capture)
                .is_err()
        );
        assert!(
            recorder
                .discard_mutations("default", &old.run_id, &capture)
                .is_err()
        );
        assert!(
            recorder
                .finish::<()>("default", &old.run_id, |_, _, _| panic!("must not publish"))
                .is_err()
        );
        assert!(
            recorder
                .cancel("default", &old.run_id, |_| panic!("must not delete"))
                .is_err()
        );
        assert_eq!(recorder.status("default").unwrap().run_id, new.run_id);
        assert_eq!(recorder.status("default").unwrap().entry_count, 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cleanup_errors_keep_the_recording_available_and_are_reported() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        let status = recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "staging".into(),
                false,
            )
            .unwrap();
        recorder.record(
            &context("SELECT 1"),
            None,
            &exec(true, 1.0),
            Some(&sample_result(1)),
            &capture,
        );
        assert!(
            recorder
                .cancel("default", &status.run_id, |_| Err(
                    "synthetic cleanup failure".into()
                ))
                .is_err()
        );
        assert_eq!(recorder.status("default").unwrap().entry_count, 1);
        let entry_id = recorder.session.read().as_ref().unwrap().entries[0]
            .id
            .clone();
        let path = capture
            .root()
            .join(&status.run_id)
            .join(format!("{entry_id}.json"));
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            recorder
                .discard_preview("default", &status.run_id, 0, &capture)
                .is_err()
        );
        assert_eq!(recorder.status("default").unwrap().entry_count, 1);
        recorder
            .cancel("default", &status.run_id, |id| capture.delete_run(id))
            .unwrap();
        assert!(recorder.status("default").is_none());
        assert!(!capture.root().join(&status.run_id).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn mutation_cleanup_failure_keeps_remaining_entries_consistent() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        let mut options = options(CaptureMode::Full);
        options.record_mutations = true;
        let status = recorder
            .start(options, "postgres".into(), None, "staging".into(), false)
            .unwrap();
        for _ in 0..2 {
            let mut query = context("UPDATE synthetic SET id = 1 RETURNING id");
            query.is_mutation = true;
            query.operation_type = QueryOperationType::Update;
            recorder.record(
                &query,
                None,
                &exec(true, 1.0),
                Some(&sample_result(1)),
                &capture,
            );
        }
        let remaining_id = recorder.session.read().as_ref().unwrap().entries[1]
            .id
            .clone();
        let path = capture
            .root()
            .join(&status.run_id)
            .join(format!("{remaining_id}.json"));
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            recorder
                .discard_mutations("default", &status.run_id, &capture)
                .is_err()
        );
        assert_eq!(recorder.status("default").unwrap().entry_count, 1);
        assert_eq!(
            recorder.recorded_previews("default", &status.run_id)[0].0,
            1
        );
        assert_eq!(
            recorder.session.read().as_ref().unwrap().entries[0].id,
            remaining_id
        );
        std::fs::remove_dir(&path).unwrap();
        assert_eq!(
            recorder
                .discard_mutations("default", &status.run_id, &capture)
                .unwrap(),
            1
        );
        assert_eq!(recorder.status("default").unwrap().entry_count, 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn context(query: &str) -> QueryContext {
        QueryContext {
            session_id: "sess".to_string(),
            query: query.to_string(),
            environment: Environment::Staging,
            driver_id: "postgres".to_string(),
            database: Some("app".to_string()),
            operation_type: QueryOperationType::Select,
            is_mutation: false,
            is_dangerous: false,
            acknowledged: false,
            read_only: false,
            source: QuerySource::User,
        }
    }

    fn exec(success: bool, ms: f64) -> QueryExecutionResult {
        QueryExecutionResult {
            success,
            error: None,
            execution_time_ms: ms,
            row_count: None,
        }
    }

    fn sample_result(rows: usize) -> QueryResult {
        QueryResult {
            columns: vec![ColumnInfo {
                name: "id".into(),
                data_type: "int".into(),
                nullable: false,
                masked: false,
            }],
            rows: (0..rows as i64)
                .map(|i| Row {
                    values: vec![Value::Int(i)],
                })
                .collect(),
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }

    /// Assembled at runtime so no source literal looks like a live key to a
    /// secret scanner.
    fn sample_key() -> String {
        format!("sk_{}_{}", "live", "4eC39HqLyjWDarjt")
    }

    fn secret_query() -> String {
        format!("SELECT * FROM users WHERE api_key = '{}'", sample_key())
    }

    fn options(mode: CaptureMode) -> RecordingOptions {
        RecordingOptions {
            name: "checkout".to_string(),
            session_id: "sess".to_string(),
            project_id: "default".to_string(),
            workspace_path: std::env::temp_dir(),
            ignored_columns: Vec::new(),
            record_mutations: false,
            capture_mode: mode,
            max_captured_rows: 1000,
            capture_budget_bytes: 64 * 1024,
            secret_policy: SecretPolicy::Warn,
            secret_patterns: Vec::new(),
        }
    }

    fn store() -> (CaptureStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("qoredb_recorder_{}", uuid::Uuid::new_v4()));
        (CaptureStore::new(dir.clone()), dir)
    }

    #[test]
    fn failed_finish_keeps_recording_and_captures_for_retry() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "staging".into(),
                false,
            )
            .unwrap();
        recorder.record(
            &context("SELECT 1"),
            None,
            &exec(true, 10.0),
            Some(&sample_result(3)),
            &capture,
        );
        let run_id = recorder.status("default").unwrap().run_id;
        let failed: Result<(), _> =
            recorder.finish("default", &current_id(&recorder), |_, set, run| {
                assert_eq!(set.entries.len(), 1);
                assert!(capture.has_entry(&run.run_id, &set.entries[0].id));
                Err("synthetic disk error".into())
            });
        assert!(failed.is_err());
        assert!(recorder.is_recording(), "failed save must be retryable");
        assert_eq!(recorder.status("default").unwrap().run_id, run_id);
        recorder
            .finish("default", &current_id(&recorder), |_, set, run| {
                assert_eq!(set.entries.len(), 1);
                assert_eq!(
                    capture
                        .load_entry(&run.run_id, &set.entries[0].id)?
                        .rows
                        .len(),
                    3
                );
                Ok(())
            })
            .unwrap();
        assert!(!recorder.is_recording());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn real_save_error_can_be_retried_without_losing_or_redacting_live_queries() {
        let dir = tempfile::tempdir().unwrap();
        let capture = CaptureStore::new(dir.path().join("captures"));
        let workspace = dir.path().join("workspace");
        std::fs::write(&workspace, "blocks directory creation").unwrap();
        let recorder = Recorder::new();
        let mut options = options(CaptureMode::Full);
        options.workspace_path = workspace.clone();
        options.secret_policy = SecretPolicy::Redact;
        recorder
            .start(options, "postgres".into(), None, "staging".into(), false)
            .unwrap();
        recorder.record(
            &context(&secret_query()),
            None,
            &exec(true, 1.0),
            Some(&sample_result(1)),
            &capture,
        );
        let previews = recorder.recorded_previews("default", &current_id(&recorder));
        let save = || {
            recorder.finish("default", &current_id(&recorder), |path, set, mut run| {
                run.set_slug = "retry".into();
                capture.save_run_meta(&run)?;
                crate::replay::store::ReplaySetStore::new(path).create("retry", set)?;
                Ok(run.run_id)
            })
        };
        assert!(save().is_err());
        assert_eq!(
            recorder.recorded_previews("default", &current_id(&recorder)),
            previews
        );
        assert!(recorder.is_recording());
        std::fs::remove_file(&workspace).unwrap();
        let run_id = save().unwrap();
        let set = crate::replay::store::ReplaySetStore::new(&workspace)
            .load("retry")
            .unwrap();
        assert!(set.redacted);
        assert!(!set.entries[0].query.contains(&sample_key()));
        assert_eq!(set.baseline_run_id, Some(run_id.clone()));
        assert!(capture.has_entry(&run_id, &set.entries[0].id));
        assert!(!recorder.is_recording());
    }

    #[test]
    fn concurrent_starts_cannot_replace_a_successful_recording() {
        for _ in 0..32 {
            let recorder = std::sync::Arc::new(Recorder::new());
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let recorder = recorder.clone();
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        barrier.wait();
                        recorder.start(
                            options(CaptureMode::MetadataOnly),
                            "postgres".into(),
                            None,
                            "staging".into(),
                            false,
                        )
                    })
                })
                .collect();
            let accepted: Vec<_> = handles
                .into_iter()
                .filter_map(|handle| handle.join().unwrap().ok())
                .collect();
            assert_eq!(accepted.len(), 1, "only one start may own the recorder");
            assert_eq!(
                recorder.status("default").unwrap().run_id,
                accepted[0].run_id
            );
        }
    }

    #[test]
    fn records_entries_in_order_with_a_digest() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "staging".into(),
                false,
            )
            .unwrap();

        recorder.record(
            &context("SELECT 1"),
            None,
            &exec(true, 10.0),
            Some(&sample_result(3)),
            &capture,
        );
        recorder.record(
            &context("SELECT 2"),
            None,
            &exec(true, 20.0),
            Some(&sample_result(4)),
            &capture,
        );

        let (set, run) = recorder.stop().unwrap();
        assert_eq!(set.entries.len(), 2);
        assert_eq!(set.entries[0].order, 1);
        assert_eq!(set.entries[1].order, 2);
        assert_eq!(set.entries[0].expected.row_count, Some(3));
        assert!(set.entries[0].expected.result_digest.is_some());
        assert!(run.is_baseline);
        assert!(run.captured_bytes > 0);
        assert!(!recorder.is_recording());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn metadata_only_keeps_digests_but_writes_no_rows() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        recorder
            .start(
                options(CaptureMode::MetadataOnly),
                "postgres".into(),
                None,
                "staging".into(),
                false,
            )
            .unwrap();
        recorder.record(
            &context("SELECT 1"),
            None,
            &exec(true, 10.0),
            Some(&sample_result(3)),
            &capture,
        );

        let (set, run) = recorder.stop().unwrap();
        assert!(set.entries[0].expected.result_digest.is_some());
        assert_eq!(run.captured_bytes, 0);
        assert!(!capture.has_entry(&run.run_id, &set.entries[0].id));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn production_downgrades_capture_unless_explicitly_allowed() {
        let recorder = Recorder::new();
        let status = recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "production".into(),
                false,
            )
            .unwrap();
        assert_eq!(status.capture_mode, CaptureMode::MetadataOnly);
        assert_eq!(
            status.capture_stopped_reason,
            Some(CaptureStopReason::ProductionPolicy)
        );
        recorder
            .cancel("default", &current_id(&recorder), |_| Ok(()))
            .unwrap();

        let status = recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "production".into(),
                true,
            )
            .unwrap();
        assert_eq!(status.capture_mode, CaptureMode::Full);
    }

    /// The budget bounds what reaches the disk. Entries keep being recorded —
    /// metadata and digest are cheap — but their rows are not stored once the
    /// budget is spent, and the run says so.
    #[test]
    fn budget_stops_capture_and_says_so() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        let first = sample_result(50);

        // Measure one capture, then allow room for exactly that one.
        let mut sizing = options(CaptureMode::Full);
        sizing.capture_budget_bytes = u64::MAX;
        recorder
            .start(sizing, "postgres".into(), None, "staging".into(), false)
            .unwrap();
        recorder.record(
            &context("SELECT 1"),
            None,
            &exec(true, 10.0),
            Some(&first),
            &capture,
        );
        let one_entry_bytes = recorder.status("default").unwrap().captured_bytes;
        recorder
            .cancel("default", &current_id(&recorder), |_| Ok(()))
            .unwrap();
        assert!(one_entry_bytes > 0);

        let mut opts = options(CaptureMode::Full);
        opts.capture_budget_bytes = one_entry_bytes + 1;
        recorder
            .start(opts, "postgres".into(), None, "staging".into(), false)
            .unwrap();
        recorder.record(
            &context("SELECT 1"),
            None,
            &exec(true, 10.0),
            Some(&first),
            &capture,
        );
        recorder.record(
            &context("SELECT 2"),
            None,
            &exec(true, 10.0),
            Some(&sample_result(50)),
            &capture,
        );

        let (set, run) = recorder.stop().unwrap();
        assert_eq!(
            set.entries.len(),
            2,
            "entries are still recorded past the budget"
        );
        assert_eq!(
            run.capture_stopped_reason,
            Some(CaptureStopReason::BudgetExceeded)
        );
        assert!(capture.has_entry(&run.run_id, &set.entries[0].id));
        assert!(!capture.has_entry(&run.run_id, &set.entries[1].id));
        assert!(
            run.captured_bytes <= one_entry_bytes + 1,
            "the run never writes past its budget"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A single result too large for the whole budget writes nothing at all,
    /// rather than overshooting by its entire size.
    #[test]
    fn a_single_oversized_entry_is_never_written() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        let mut opts = options(CaptureMode::Full);
        opts.capture_budget_bytes = 32;
        recorder
            .start(opts, "postgres".into(), None, "staging".into(), false)
            .unwrap();

        recorder.record(
            &context("SELECT 1"),
            None,
            &exec(true, 10.0),
            Some(&sample_result(500)),
            &capture,
        );

        let (set, run) = recorder.stop().unwrap();
        assert_eq!(run.captured_bytes, 0);
        assert_eq!(
            run.capture_stopped_reason,
            Some(CaptureStopReason::BudgetExceeded)
        );
        assert!(!capture.has_entry(&run.run_id, &set.entries[0].id));
        assert!(
            set.entries[0].expected.result_digest.is_some(),
            "the digest still describes the result"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_recording_is_refused() {
        let recorder = Recorder::new();
        recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "staging".into(),
                false,
            )
            .unwrap();
        assert!(
            recorder
                .start(
                    options(CaptureMode::Full),
                    "postgres".into(),
                    None,
                    "staging".into(),
                    false
                )
                .is_err()
        );
    }

    #[test]
    fn discarding_a_preview_renumbers_the_rest() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "staging".into(),
                false,
            )
            .unwrap();
        recorder.record(&context("SELECT 1"), None, &exec(true, 1.0), None, &capture);
        recorder.record(&context("SELECT 2"), None, &exec(true, 1.0), None, &capture);
        recorder.record(&context("SELECT 3"), None, &exec(true, 1.0), None, &capture);

        recorder
            .discard_preview("default", &current_id(&recorder), 1, &capture)
            .unwrap();
        let previews = recorder.recorded_previews("default", &current_id(&recorder));
        assert_eq!(previews.len(), 2);
        assert_eq!(previews[0].0, 1);
        assert_eq!(previews[1].0, 2);
        assert!(previews[1].1.contains("SELECT 3"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A recording belongs to one connection. Anything else — another tab, a
    /// production connection — is counted and left out.
    #[test]
    fn an_execution_from_another_connection_is_not_captured() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "staging".into(),
                false,
            )
            .unwrap();

        let mut elsewhere = context("SELECT secret FROM prod_users");
        elsewhere.session_id = "another-session".to_string();
        elsewhere.environment = crate::interceptor::Environment::Production;
        recorder.record(
            &elsewhere,
            None,
            &exec(true, 1.0),
            Some(&sample_result(3)),
            &capture,
        );

        recorder.record(
            &context("SELECT 1"),
            None,
            &exec(true, 1.0),
            Some(&sample_result(1)),
            &capture,
        );

        let status = recorder.status("default").unwrap();
        assert_eq!(status.ignored_other_session, 1);
        assert_eq!(status.entry_count, 1);

        let (set, run) = recorder.stop().unwrap();
        assert_eq!(set.entries.len(), 1);
        assert!(set.entries[0].query.contains("SELECT 1"));
        assert!(
            !set.entries.iter().any(|e| e.query.contains("prod_users")),
            "the other connection's query must not be in the set"
        );
        assert_eq!(run.entry_count, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The default policy flags without touching the query: the set stays
    /// replayable, and the user decides what to drop.
    #[test]
    fn warn_flags_the_entry_but_leaves_it_replayable() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "staging".into(),
                false,
            )
            .unwrap();

        recorder.record(
            &context(&secret_query()),
            None,
            &exec(true, 1.0),
            None,
            &capture,
        );
        recorder.record(
            &context("SELECT id FROM orders WHERE status = 'pending'"),
            None,
            &exec(true, 1.0),
            None,
            &capture,
        );

        let status = recorder.status("default").unwrap();
        assert_eq!(status.secrets_detected, 1, "only the credential is flagged");

        let previews = recorder.recorded_previews("default", &current_id(&recorder));
        assert!(previews[0].3, "the api_key query is flagged");
        assert!(!previews[1].3, "the harmless literal is not");

        let (set, _) = recorder.stop().unwrap();
        assert!(!set.redacted);
        assert!(
            set.entries[0].query.contains(&sample_key()),
            "warning must not alter the query, or the set stops replaying"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Redacting is the opposite trade: shareable without reservation, and
    /// no longer replayable — which the set records so the runner refuses it.
    #[test]
    fn redact_strips_the_literals_and_marks_the_set() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        let mut opts = options(CaptureMode::Full);
        opts.secret_policy = SecretPolicy::Redact;
        recorder
            .start(opts, "postgres".into(), None, "staging".into(), false)
            .unwrap();

        recorder.record(
            &context(&secret_query()),
            None,
            &exec(true, 1.0),
            None,
            &capture,
        );

        let (set, _) = recorder.stop().unwrap();
        assert!(set.redacted);
        assert!(!set.entries[0].query.contains(&sample_key()));
        // The assignment pattern gets there first, so the marker is `***`
        // rather than `[REDACTED]`; either way the value is gone.
        assert!(set.entries[0].query.contains("***") || set.entries[0].query.contains("REDACTED"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_off_policy_flags_nothing() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        let mut opts = options(CaptureMode::Full);
        opts.secret_policy = SecretPolicy::Off;
        recorder
            .start(opts, "postgres".into(), None, "staging".into(), false)
            .unwrap();

        recorder.record(
            &context("SELECT * FROM t WHERE password = 'hunter2'"),
            None,
            &exec(true, 1.0),
            None,
            &capture,
        );

        assert_eq!(recorder.status("default").unwrap().secrets_detected, 0);
        let (set, _) = recorder.stop().unwrap();
        assert!(set.entries[0].query.contains("hunter2"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn mutation_context(query: &str) -> QueryContext {
        QueryContext {
            operation_type: QueryOperationType::Insert,
            is_mutation: true,
            ..context(query)
        }
    }

    /// A migration run while a recording is live must not end up in the set.
    #[test]
    fn mutations_stay_out_unless_asked_for() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        recorder
            .start(
                options(CaptureMode::Full),
                "postgres".into(),
                None,
                "staging".into(),
                false,
            )
            .unwrap();

        recorder.record(
            &context("SELECT 1"),
            None,
            &exec(true, 1.0),
            Some(&sample_result(1)),
            &capture,
        );
        recorder.record(
            &mutation_context("ALTER TABLE users ADD COLUMN plan text"),
            None,
            &exec(true, 1.0),
            None,
            &capture,
        );

        let status = recorder.status("default").unwrap();
        assert_eq!(status.entry_count, 1);
        assert_eq!(status.excluded_mutations, 1);

        let (set, _) = recorder.stop().unwrap();
        assert_eq!(set.entries.len(), 1);
        assert!(!set.entries[0].is_mutation);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recorded_mutations_can_be_dropped_before_saving() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        let mut opts = options(CaptureMode::Full);
        opts.record_mutations = true;
        recorder
            .start(opts, "postgres".into(), None, "staging".into(), false)
            .unwrap();

        recorder.record(
            &context("SELECT 1"),
            None,
            &exec(true, 1.0),
            Some(&sample_result(1)),
            &capture,
        );
        recorder.record(
            &mutation_context("UPDATE users SET plan = 'pro'"),
            None,
            &exec(true, 1.0),
            Some(&sample_result(2)),
            &capture,
        );
        assert_eq!(recorder.status("default").unwrap().mutation_count, 1);

        assert_eq!(
            recorder
                .discard_mutations("default", &current_id(&recorder), &capture)
                .unwrap(),
            1
        );

        let (set, run) = recorder.stop().unwrap();
        assert_eq!(set.entries.len(), 1);
        assert_eq!(set.entries[0].order, 1);
        assert!(capture.has_entry(&run.run_id, &set.entries[0].id));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_without_a_session_is_a_no_op() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        recorder.record(&context("SELECT 1"), None, &exec(true, 1.0), None, &capture);
        assert!(recorder.stop().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

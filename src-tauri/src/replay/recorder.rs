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
    pub ignored_columns: Vec<String>,
    pub capture_mode: CaptureMode,
    pub max_captured_rows: usize,
    pub capture_budget_bytes: u64,
}

struct RecordingSession {
    run_id: String,
    name: String,
    session_id: String,
    project_id: String,
    started_at: String,
    driver_id: String,
    connection_label: Option<String>,
    environment: String,
    ignored_columns: Vec<String>,
    capture_mode: CaptureMode,
    max_captured_rows: usize,
    capture_budget_bytes: u64,
    entries: Vec<ReplayEntry>,
    captured_bytes: u64,
    stop_reason: Option<CaptureStopReason>,
    ignored_other_session: usize,
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

    /// Workspace the live recording belongs to — not necessarily the active
    /// one, since the user can switch workspaces mid-recording.
    pub fn project_id(&self) -> Option<String> {
        self.session.read().as_ref().map(|s| s.project_id.clone())
    }

    pub fn status(&self) -> Option<RecordingStatus> {
        self.session.read().as_ref().map(|s| RecordingStatus {
            run_id: s.run_id.clone(),
            name: s.name.clone(),
            started_at: s.started_at.clone(),
            entry_count: s.entries.len(),
            captured_bytes: s.captured_bytes,
            capture_mode: s.capture_mode,
            capture_stopped_reason: s.stop_reason,
            ignored_other_session: s.ignored_other_session,
        })
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
        if self.session.read().is_some() {
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
            started_at: chrono::Utc::now().to_rfc3339(),
            driver_id,
            connection_label,
            environment,
            ignored_columns: options.ignored_columns,
            capture_mode,
            max_captured_rows: options.max_captured_rows,
            capture_budget_bytes: options.capture_budget_bytes,
            entries: Vec::new(),
            captured_bytes: 0,
            stop_reason,
            ignored_other_session: 0,
        };

        *self.session.write() = Some(session);
        Ok(self.status().expect("session was just installed"))
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
        let session = self.session.write().take()?;

        let entry_count = session.entries.len();
        let set = ReplaySet {
            version: REPLAY_SET_VERSION,
            name: session.name.clone(),
            created_at: session.started_at.clone(),
            source: ReplaySource {
                driver_id: session.driver_id.clone(),
                connection_label: session.connection_label.clone(),
                environment: session.environment.clone(),
            },
            ignored_columns: session.ignored_columns.clone(),
            entries: session.entries,
        };

        let run = RunMeta {
            run_id: session.run_id,
            project_id: session.project_id,
            set_slug: String::new(),
            set_name: session.name,
            started_at: session.started_at,
            finished_at: Some(chrono::Utc::now().to_rfc3339()),
            connection_label: session.connection_label,
            driver_id: session.driver_id,
            environment: session.environment,
            capture_mode: session.capture_mode,
            capture_stopped_reason: session.stop_reason,
            is_baseline: true,
            captured_bytes: session.captured_bytes,
            entry_count,
        };

        Some((set, run))
    }

    /// Drops a recording without producing a set.
    pub fn cancel(&self) -> Option<String> {
        self.session.write().take().map(|s| s.run_id)
    }

    /// Drops a recorded entry, and the rows captured for it: leaving them on
    /// disk would keep values the user explicitly removed from the set.
    pub fn discard_preview(
        &self,
        index: usize,
        capture_store: &CaptureStore,
    ) -> Result<(), String> {
        let mut guard = self.session.write();
        let session = guard
            .as_mut()
            .ok_or_else(|| "No recording in progress".to_string())?;
        if index >= session.entries.len() {
            return Err("No such recorded entry".to_string());
        }
        let removed = session.entries.remove(index);
        let _ = capture_store.delete_entry(&session.run_id, &removed.id);
        for (position, entry) in session.entries.iter_mut().enumerate() {
            entry.order = position as u32 + 1;
        }
        Ok(())
    }

    pub fn recorded_previews(&self) -> Vec<(u32, String, bool)> {
        self.session
            .read()
            .as_ref()
            .map(|s| {
                s.entries
                    .iter()
                    .map(|e| (e.order, query_preview(&e.query), e.is_mutation))
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

    fn options(mode: CaptureMode) -> RecordingOptions {
        RecordingOptions {
            name: "checkout".to_string(),
            session_id: "sess".to_string(),
            project_id: "default".to_string(),
            ignored_columns: Vec::new(),
            capture_mode: mode,
            max_captured_rows: 1000,
            capture_budget_bytes: 64 * 1024,
        }
    }

    fn store() -> (CaptureStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("qoredb_recorder_{}", uuid::Uuid::new_v4()));
        (CaptureStore::new(dir.clone()), dir)
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
        recorder.cancel();

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
        let one_entry_bytes = recorder.status().unwrap().captured_bytes;
        recorder.cancel();
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

        recorder.discard_preview(1, &capture).unwrap();
        let previews = recorder.recorded_previews();
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

        let status = recorder.status().unwrap();
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

    #[test]
    fn record_without_a_session_is_a_no_op() {
        let (capture, dir) = store();
        let recorder = Recorder::new();
        recorder.record(&context("SELECT 1"), None, &exec(true, 1.0), None, &capture);
        assert!(recorder.stop().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

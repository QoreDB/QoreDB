// SPDX-License-Identifier: BUSL-1.1

//! Sequential replay of a set against one connection.
//!
//! Entries go through the same preflight and execute path as a user query,
//! tagged `QuerySource::Replay`, so audit, safety rules and rate limiting all
//! apply without a parallel code path.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use qore_drivers::query_manager::QueryManager;
use qore_drivers::session_manager::SessionManager;
use qore_service::cache::QueryCache;
use qore_service::interceptor::{Environment, InterceptorPipeline, QuerySource, map_environment};
use qore_service::policy::SafetyPolicy;
use qore_service::query as service_query;
use qore_service::ratelimit::QueryRateLimiter;

use crate::engine::types::SessionId;

use super::capture::CaptureStore;
use super::compare::{Observed, SlowerThresholds, classify, compare_sides, summarize};
use super::digest::compute_digest;
use super::types::{
    CaptureMode, CaptureStopReason, ReplayAbReport, ReplayEntryResult, ReplayProgress,
    ReplayReport, ReplayRunOptions, ReplaySet, ReplayVerdict, RunMeta, query_preview,
};

pub const MUTATION_EXCLUDED: &str = "Mutation excluded from replay";
pub const MUTATION_PRODUCTION_BLOCKED: &str = "Mutations are never replayed against production";
pub const CANCELLED: &str = "Replay cancelled";
pub const SET_IS_REDACTED: &str =
    "This replay set was recorded with redaction: its queries cannot be replayed";
pub const SAME_CONNECTION: &str = "A/B replay needs two different connections";

pub struct ReplayServices<'a> {
    pub session_manager: &'a Arc<SessionManager>,
    pub query_manager: &'a Arc<QueryManager>,
    pub query_rate_limiter: &'a Arc<QueryRateLimiter>,
    pub query_cache: &'a Arc<QueryCache>,
    pub interceptor: &'a Arc<InterceptorPipeline>,
    pub policy: &'a SafetyPolicy,
}

fn skipped(entry: &super::types::ReplayEntry, reason: &str) -> ReplayEntryResult {
    ReplayEntryResult {
        entry_id: entry.id.clone(),
        order: entry.order,
        query_preview: query_preview(&entry.query),
        verdict: ReplayVerdict::Skipped,
        success: false,
        error: None,
        skip_reason: Some(reason.to_string()),
        execution_time_ms: 0.0,
        expected_execution_time_ms: entry.expected.execution_time_ms,
        row_count: None,
        expected_row_count: entry.expected.row_count,
        digest: None,
        expected_digest: entry.expected.result_digest.clone(),
        captured: false,
        partial_comparison: false,
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_set(
    services: ReplayServices<'_>,
    session: SessionId,
    session_id: &str,
    set: &ReplaySet,
    set_slug: &str,
    project_id: &str,
    options: &ReplayRunOptions,
    capture_store: &CaptureStore,
    baseline_run_id: Option<String>,
    // Entries decided out of the run before it starts, by entry id and reason.
    // A/B uses it so one side's exclusion is not executed by the other.
    excluded: &HashMap<String, String>,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(ReplayProgress),
) -> Result<ReplayReport, String> {
    if set.redacted {
        return Err(SET_IS_REDACTED.to_string());
    }

    let driver = services
        .session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;
    let driver_id = driver.driver_id().to_string();

    let environment_label = services
        .session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let is_production = matches!(map_environment(&environment_label), Environment::Production);

    let connection_label = services.session_manager.connection_key(session).await;

    let run_id = uuid::Uuid::new_v4().to_string();
    let mut run = RunMeta {
        run_id: run_id.clone(),
        project_id: project_id.to_string(),
        set_slug: set_slug.to_string(),
        set_name: set.name.clone(),
        started_at: chrono::Utc::now().to_rfc3339(),
        finished_at: None,
        connection_label: connection_label.clone(),
        driver_id: driver_id.clone(),
        environment: environment_label.clone(),
        capture_mode: options.capture_mode,
        capture_stopped_reason: None,
        is_baseline: false,
        captured_bytes: 0,
        entry_count: set.entries.len(),
    };

    // Value capture in production is opt-in at the recording level; a replay
    // against production never writes rows.
    let mut capture_mode = options.capture_mode;
    if capture_mode == CaptureMode::Full && is_production {
        capture_mode = CaptureMode::MetadataOnly;
        run.capture_stopped_reason = Some(CaptureStopReason::ProductionPolicy);
    } else if capture_mode == CaptureMode::MetadataOnly {
        run.capture_stopped_reason = Some(CaptureStopReason::MetadataOnly);
    }
    run.capture_mode = capture_mode;
    capture_store.save_run_meta(&run)?;

    let thresholds = SlowerThresholds {
        ratio: options.slower_ratio,
        min_delta_ms: options.slower_min_delta_ms,
    };

    let total = set.entries.len();
    let mut results = Vec::with_capacity(total);

    for (index, entry) in set.entries.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.to_string());
        }

        on_progress(ReplayProgress {
            run_id: run_id.clone(),
            completed: index,
            total,
            current_query_preview: query_preview(&entry.query),
        });

        if let Some(reason) = excluded.get(&entry.id) {
            results.push(skipped(entry, reason));
            continue;
        }

        let preflight = match service_query::preflight_with_source(
            services.session_manager,
            services.query_rate_limiter,
            services.interceptor,
            services.policy,
            session,
            session_id,
            &entry.query,
            entry.namespace.as_ref(),
            false,
            QuerySource::Replay,
        )
        .await
        {
            Ok(preflight) => preflight,
            // A refused preflight is QoreDB declining to run the query — a
            // safety rule, read-only mode, the rate limiter. That is not the
            // database answering differently, so it is not a regression.
            Err(message) => {
                results.push(skipped(entry, &message));
                continue;
            }
        };

        // The trustworthy classification is the preflight's, never the set's:
        // `.qreplay.json` is a versioned, hand-editable file, and a mutation
        // declared as a read would otherwise run — in production included.
        if preflight.is_mutation {
            let reason = if is_production {
                MUTATION_PRODUCTION_BLOCKED
            } else if !options.allow_mutations {
                MUTATION_EXCLUDED
            } else {
                ""
            };
            if !reason.is_empty() {
                results.push(skipped(entry, reason));
                continue;
            }
        }

        let query_id = services.query_manager.register(session).await;
        let outcome = service_query::execute(
            services.query_manager,
            services.query_cache,
            services.interceptor,
            services.policy,
            preflight.driver,
            &preflight.context,
            session,
            entry.namespace.clone(),
            &entry.query,
            query_id,
            preflight.is_mutation,
            preflight.connection_key.as_deref(),
            preflight.safety_warning.as_deref(),
            services.policy.max_query_duration_ms,
            false,
            None,
            None,
            |_, _| {},
        )
        .await;

        let execution_time_ms = outcome
            .result
            .as_ref()
            .map(|r| r.execution_time_ms)
            .unwrap_or(0.0);

        let (digest, partial) = match outcome.result.as_ref() {
            Some(result) => {
                let outcome =
                    compute_digest(result, &set.ignored_columns, options.max_captured_rows);
                (Some(outcome.digest), outcome.partial)
            }
            None => (None, false),
        };

        let row_count = outcome.result.as_ref().map(|r| r.rows.len() as i64);

        let mut captured = false;
        if capture_mode == CaptureMode::Full {
            let budget_left = options
                .capture_budget_bytes
                .saturating_sub(run.captured_bytes);
            if budget_left == 0 {
                run.capture_stopped_reason = Some(CaptureStopReason::BudgetExceeded);
            } else if let Some(result) = outcome.result.as_ref() {
                match capture_store.save_entry(
                    &run_id,
                    &entry.id,
                    &entry.query,
                    &driver_id,
                    connection_label.as_deref(),
                    entry.namespace.clone(),
                    result,
                    options.max_captured_rows,
                    budget_left,
                ) {
                    Ok(Some(written)) => {
                        captured = true;
                        run.captured_bytes += written;
                        if run.captured_bytes >= options.capture_budget_bytes {
                            run.capture_stopped_reason = Some(CaptureStopReason::BudgetExceeded);
                        }
                    }
                    Ok(None) => {
                        run.capture_stopped_reason = Some(CaptureStopReason::BudgetExceeded);
                    }
                    Err(e) => tracing::warn!(error = %e, "replay capture failed"),
                }
            }
        }

        let verdict = classify(
            &entry.expected,
            &Observed {
                success: outcome.success,
                execution_time_ms,
                row_count,
                digest: digest.clone(),
            },
            &thresholds,
        );

        results.push(ReplayEntryResult {
            entry_id: entry.id.clone(),
            order: entry.order,
            query_preview: query_preview(&entry.query),
            verdict,
            success: outcome.success,
            error: outcome.error,
            skip_reason: None,
            execution_time_ms,
            expected_execution_time_ms: entry.expected.execution_time_ms,
            row_count,
            expected_row_count: entry.expected.row_count,
            digest,
            expected_digest: entry.expected.result_digest.clone(),
            captured,
            partial_comparison: partial,
        });
    }

    on_progress(ReplayProgress {
        run_id: run_id.clone(),
        completed: total,
        total,
        current_query_preview: String::new(),
    });

    run.finished_at = Some(chrono::Utc::now().to_rfc3339());
    capture_store.save_run_meta(&run)?;
    let _ = capture_store.prune(set_slug, options.run_retention);

    let summary = summarize(results.iter().map(|r| r.verdict));

    let report = ReplayReport {
        run,
        baseline_run_id,
        results,
        summary,
    };
    capture_store.save_report(&run_id, &report)?;

    Ok(report)
}

/// Which entries a side will refuse, decided from the query text and the
/// connection's environment alone — no execution, no preflight side effects.
/// The per-entry preflight guard in `run_set` stays as the last word.
async fn plan_exclusions(
    session_manager: &Arc<SessionManager>,
    session: SessionId,
    set: &ReplaySet,
    options: &ReplayRunOptions,
) -> Result<HashMap<String, String>, String> {
    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let is_production = matches!(map_environment(&environment), Environment::Production);

    let mut excluded = HashMap::new();
    for entry in &set.entries {
        // Classified from the query text, never from the set's own flag.
        let is_mutation =
            service_query::classify_mutation(&entry.driver_id, &entry.query).unwrap_or(true);
        if !is_mutation {
            continue;
        }
        let reason = if is_production {
            MUTATION_PRODUCTION_BLOCKED
        } else if !options.allow_mutations {
            MUTATION_EXCLUDED
        } else {
            continue;
        };
        excluded.insert(entry.id.clone(), reason.to_string());
    }
    Ok(excluded)
}

/// Runs the same set on two connections and compares the two results against
/// each other. Both sides are live, so this works where a recorded baseline
/// would not — comparing production to a migrated staging, for instance.
#[allow(clippy::too_many_arguments)]
pub async fn run_ab(
    services: ReplayServices<'_>,
    left_session: SessionId,
    left_session_id: &str,
    right_session: SessionId,
    right_session_id: &str,
    set: &ReplaySet,
    set_slug: &str,
    project_id: &str,
    options: &ReplayRunOptions,
    capture_store: &CaptureStore,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(ReplayProgress),
) -> Result<ReplayAbReport, String> {
    if left_session == right_session {
        return Err(SAME_CONNECTION.to_string());
    }

    // Decide the exclusions for both sides up front. Running them in sequence
    // and merging afterwards would let one side execute a mutation the other
    // refuses, and only then label the pair "skipped".
    let mut excluded =
        plan_exclusions(services.session_manager, left_session, set, options).await?;
    for (entry_id, reason) in
        plan_exclusions(services.session_manager, right_session, set, options).await?
    {
        excluded.entry(entry_id).or_insert(reason);
    }

    let total = set.entries.len() * 2;
    let mut done = 0usize;
    let forward =
        |progress: ReplayProgress, done: &mut usize, on: &mut dyn FnMut(ReplayProgress)| {
            *done += 1;
            on(ReplayProgress {
                completed: (*done).min(total),
                total,
                ..progress
            });
        };

    let left = run_set(
        ReplayServices { ..services },
        left_session,
        left_session_id,
        set,
        set_slug,
        project_id,
        options,
        capture_store,
        None,
        &excluded,
        cancel,
        |p| forward(p, &mut done, &mut on_progress),
    )
    .await?;

    let right = run_set(
        ReplayServices { ..services },
        right_session,
        right_session_id,
        set,
        set_slug,
        project_id,
        options,
        capture_store,
        Some(left.run.run_id.clone()),
        &excluded,
        cancel,
        |p| forward(p, &mut done, &mut on_progress),
    )
    .await?;

    let thresholds = SlowerThresholds {
        ratio: options.slower_ratio,
        min_delta_ms: options.slower_min_delta_ms,
    };

    let results: Vec<ReplayEntryResult> = left
        .results
        .iter()
        .zip(right.results.iter())
        .map(|(l, r)| compare_sides(l, r, &thresholds))
        .collect();
    let summary = summarize(results.iter().map(|r| r.verdict));

    let report = ReplayAbReport {
        left: left.run,
        right: right.run,
        results,
        summary,
    };
    capture_store.save_ab_report(&report.right.run_id, &report)?;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qore_core::registry::DriverRegistry;
    use qore_service::cache::QueryCache;
    use qore_service::policy::SafetyPolicy;

    use crate::engine::testing::MockDriver;
    use crate::engine::types::{ColumnInfo, ConnectionConfig, QueryResult, Row, Value};
    use crate::replay::types::{
        ExpectedOutcome, REPLAY_SET_VERSION, ReplayEntry, ReplaySet, ReplaySource,
    };

    struct Harness {
        session_manager: Arc<SessionManager>,
        query_manager: Arc<QueryManager>,
        query_rate_limiter: Arc<QueryRateLimiter>,
        query_cache: Arc<QueryCache>,
        interceptor: Arc<InterceptorPipeline>,
        policy: SafetyPolicy,
        captures: CaptureStore,
        capture_dir: std::path::PathBuf,
        session: SessionId,
    }

    impl Harness {
        fn services(&self) -> ReplayServices<'_> {
            ReplayServices {
                session_manager: &self.session_manager,
                query_manager: &self.query_manager,
                query_rate_limiter: &self.query_rate_limiter,
                query_cache: &self.query_cache,
                interceptor: &self.interceptor,
                policy: &self.policy,
            }
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.capture_dir);
        }
    }

    async fn harness(driver: Arc<MockDriver>, environment: &str) -> Harness {
        let mut registry = DriverRegistry::new();
        registry.register(driver);
        let registry = Arc::new(registry);
        let session_manager = Arc::new(SessionManager::new(registry));

        let session = session_manager
            .connect(ConnectionConfig {
                driver: "postgres".to_string(),
                environment: environment.to_string(),
                ..Default::default()
            })
            .await
            .expect("mock connect");

        let capture_dir =
            std::env::temp_dir().join(format!("qoredb_runner_{}", uuid::Uuid::new_v4()));

        let policy = SafetyPolicy {
            query_rate_limit_enabled: false,
            max_query_duration_ms: None,
            ..SafetyPolicy::default()
        };

        Harness {
            session_manager,
            query_manager: Arc::new(QueryManager::new()),
            query_rate_limiter: Arc::new(QueryRateLimiter::with_defaults()),
            query_cache: Arc::new(QueryCache::new()),
            interceptor: Arc::new(InterceptorPipeline::new(capture_dir.join("interceptor"))),
            policy,
            captures: CaptureStore::new(capture_dir.clone()),
            capture_dir,
            session,
        }
    }

    fn result(ids: &[i64]) -> QueryResult {
        QueryResult {
            columns: vec![ColumnInfo {
                name: "id".into(),
                data_type: "int".into(),
                nullable: false,
            }],
            rows: ids
                .iter()
                .map(|i| Row {
                    values: vec![Value::Int(*i)],
                })
                .collect(),
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }

    fn set_with(entries: Vec<ReplayEntry>) -> ReplaySet {
        ReplaySet {
            version: REPLAY_SET_VERSION,
            name: "checkout".to_string(),
            created_at: "2026-08-21T10:00:00Z".to_string(),
            source: ReplaySource {
                driver_id: "postgres".to_string(),
                connection_label: None,
                environment: "staging".to_string(),
            },
            ignored_columns: Vec::new(),
            redacted: false,
            entries,
        }
    }

    fn entry(order: u32, query: &str, is_mutation: bool, expected: ExpectedOutcome) -> ReplayEntry {
        ReplayEntry {
            id: uuid::Uuid::new_v4().to_string(),
            order,
            query: query.to_string(),
            driver_id: "postgres".to_string(),
            namespace: None,
            operation_type: if is_mutation { "delete" } else { "select" }.to_string(),
            is_mutation,
            expected: ExpectedOutcome { ..expected },
        }
    }

    fn expectation(rows: i64, digest: Option<&str>) -> ExpectedOutcome {
        ExpectedOutcome {
            execution_time_ms: 10.0,
            row_count: Some(rows),
            success: true,
            fingerprint: None,
            result_digest: digest.map(|d| d.to_string()),
        }
    }

    async fn run(
        harness: &Harness,
        set: &ReplaySet,
        options: ReplayRunOptions,
    ) -> Result<ReplayReport, String> {
        run_set(
            harness.services(),
            harness.session,
            &harness.session.0.to_string(),
            set,
            "checkout",
            "default",
            &options,
            &harness.captures,
            None,
            &Default::default(),
            &AtomicBool::new(false),
            |_| {},
        )
        .await
    }

    #[tokio::test]
    async fn an_unchanged_result_matches() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.add("SELECT id FROM orders", result(&[1, 2, 3]));
        let harness = harness(driver, "staging").await;

        let digest = crate::replay::digest::compute_digest(&result(&[1, 2, 3]), &[], 1000).digest;
        let set = set_with(vec![entry(
            1,
            "SELECT id FROM orders",
            false,
            expectation(3, Some(&digest)),
        )]);

        let report = run(&harness, &set, ReplayRunOptions::default())
            .await
            .unwrap();
        assert_eq!(report.summary.matched, 1);
        assert_eq!(report.results[0].verdict, ReplayVerdict::Match);
        assert!(report.results[0].captured);
    }

    #[tokio::test]
    async fn a_renamed_column_breaks_the_entry() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.add_err("SELECT customer_id FROM orders", "column does not exist");
        let harness = harness(driver, "staging").await;

        let set = set_with(vec![entry(
            1,
            "SELECT customer_id FROM orders",
            false,
            expectation(3, Some("sha256:whatever")),
        )]);

        let report = run(&harness, &set, ReplayRunOptions::default())
            .await
            .unwrap();
        assert_eq!(report.summary.broken, 1);
        assert_eq!(report.results[0].verdict, ReplayVerdict::Broken);
        assert!(report.results[0].error.is_some());
    }

    #[tokio::test]
    async fn changed_content_at_equal_row_count_is_a_digest_diff() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.add("SELECT id FROM orders", result(&[1, 2, 9]));
        let harness = harness(driver, "staging").await;

        let digest = crate::replay::digest::compute_digest(&result(&[1, 2, 3]), &[], 1000).digest;
        let set = set_with(vec![entry(
            1,
            "SELECT id FROM orders",
            false,
            expectation(3, Some(&digest)),
        )]);

        let report = run(&harness, &set, ReplayRunOptions::default())
            .await
            .unwrap();
        assert_eq!(report.results[0].verdict, ReplayVerdict::DigestDiff);
    }

    #[tokio::test]
    async fn mutations_are_excluded_unless_enabled() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.set_default(result(&[]));
        let harness = harness(Arc::clone(&driver), "staging").await;

        let set = set_with(vec![entry(
            1,
            "DELETE FROM orders WHERE id = 1",
            true,
            expectation(1, None),
        )]);

        let report = run(&harness, &set, ReplayRunOptions::default())
            .await
            .unwrap();
        assert_eq!(report.results[0].verdict, ReplayVerdict::Skipped);
        assert_eq!(
            report.results[0].skip_reason.as_deref(),
            Some(MUTATION_EXCLUDED)
        );
        assert!(
            driver.calls().is_empty(),
            "the mutation must never reach the driver"
        );

        let options = ReplayRunOptions {
            allow_mutations: true,
            ..ReplayRunOptions::default()
        };
        let report = run(&harness, &set, options).await.unwrap();
        assert_ne!(report.results[0].verdict, ReplayVerdict::Skipped);
        assert!(!driver.calls().is_empty());
    }

    /// The set is a versioned file anyone can edit. A mutation declared as a
    /// read must still be refused: the classification that counts is the
    /// preflight's, computed from the query text.
    #[tokio::test]
    async fn a_forged_is_mutation_flag_does_not_get_a_write_executed() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.set_default(result(&[]));
        let harness = harness(Arc::clone(&driver), "staging").await;

        // `is_mutation: false` on a DELETE — what a tampered file looks like.
        let set = set_with(vec![entry(
            1,
            "DELETE FROM orders WHERE id = 1",
            false,
            expectation(1, None),
        )]);

        let report = run(&harness, &set, ReplayRunOptions::default())
            .await
            .unwrap();
        assert_eq!(report.results[0].verdict, ReplayVerdict::Skipped);
        assert_eq!(
            report.results[0].skip_reason.as_deref(),
            Some(MUTATION_EXCLUDED)
        );
        assert!(
            driver.calls().is_empty(),
            "the forged flag must not get the write past the guard"
        );
    }

    /// Same forgery, against production: refused even with mutations enabled.
    #[tokio::test]
    async fn a_forged_flag_is_refused_in_production_even_with_mutations_enabled() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.set_default(result(&[]));
        let harness = harness(Arc::clone(&driver), "production").await;

        let set = set_with(vec![entry(
            1,
            "INSERT INTO audit_log (message) VALUES ('x')",
            false,
            expectation(1, None),
        )]);

        let options = ReplayRunOptions {
            allow_mutations: true,
            ..ReplayRunOptions::default()
        };
        let report = run(&harness, &set, options).await.unwrap();

        assert_eq!(report.results[0].verdict, ReplayVerdict::Skipped);
        assert_eq!(
            report.results[0].skip_reason.as_deref(),
            Some(MUTATION_PRODUCTION_BLOCKED)
        );
        assert!(driver.calls().is_empty());
    }

    #[tokio::test]
    async fn production_refuses_mutations_even_when_enabled() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.set_default(result(&[]));
        let harness = harness(Arc::clone(&driver), "production").await;

        let set = set_with(vec![entry(
            1,
            "DELETE FROM orders WHERE id = 1",
            true,
            expectation(1, None),
        )]);

        let options = ReplayRunOptions {
            allow_mutations: true,
            ..ReplayRunOptions::default()
        };
        let report = run(&harness, &set, options).await.unwrap();

        // A DELETE is dangerous, so the production policy stops it in the
        // preflight, before the replay's own mutation guard is reached.
        assert_eq!(report.results[0].verdict, ReplayVerdict::Skipped);
        assert!(report.results[0].skip_reason.is_some());
        assert!(driver.calls().is_empty());
    }

    #[tokio::test]
    async fn production_never_writes_captured_rows() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.add("SELECT id FROM orders", result(&[1, 2, 3]));
        let harness = harness(driver, "production").await;

        let set = set_with(vec![entry(
            1,
            "SELECT id FROM orders",
            false,
            expectation(3, None),
        )]);

        let report = run(&harness, &set, ReplayRunOptions::default())
            .await
            .unwrap();
        assert_eq!(report.run.capture_mode, CaptureMode::MetadataOnly);
        assert_eq!(
            report.run.capture_stopped_reason,
            Some(CaptureStopReason::ProductionPolicy)
        );
        assert!(!report.results[0].captured);
        assert!(
            report.results[0].digest.is_some(),
            "digests still work without rows"
        );
    }

    #[tokio::test]
    async fn cancellation_stops_the_run() {
        let driver = Arc::new(MockDriver::new("postgres"));
        driver.set_default(result(&[1]));
        let harness = harness(Arc::clone(&driver), "staging").await;

        let set = set_with(vec![
            entry(1, "SELECT id FROM orders", false, expectation(1, None)),
            entry(2, "SELECT id FROM items", false, expectation(1, None)),
        ]);

        let cancel = AtomicBool::new(true);
        let outcome = run_set(
            harness.services(),
            harness.session,
            &harness.session.0.to_string(),
            &set,
            "checkout",
            "default",
            &ReplayRunOptions::default(),
            &harness.captures,
            None,
            &Default::default(),
            &cancel,
            |_| {},
        )
        .await;

        assert_eq!(outcome.unwrap_err(), CANCELLED);
        assert!(driver.calls().is_empty());
    }
}

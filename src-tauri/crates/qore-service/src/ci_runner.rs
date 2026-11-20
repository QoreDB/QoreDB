// SPDX-License-Identifier: BUSL-1.1

//! Headless CI runners for Data Contracts and Query Replay Sets.
//!
//! Shared by `qore-cli` so CI pipelines can evaluate `.qoredb/contracts/` and
//! `.qoredb/replays/` against an AgentVault connection without requiring a
//! desktop session or a Pro license key on the runner.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use qore_core::types::SessionId;

use crate::ServiceContext;
use crate::contracts::events::NoopSink;
use crate::contracts::parser::{Format, parse_contract};
use crate::contracts::runner::{RunOptions, RunnerError, run_contract};
use crate::contracts::storage;
use crate::contracts::{ContractRun, Rule, RuleStatus};
use crate::interceptor::{Environment, QuerySource, map_environment};
use crate::paths::QUERY_TIMEOUT_MS;
use crate::query as service_query;
use crate::replay::compare::{Observed, SlowerThresholds, classify, summarize};
use crate::replay::digest::compute_digest;
use crate::replay::{
    CaptureMode, ReplayEntry, ReplayEntryResult, ReplayReport, ReplayRunOptions, ReplaySetStore,
    ReplaySummary, ReplayVerdict, RunMeta, query_preview,
};

const CODE_MUTATION_EXCLUDED: &str = "mutation_excluded";
const CODE_MUTATION_PRODUCTION_BLOCKED: &str = "mutation_production_blocked";
const MUTATION_EXCLUDED: &str = "Mutation excluded from replay";
const MUTATION_PRODUCTION_BLOCKED: &str = "Mutations are never replayed against production";

/// How a CI run decides pass/fail beyond hard failures.
#[derive(Debug, Clone, Copy, Default)]
pub struct CiGate {
    /// Latency regressions are noisy on shared runners, so they only fail the
    /// run when the pipeline asks for it.
    pub fail_on_slower: bool,
}

/// Executes all contracts in `<workspace_root>/contracts/` (or the named one)
/// against `session`. Honors column masking rules attached to `session`.
/// `record_history` appends each run to `contracts/.history/`, which dirties
/// the checkout, so CI leaves it off unless asked.
pub async fn run_workspace_contracts(
    ctx: &ServiceContext,
    session: SessionId,
    connection_id: &str,
    workspace_root: &Path,
    contract_name: Option<&str>,
    record_history: bool,
) -> Result<Vec<ContractRun>, String> {
    let driver = ctx
        .session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;

    let sources: Vec<(String, String)> = if let Some(name) = contract_name {
        let src = storage::load_contract_source(workspace_root, name).map_err(|e| e.to_string())?;
        vec![(name.to_string(), src)]
    } else {
        let metas = storage::list_contracts(workspace_root).map_err(|e| e.to_string())?;
        let mut loaded = Vec::with_capacity(metas.len());
        for meta in metas {
            let src = storage::load_contract_source(workspace_root, &meta.name)
                .map_err(|e| e.to_string())?;
            loaded.push((meta.name, src));
        }
        loaded
    };

    let masking = ctx.session_manager.masking(session).await;
    let mut runs = Vec::with_capacity(sources.len());

    for (name, source) in sources {
        let contract = parse_contract(&source, Format::Auto).map_err(|e| e.to_string())?;

        if let Some(ref m) = masking {
            for rule in &contract.rules {
                if let Rule::CustomSql { sql, .. } = rule {
                    crate::masking_guard::check_agent_query(driver.driver_id(), sql, &m.config)?;
                }
            }
        }

        let mut run = run_contract(
            Arc::clone(&driver),
            session,
            connection_id.to_string(),
            &contract,
            RunOptions::default(),
            &NoopSink,
        )
        .await
        .map_err(|e| match e {
            RunnerError::UnknownDialect(d) => {
                format!("Driver '{d}' is not supported by Data Contracts")
            }
        })?;

        if let Some(ref m) = masking {
            for sample in run
                .results
                .iter_mut()
                .filter_map(|rule| rule.samples.as_mut())
                .flatten()
            {
                m.apply_json(Some(&contract.target.table), sample);
            }
        }

        if record_history && let Err(err) = storage::append_run(workspace_root, &name, &run) {
            tracing::warn!("failed to append contract run history in CI runner: {err}");
        }
        runs.push(run);
    }

    Ok(runs)
}

/// Replays a saved `.qoredb/replays/<slug>.qreplay.json` set headlessly
/// through the shared safety, rate-limit, audit, and column-masking pipeline.
pub async fn run_workspace_replay(
    ctx: &ServiceContext,
    session: SessionId,
    workspace_root: &Path,
    set_slug: &str,
) -> Result<ReplayReport, String> {
    let store = ReplaySetStore::new(workspace_root);
    let set = store.load(set_slug)?;
    if set.redacted {
        return Err(
            "This replay set was recorded with redaction: its queries cannot be replayed"
                .to_string(),
        );
    }

    let driver = ctx
        .session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;
    let driver_id = driver.driver_id().to_string();
    let environment_label = ctx
        .session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let is_production = matches!(map_environment(&environment_label), Environment::Production);
    let connection_label = ctx.session_manager.connection_key(session).await;
    let masking = ctx.session_manager.masking(session).await;

    let options = ReplayRunOptions::default();
    let thresholds = SlowerThresholds {
        ratio: options.slower_ratio,
        min_delta_ms: options.slower_min_delta_ms,
    };

    let session_id = session.0.to_string();
    let run_id = uuid::Uuid::new_v4().to_string();
    let started_at = chrono::Utc::now().to_rfc3339();
    let mut results = Vec::with_capacity(set.entries.len());

    for entry in &set.entries {
        if let Some(ref m) = masking
            && let Err(reason) =
                crate::masking_guard::check_agent_query(&driver_id, &entry.query, &m.config)
        {
            results.push(skipped_result(entry, reason, None));
            continue;
        }

        let preflight = match service_query::preflight_with_source(
            &ctx.session_manager,
            &ctx.query_rate_limiter,
            &ctx.interceptor,
            &ctx.policy,
            session,
            &session_id,
            &entry.query,
            entry.namespace.as_ref(),
            false,
            QuerySource::Cli,
        )
        .await
        {
            Ok(pf) => pf,
            Err(reason) => {
                results.push(skipped_result(entry, reason, None));
                continue;
            }
        };

        if preflight.is_mutation {
            let (code, reason) = if is_production {
                (
                    CODE_MUTATION_PRODUCTION_BLOCKED,
                    MUTATION_PRODUCTION_BLOCKED,
                )
            } else {
                (CODE_MUTATION_EXCLUDED, MUTATION_EXCLUDED)
            };
            results.push(skipped_result(entry, reason.to_string(), Some(code)));
            continue;
        }

        let query_id = ctx.query_manager.register(session).await;
        let outcome = service_query::execute(
            &ctx.query_manager,
            &ctx.query_cache,
            &ctx.interceptor,
            &ctx.policy,
            preflight.driver,
            &preflight.context,
            session,
            entry.namespace.clone(),
            &entry.query,
            query_id,
            preflight.is_mutation,
            preflight.connection_key.as_deref(),
            preflight.safety_warning.as_deref(),
            Some(QUERY_TIMEOUT_MS),
            false,
            None,
            None,
            preflight.masking.clone(),
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
                let d = compute_digest(result, &set.ignored_columns, options.max_captured_rows);
                (Some(d.digest), d.partial)
            }
            None => (None, false),
        };
        let row_count = outcome.result.as_ref().map(|r| r.rows.len() as i64);

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
            skip_code: None,
            execution_time_ms,
            expected_execution_time_ms: entry.expected.execution_time_ms,
            row_count,
            expected_row_count: entry.expected.row_count,
            digest,
            expected_digest: entry.expected.result_digest.clone(),
            captured: false,
            partial_comparison: partial,
        });
    }

    let summary: ReplaySummary = summarize(results.iter().map(|r| r.verdict));
    let run = RunMeta {
        run_id,
        project_id: workspace_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("cli")
            .to_string(),
        set_slug: set_slug.to_string(),
        set_name: set.name,
        started_at,
        finished_at: Some(chrono::Utc::now().to_rfc3339()),
        cancelled: false,
        connection_label,
        driver_id,
        environment: environment_label,
        capture_mode: CaptureMode::MetadataOnly,
        capture_stopped_reason: Some(crate::replay::CaptureStopReason::MetadataOnly),
        is_baseline: false,
        reference_generation: false,
        captured_bytes: 0,
        entry_count: set.entries.len(),
    };

    Ok(ReplayReport {
        run,
        baseline_run_id: set.baseline_run_id,
        results,
        summary,
    })
}

fn skipped_result(entry: &ReplayEntry, reason: String, code: Option<&str>) -> ReplayEntryResult {
    ReplayEntryResult {
        entry_id: entry.id.clone(),
        order: entry.order,
        query_preview: query_preview(&entry.query),
        verdict: ReplayVerdict::Skipped,
        success: false,
        error: None,
        skip_reason: Some(reason),
        skip_code: code.map(str::to_string),
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

pub fn contract_run_passed(run: &ContractRun) -> bool {
    run.fail_count == 0 && run.error_count == 0
}

/// A pipeline that checked nothing must not report green.
pub fn contract_runs_passed(runs: &[ContractRun]) -> bool {
    !runs.is_empty() && runs.iter().all(contract_run_passed)
}

pub fn replay_report_passed(report: &ReplayReport, gate: CiGate) -> bool {
    let summary = &report.summary;
    summary.total > summary.skipped
        && summary.broken == 0
        && summary.row_count_diff == 0
        && summary.digest_diff == 0
        && (!gate.fail_on_slower || summary.slower == 0)
}

fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn contracts_to_junit_xml(runs: &[ContractRun]) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites>\n");
    for run in runs {
        let skipped = run
            .results
            .iter()
            .filter(|r| matches!(r.status, RuleStatus::Skipped))
            .count();
        let tests = run.results.len();
        let time_secs = run.duration_ms as f64 / 1000.0;
        let _ = writeln!(
            xml,
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" time=\"{:.3}\">",
            escape_xml(&run.contract_name),
            tests,
            run.fail_count,
            run.error_count,
            skipped,
            time_secs
        );
        for rule in &run.results {
            let rule_time = rule.duration_ms as f64 / 1000.0;
            let _ = write!(
                xml,
                "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\"",
                escape_xml(&run.contract_name),
                escape_xml(&rule.id),
                rule_time
            );
            match rule.status {
                RuleStatus::Pass => xml.push_str(" />\n"),
                RuleStatus::Skipped => {
                    xml.push_str(">\n      <skipped");
                    if let Some(msg) = &rule.error {
                        let _ = write!(xml, " message=\"{}\"", escape_xml(msg));
                    }
                    xml.push_str(" />\n    </testcase>\n");
                }
                RuleStatus::Fail => {
                    let msg = format!(
                        "rule '{}' ({}) failed: violations={:?}, metric={:?}",
                        rule.id, rule.rule_type, rule.violations_count, rule.metric
                    );
                    let _ = write!(
                        xml,
                        ">\n      <failure message=\"{}\" />\n    </testcase>\n",
                        escape_xml(&msg)
                    );
                }
                RuleStatus::Error => {
                    let msg = rule.error.as_deref().unwrap_or("contract rule error");
                    let _ = write!(
                        xml,
                        ">\n      <error message=\"{}\" />\n    </testcase>\n",
                        escape_xml(msg)
                    );
                }
            }
        }
        xml.push_str("  </testsuite>\n");
    }
    xml.push_str("</testsuites>");
    xml
}

pub fn replay_to_junit_xml(report: &ReplayReport, gate: CiGate) -> String {
    let slower_failures = if gate.fail_on_slower {
        report.summary.slower
    } else {
        0
    };
    let failures = report.summary.row_count_diff + report.summary.digest_diff + slower_failures;
    let errors = report.summary.broken;
    let total_ms: f64 = report.results.iter().map(|r| r.execution_time_ms).sum();

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites>\n");
    let _ = writeln!(
        xml,
        "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" time=\"{:.3}\">",
        escape_xml(&report.run.set_name),
        report.summary.total,
        failures,
        errors,
        report.summary.skipped,
        total_ms / 1000.0
    );

    for entry in &report.results {
        let _ = write!(
            xml,
            "    <testcase classname=\"{}\" name=\"#{}: {}\" time=\"{:.3}\"",
            escape_xml(&report.run.set_slug),
            entry.order,
            escape_xml(&entry.query_preview),
            entry.execution_time_ms / 1000.0
        );
        match entry.verdict {
            ReplayVerdict::Match | ReplayVerdict::NotCompared => xml.push_str(" />\n"),
            ReplayVerdict::Slower if !gate.fail_on_slower => {
                let _ = write!(
                    xml,
                    ">\n      <system-out>{}</system-out>\n    </testcase>\n",
                    escape_xml(&format!(
                        "slower: expected_ms={:.2} actual_ms={:.2}",
                        entry.expected_execution_time_ms, entry.execution_time_ms
                    ))
                );
            }
            ReplayVerdict::Skipped => {
                xml.push_str(">\n      <skipped");
                if let Some(msg) = &entry.skip_reason {
                    let _ = write!(xml, " message=\"{}\"", escape_xml(msg));
                }
                xml.push_str(" />\n    </testcase>\n");
            }
            ReplayVerdict::Broken => {
                let msg = entry
                    .error
                    .as_deref()
                    .unwrap_or("query execution status changed");
                let _ = write!(
                    xml,
                    ">\n      <error message=\"{}\" />\n    </testcase>\n",
                    escape_xml(msg)
                );
            }
            ReplayVerdict::RowCountDiff | ReplayVerdict::DigestDiff | ReplayVerdict::Slower => {
                let msg = format!(
                    "verdict={:?} expected_rows={:?} actual_rows={:?} expected_ms={:.2} actual_ms={:.2}",
                    entry.verdict,
                    entry.expected_row_count,
                    entry.row_count,
                    entry.expected_execution_time_ms,
                    entry.execution_time_ms
                );
                let _ = write!(
                    xml,
                    ">\n      <failure message=\"{}\" />\n    </testcase>\n",
                    escape_xml(&msg)
                );
            }
        }
    }

    xml.push_str("  </testsuite>\n</testsuites>");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ContractRun, RuleResult, RuleStatus};
    use crate::replay::{CaptureMode, ReplayEntryResult, ReplaySummary, ReplayVerdict, RunMeta};

    #[test]
    fn junit_escapes_and_reports_contract_failures() {
        let run = ContractRun {
            contract_id: "users".into(),
            contract_name: "users & <orders>".into(),
            connection_id: "c1".into(),
            started_at: "2026-01-01T00:00:00Z".into(),
            finished_at: "2026-01-01T00:00:01Z".into(),
            duration_ms: 1200,
            pass_count: 1,
            fail_count: 1,
            error_count: 0,
            results: vec![
                RuleResult {
                    id: "not_null_email".into(),
                    rule_type: "not_null_pct".into(),
                    status: RuleStatus::Pass,
                    violations_count: None,
                    metric: Some(100.0),
                    samples: None,
                    duration_ms: 100,
                    error: None,
                },
                RuleResult {
                    id: "unique_id".into(),
                    rule_type: "unique".into(),
                    status: RuleStatus::Fail,
                    violations_count: Some(3),
                    metric: None,
                    samples: None,
                    duration_ms: 200,
                    error: None,
                },
            ],
        };

        assert!(!contract_run_passed(&run));
        let xml = contracts_to_junit_xml(&[run]);
        assert!(xml.contains("name=\"users &amp; &lt;orders&gt;\""));
        assert!(xml.contains("failures=\"1\""));
        assert!(xml.contains("<failure message="));
    }

    #[test]
    fn junit_reports_replay_regressions() {
        let report = ReplayReport {
            run: RunMeta {
                run_id: "r1".into(),
                project_id: "proj".into(),
                set_slug: "smoke".into(),
                set_name: "Smoke".into(),
                started_at: "2026-01-01T00:00:00Z".into(),
                finished_at: Some("2026-01-01T00:00:01Z".into()),
                cancelled: false,
                connection_label: None,
                driver_id: "postgres".into(),
                environment: "staging".into(),
                capture_mode: CaptureMode::MetadataOnly,
                capture_stopped_reason: None,
                is_baseline: false,
                reference_generation: false,
                captured_bytes: 0,
                entry_count: 1,
            },
            baseline_run_id: None,
            results: vec![ReplayEntryResult {
                entry_id: "e1".into(),
                order: 1,
                query_preview: "SELECT * FROM users".into(),
                verdict: ReplayVerdict::DigestDiff,
                success: true,
                error: None,
                skip_reason: None,
                skip_code: None,
                execution_time_ms: 15.0,
                expected_execution_time_ms: 10.0,
                row_count: Some(2),
                expected_row_count: Some(2),
                digest: Some("new".into()),
                expected_digest: Some("old".into()),
                captured: false,
                partial_comparison: false,
            }],
            summary: ReplaySummary {
                total: 1,
                digest_diff: 1,
                ..Default::default()
            },
        };

        assert!(!replay_report_passed(&report, CiGate::default()));
        let xml = replay_to_junit_xml(&report, CiGate::default());
        assert!(xml.contains("failures=\"1\""));
        assert!(xml.contains("<failure message=\"verdict=DigestDiff"));
    }

    #[cfg(feature = "driver-sqlite")]
    mod workspace_runs {
        use super::*;
        use crate::cache::QueryCache;
        use crate::interceptor::InterceptorPipeline;
        use crate::license::LicenseManager;
        use crate::policy::SafetyPolicy;
        use crate::ratelimit::QueryRateLimiter;
        use crate::replay::{ExpectedOutcome, ReplayEntry, ReplaySet, ReplaySource};
        use crate::vault::VaultLock;
        use crate::vault::backend::MockProvider;
        use crate::virtual_relations::VirtualRelationStore;
        use qore_core::ConnectionConfig;
        use qore_core::registry::DriverRegistry;
        use qore_drivers::drivers::sqlite::SqliteDriver;
        use qore_drivers::query_manager::QueryManager;
        use qore_drivers::session_manager::SessionManager;

        struct Fixture {
            ctx: ServiceContext,
            session: SessionId,
            root: std::path::PathBuf,
            _dir: tempfile::TempDir,
        }

        async fn fixture() -> Fixture {
            let dir = tempfile::tempdir().unwrap();
            let mut registry = DriverRegistry::new();
            registry.register(Arc::new(SqliteDriver::new()));
            let registry = Arc::new(registry);
            let ctx = ServiceContext {
                registry: Arc::clone(&registry),
                session_manager: Arc::new(SessionManager::new(registry)),
                query_manager: Arc::new(QueryManager::new()),
                query_rate_limiter: Arc::new(QueryRateLimiter::with_defaults()),
                query_cache: Arc::new(QueryCache::new()),
                policy: SafetyPolicy {
                    prod_require_confirmation: true,
                    prod_block_dangerous_sql: false,
                    max_query_duration_ms: None,
                    max_result_rows: None,
                    max_concurrent_queries: None,
                    query_rate_limit_enabled: false,
                },
                interceptor: Arc::new(InterceptorPipeline::new(dir.path().join("interceptor"))),
                virtual_relations: Arc::new(VirtualRelationStore::new(dir.path().join("vr"))),
                vault_lock: VaultLock::new(Box::new(MockProvider::new())),
                license_manager: LicenseManager::new(Box::new(MockProvider::new())),
            };
            let config = ConnectionConfig {
                options: Default::default(),
                driver: "sqlite".to_string(),
                host: dir.path().join("ci.db").to_string_lossy().to_string(),
                port: 0,
                username: String::new(),
                password: String::new(),
                database: None,
                ssl: false,
                ssl_mode: None,
                environment: "development".to_string(),
                read_only: false,
                ssh_tunnel: None,
                pool_acquire_timeout_secs: None,
                pool_max_connections: None,
                pool_min_connections: None,
                proxy: None,
                mssql_auth: None,
                clickhouse_cluster: None,
                search_auth_mode: None,
                ssl_ca_cert: None,
                ssl_client_cert: None,
                ssl_client_key: None,
            };
            let session = crate::connection::connect(&ctx.session_manager, config)
                .await
                .unwrap();
            let driver = ctx.session_manager.get_driver(session).await.unwrap();
            for sql in [
                "CREATE TABLE users (id INTEGER PRIMARY KEY, city TEXT NOT NULL)",
                "INSERT INTO users (city) VALUES ('Paris'), ('Lyon'), ('Nice')",
            ] {
                driver
                    .execute(session, sql, qore_core::QueryId::new())
                    .await
                    .unwrap();
            }
            let root = dir.path().join(".qoredb");
            std::fs::create_dir_all(&root).unwrap();
            Fixture {
                ctx,
                session,
                root,
                _dir: dir,
            }
        }

        const CONTRACT: &str = r#"
name: users_quality
version: 1
target:
  connection: ci
  table: users
rules:
  - id: id_unique
    type: unique
    columns: [id]
  - id: city_known
    type: allowed_values
    column: city
    values: [Paris, Lyon]
"#;

        #[tokio::test]
        async fn contracts_fail_on_violations_and_skip_history_unless_asked() {
            let f = fixture().await;
            let empty = run_workspace_contracts(&f.ctx, f.session, "ci", &f.root, None, false)
                .await
                .unwrap();
            assert!(!contract_runs_passed(&empty));

            storage::save_contract_source(&f.root, "users_quality", CONTRACT).unwrap();
            let runs = run_workspace_contracts(&f.ctx, f.session, "ci", &f.root, None, false)
                .await
                .unwrap();
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].pass_count, 1);
            assert_eq!(runs[0].fail_count, 1);
            assert!(!contract_runs_passed(&runs));
            let history = f.root.join("contracts/.history/users_quality.jsonl");
            assert!(!history.exists());

            run_workspace_contracts(&f.ctx, f.session, "ci", &f.root, None, true)
                .await
                .unwrap();
            assert!(history.exists());
        }

        fn entry(id: &str, order: u32, query: &str, rows: i64, is_mutation: bool) -> ReplayEntry {
            ReplayEntry {
                id: id.into(),
                order,
                query: query.into(),
                driver_id: "sqlite".into(),
                namespace: None,
                operation_type: if is_mutation { "update" } else { "select" }.into(),
                is_mutation,
                expected: ExpectedOutcome {
                    execution_time_ms: 1_000.0,
                    row_count: Some(rows),
                    success: true,
                    fingerprint: None,
                    result_digest: None,
                },
            }
        }

        fn replay_set(entries: Vec<ReplayEntry>) -> ReplaySet {
            ReplaySet {
                version: 1,
                name: "Smoke".into(),
                created_at: "2026-01-01T00:00:00Z".into(),
                source: ReplaySource {
                    driver_id: "sqlite".into(),
                    connection_label: None,
                    environment: "development".into(),
                },
                ignored_columns: Vec::new(),
                redacted: false,
                entries,
                baseline_run_id: None,
            }
        }

        #[tokio::test]
        async fn replay_detects_row_count_regressions_and_excludes_mutations() {
            let f = fixture().await;
            let store = ReplaySetStore::new(&f.root);
            store
                .save(
                    "smoke",
                    &replay_set(vec![
                        entry("e1", 1, "SELECT id FROM users", 3, false),
                        entry(
                            "e2",
                            2,
                            "SELECT id FROM users WHERE city = 'Paris'",
                            5,
                            false,
                        ),
                        entry("e3", 3, "UPDATE users SET city = 'Lille'", 0, true),
                    ]),
                )
                .unwrap();
            let report = run_workspace_replay(&f.ctx, f.session, &f.root, "smoke")
                .await
                .unwrap();
            assert_eq!(report.summary.matched, 1);
            assert_eq!(report.summary.row_count_diff, 1);
            assert_eq!(report.summary.skipped, 1);
            assert_eq!(
                report.results[2].skip_code.as_deref(),
                Some(CODE_MUTATION_EXCLUDED)
            );
            assert!(!replay_report_passed(&report, CiGate::default()));
        }

        #[tokio::test]
        async fn replay_with_only_skipped_entries_does_not_pass() {
            let f = fixture().await;
            let store = ReplaySetStore::new(&f.root);
            store
                .save(
                    "writes",
                    &replay_set(vec![entry("e1", 1, "DELETE FROM users", 0, true)]),
                )
                .unwrap();
            let report = run_workspace_replay(&f.ctx, f.session, &f.root, "writes")
                .await
                .unwrap();
            assert_eq!(report.summary.skipped, 1);
            assert!(!replay_report_passed(&report, CiGate::default()));
        }
    }
}

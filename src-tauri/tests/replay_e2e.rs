// SPDX-License-Identifier: BUSL-1.1

//! Query Replay Lab against a real PostgreSQL from `docker-compose.yml`.
//!
//! This is the scenario the feature exists for: record a set, apply a
//! migration, replay, and read what broke. Unit tests cover the pieces against
//! a mock driver; only this one proves the whole path works on a real engine.
//!
//! Skips when PostgreSQL is unreachable, unless `QOREDB_TEST_POSTGRES_REQUIRED`
//! is set — same contract as `integration_databases.rs`.

#![cfg(feature = "pro")]

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use qore_core::registry::DriverRegistry;
use qore_drivers::query_manager::QueryManager;
use qore_drivers::session_manager::SessionManager;
use qore_service::cache::QueryCache;
use qore_service::interceptor::{
    Environment, InterceptorPipeline, QueryContext, QueryExecutionResult, QueryOperationType,
    QuerySource,
};
use qore_service::policy::SafetyPolicy;
use qore_service::ratelimit::QueryRateLimiter;
use qoredb_lib::engine::drivers::postgres::PostgresDriver;
use qoredb_lib::engine::error::{EngineError, EngineResult};
use qoredb_lib::engine::traits::DataEngine;
use qoredb_lib::engine::types::{ConnectionConfig, QueryId, SessionId};
use qoredb_lib::replay::capture::CaptureStore;
use qoredb_lib::replay::recorder::{Recorder, RecordingOptions};
use qoredb_lib::replay::runner::{ReplayServices, run_set};
use qoredb_lib::replay::secrets::SecretPolicy;
use qoredb_lib::replay::types::{CaptureMode, ReplayRunOptions, ReplayVerdict};

fn env_or_default(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn postgres_required() -> bool {
    matches!(
        std::env::var("QOREDB_TEST_POSTGRES_REQUIRED")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn postgres_config() -> ConnectionConfig {
    ConnectionConfig {
        driver: "postgres".to_string(),
        host: env_or_default("QOREDB_TEST_PG_HOST", "127.0.0.1"),
        port: env_or_default("QOREDB_TEST_PG_PORT", "54321")
            .parse()
            .unwrap_or(54321),
        username: env_or_default("QOREDB_TEST_PG_USER", "qoredb"),
        password: env_or_default("QOREDB_TEST_PG_PASSWORD", "qoredb_test"),
        database: Some(env_or_default("QOREDB_TEST_PG_DB", "testdb")),
        environment: "development".to_string(),
        ..Default::default()
    }
}

struct Harness {
    session_manager: Arc<SessionManager>,
    query_manager: Arc<QueryManager>,
    query_rate_limiter: Arc<QueryRateLimiter>,
    query_cache: Arc<QueryCache>,
    interceptor: Arc<InterceptorPipeline>,
    policy: SafetyPolicy,
    captures: CaptureStore,
    capture_dir: std::path::PathBuf,
    driver: Arc<dyn DataEngine>,
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

    async fn exec(&self, sql: &str) -> EngineResult<()> {
        self.driver
            .execute(self.session, sql, QueryId::new())
            .await
            .map(|_| ())
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.capture_dir);
    }
}

/// `Ok(None)` when PostgreSQL is unreachable and not required.
async fn harness_or_skip() -> EngineResult<Option<Harness>> {
    let config = postgres_config();
    let driver = Arc::new(PostgresDriver::new());

    if let Err(err) = driver.test_connection(&config).await {
        if postgres_required() {
            return Err(err);
        }
        eprintln!("replay_e2e skipped: PostgreSQL is unavailable: {err}");
        return Ok(None);
    }

    let mut registry = DriverRegistry::new();
    registry.register(driver);
    let registry = Arc::new(registry);
    let session_manager = Arc::new(SessionManager::new(Arc::clone(&registry)));
    let session = session_manager.connect(config).await?;
    let driver = session_manager.get_driver(session).await?;

    let capture_dir =
        std::env::temp_dir().join(format!("qoredb_replay_e2e_{}", uuid::Uuid::new_v4()));
    let policy = SafetyPolicy {
        query_rate_limit_enabled: false,
        max_query_duration_ms: None,
        ..SafetyPolicy::default()
    };

    Ok(Some(Harness {
        session_manager,
        query_manager: Arc::new(QueryManager::new()),
        query_rate_limiter: Arc::new(QueryRateLimiter::with_defaults()),
        query_cache: Arc::new(QueryCache::new()),
        interceptor: Arc::new(InterceptorPipeline::new(capture_dir.join("interceptor"))),
        policy,
        captures: CaptureStore::new(capture_dir.clone()),
        capture_dir,
        driver,
        session,
    }))
}

/// The connection a recording is bound to; anything else is ignored.
const RECORDING_SESSION: &str = "replay-e2e";

fn context(query: &str) -> QueryContext {
    QueryContext {
        session_id: RECORDING_SESSION.to_string(),
        query: query.to_string(),
        environment: Environment::Development,
        driver_id: "postgres".to_string(),
        database: Some("testdb".to_string()),
        operation_type: QueryOperationType::Select,
        is_mutation: false,
        is_dangerous: false,
        acknowledged: false,
        read_only: false,
        source: QuerySource::User,
    }
}

/// Records two SELECTs, renames a column one of them reads, replays, and
/// expects exactly that query to come back broken while the other still matches.
#[tokio::test]
async fn a_renamed_column_is_reported_as_broken_after_replay() -> EngineResult<()> {
    let Some(harness) = harness_or_skip().await? else {
        return Ok(());
    };

    let table = format!("qoredb_replay_{}", uuid::Uuid::new_v4().simple());
    harness
        .exec(&format!(
            "CREATE TABLE {table} (id int primary key, email text, updated_at timestamptz default now())"
        ))
        .await?;
    harness
        .exec(&format!(
            "INSERT INTO {table} (id, email) VALUES (1, 'a@b.c'), (2, 'd@e.f'), (3, 'g@h.i')"
        ))
        .await?;

    let stable_query = format!("SELECT id FROM {table} ORDER BY id");
    let fragile_query = format!("SELECT id, email FROM {table} ORDER BY id");

    let recorder = Recorder::new();
    recorder
        .start(
            RecordingOptions {
                name: "e2e".to_string(),
                session_id: RECORDING_SESSION.to_string(),
                project_id: "default".to_string(),
                workspace_path: std::env::temp_dir(),
                ignored_columns: vec!["updated_at".to_string()],
                capture_mode: CaptureMode::Full,
                max_captured_rows: 1000,
                capture_budget_bytes: 8 * 1024 * 1024,
                secret_policy: SecretPolicy::Warn,
                secret_patterns: Vec::new(),
            },
            "postgres".to_string(),
            None,
            "development".to_string(),
            false,
        )
        .map_err(EngineError::internal)?;

    for query in [&stable_query, &fragile_query] {
        let result = harness
            .driver
            .execute(harness.session, query, QueryId::new())
            .await?;
        recorder.record(
            &context(query),
            None,
            &QueryExecutionResult {
                success: true,
                error: None,
                execution_time_ms: result.execution_time_ms,
                row_count: None,
            },
            Some(&result),
            &harness.captures,
        );
    }

    let (set, baseline) = recorder.stop().expect("a recording was in progress");
    assert_eq!(set.entries.len(), 2);
    assert!(
        set.entries
            .iter()
            .all(|e| e.expected.result_digest.is_some()),
        "each entry carries a digest"
    );

    // The migration: the replayed set still asks for `email`.
    harness
        .exec(&format!(
            "ALTER TABLE {table} RENAME COLUMN email TO email_address"
        ))
        .await?;

    let report = run_set(
        harness.services(),
        harness.session,
        &harness.session.0.to_string(),
        &set,
        "e2e",
        "default",
        &ReplayRunOptions::default(),
        &harness.captures,
        Some(baseline.run_id.clone()),
        &Default::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .await
    .map_err(EngineError::internal)?;

    assert_eq!(report.summary.total, 2);
    assert_eq!(
        report.summary.broken, 1,
        "the renamed column breaks one query"
    );
    assert_eq!(report.summary.matched, 1, "the other is untouched");

    let broken = report
        .results
        .iter()
        .find(|r| r.verdict == ReplayVerdict::Broken)
        .expect("one broken entry");
    assert!(broken.query_preview.contains("email"));
    assert!(broken.error.is_some(), "the engine error is reported");

    let matched = report
        .results
        .iter()
        .find(|r| r.verdict == ReplayVerdict::Match)
        .expect("one matching entry");
    assert!(matched.captured, "its rows are on disk for the diff");
    assert_eq!(matched.row_count, Some(3));

    harness.exec(&format!("DROP TABLE {table}")).await?;
    harness.session_manager.disconnect(harness.session).await?;
    Ok(())
}

/// A column whose value moves between runs must be reported as a content
/// difference, and silenced once the column is ignored by the set.
#[tokio::test]
async fn changed_content_is_a_digest_diff_unless_the_column_is_ignored() -> EngineResult<()> {
    let Some(harness) = harness_or_skip().await? else {
        return Ok(());
    };

    let table = format!("qoredb_replay_{}", uuid::Uuid::new_v4().simple());
    harness
        .exec(&format!(
            "CREATE TABLE {table} (id int primary key, status text)"
        ))
        .await?;
    harness
        .exec(&format!(
            "INSERT INTO {table} (id, status) VALUES (1, 'pending'), (2, 'pending')"
        ))
        .await?;

    let query = format!("SELECT id, status FROM {table} ORDER BY id");

    let record = |ignored: Vec<String>| {
        let recorder = Recorder::new();
        recorder
            .start(
                RecordingOptions {
                    name: "e2e-digest".to_string(),
                    session_id: RECORDING_SESSION.to_string(),
                    project_id: "default".to_string(),
                    workspace_path: std::env::temp_dir(),
                    ignored_columns: ignored,
                    capture_mode: CaptureMode::Full,
                    max_captured_rows: 1000,
                    capture_budget_bytes: 8 * 1024 * 1024,
                    secret_policy: SecretPolicy::Warn,
                    secret_patterns: Vec::new(),
                },
                "postgres".to_string(),
                None,
                "development".to_string(),
                false,
            )
            .expect("recorder starts");
        recorder
    };

    for (ignored, expected) in [
        (Vec::new(), ReplayVerdict::DigestDiff),
        (vec!["status".to_string()], ReplayVerdict::Match),
    ] {
        harness
            .exec(&format!("UPDATE {table} SET status = 'pending'"))
            .await?;

        let recorder = record(ignored);
        let result = harness
            .driver
            .execute(harness.session, &query, QueryId::new())
            .await?;
        recorder.record(
            &context(&query),
            None,
            &QueryExecutionResult {
                success: true,
                error: None,
                execution_time_ms: result.execution_time_ms,
                row_count: None,
            },
            Some(&result),
            &harness.captures,
        );
        let (set, baseline) = recorder.stop().expect("a recording was in progress");

        harness
            .exec(&format!("UPDATE {table} SET status = 'shipped'"))
            .await?;

        let report = run_set(
            harness.services(),
            harness.session,
            &harness.session.0.to_string(),
            &set,
            "e2e-digest",
            "default",
            &ReplayRunOptions::default(),
            &harness.captures,
            Some(baseline.run_id.clone()),
            &Default::default(),
            &AtomicBool::new(false),
            |_| {},
        )
        .await
        .map_err(EngineError::internal)?;

        assert_eq!(
            report.results[0].verdict, expected,
            "row count is unchanged, only the content moved"
        );
        assert_eq!(report.results[0].row_count, Some(2));
    }

    harness.exec(&format!("DROP TABLE {table}")).await?;
    harness.session_manager.disconnect(harness.session).await?;
    Ok(())
}

/// The set is committed to a repository: it must never carry row values, even
/// when the recording captured them.
#[tokio::test]
async fn a_recorded_set_never_carries_row_values() -> EngineResult<()> {
    let Some(harness) = harness_or_skip().await? else {
        return Ok(());
    };

    let table = format!("qoredb_replay_{}", uuid::Uuid::new_v4().simple());
    harness
        .exec(&format!("CREATE TABLE {table} (id int, secret text)"))
        .await?;
    harness
        .exec(&format!(
            "INSERT INTO {table} (id, secret) VALUES (1, 'hunter2-should-never-be-versioned')"
        ))
        .await?;

    let query = format!("SELECT id, secret FROM {table}");
    let recorder = Recorder::new();
    recorder
        .start(
            RecordingOptions {
                name: "e2e-privacy".to_string(),
                session_id: RECORDING_SESSION.to_string(),
                project_id: "default".to_string(),
                workspace_path: std::env::temp_dir(),
                ignored_columns: Vec::new(),
                capture_mode: CaptureMode::Full,
                max_captured_rows: 1000,
                capture_budget_bytes: 8 * 1024 * 1024,
                secret_policy: SecretPolicy::Warn,
                secret_patterns: Vec::new(),
            },
            "postgres".to_string(),
            None,
            "development".to_string(),
            false,
        )
        .map_err(EngineError::internal)?;

    let result = harness
        .driver
        .execute(harness.session, &query, QueryId::new())
        .await?;
    recorder.record(
        &context(&query),
        None,
        &QueryExecutionResult {
            success: true,
            error: None,
            execution_time_ms: result.execution_time_ms,
            row_count: None,
        },
        Some(&result),
        &harness.captures,
    );
    let (set, run) = recorder.stop().expect("a recording was in progress");

    let serialized = serde_json::to_string(&set).expect("the set serializes");
    assert!(
        !serialized.contains("hunter2-should-never-be-versioned"),
        "the versioned set leaked a row value"
    );

    // The value does live in the local capture, which is what the diff reads.
    let snapshot = harness
        .captures
        .load_entry(&run.run_id, &set.entries[0].id)
        .map_err(EngineError::internal)?;
    assert_eq!(snapshot.rows.len(), 1);

    harness.exec(&format!("DROP TABLE {table}")).await?;
    harness.session_manager.disconnect(harness.session).await?;
    Ok(())
}

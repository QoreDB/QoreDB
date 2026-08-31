// SPDX-License-Identifier: Apache-2.0

//! Schema migration runner: applies/rolls back versioned migration files against
//! a live connection and tracks applied state in a `qoredb_migrations` table
//! inside the target database.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use qore_core::{DataEngine, Namespace, SessionId};
use qore_service::interceptor::{
    Environment, InterceptorPipeline, QueryContext, QueryExecutionResult, QueryOperationType,
    SafetyAction, map_environment,
};
use qore_service::policy::SafetyPolicy;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;

use crate::commands::parse_session_id;
use crate::commands::workspace::SharedWorkspaceManager;
use crate::commands::workspace_migrations::{
    lint_migrations, list_migration_filenames, read_migration_file, split_up_down, summarize,
};
use crate::engine::types::QueryId;
use crate::workspace::types::WorkspaceSource;

const HISTORY_TABLE: &str = "qoredb_migrations";
const MIGRATION_CONFIRMATION_ACTION: &str = "apply_migration";
/// Must stay aligned with `src/lib/migrations/drivers.ts`. These engines expose
/// raw SQL DDL with semantics understood by the migration splitter/runner.
const SCHEMA_MIGRATION_DRIVERS: &[&str] = &[
    "postgres",
    "cockroachdb",
    "yugabytedb",
    "mysql",
    "mariadb",
    "planetscale",
    "tidb",
    "starrocks",
    "doris",
    "singlestore",
    "sqlite",
    "duckdb",
    "motherduck",
    "sqlserver",
    "azuresql",
    "synapse",
    "timescaledb",
    "supabase",
    "neon",
];

fn schema_migration_driver_supported(driver_id: &str) -> bool {
    SCHEMA_MIGRATION_DRIVERS.contains(&driver_id)
}

fn consume_migration_confirmation(
    store: &crate::commands::confirmation::ConfirmationTokenStore,
    token: Option<&str>,
) -> Result<bool, String> {
    let Some(token) = token else {
        return Ok(false);
    };
    store.consume(MIGRATION_CONFIRMATION_ACTION, token)?;
    Ok(true)
}

/// Marks a checksum computed over the `up` script alone. Rows written before
/// this format hashed the whole file; the prefix lets both be verified on their
/// own terms instead of reporting every pre-existing row as drifted.
const CHECKSUM_V2_PREFIX: &str = "v2:";

/// Why a migration was refused before running. Machine-readable so the UI can
/// translate it rather than pattern-match an English string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationBlockReason {
    AlreadyApplied,
    NotApplied,
    AlreadyRolledBack,
    ChecksumMismatch,
    ConcurrentApply,
    DuplicateVersion,
    MalformedVersion,
    UnsplittableScript,
    SafetyBlocked,
    UnsupportedDriver,
    /// A run died part-way and the driver could not undo it.
    PartiallyApplied,
}

#[derive(Debug, Serialize)]
pub struct ApplyMigrationResponse {
    pub success: bool,
    pub execution_ms: u64,
    pub error: Option<String>,
    /// Index of the statement that failed, when the failure was in the script.
    pub failed_statement: Option<usize>,
    pub blocked_reason: Option<MigrationBlockReason>,
    /// True when re-calling with `force` would get past this refusal.
    pub overridable: bool,
}

#[derive(Debug, Serialize)]
pub struct MigrationStatusEntry {
    pub version: String,
    pub name: String,
    pub filename: String,
    /// "applied" | "pending" | "rolled_back" | "failed"
    pub status: String,
    pub applied_at: Option<String>,
    /// Direction that failed on a non-transactional path. Lets the UI resume a
    /// failed rollback as `down` instead of accidentally offering `up`.
    pub failed_direction: Option<&'static str>,
    /// True when an applied file was edited after being applied (checksum drift).
    pub checksum_mismatch: bool,
    /// True when another file claims the same version — they would share one
    /// history row, so neither can be tracked correctly.
    pub duplicate_version: bool,
    /// True when the filename doesn't parse as `<version>_<slug>.sql`.
    pub malformed: bool,
}

fn fail(msg: String) -> ApplyMigrationResponse {
    ApplyMigrationResponse {
        success: false,
        execution_ms: 0,
        error: Some(msg),
        failed_statement: None,
        blocked_reason: None,
        overridable: false,
    }
}

fn blocked(reason: MigrationBlockReason, msg: String, overridable: bool) -> ApplyMigrationResponse {
    ApplyMigrationResponse {
        success: false,
        execution_ms: 0,
        error: Some(msg),
        failed_statement: None,
        blocked_reason: Some(reason),
        overridable,
    }
}

fn checksum(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Checksum stored on new rows: SHA-256 of the `up` script only, so editing the
/// rollback section never reports as drift.
pub(crate) fn checksum_v2(file_content: &str) -> String {
    format!(
        "{CHECKSUM_V2_PREFIX}{}",
        checksum(&split_up_down(file_content).0)
    )
}

/// Whether `stored` still matches `file_content`, in either checksum format.
pub(crate) fn checksum_matches(stored: &str, file_content: &str) -> bool {
    match stored.strip_prefix(CHECKSUM_V2_PREFIX) {
        Some(hash) => hash == checksum(&split_up_down(file_content).0),
        // Legacy row: hashed the whole file. Judge it on its own terms.
        None => stored == checksum(file_content),
    }
}

/// Escapes a value as a single-quoted SQL string literal.
fn sql_str(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn is_sqlserver(driver_id: &str) -> bool {
    matches!(driver_id, "sqlserver" | "mssql" | "azuresql" | "synapse")
}

fn is_mysql_family(driver_id: &str) -> bool {
    matches!(
        driver_id,
        "mysql" | "mariadb" | "planetscale" | "tidb" | "starrocks" | "doris" | "singlestore"
    )
}

fn migration_namespace(database: &str) -> Option<Namespace> {
    let database = database.trim();
    (!database.is_empty()).then(|| Namespace {
        database: database.to_string(),
        schema: None,
    })
}

async fn execute_in_migration_database(
    driver: &Arc<dyn DataEngine>,
    session: SessionId,
    database: &str,
    sql: &str,
) -> qore_core::EngineResult<qore_core::QueryResult> {
    driver
        .execute_in_namespace(session, migration_namespace(database), sql, QueryId::new())
        .await
}

/// MySQL has more implicit-commit statements than the generic operation enum can
/// express (`RENAME TABLE`, `LOCK TABLES`, account-management statements, ...).
/// Use a positive list instead: only statements known to remain inside an
/// InnoDB transaction may opt into the transactional path. Unknown SQL is
/// deliberately conservative so a rollback can never pretend to undo a commit.
fn mysql_statement_is_transaction_safe(op: QueryOperationType, sql: &str) -> bool {
    if matches!(
        op,
        QueryOperationType::Select
            | QueryOperationType::Insert
            | QueryOperationType::Update
            | QueryOperationType::Delete
    ) {
        return true;
    }

    sql.trim_start()
        .split_whitespace()
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("REPLACE"))
}

/// DDL to create the history table if absent. Portable across the SQL drivers
/// except SQL Server, which lacks `CREATE TABLE IF NOT EXISTS`.
fn history_table_ddl(driver_id: &str) -> String {
    if is_sqlserver(driver_id) {
        format!(
            "IF NOT EXISTS (SELECT * FROM sys.tables WHERE name = '{HISTORY_TABLE}') \
             CREATE TABLE {HISTORY_TABLE} (version NVARCHAR(255) PRIMARY KEY, name NVARCHAR(MAX) NOT NULL, \
             checksum NVARCHAR(255) NOT NULL, applied_at NVARCHAR(64) NOT NULL, applied_by NVARCHAR(255), \
             execution_ms BIGINT, rolled_back_at NVARCHAR(64), failed_at NVARCHAR(64))"
        )
    } else {
        format!(
            "CREATE TABLE IF NOT EXISTS {HISTORY_TABLE} (version VARCHAR(255) PRIMARY KEY, name TEXT NOT NULL, \
             checksum TEXT NOT NULL, applied_at VARCHAR(64) NOT NULL, applied_by TEXT, \
             execution_ms BIGINT, rolled_back_at VARCHAR(64), failed_at VARCHAR(64))"
        )
    }
}

fn history_failed_column_probe() -> String {
    format!("SELECT failed_at FROM {HISTORY_TABLE} WHERE 1 = 0")
}

fn history_add_failed_column_ddl(driver_id: &str) -> String {
    if is_sqlserver(driver_id) {
        format!("ALTER TABLE {HISTORY_TABLE} ADD failed_at NVARCHAR(64) NULL")
    } else {
        format!("ALTER TABLE {HISTORY_TABLE} ADD COLUMN failed_at VARCHAR(64)")
    }
}

/// Creates the history table on first use and upgrades the pre-`failed_at`
/// schema in place. The final probe also tolerates a concurrent process winning
/// the ADD COLUMN race between our first probe and ALTER.
async fn prepare_history(
    driver: &Arc<dyn DataEngine>,
    session: SessionId,
    driver_id: &str,
    database: &str,
) -> Result<(), String> {
    execute_in_migration_database(driver, session, database, &history_table_ddl(driver_id))
        .await
        .map_err(|e| {
            format!(
                "Failed to prepare migration history: {}",
                e.sanitized_message()
            )
        })?;

    let probe = history_failed_column_probe();
    if execute_in_migration_database(driver, session, database, &probe)
        .await
        .is_ok()
    {
        return Ok(());
    }

    let alter = history_add_failed_column_ddl(driver_id);
    match execute_in_migration_database(driver, session, database, &alter).await {
        Ok(_) => Ok(()),
        Err(alter_error) => {
            // Another app process may have added the column after our probe.
            if execute_in_migration_database(driver, session, database, &probe)
                .await
                .is_ok()
            {
                Ok(())
            } else {
                Err(format!(
                    "Failed to upgrade migration history with `failed_at`: {}",
                    alter_error.sanitized_message()
                ))
            }
        }
    }
}

/// A migration's recorded state in the target database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistoryRow {
    pub checksum: String,
    pub applied_at: Option<String>,
    pub rolled_back_at: Option<String>,
    /// Set when a run failed and the driver could not roll it back, so the
    /// schema is in an unknown half-migrated state.
    pub failed_at: Option<String>,
}

/// The three states a recorded migration can be in. `failed` wins over
/// `rolled_back`: a rollback that died part-way sets both, and the unknown
/// schema state is what matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryState {
    Applied,
    RolledBack,
    Failed,
}

impl HistoryRow {
    pub(crate) fn state(&self) -> HistoryState {
        if self.failed_at.is_some() {
            HistoryState::Failed
        } else if self.rolled_back_at.is_some() {
            HistoryState::RolledBack
        } else {
            HistoryState::Applied
        }
    }

    fn failed_direction(&self) -> Option<&'static str> {
        (self.state() == HistoryState::Failed).then_some(if self.rolled_back_at.is_some() {
            "down"
        } else {
            "up"
        })
    }
}

pub(crate) struct GuardRefusal {
    pub reason: MigrationBlockReason,
    pub message: String,
    pub overridable: bool,
}

/// Decides whether an apply/rollback may start. Pure: no I/O, so the truth table
/// is testable on its own.
pub(crate) fn check_guard(
    is_up: bool,
    row: Option<&HistoryRow>,
    checksum_ok: bool,
    force: bool,
) -> Result<(), GuardRefusal> {
    let drift = |overridable: bool| GuardRefusal {
        reason: MigrationBlockReason::ChecksumMismatch,
        message: if overridable {
            "This migration file changed since it was applied. Applying it now may not match \
             what ran before — re-run with force to proceed anyway."
                .to_string()
        } else {
            "This migration file changed after it was applied. Fix the schema with a new \
             migration rather than re-running an edited one."
                .to_string()
        },
        overridable,
    };

    let Some(row) = row else {
        return if is_up {
            Ok(())
        } else {
            Err(GuardRefusal {
                reason: MigrationBlockReason::NotApplied,
                message: "This migration was never applied, so there is nothing to roll back."
                    .to_string(),
                overridable: false,
            })
        };
    };

    match (row.state(), is_up) {
        // A run died part-way on a driver that could not roll it back, so the
        // schema is in an unknown state. Neither direction is safe until a human
        // has looked; forcing declares the database cleaned up by hand.
        (HistoryState::Failed, _) if !force => Err(GuardRefusal {
            reason: MigrationBlockReason::PartiallyApplied,
            message: "A previous run of this migration failed part-way and could not be rolled \
                      back, so the schema state is unknown. Inspect the database, fix it by \
                      hand, then force to proceed."
                .to_string(),
            overridable: true,
        }),
        (HistoryState::Failed, _) => Ok(()),

        (HistoryState::Applied, true) if checksum_ok => Err(GuardRefusal {
            reason: MigrationBlockReason::AlreadyApplied,
            message: "This migration is already applied.".to_string(),
            overridable: false,
        }),
        // Forcing would re-run an edited `up` over a schema it no longer
        // describes. A new migration is the only safe repair.
        (HistoryState::Applied, true) => Err(drift(false)),

        (HistoryState::Applied, false) if checksum_ok || force => Ok(()),
        // The `down` on disk may not undo what the `up` actually did.
        (HistoryState::Applied, false) => Err(drift(true)),

        // Rolled back: re-applying is legitimate.
        (HistoryState::RolledBack, true) if checksum_ok || force => Ok(()),
        (HistoryState::RolledBack, true) => Err(drift(true)),
        (HistoryState::RolledBack, false) => Err(GuardRefusal {
            reason: MigrationBlockReason::AlreadyRolledBack,
            message: "This migration is already rolled back.".to_string(),
            overridable: false,
        }),
    }
}

/// Serialises concurrent `apply_migration` calls for the same (session, version).
/// A double-click dispatches two commands onto Tauri's async runtime; without
/// this both clear the guard before either writes its history row.
fn apply_locks() -> &'static Mutex<HashSet<(SessionId, String)>> {
    static LOCKS: OnceLock<Mutex<HashSet<(SessionId, String)>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// RAII claim, released on drop so no early return can leak the key.
struct ApplyClaim {
    key: (SessionId, String),
}

impl ApplyClaim {
    fn try_acquire(session: SessionId, version: &str) -> Option<Self> {
        let key = (session, version.to_string());
        let mut locks = apply_locks().lock().ok()?;
        locks.insert(key.clone()).then_some(Self { key })
    }
}

impl Drop for ApplyClaim {
    fn drop(&mut self) {
        if let Ok(mut locks) = apply_locks().lock() {
            locks.remove(&self.key);
        }
    }
}

/// Everything the runner needs, resolved by the Tauri command from app state.
/// Plain values rather than `State<_>` so tests can drive it with a mock driver.
pub(crate) struct MigrationRun<'a> {
    pub driver: Arc<dyn DataEngine>,
    pub interceptor: &'a InterceptorPipeline,
    pub policy: &'a SafetyPolicy,
    pub session: SessionId,
    pub session_id: &'a str,
    /// Connection identity, not the ephemeral session UUID.
    pub applied_by: &'a str,
    pub environment: Environment,
    pub database: &'a str,
    pub version: &'a str,
    pub name: &'a str,
    /// Raw file body — the runner derives both checksums from it.
    pub file_content: &'a str,
    /// The already-selected section (`up` or `down`).
    pub script: &'a str,
    pub is_up: bool,
    pub acknowledged: bool,
    pub force: bool,
}

fn exec_result(
    success: bool,
    error: Option<String>,
    ms: f64,
    rows: Option<i64>,
) -> QueryExecutionResult {
    QueryExecutionResult {
        success,
        error,
        execution_time_ms: ms,
        row_count: rows,
    }
}

/// Reads the migration's history row. A missing table means nothing has been
/// applied yet; any other error fails closed rather than pretending that.
async fn read_history_row(
    driver: &Arc<dyn DataEngine>,
    session: SessionId,
    database: &str,
    version: &str,
) -> Result<Option<HistoryRow>, String> {
    let sql = format!(
        "SELECT checksum, applied_at, rolled_back_at, failed_at FROM {HISTORY_TABLE} WHERE version = {}",
        sql_str(version)
    );
    let result = execute_in_migration_database(driver, session, database, &sql)
        .await
        .map_err(|e| {
            format!(
                "Failed to read migration history: {}",
                e.sanitized_message()
            )
        })?;

    Ok(result.rows.first().map(|row| {
        let cell = |i: usize| {
            row.values
                .get(i)
                .and_then(|v| v.as_text())
                .map(String::from)
        };
        HistoryRow {
            checksum: cell(0).unwrap_or_default(),
            applied_at: cell(1),
            rolled_back_at: cell(2),
            failed_at: cell(3),
        }
    }))
}

/// Applies or rolls back a migration. Pure orchestration over `DataEngine` +
/// `InterceptorPipeline` — no Tauri, no filesystem, no shared state.
///
/// Never returns `Err`: every refusal is a `success: false` response carrying a
/// `blocked_reason`, which is the shape the frontend already consumes.
pub(crate) async fn run_migration(run: MigrationRun<'_>) -> ApplyMigrationResponse {
    let driver = &run.driver;
    let driver_id = driver.driver_id().to_string();
    let is_production = matches!(run.environment, Environment::Production);

    // Defense in depth for internal callers: never let an unsupported engine
    // reach dialect selection or migration history, even if command preflight
    // is bypassed in a future code path.
    if !schema_migration_driver_supported(&driver_id) {
        return blocked(
            MigrationBlockReason::UnsupportedDriver,
            format!("Driver '{driver_id}' is not supported by the Migrations Manager"),
            false,
        );
    }

    if is_mysql_family(&driver_id) && migration_namespace(run.database).is_none() {
        return fail(
            "Select a target database before applying or rolling back a MySQL migration."
                .to_string(),
        );
    }

    let statements =
        match qore_sql::migration_split::split_migration_statements(&driver_id, run.script) {
            Ok(s) => s,
            Err(e) => {
                return blocked(
                    MigrationBlockReason::UnsplittableScript,
                    format!("Cannot split this migration safely: {e}"),
                    false,
                );
            }
        };
    if statements.is_empty() {
        return fail("Migration script has no statements".to_string());
    }

    // Phase A — classify and vet EVERY statement before executing any of them.
    // A migration half-applied because statement 7 was blocked is worse than one
    // that never started. `analyze_sql` is LRU-cached and `pre_execute` is a
    // local rule match, so the extra pass costs nothing.
    let mut planned: Vec<(usize, &str, QueryContext, Option<String>)> = Vec::new();
    for stmt in &statements {
        let analysis = match qore_sql::safety::analyze_sql(&driver_id, stmt.text) {
            Ok(a) => Some(a),
            Err(e) => {
                if is_production && (run.policy.prod_block_dangerous_sql || !run.acknowledged) {
                    return blocked(
                        MigrationBlockReason::SafetyBlocked,
                        format!(
                            "Statement {}: SQL could not be parsed for safety analysis ({e})",
                            stmt.index
                        ),
                        false,
                    );
                }
                None
            }
        };

        let ctx = run.interceptor.build_context(
            run.session_id,
            stmt.text,
            &driver_id,
            run.environment,
            // Read-only sessions were already refused by the caller's preflight.
            false,
            run.acknowledged,
            Some(run.database),
            analysis.as_ref(),
            false,
        );

        if is_production && analysis.map(|a| a.is_dangerous).unwrap_or(false) {
            let refuse = |msg: &str| -> Option<ApplyMigrationResponse> {
                Some(blocked(
                    MigrationBlockReason::SafetyBlocked,
                    format!("Statement {}: {msg}", stmt.index),
                    false,
                ))
            };
            if run.policy.prod_block_dangerous_sql {
                run.interceptor.post_execute(
                    &ctx,
                    &exec_result(
                        false,
                        Some("blocked by production policy".into()),
                        0.0,
                        None,
                    ),
                    true,
                    None,
                );
                return refuse("dangerous statement blocked by production policy").unwrap();
            }
            if run.policy.prod_require_confirmation && !run.acknowledged {
                run.interceptor.post_execute(
                    &ctx,
                    &exec_result(false, Some("confirmation required".into()), 0.0, None),
                    true,
                    None,
                );
                return refuse("dangerous statement requires confirmation").unwrap();
            }
        }

        let safety = run.interceptor.pre_execute(&ctx);
        if !safety.allowed {
            run.interceptor.post_execute(
                &ctx,
                &exec_result(false, safety.message.clone(), 0.0, None),
                true,
                safety.triggered_rule.as_deref(),
            );
            return ApplyMigrationResponse {
                success: false,
                execution_ms: 0,
                error: Some(format!(
                    "Statement {} blocked: {}",
                    stmt.index,
                    safety.message.unwrap_or_default()
                )),
                // The UI's contract is 0-based.
                failed_statement: Some(stmt.index - 1),
                blocked_reason: Some(MigrationBlockReason::SafetyBlocked),
                overridable: false,
            };
        }
        let warn = if matches!(safety.action, SafetyAction::Warn) {
            safety.triggered_rule.clone()
        } else {
            None
        };
        planned.push((stmt.index, stmt.text, ctx, warn));
    }

    // History preparation stays outside the migration transaction so it
    // persists on rollback. It also upgrades tables created before `failed_at`
    // existed, before any guard attempts to read that column.
    if let Err(msg) = prepare_history(driver, run.session, &driver_id, run.database).await {
        return fail(msg);
    }

    let row = match read_history_row(driver, run.session, run.database, run.version).await {
        Ok(r) => r,
        Err(msg) => return fail(msg),
    };
    let checksum_ok = row
        .as_ref()
        .map(|r| checksum_matches(&r.checksum, run.file_content))
        .unwrap_or(true);

    if let Err(refusal) = check_guard(run.is_up, row.as_ref(), checksum_ok, run.force) {
        return blocked(refusal.reason, refusal.message, refusal.overridable);
    }

    // MySQL reports transaction support, but many statements commit implicitly.
    // Only an all-safe DML/read script may use its transactional path; unknown
    // operations are treated conservatively because the generic enum does not
    // cover statements such as `RENAME TABLE` or `LOCK TABLES`.
    let transaction_safe = !is_mysql_family(&driver_id)
        || planned
            .iter()
            .all(|(_, sql, ctx, _)| mysql_statement_is_transaction_safe(ctx.operation_type, sql));
    let supports_tx =
        driver.supports_transactions_for_session(run.session).await && transaction_safe;

    if supports_tx {
        if let Err(e) = driver.begin_transaction(run.session).await {
            return fail(format!(
                "Failed to begin transaction: {}",
                e.sanitized_message()
            ));
        }
    }

    let now = chrono::Utc::now().to_rfc3339();

    // Claim before running the script: whoever writes the history row owns the
    // migration. This is what makes the guard above more than a TOCTOU read.
    if let Err(resp) = claim(&run, supports_tx, &now).await {
        return resp;
    }

    let started = std::time::Instant::now();
    for (index, sql, ctx, warn) in &planned {
        let t0 = std::time::Instant::now();
        let res = execute_in_migration_database(driver, run.session, run.database, sql).await;
        let ms = t0.elapsed().as_micros() as f64 / 1000.0;
        match res {
            Ok(r) => run.interceptor.post_execute(
                ctx,
                &exec_result(true, None, ms, r.affected_rows.map(|n| n as i64)),
                false,
                warn.as_deref(),
            ),
            Err(e) => {
                let msg = e.sanitized_message();
                run.interceptor.post_execute(
                    ctx,
                    &exec_result(false, Some(msg.clone()), ms, None),
                    false,
                    warn.as_deref(),
                );
                let marker_error = if supports_tx {
                    // The claim and every statement so far go away together.
                    let _ = driver.rollback(run.session).await;
                    None
                } else {
                    // Nothing can undo what already ran. Deleting the claim would
                    // report "pending" over a half-migrated schema, so record the
                    // failure instead and let a human resolve it.
                    let failed_at = chrono::Utc::now().to_rfc3339();
                    mark_failed(driver, run.session, run.database, run.version, &failed_at)
                        .await
                        .err()
                };
                return ApplyMigrationResponse {
                    success: false,
                    execution_ms: started.elapsed().as_millis() as u64,
                    error: Some(match (supports_tx, marker_error) {
                        (true, _) => msg,
                        (false, None) => format!(
                            "{msg}\n\nThis driver may commit statements implicitly, so the statements \
                             that already ran could not be undone. The migration is marked as \
                             failed: check the schema before retrying."
                        ),
                        (false, Some(marker)) => format!(
                            "{msg}\n\nThis driver may commit statements implicitly, so the statements \
                             that already ran could not be undone. QoreDB also could not mark the \
                             migration as failed ({marker}); the history state is uncertain and \
                             must be inspected manually."
                        ),
                    }),
                    failed_statement: Some(index - 1),
                    blocked_reason: None,
                    overridable: false,
                };
            }
        }
    }

    // Measured once, then used for both the history row and the response so the
    // two can never disagree.
    let elapsed_ms = started.elapsed().as_millis() as u64;

    if run.is_up {
        let sql = format!(
            "UPDATE {HISTORY_TABLE} SET execution_ms = {elapsed_ms} WHERE version = {}",
            sql_str(run.version)
        );
        if let Err(e) = execute_in_migration_database(driver, run.session, run.database, &sql).await
        {
            if supports_tx {
                let _ = driver.rollback(run.session).await;
            }
            return fail(format!(
                "Failed to record migration history: {}",
                e.sanitized_message()
            ));
        }
    }

    if supports_tx {
        if let Err(e) = driver.commit(run.session).await {
            return fail(format!("Failed to commit: {}", e.sanitized_message()));
        }
    }

    ApplyMigrationResponse {
        success: true,
        execution_ms: elapsed_ms,
        error: None,
        failed_statement: None,
        blocked_reason: None,
        overridable: false,
    }
}

/// Marks a run that could not be undone. The schema is neither migrated nor
/// untouched, and saying either would be a lie.
fn mark_failed_sql(version: &str, now: &str) -> String {
    format!(
        "UPDATE {HISTORY_TABLE} SET failed_at = {} WHERE version = {}",
        sql_str(now),
        sql_str(version)
    )
}

async fn mark_failed(
    driver: &Arc<dyn DataEngine>,
    session: SessionId,
    database: &str,
    version: &str,
    now: &str,
) -> Result<(), String> {
    let result =
        execute_in_migration_database(driver, session, database, &mark_failed_sql(version, now))
            .await
            .map_err(|e| e.sanitized_message())?;
    if result.affected_rows == Some(0) {
        return Err("the history claim no longer exists".to_string());
    }
    Ok(())
}

/// Writes the history row before the script runs. A lost race surfaces as a
/// primary-key conflict (up) or a zero-row update (down).
async fn claim(
    run: &MigrationRun<'_>,
    supports_tx: bool,
    now: &str,
) -> Result<(), ApplyMigrationResponse> {
    let driver = &run.driver;
    let abort = |resp: ApplyMigrationResponse| async move {
        if supports_tx {
            let _ = run.driver.rollback(run.session).await;
        }
        Err(resp)
    };

    if run.is_up {
        // Clear a row the guard already cleared us to reuse — rolled back, or
        // failed and force-retried — so the insert can claim it.
        let del = format!(
            "DELETE FROM {HISTORY_TABLE} WHERE version = {} \
             AND (rolled_back_at IS NOT NULL OR failed_at IS NOT NULL)",
            sql_str(run.version)
        );
        if let Err(e) = execute_in_migration_database(driver, run.session, run.database, &del).await
        {
            return abort(fail(format!(
                "Failed to record migration history: {}",
                e.sanitized_message()
            )))
            .await;
        }

        let ins = format!(
            "INSERT INTO {HISTORY_TABLE} (version, name, checksum, applied_at, applied_by, execution_ms, rolled_back_at, failed_at) \
             VALUES ({}, {}, {}, {}, {}, 0, NULL, NULL)",
            sql_str(run.version),
            sql_str(run.name),
            sql_str(&checksum_v2(run.file_content)),
            sql_str(now),
            sql_str(run.applied_by),
        );
        if execute_in_migration_database(driver, run.session, run.database, &ins)
            .await
            .is_err()
        {
            return abort(blocked(
                MigrationBlockReason::ConcurrentApply,
                "This migration is being applied by someone else.".to_string(),
                false,
            ))
            .await;
        }
        return Ok(());
    }

    // Matches an applied row, or a failed one the guard cleared us to retry.
    let upd = format!(
        "UPDATE {HISTORY_TABLE} SET rolled_back_at = {}, failed_at = NULL WHERE version = {} \
         AND (rolled_back_at IS NULL OR failed_at IS NOT NULL)",
        sql_str(now),
        sql_str(run.version)
    );
    match execute_in_migration_database(driver, run.session, run.database, &upd).await {
        Ok(r) => {
            if r.affected_rows == Some(0) {
                return abort(blocked(
                    MigrationBlockReason::ConcurrentApply,
                    "This migration was rolled back by someone else.".to_string(),
                    false,
                ))
                .await;
            }
            Ok(())
        }
        Err(e) => {
            abort(fail(format!(
                "Failed to record migration history: {}",
                e.sanitized_message()
            )))
            .await
        }
    }
}

/// Applies (`up`) or rolls back (`down`) a migration against the session's
/// database, recording the result in the history table — all transactionally
/// when the driver supports it.
#[tauri::command]
pub async fn apply_migration(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    session_id: String,
    filename: String,
    direction: String,
    database: String,
    confirmation_token: Option<String>,
    force: Option<bool>,
) -> Result<ApplyMigrationResponse, String> {
    let (content, siblings) = {
        let mgr = ws_manager.lock().await;
        let ws = mgr.active();
        if ws.source == WorkspaceSource::Default {
            return Ok(fail(
                "Migrations require a file-based workspace".to_string(),
            ));
        }
        (
            read_migration_file(&ws.path, &filename)?,
            list_migration_filenames(&ws.path).unwrap_or_default(),
        )
    };

    let summary = summarize(&filename);

    // The version prefix is the history table's primary key. A duplicate means
    // two files share one row; a malformed name means the version — and so the
    // ordering — is meaningless. Refuse before touching the database.
    let issues = lint_migrations(&siblings);
    if issues.iter().any(|i| i.affects_duplicate(&filename)) {
        return Ok(blocked(
            MigrationBlockReason::DuplicateVersion,
            format!(
                "Version {} is used by more than one migration file. Renumber them before applying.",
                summary.version
            ),
            false,
        ));
    }
    if issues.iter().any(|i| i.affects_malformed(&filename)) {
        return Ok(blocked(
            MigrationBlockReason::MalformedVersion,
            format!(
                "`{filename}` must be named `<version>_<name>.sql`, e.g. 0001_create_users.sql."
            ),
            false,
        ));
    }

    let is_up = match direction.as_str() {
        "up" => true,
        "down" => false,
        _ => {
            return Ok(fail(
                "Migration direction must be 'up' or 'down'".to_string(),
            ));
        }
    };
    let (up, down) = split_up_down(&content);
    let script = if is_up { up } else { down };
    if script.trim().is_empty() {
        return Ok(fail(format!("Migration has no {} script", direction)));
    }

    let (session_manager, interceptor, policy, confirmation_tokens) = {
        let guard = state.lock().await;
        (
            Arc::clone(&guard.session_manager),
            Arc::clone(&guard.interceptor),
            guard.policy.clone(),
            Arc::clone(&guard.confirmation_tokens),
        )
    };
    let session = parse_session_id(&session_id)?;
    let session_driver = session_manager
        .get_driver(session)
        .await
        .map_err(|error| error.sanitized_message())?;
    let driver_id = session_driver.driver_id();
    if !schema_migration_driver_supported(driver_id) {
        return Ok(blocked(
            MigrationBlockReason::UnsupportedDriver,
            format!("Driver '{driver_id}' is not supported by the Migrations Manager"),
            false,
        ));
    }

    let acknowledged = match consume_migration_confirmation(
        confirmation_tokens.as_ref(),
        confirmation_token.as_deref(),
    ) {
        Ok(acknowledged) => acknowledged,
        Err(error) => return Ok(blocked(MigrationBlockReason::SafetyBlocked, error, false)),
    };

    // Session-level gate: read-only, driver capabilities, and the driver handle.
    // Its context is deliberately discarded — it hardcodes `sql_analysis: None`,
    // so it can't classify individual statements. `run_migration` builds its own.
    let preflight = match qore_service::mutation::preflight(
        &session_manager,
        &interceptor,
        session,
        &session_id,
        &script,
        &database,
        acknowledged,
    )
    .await
    {
        Ok(pf) => pf,
        Err(msg) => return Ok(fail(msg)),
    };

    let environment = session_manager
        .get_environment(session)
        .await
        .unwrap_or_else(|_| "development".to_string());
    let connection_key = session_manager.connection_key(session).await;
    let applied_by = connection_key
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    let Some(_claim) = ApplyClaim::try_acquire(session, &summary.version) else {
        return Ok(blocked(
            MigrationBlockReason::ConcurrentApply,
            "This migration is already being applied.".to_string(),
            false,
        ));
    };

    let response = run_migration(MigrationRun {
        driver: preflight.driver,
        interceptor: &interceptor,
        policy: &policy,
        session,
        session_id: &session_id,
        applied_by: &applied_by,
        environment: map_environment(&environment),
        database: &database,
        version: &summary.version,
        name: &summary.name,
        file_content: &content,
        script: &script,
        is_up,
        acknowledged,
        force: force.unwrap_or(false),
    })
    .await;

    // Schema likely changed — drop cached previews for this connection.
    if response.success {
        if let Some(key) = connection_key {
            let query_cache = {
                let guard = state.lock().await;
                Arc::clone(&guard.query_cache)
            };
            query_cache.invalidate_connection(&key);
        }
    }

    Ok(response)
}

/// Returns the applied/pending status of the active workspace's migrations for
/// the given connection. None if the workspace is the default.
#[tauri::command]
pub async fn get_migration_status(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    session_id: String,
    database: Option<String>,
) -> Result<Option<Vec<MigrationStatusEntry>>, String> {
    let files: Vec<(String, String)> = {
        let mgr = ws_manager.lock().await;
        let ws = mgr.active();
        if ws.source == WorkspaceSource::Default {
            return Ok(None);
        }
        let dir = ws.path.join("migrations");
        if !dir.exists() {
            return Ok(Some(Vec::new()));
        }
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .map_err(|e| format!("Failed to read migrations: {}", e))?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.ends_with(".sql"))
            .collect();
        names.sort();
        names
            .into_iter()
            .map(|name| {
                let content = std::fs::read_to_string(dir.join(&name)).unwrap_or_default();
                (name, content)
            })
            .collect()
    };

    let filenames: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
    let issues = lint_migrations(&filenames);

    let session_manager = {
        let guard = state.lock().await;
        Arc::clone(&guard.session_manager)
    };
    let session = parse_session_id(&session_id)?;
    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;
    let database = database.unwrap_or_default();

    // Reading status must remain compatible with history tables created before
    // `failed_at`. Applying a migration upgrades the table; until then, fall
    // back to the legacy projection instead of reporting every file as pending.
    let history: HashMap<String, HistoryRow> = {
        let current = format!(
            "SELECT version, checksum, applied_at, rolled_back_at, failed_at FROM {HISTORY_TABLE}"
        );
        let legacy =
            format!("SELECT version, checksum, applied_at, rolled_back_at FROM {HISTORY_TABLE}");
        let (result, has_failed_at) =
            match execute_in_migration_database(&driver, session, &database, &current).await {
                Ok(result) => (Some(result), true),
                Err(_) => match execute_in_migration_database(&driver, session, &database, &legacy)
                    .await
                {
                    Ok(result) => (Some(result), false),
                    // An absent history table still means nothing has been applied.
                    Err(_) => (None, false),
                },
            };

        result
            .map(|result| {
                result
                    .rows
                    .iter()
                    .filter_map(|row| {
                        let cell = |i: usize| row.values.get(i).and_then(|v| v.as_text());
                        let version = cell(0)?.to_string();
                        Some((
                            version,
                            HistoryRow {
                                checksum: cell(1).unwrap_or("").to_string(),
                                applied_at: cell(2).map(|s| s.to_string()),
                                rolled_back_at: cell(3).map(|s| s.to_string()),
                                failed_at: if has_failed_at {
                                    cell(4).map(str::to_string)
                                } else {
                                    None
                                },
                            },
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    let entries = files
        .iter()
        .map(|(filename, content)| {
            let summary = summarize(filename);
            let duplicate_version = issues.iter().any(|i| i.affects_duplicate(filename));
            let malformed = issues.iter().any(|i| i.affects_malformed(filename));
            match history.get(&summary.version) {
                Some(row) => {
                    let state = row.state();
                    MigrationStatusEntry {
                        version: summary.version,
                        name: summary.name,
                        filename: filename.clone(),
                        status: match state {
                            HistoryState::Applied => "applied",
                            HistoryState::RolledBack => "rolled_back",
                            HistoryState::Failed => "failed",
                        }
                        .to_string(),
                        applied_at: row.applied_at.clone(),
                        failed_direction: row.failed_direction(),
                        checksum_mismatch: state == HistoryState::Applied
                            && !checksum_matches(&row.checksum, content),
                        duplicate_version,
                        malformed,
                    }
                }
                None => MigrationStatusEntry {
                    version: summary.version,
                    name: summary.name,
                    filename: filename.clone(),
                    status: "pending".to_string(),
                    applied_at: None,
                    failed_direction: None,
                    checksum_mismatch: false,
                    duplicate_version,
                    malformed,
                },
            }
        })
        .collect();

    Ok(Some(entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testing::{DriverCall, MockDriver, affected_result, empty_result};
    use qore_core::types::{ColumnInfo, QueryResult, Row, Value};

    const FILE: &str =
        "-- migrate:up\nCREATE TABLE t (id int);\n\n-- migrate:down\nDROP TABLE t;\n";

    #[test]
    fn migration_confirmation_requires_a_matching_single_use_token() {
        let store = crate::commands::confirmation::ConfirmationTokenStore::new();
        assert!(!consume_migration_confirmation(&store, None).unwrap());

        let (wrong_action, _) = store.issue("clear_audit_log");
        assert!(consume_migration_confirmation(&store, Some(&wrong_action)).is_err());

        let (token, _) = store.issue(MIGRATION_CONFIRMATION_ACTION);
        assert!(consume_migration_confirmation(&store, Some(&token)).unwrap());
        assert!(consume_migration_confirmation(&store, Some(&token)).is_err());
    }

    fn row(checksum: &str, rolled_back: Option<&str>) -> HistoryRow {
        HistoryRow {
            checksum: checksum.to_string(),
            applied_at: Some("2026-01-01T00:00:00Z".to_string()),
            rolled_back_at: rolled_back.map(String::from),
            failed_at: None,
        }
    }

    fn failed_row(checksum: &str) -> HistoryRow {
        HistoryRow {
            failed_at: Some("2026-01-02T00:00:00Z".to_string()),
            ..row(checksum, None)
        }
    }

    fn text_col(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
        }
    }

    fn history_columns() -> Vec<ColumnInfo> {
        vec![
            text_col("checksum"),
            text_col("applied_at"),
            text_col("rolled_back_at"),
            text_col("failed_at"),
        ]
    }

    fn history_result(
        checksum: &str,
        rolled_back: Option<&str>,
        failed: Option<&str>,
    ) -> QueryResult {
        QueryResult {
            columns: history_columns(),
            rows: vec![Row {
                values: vec![
                    Value::Text(checksum.into()),
                    Value::Text("2026-01-01T00:00:00Z".into()),
                    rolled_back.map_or(Value::Null, |s| Value::Text(s.into())),
                    failed.map_or(Value::Null, |s| Value::Text(s.into())),
                ],
            }],
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }

    fn empty_history() -> QueryResult {
        QueryResult {
            columns: history_columns(),
            rows: Vec::new(),
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }

    struct Harness {
        _dir: tempfile::TempDir,
        interceptor: InterceptorPipeline,
        policy: SafetyPolicy,
    }

    fn harness() -> Harness {
        let dir = tempfile::tempdir().expect("tempdir");
        let interceptor = InterceptorPipeline::new(dir.path().join("interceptor"));
        Harness {
            _dir: dir,
            interceptor,
            policy: SafetyPolicy {
                prod_require_confirmation: true,
                prod_block_dangerous_sql: false,
                max_query_duration_ms: None,
                max_result_rows: None,
                max_concurrent_queries: None,
                query_rate_limit_enabled: false,
            },
        }
    }

    fn run<'a>(
        h: &'a Harness,
        driver: Arc<dyn DataEngine>,
        is_up: bool,
        script: &'a str,
        environment: Environment,
    ) -> MigrationRun<'a> {
        MigrationRun {
            driver,
            interceptor: &h.interceptor,
            policy: &h.policy,
            session: SessionId::new(),
            session_id: "sess-1",
            applied_by: "conn-1",
            environment,
            database: "app",
            version: "0001",
            name: "create_t",
            file_content: FILE,
            script,
            is_up,
            acknowledged: false,
            force: false,
        }
    }

    fn executed(driver: &MockDriver) -> Vec<String> {
        driver
            .call_log()
            .into_iter()
            .filter_map(|c| match c {
                DriverCall::Execute(q) => Some(q),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn schema_migration_driver_allowlist_matches_the_frontend_contract() {
        assert_eq!(
            SCHEMA_MIGRATION_DRIVERS,
            &[
                "postgres",
                "cockroachdb",
                "yugabytedb",
                "mysql",
                "mariadb",
                "planetscale",
                "tidb",
                "starrocks",
                "doris",
                "singlestore",
                "sqlite",
                "duckdb",
                "motherduck",
                "sqlserver",
                "azuresql",
                "synapse",
                "timescaledb",
                "supabase",
                "neon",
            ]
        );
        for unsupported in [
            "clickhouse",
            "mongodb",
            "documentdb",
            "redis",
            "valkey",
            "dragonfly",
            "keydb",
            "garnet",
            "elasticsearch",
            "opensearch",
        ] {
            assert!(!schema_migration_driver_supported(unsupported));
        }
    }

    #[tokio::test]
    async fn unsupported_driver_is_refused_before_any_database_call() {
        let h = harness();
        let driver = Arc::new(MockDriver::new("clickhouse"));
        let response = run_migration(run(
            &h,
            driver.clone(),
            true,
            "CREATE TABLE t (id Int64)",
            Environment::Development,
        ))
        .await;

        assert!(!response.success);
        assert_eq!(
            response.blocked_reason,
            Some(MigrationBlockReason::UnsupportedDriver)
        );
        assert!(driver.call_log().is_empty());
    }

    #[test]
    fn migration_namespace_rejects_blank_database_names() {
        assert_eq!(migration_namespace("  "), None);
        assert_eq!(
            migration_namespace(" app ").map(|ns| ns.database),
            Some("app".into())
        );
    }

    #[tokio::test]
    async fn down_on_unapplied_migration_never_executes_the_script() {
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            false,
            "DROP TABLE t;",
            Environment::Development,
        ))
        .await;

        assert!(!resp.success);
        assert_eq!(resp.blocked_reason, Some(MigrationBlockReason::NotApplied));
        // The whole point: the destructive script must never have run.
        assert!(
            !executed(&driver).iter().any(|q| q.contains("DROP TABLE t")),
            "rollback script ran despite the migration never being applied"
        );
    }

    #[tokio::test]
    async fn mysql_requires_a_target_database_before_touching_history() {
        let driver = Arc::new(MockDriver::new("mysql").with_transactions(true));
        driver.set_default(empty_result());
        let h = harness();
        let d = Arc::clone(&driver);
        let mut migration = run(
            &h,
            d,
            true,
            "CREATE TABLE t (id int);",
            Environment::Development,
        );
        migration.database = "";

        let response = run_migration(migration).await;

        assert!(!response.success);
        assert!(
            response
                .error
                .is_some_and(|error| error.contains("target database"))
        );
        assert!(driver.calls().is_empty());
    }

    #[tokio::test]
    async fn up_already_applied_is_refused_without_touching_the_database() {
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add(
            "SELECT checksum",
            history_result(&checksum_v2(FILE), None, None),
        );
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "CREATE TABLE t (id int);",
            Environment::Development,
        ))
        .await;

        assert_eq!(
            resp.blocked_reason,
            Some(MigrationBlockReason::AlreadyApplied)
        );
        assert!(
            !executed(&driver)
                .iter()
                .any(|q| q.contains("CREATE TABLE t"))
        );
        assert!(!driver.call_log().contains(&DriverCall::Begin));
    }

    #[tokio::test]
    async fn up_claims_history_before_running_statements() {
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "CREATE TABLE t (id int);",
            Environment::Development,
        ))
        .await;
        assert!(resp.success, "{:?}", resp.error);
        assert!(driver.namespace_calls().iter().all(|namespace| {
            namespace
                .as_ref()
                .is_some_and(|namespace| namespace.database == "app")
        }));

        let log = driver.call_log();
        let pos = |needle: &str| {
            log.iter()
                .position(|c| matches!(c, DriverCall::Execute(q) if q.contains(needle)))
        };
        let begin = log
            .iter()
            .position(|c| *c == DriverCall::Begin)
            .expect("begin");
        let ddl = pos("CREATE TABLE IF NOT EXISTS qoredb_migrations").expect("history ddl");
        let insert = pos("INSERT INTO qoredb_migrations").expect("claim");
        let script = pos("CREATE TABLE t (id int)").expect("script");
        let commit = log
            .iter()
            .position(|c| *c == DriverCall::Commit)
            .expect("commit");

        // History DDL stays outside the transaction; the claim is inside it and
        // precedes the script, so a lost race can't run the script twice.
        assert!(ddl < begin, "history DDL must run before BEGIN");
        assert!(begin < insert, "claim must be inside the transaction");
        assert!(insert < script, "claim must precede the script");
        assert!(script < commit);
    }

    #[tokio::test]
    async fn failed_statement_rolls_back_and_reports_zero_based_index() {
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.add_err("CREATE TABLE b", "boom");
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "CREATE TABLE a (id int);\nCREATE TABLE b (id int);",
            Environment::Development,
        ))
        .await;

        assert!(!resp.success);
        assert_eq!(resp.failed_statement, Some(1));
        assert_eq!(
            *driver.call_log().last().expect("last"),
            DriverCall::Rollback
        );
    }

    #[tokio::test]
    async fn mysql_ddl_migration_runs_without_a_transaction() {
        // The real MySQL driver reports transaction support, but DDL commits
        // implicitly — wrapping it in BEGIN/ROLLBACK would only look safe.
        let driver = Arc::new(MockDriver::new("mysql").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "CREATE TABLE a (id int);",
            Environment::Development,
        ))
        .await;

        assert!(resp.success, "{:?}", resp.error);
        assert!(!driver.call_log().contains(&DriverCall::Begin));
    }

    #[tokio::test]
    async fn mysql_dml_only_migration_still_uses_a_transaction() {
        // DML rolls back fine on MySQL; only DDL forfeits the transaction.
        let driver = Arc::new(MockDriver::new("mysql").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "INSERT INTO a VALUES (1);",
            Environment::Development,
        ))
        .await;

        assert!(resp.success, "{:?}", resp.error);
        assert!(driver.call_log().contains(&DriverCall::Begin));
    }

    #[tokio::test]
    async fn mysql_rename_then_failure_marks_partial_instead_of_rolling_back() {
        // `RENAME TABLE` is classified as Other by the generic first-keyword
        // mapper, but MySQL commits it implicitly. Unknown operations must stay
        // on the conservative non-transactional path.
        let driver = Arc::new(MockDriver::new("mysql").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.add_err("INSERT INTO missing", "table does not exist");
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "RENAME TABLE users TO old_users; INSERT INTO missing VALUES (1);",
            Environment::Development,
        ))
        .await;

        assert!(!resp.success);
        assert!(!driver.call_log().contains(&DriverCall::Begin));
        assert!(
            executed(&driver)
                .iter()
                .any(|q| q.contains("SET failed_at"))
        );
    }

    #[tokio::test]
    async fn failed_mysql_ddl_marks_the_migration_failed_not_pending() {
        let driver = Arc::new(MockDriver::new("mysql").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.add_err("CREATE TABLE a (id int)", "boom");
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "CREATE TABLE a (id int);",
            Environment::Development,
        ))
        .await;

        assert!(!resp.success);
        let ran = executed(&driver);
        // The statements that already ran cannot be undone, so claiming the
        // migration is pending would be a lie about the schema.
        assert!(
            ran.iter().any(|q| q.contains("SET failed_at")),
            "expected a failure marker, got {ran:?}"
        );
        // The claim's own DELETE is conditional (it only clears a rolled-back or
        // previously-failed row). An unconditional one would erase the record and
        // report the half-migrated schema as pending.
        assert!(
            !ran.iter()
                .any(|q| q.starts_with("DELETE") && !q.contains("failed_at IS NOT NULL")),
            "the claim must not be erased after a failure: {ran:?}"
        );
        assert!(
            resp.error
                .expect("error")
                .contains("may commit statements implicitly")
        );
    }

    #[tokio::test]
    async fn failed_marker_error_is_reported_as_uncertain_history() {
        let driver = Arc::new(MockDriver::new("mysql").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.add_err("CREATE TABLE a (id int)", "script failed");
        driver.add_err("SET failed_at", "history unavailable");
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "CREATE TABLE a (id int);",
            Environment::Development,
        ))
        .await;

        let error = resp.error.expect("error");
        assert!(error.contains("could not mark the migration as failed"));
        assert!(error.contains("history state is uncertain"));
    }

    #[tokio::test]
    async fn pre_failed_at_history_table_is_upgraded_before_guard_read() {
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add_err("SELECT failed_at", "column does not exist");
        driver.add("SELECT checksum", empty_history());
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "CREATE TABLE a (id int);",
            Environment::Development,
        ))
        .await;

        assert!(resp.success, "{:?}", resp.error);
        assert!(
            executed(&driver)
                .iter()
                .any(|q| q == "ALTER TABLE qoredb_migrations ADD COLUMN failed_at VARCHAR(64)")
        );
    }

    #[tokio::test]
    async fn a_failed_migration_is_refused_until_forced() {
        let driver = Arc::new(MockDriver::new("mysql").with_transactions(true));
        driver.add(
            "SELECT checksum",
            history_result(&checksum_v2(FILE), None, Some("2026-01-02T00:00:00Z")),
        );
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "CREATE TABLE t (id int);",
            Environment::Development,
        ))
        .await;

        assert_eq!(
            resp.blocked_reason,
            Some(MigrationBlockReason::PartiallyApplied)
        );
        assert!(resp.overridable);
        assert!(
            !executed(&driver)
                .iter()
                .any(|q| q.contains("CREATE TABLE t (id int)"))
        );
    }

    #[tokio::test]
    async fn down_claim_with_zero_affected_rows_aborts_before_the_script() {
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add(
            "SELECT checksum",
            history_result(&checksum_v2(FILE), None, None),
        );
        driver.add(
            "UPDATE qoredb_migrations SET rolled_back_at",
            affected_result(0),
        );
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            false,
            "DROP TABLE t;",
            Environment::Development,
        ))
        .await;

        assert_eq!(
            resp.blocked_reason,
            Some(MigrationBlockReason::ConcurrentApply)
        );
        assert!(!executed(&driver).iter().any(|q| q.contains("DROP TABLE t")));
    }

    #[tokio::test]
    async fn multi_statement_up_executes_each_statement_verbatim() {
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.set_default(empty_result());

        let script = "CREATE TABLE a (id int); -- keep\nINSERT INTO a VALUES ('x;y');";
        let d = Arc::clone(&driver);
        let resp = run_migration(run(&harness(), d, true, script, Environment::Development)).await;
        assert!(resp.success, "{:?}", resp.error);

        let ran = executed(&driver);
        assert!(ran.iter().any(|q| q == "CREATE TABLE a (id int)"));
        // The comment trails the separator, so it belongs to the next statement
        // and must survive verbatim — a sqlparser re-render would drop it.
        assert!(
            ran.iter()
                .any(|q| q == "-- keep\nINSERT INTO a VALUES ('x;y')"),
            "statement text was not preserved verbatim: {ran:?}"
        );
    }

    #[tokio::test]
    async fn unsplittable_script_is_refused_before_any_execution() {
        let driver = Arc::new(MockDriver::new("mysql"));
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; END //",
            Environment::Development,
        ))
        .await;

        assert_eq!(
            resp.blocked_reason,
            Some(MigrationBlockReason::UnsplittableScript)
        );
        assert!(
            driver.call_log().is_empty(),
            "nothing may run before a safe split"
        );
    }

    #[tokio::test]
    async fn history_read_failure_fails_closed() {
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add("CREATE TABLE IF NOT EXISTS", empty_result());
        driver.add_err("SELECT checksum", "connection lost");
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "CREATE TABLE t (id int);",
            Environment::Development,
        ))
        .await;

        // An unreadable history must never be treated as "nothing applied".
        assert!(!resp.success);
        assert!(
            resp.error
                .as_deref()
                .is_some_and(|e| e.contains("Failed to read migration history"))
        );
        assert!(
            !executed(&driver)
                .iter()
                .any(|q| q.contains("CREATE TABLE t (id int)"))
        );
    }

    #[tokio::test]
    async fn execution_ms_in_history_matches_the_response() {
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &harness(),
            d,
            true,
            "CREATE TABLE t (id int);",
            Environment::Development,
        ))
        .await;
        assert!(resp.success);

        let update = executed(&driver)
            .into_iter()
            .find(|q| q.contains("SET execution_ms"))
            .expect("execution_ms update");
        assert!(
            update.contains(&format!("execution_ms = {}", resp.execution_ms)),
            "history and response disagree: {update} vs {}",
            resp.execution_ms
        );
    }

    #[tokio::test]
    async fn applied_by_records_the_connection_not_the_session() {
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        run_migration(run(
            &harness(),
            d,
            true,
            "CREATE TABLE t (id int);",
            Environment::Development,
        ))
        .await;

        let insert = executed(&driver)
            .into_iter()
            .find(|q| q.contains("INSERT INTO qoredb_migrations"))
            .expect("claim");
        assert!(insert.contains("'conn-1'"));
        assert!(!insert.contains("sess-1"));
    }

    #[tokio::test]
    async fn claim_stores_the_up_only_checksum() {
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        run_migration(run(
            &harness(),
            d,
            true,
            "CREATE TABLE t (id int);",
            Environment::Development,
        ))
        .await;

        let insert = executed(&driver)
            .into_iter()
            .find(|q| q.contains("INSERT INTO qoredb_migrations"))
            .expect("claim");
        assert!(insert.contains(&checksum_v2(FILE)));
    }

    #[tokio::test]
    async fn each_statement_is_audited() {
        let h = harness();
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        let resp = run_migration(run(
            &h,
            d,
            true,
            "CREATE TABLE a (id int);\nCREATE TABLE b (id int);",
            Environment::Development,
        ))
        .await;
        assert!(resp.success, "{:?}", resp.error);

        let audit = h
            .interceptor
            .get_audit_entries(100, 0, None, None, None, None, None, None);
        // Two script statements audited; history bookkeeping is not user SQL.
        assert_eq!(
            audit
                .iter()
                .filter(|e| e.query.contains("CREATE TABLE"))
                .count(),
            2
        );
        assert!(
            !audit.iter().any(|e| e.query.contains("qoredb_migrations")),
            "internal bookkeeping must not pollute the audit log"
        );
    }

    #[tokio::test]
    async fn create_then_drop_classifies_the_drop_separately() {
        let h = harness();
        let driver = Arc::new(MockDriver::new("postgres").with_transactions(true));
        driver.add("SELECT checksum", empty_history());
        driver.set_default(empty_result());

        let d = Arc::clone(&driver);
        run_migration(run(
            &h,
            d,
            true,
            "CREATE TABLE a (id int);\nDROP TABLE b;",
            Environment::Development,
        ))
        .await;

        // The old path classified the whole script by its first word, so the DROP
        // hid behind the CREATE and escaped every operation-scoped safety rule.
        let audit = h
            .interceptor
            .get_audit_entries(100, 0, None, None, None, None, None, None);
        let drop_entry = audit
            .iter()
            .find(|e| e.query.contains("DROP TABLE b"))
            .expect("drop audited");
        assert_eq!(
            drop_entry.operation_type,
            qore_service::interceptor::QueryOperationType::Drop
        );
    }

    #[test]
    fn checksum_covers_up_script_only() {
        let edited_down =
            "-- migrate:up\nCREATE TABLE t (id int);\n\n-- migrate:down\nDROP TABLE t CASCADE;\n";
        assert_eq!(checksum_v2(FILE), checksum_v2(edited_down));
    }

    #[test]
    fn checksum_changes_when_up_edited() {
        let edited_up =
            "-- migrate:up\nCREATE TABLE t (id bigint);\n\n-- migrate:down\nDROP TABLE t;\n";
        assert_ne!(checksum_v2(FILE), checksum_v2(edited_up));
    }

    #[test]
    fn checksum_matches_accepts_legacy_whole_file_row() {
        // Rows written before the v2 format hashed the whole file.
        assert!(checksum_matches(&checksum(FILE), FILE));
    }

    #[test]
    fn checksum_matches_rejects_drifted_legacy_row() {
        assert!(!checksum_matches(&checksum("something else"), FILE));
    }

    #[test]
    fn checksum_matches_accepts_v2_row() {
        assert!(checksum_matches(&checksum_v2(FILE), FILE));
    }

    #[test]
    fn legacy_row_tolerates_down_edit_only_via_v2() {
        // A legacy row still reports a down-only edit as drift; that's expected
        // and self-heals once the row is rewritten in v2 form.
        let edited_down =
            "-- migrate:up\nCREATE TABLE t (id int);\n\n-- migrate:down\nDROP TABLE t CASCADE;\n";
        assert!(!checksum_matches(&checksum(FILE), edited_down));
        assert!(checksum_matches(&checksum_v2(FILE), edited_down));
    }

    #[test]
    fn guard_allows_fresh_up() {
        assert!(check_guard(true, None, true, false).is_ok());
    }

    #[test]
    fn guard_refuses_up_when_already_applied() {
        let r = row("c", None);
        let e = check_guard(true, Some(&r), true, false)
            .err()
            .expect("refused");
        assert_eq!(e.reason, MigrationBlockReason::AlreadyApplied);
        assert!(!e.overridable);
    }

    #[test]
    fn guard_allows_up_after_rollback() {
        let r = row("c", Some("2026-01-02T00:00:00Z"));
        assert!(check_guard(true, Some(&r), true, false).is_ok());
    }

    #[test]
    fn guard_refuses_down_when_never_applied() {
        let e = check_guard(false, None, true, false)
            .err()
            .expect("refused");
        assert_eq!(e.reason, MigrationBlockReason::NotApplied);
        assert!(!e.overridable);
    }

    #[test]
    fn guard_refuses_down_when_already_rolled_back() {
        let r = row("c", Some("2026-01-02T00:00:00Z"));
        let e = check_guard(false, Some(&r), true, false)
            .err()
            .expect("refused");
        assert_eq!(e.reason, MigrationBlockReason::AlreadyRolledBack);
    }

    #[test]
    fn guard_refuses_applied_up_on_checksum_drift_without_override() {
        let r = row("c", None);
        let e = check_guard(true, Some(&r), false, false)
            .err()
            .expect("refused");
        assert_eq!(e.reason, MigrationBlockReason::ChecksumMismatch);
        // Re-running an edited up over a schema it no longer describes is never right.
        assert!(!e.overridable);
    }

    #[test]
    fn guard_refuses_applied_up_on_drift_even_with_force() {
        let r = row("c", None);
        assert!(check_guard(true, Some(&r), false, true).is_err());
    }

    #[test]
    fn guard_down_on_drift_is_overridable() {
        let r = row("c", None);
        let e = check_guard(false, Some(&r), false, false)
            .err()
            .expect("refused");
        assert_eq!(e.reason, MigrationBlockReason::ChecksumMismatch);
        assert!(e.overridable);
        assert!(check_guard(false, Some(&r), false, true).is_ok());
    }

    #[test]
    fn guard_reapply_on_drift_is_overridable() {
        let r = row("c", Some("2026-01-02T00:00:00Z"));
        let e = check_guard(true, Some(&r), false, false)
            .err()
            .expect("refused");
        assert!(e.overridable);
        assert!(check_guard(true, Some(&r), false, true).is_ok());
    }

    #[test]
    fn guard_force_does_not_bypass_already_applied() {
        let r = row("c", None);
        let e = check_guard(true, Some(&r), true, true)
            .err()
            .expect("refused");
        assert_eq!(e.reason, MigrationBlockReason::AlreadyApplied);
    }

    #[test]
    fn guard_force_does_not_bypass_not_applied() {
        let e = check_guard(false, None, true, true).err().expect("refused");
        assert_eq!(e.reason, MigrationBlockReason::NotApplied);
    }

    #[test]
    fn guard_refuses_both_directions_on_a_failed_row() {
        let r = failed_row(&checksum_v2(FILE));
        for is_up in [true, false] {
            let e = check_guard(is_up, Some(&r), true, false)
                .err()
                .expect("refused");
            assert_eq!(e.reason, MigrationBlockReason::PartiallyApplied);
            assert!(e.overridable);
        }
    }

    #[test]
    fn guard_force_clears_a_failed_row_in_both_directions() {
        let r = failed_row(&checksum_v2(FILE));
        assert!(check_guard(true, Some(&r), true, true).is_ok());
        assert!(check_guard(false, Some(&r), true, true).is_ok());
    }

    #[test]
    fn failed_wins_over_rolled_back() {
        // A rollback that died part-way sets both; the unknown schema state is
        // what the user needs to hear about.
        let r = HistoryRow {
            rolled_back_at: Some("2026-01-03T00:00:00Z".to_string()),
            ..failed_row("c")
        };
        assert_eq!(r.state(), HistoryState::Failed);
        assert_eq!(r.failed_direction(), Some("down"));
        assert_eq!(failed_row("c").failed_direction(), Some("up"));
        assert_eq!(row("c", None).failed_direction(), None);
    }

    #[test]
    fn mysql_transaction_safety_uses_a_positive_list() {
        assert!(mysql_statement_is_transaction_safe(
            QueryOperationType::Insert,
            "INSERT INTO t VALUES (1)"
        ));
        assert!(mysql_statement_is_transaction_safe(
            QueryOperationType::Other,
            "REPLACE INTO t VALUES (1)"
        ));
        assert!(!mysql_statement_is_transaction_safe(
            QueryOperationType::Other,
            "RENAME TABLE t TO old_t"
        ));
        assert!(!mysql_statement_is_transaction_safe(
            QueryOperationType::Other,
            "LOCK TABLES t WRITE"
        ));
    }

    #[test]
    fn history_ddl_differs_for_sqlserver() {
        assert!(history_table_ddl("sqlserver").contains("sys.tables"));
        assert!(history_table_ddl("postgres").contains("IF NOT EXISTS"));
        assert_eq!(
            history_add_failed_column_ddl("sqlserver"),
            "ALTER TABLE qoredb_migrations ADD failed_at NVARCHAR(64) NULL"
        );
    }

    #[test]
    fn sql_str_escapes_quotes() {
        assert_eq!(sql_str("a'b"), "'a''b'");
    }
}

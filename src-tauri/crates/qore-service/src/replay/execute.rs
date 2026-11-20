// SPDX-License-Identifier: BUSL-1.1

//! Replays one entry through the shared preflight and execute path, so the
//! desktop runner and the CI runner apply the same safety, mutation and
//! comparison rules.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use qore_core::QueryResult;
use qore_core::types::SessionId;
use qore_drivers::query_manager::QueryManager;
use qore_drivers::session_manager::SessionManager;

use crate::cache::QueryCache;
use crate::interceptor::{InterceptorPipeline, QuerySource};
use crate::policy::SafetyPolicy;
use crate::query as service_query;
use crate::ratelimit::QueryRateLimiter;

use super::compare::{Observed, SlowerThresholds, classify};
use super::digest::compute_digest;
use super::types::{ReplayEntry, ReplayEntryResult, ReplayVerdict, query_preview};

pub const MUTATION_EXCLUDED: &str = "Mutation excluded from replay";
pub const MUTATION_PRODUCTION_BLOCKED: &str = "Mutations are never replayed against production";
/// Stable identifiers for the reasons above. The message travels with them so a
/// report stays readable outside the app; the UI translates from the code.
pub const CODE_MUTATION_EXCLUDED: &str = "mutation_excluded";
pub const CODE_MUTATION_PRODUCTION_BLOCKED: &str = "mutation_production_blocked";
pub const CANCELLED: &str = "Replay cancelled";
pub const CODE_CANCELLED: &str = "cancelled";
pub const SET_IS_REDACTED: &str =
    "This replay set was recorded with redaction: its queries cannot be replayed";

pub struct ReplayServices<'a> {
    pub session_manager: &'a Arc<SessionManager>,
    pub query_manager: &'a Arc<QueryManager>,
    pub query_rate_limiter: &'a Arc<QueryRateLimiter>,
    pub query_cache: &'a Arc<QueryCache>,
    pub interceptor: &'a Arc<InterceptorPipeline>,
    pub policy: &'a SafetyPolicy,
}

/// How one run replays its entries.
pub struct EntryRun<'a> {
    pub session: SessionId,
    pub session_id: &'a str,
    pub source: QuerySource,
    pub timeout_ms: Option<u64>,
    pub is_production: bool,
    pub allow_mutations: bool,
    pub ignored_columns: &'a [String],
    pub max_captured_rows: usize,
    pub thresholds: SlowerThresholds,
    pub cancel: Option<&'a AtomicBool>,
}

impl EntryRun<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.is_some_and(|flag| flag.load(Ordering::SeqCst))
    }
}

pub enum EntryStep {
    Skipped(ReplayEntryResult),
    Cancelled(ReplayEntryResult),
    Executed {
        result: ReplayEntryResult,
        query_result: Option<QueryResult>,
    },
}

pub fn skipped_result(entry: &ReplayEntry, reason: &str, code: Option<&str>) -> ReplayEntryResult {
    ReplayEntryResult {
        entry_id: entry.id.clone(),
        order: entry.order,
        query_preview: query_preview(&entry.query),
        verdict: ReplayVerdict::Skipped,
        success: false,
        error: None,
        skip_reason: Some(reason.to_string()),
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

pub fn cancelled_result(entry: &ReplayEntry) -> ReplayEntryResult {
    skipped_result(entry, CANCELLED, Some(CODE_CANCELLED))
}

/// `(code, message)` when a mutation must not run on this connection.
pub fn mutation_refusal(
    is_production: bool,
    allow_mutations: bool,
) -> Option<(&'static str, &'static str)> {
    if is_production {
        Some((
            CODE_MUTATION_PRODUCTION_BLOCKED,
            MUTATION_PRODUCTION_BLOCKED,
        ))
    } else if !allow_mutations {
        Some((CODE_MUTATION_EXCLUDED, MUTATION_EXCLUDED))
    } else {
        None
    }
}

pub async fn replay_entry(
    services: &ReplayServices<'_>,
    entry: &ReplayEntry,
    run: &EntryRun<'_>,
) -> EntryStep {
    let preflight = match service_query::preflight_with_source(
        services.session_manager,
        services.query_rate_limiter,
        services.interceptor,
        services.policy,
        run.session,
        run.session_id,
        &entry.query,
        entry.namespace.as_ref(),
        false,
        run.source,
    )
    .await
    {
        Ok(preflight) => preflight,
        // A refused preflight is QoreDB declining to run the query — a safety
        // rule, read-only mode, the rate limiter. That is not the database
        // answering differently, so it is not a regression.
        Err(message) => return EntryStep::Skipped(skipped_result(entry, &message, None)),
    };

    // The trustworthy classification is the preflight's, never the set's:
    // `.qreplay.json` is a versioned, hand-editable file, and a mutation
    // declared as a read would otherwise run — in production included.
    if preflight.is_mutation
        && let Some((code, reason)) = mutation_refusal(run.is_production, run.allow_mutations)
    {
        return EntryStep::Skipped(skipped_result(entry, reason, Some(code)));
    }

    // Preflight can await locks and interceptors. Recheck before dispatch.
    if run.cancelled() {
        return EntryStep::Cancelled(cancelled_result(entry));
    }
    let query_id = services.query_manager.register(run.session).await;
    if run.cancelled() {
        services.query_manager.finish(query_id).await;
        return EntryStep::Cancelled(cancelled_result(entry));
    }

    let outcome = service_query::execute(
        services.query_manager,
        services.query_cache,
        services.interceptor,
        services.policy,
        preflight.driver,
        &preflight.context,
        run.session,
        entry.namespace.clone(),
        &entry.query,
        query_id,
        preflight.is_mutation,
        preflight.connection_key.as_deref(),
        preflight.safety_warning.as_deref(),
        run.timeout_ms,
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
            let computed = compute_digest(result, run.ignored_columns, run.max_captured_rows);
            (Some(computed.digest), computed.partial)
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
        &run.thresholds,
    );

    EntryStep::Executed {
        result: ReplayEntryResult {
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
        },
        query_result: outcome.result,
    }
}

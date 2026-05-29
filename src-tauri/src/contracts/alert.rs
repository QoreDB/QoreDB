// SPDX-License-Identifier: BUSL-1.1

//! Post-mutation contract re-evaluation hook (Pro).
//!
//! When a mutation succeeds on a table that has at least one enabled contract,
//! we asynchronously re-evaluate the matching contracts and fire a
//! `contract.alert` event if a previously-passing rule starts failing.
//!
//! Design notes:
//! - **Best-effort** : never blocks the mutation. Failures are logged and
//!   swallowed.
//! - **Same dialect only** : we filter contracts by `target.table` and (when
//!   present) `target.schema`. The connection match is enforced implicitly
//!   because we run on the session that just performed the mutation.
//! - **Samples off** : `collect_samples: false` to avoid extra round-trips.
//!   The Contracts panel can still gather samples on explicit runs.
//! - **No event flood** : the per-rule progress events are suppressed by
//!   using a `NoopSink`; we only emit the high-level `contract.alert` event.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use qore_core::types::SessionId;
use qore_drivers::session_manager::SessionManager;

use super::events::NoopSink;
use super::runner::{run_contract, RunOptions};
use super::storage;
use super::{Contract, ContractRun, RuleStatus};

/// Tauri event topic for regression alerts.
pub const CONTRACT_ALERT_EVENT: &str = "contract.alert";

#[derive(Debug, Clone, Serialize)]
pub struct ContractAlertPayload {
    pub contract_id: String,
    pub contract_name: String,
    pub table: String,
    pub schema: Option<String>,
    /// IDs of rules whose status was `pass` in the previous run and is no
    /// longer `pass` in this one. Empty means "no new regression".
    pub regressed_rules: Vec<String>,
    pub run: ContractRun,
}

/// Schedules a non-blocking check. Returns immediately. Safe to call even if
/// no contracts exist or the workspace cannot be resolved — failures are
/// logged at `warn` level only.
pub fn schedule_post_mutation_check(
    app: AppHandle,
    session_id: SessionId,
    schema: Option<String>,
    table: String,
) {
    tokio::spawn(async move {
        if let Err(e) = run_check(app, session_id, schema, table).await {
            tracing::warn!(error = %e, "contract alert: post-mutation check failed");
        }
    });
}

async fn run_check(
    app: AppHandle,
    session_id: SessionId,
    schema: Option<String>,
    table: String,
) -> Result<(), String> {
    let (session_manager, connection_id) = resolve_session(&app, session_id).await?;
    let Some(workspace_root) = resolve_workspace_root(&app).await else {
        return Ok(());
    };

    let candidates = collect_candidate_contracts(&workspace_root, &table, schema.as_deref());
    if candidates.is_empty() {
        return Ok(());
    }

    let driver = session_manager
        .get_driver(session_id)
        .await
        .map_err(|e| e.sanitized_message())?;

    for (name, contract) in candidates {
        let previous = storage::read_history(&workspace_root, &name, Some(1))
            .ok()
            .and_then(|mut v| v.pop());

        let run = match run_contract(
            Arc::clone(&driver),
            session_id,
            connection_id.clone(),
            &contract,
            RunOptions {
                sample_limit: 0,
                collect_samples: false,
            },
            &NoopSink,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(contract = %name, error = ?e, "post-mutation re-eval failed");
                continue;
            }
        };

        let _ = storage::append_run(&workspace_root, &name, &run);

        let regressed = compute_regressions(previous.as_ref(), &run);
        if !regressed.is_empty() || run.fail_count > 0 {
            let payload = ContractAlertPayload {
                contract_id: name.clone(),
                contract_name: contract.name.clone(),
                table: contract.target.table.clone(),
                schema: contract.target.schema.clone(),
                regressed_rules: regressed,
                run,
            };
            let _ = app.emit(CONTRACT_ALERT_EVENT, payload);
        }
    }
    Ok(())
}

async fn resolve_session(
    app: &AppHandle,
    session_id: SessionId,
) -> Result<(Arc<SessionManager>, String), String> {
    let state = app.state::<crate::SharedState>();
    let guard = state.lock().await;
    let session_manager = Arc::clone(&guard.session_manager);
    drop(guard);
    let connection_id = session_manager
        .get_session_info(session_id)
        .await
        .unwrap_or_else(|| session_id.0.to_string());
    Ok((session_manager, connection_id))
}

async fn resolve_workspace_root(app: &AppHandle) -> Option<PathBuf> {
    let state = app.try_state::<crate::commands::workspace::SharedWorkspaceManager>()?;
    let mgr = state.lock().await;
    Some(mgr.active().path.clone())
}

/// Loads each contract YAML in the workspace and keeps the ones whose target
/// matches the mutated table. Matching is case-sensitive on the table name
/// and schema, mirroring how SQL identifiers are emitted by `sql::dialect`.
fn collect_candidate_contracts(
    workspace_root: &std::path::Path,
    table: &str,
    schema: Option<&str>,
) -> Vec<(String, Contract)> {
    let metas = match storage::list_contracts(workspace_root) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for meta in metas {
        let source = match storage::load_contract_source(workspace_root, &meta.name) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let contract = match super::parser::parse_contract(&source, super::parser::Format::Auto) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if contract.target.table != table {
            continue;
        }
        if let Some(expected) = schema {
            if let Some(ref t) = contract.target.schema {
                if t != expected {
                    continue;
                }
            }
        }
        out.push((meta.name, contract));
    }
    out
}

/// Returns rules whose status regressed: was `pass` in `previous`, no longer
/// `pass` in `current`. An empty list means no new regressions vs. the last
/// run on record.
fn compute_regressions(previous: Option<&ContractRun>, current: &ContractRun) -> Vec<String> {
    let Some(prev) = previous else {
        return current
            .results
            .iter()
            .filter(|r| !matches!(r.status, RuleStatus::Pass | RuleStatus::Skipped))
            .map(|r| r.id.clone())
            .collect();
    };
    let mut out = Vec::new();
    for cur in &current.results {
        if matches!(cur.status, RuleStatus::Pass | RuleStatus::Skipped) {
            continue;
        }
        if prev
            .results
            .iter()
            .any(|r| r.id == cur.id && matches!(r.status, RuleStatus::Pass))
        {
            out.push(cur.id.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Contract, ContractRun, ContractTarget, Rule, RuleResult, RuleStatus};

    fn rule_result(id: &str, status: RuleStatus) -> RuleResult {
        RuleResult {
            id: id.into(),
            rule_type: "row_count".into(),
            status,
            violations_count: None,
            metric: None,
            samples: None,
            duration_ms: 1,
            error: None,
        }
    }

    fn run_with(results: Vec<RuleResult>) -> ContractRun {
        let pass = results
            .iter()
            .filter(|r| matches!(r.status, RuleStatus::Pass))
            .count() as u32;
        let fail = results
            .iter()
            .filter(|r| matches!(r.status, RuleStatus::Fail))
            .count() as u32;
        let err = results
            .iter()
            .filter(|r| matches!(r.status, RuleStatus::Error))
            .count() as u32;
        ContractRun {
            contract_id: "c".into(),
            contract_name: "c".into(),
            connection_id: "x".into(),
            started_at: "".into(),
            finished_at: "".into(),
            duration_ms: 0,
            pass_count: pass,
            fail_count: fail,
            error_count: err,
            results,
        }
    }

    #[test]
    fn no_regression_when_status_unchanged() {
        let prev = run_with(vec![
            rule_result("a", RuleStatus::Pass),
            rule_result("b", RuleStatus::Fail),
        ]);
        let curr = run_with(vec![
            rule_result("a", RuleStatus::Pass),
            rule_result("b", RuleStatus::Fail),
        ]);
        assert!(compute_regressions(Some(&prev), &curr).is_empty());
    }

    #[test]
    fn regression_detected_when_pass_to_fail() {
        let prev = run_with(vec![rule_result("a", RuleStatus::Pass)]);
        let curr = run_with(vec![rule_result("a", RuleStatus::Fail)]);
        assert_eq!(
            compute_regressions(Some(&prev), &curr),
            vec!["a".to_string()]
        );
    }

    #[test]
    fn no_history_treats_all_failures_as_regressions() {
        let curr = run_with(vec![
            rule_result("a", RuleStatus::Fail),
            rule_result("b", RuleStatus::Pass),
        ]);
        assert_eq!(compute_regressions(None, &curr), vec!["a".to_string()]);
    }

    #[test]
    fn skipped_rules_never_count_as_regressions() {
        let prev = run_with(vec![rule_result("a", RuleStatus::Pass)]);
        let curr = run_with(vec![rule_result("a", RuleStatus::Skipped)]);
        assert!(compute_regressions(Some(&prev), &curr).is_empty());
    }

    #[test]
    fn collect_candidates_filters_by_table_and_schema() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();

        let yaml_a = r#"name: a
version: 1
target:
  connection: c
  schema: public
  table: orders
rules:
  - id: r1
    type: not_empty
    column: id
"#;
        let yaml_b = r#"name: b
version: 1
target:
  connection: c
  schema: analytics
  table: orders
rules:
  - id: r1
    type: not_empty
    column: id
"#;
        let yaml_c = r#"name: c
version: 1
target:
  connection: c
  table: customers
rules:
  - id: r1
    type: not_empty
    column: id
"#;
        storage::save_contract_source(tmp.path(), "a", yaml_a).unwrap();
        storage::save_contract_source(tmp.path(), "b", yaml_b).unwrap();
        storage::save_contract_source(tmp.path(), "c", yaml_c).unwrap();

        let matches = collect_candidate_contracts(tmp.path(), "orders", Some("public"));
        let names: Vec<_> = matches.iter().map(|(n, _)| n.clone()).collect();
        assert_eq!(names, vec!["a".to_string()]);

        let matches_no_schema = collect_candidate_contracts(tmp.path(), "orders", None);
        let names: Vec<_> = matches_no_schema.iter().map(|(n, _)| n.clone()).collect();
        // When no schema filter, both a (schema=public) and b (schema=analytics) match.
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"b".to_string()));
        assert!(!names.contains(&"c".to_string()));

        // Reference unused-import guard
        let _ = ContractTarget {
            connection: String::new(),
            schema: None,
            table: String::new(),
        };
        let _: Option<Contract> = None;
        let _: Option<Rule> = None;
    }
}

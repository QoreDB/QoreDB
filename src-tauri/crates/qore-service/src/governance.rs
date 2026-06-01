// SPDX-License-Identifier: Apache-2.0

//! Governance helpers shared across read-side commands.
//!
//! Centralises the three runtime guardrails surfaced by `SafetyPolicy`:
//! - **max_result_rows**: clamp requested page sizes
//! - **max_concurrent_queries**: refuse new work when the pool is saturated
//! - **max_query_duration_ms**: hard timeout on driver futures
//!
//! `execute_query` has its own bespoke wiring because it also handles streaming
//! and interceptor hooks. The browse endpoints (`preview_table`, `query_table`,
//! `peek_foreign_key`) reach for these helpers instead of duplicating the
//! logic — keeping the audit recommendation (cf. `SECURITY_AUDIT.md` § 2)
//! enforced uniformly.

use std::future::Future;
use std::time::Duration;

use tokio::time::timeout;

use qore_drivers::query_manager::QueryManager;
use crate::policy::SafetyPolicy;

/// Clamp a requested row count against the policy's `max_result_rows`.
/// Returns `requested` unchanged if no limit is set.
pub fn clamp_rows(policy: &SafetyPolicy, requested: u32) -> u32 {
    match policy.max_result_rows {
        Some(max) => requested.min(max as u32),
        None => requested,
    }
}

/// Reject the call early if the concurrent-query budget is exhausted.
/// The error string is user-visible.
pub async fn check_concurrent_limit(
    policy: &SafetyPolicy,
    query_manager: &QueryManager,
) -> Result<(), String> {
    if let Some(limit) = policy.max_concurrent_queries {
        let active = query_manager.count_active().await;
        if active >= limit as usize {
            return Err(format!(
                "Too many concurrent queries ({}/{})",
                active, limit
            ));
        }
    }
    Ok(())
}

/// Run `fut` under the policy's `max_query_duration_ms`. When no limit is
/// configured the future is simply awaited.
pub async fn with_timeout<F, T>(policy: &SafetyPolicy, fut: F) -> Result<T, String>
where
    F: Future<Output = T>,
{
    match policy.max_query_duration_ms {
        Some(ms) => match timeout(Duration::from_millis(ms), fut).await {
            Ok(value) => Ok(value),
            Err(_) => Err(format!("Operation timed out after {}ms", ms)),
        },
        None => Ok(fut.await),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy_with(max_rows: Option<u64>, max_dur_ms: Option<u64>) -> SafetyPolicy {
        SafetyPolicy {
            prod_require_confirmation: true,
            prod_block_dangerous_sql: false,
            max_query_duration_ms: max_dur_ms,
            max_result_rows: max_rows,
            max_concurrent_queries: None,
            query_rate_limit_enabled: true,
        }
    }

    #[test]
    fn clamp_rows_caps_to_policy() {
        let p = policy_with(Some(50), None);
        assert_eq!(clamp_rows(&p, 200), 50);
        assert_eq!(clamp_rows(&p, 10), 10);
    }

    #[test]
    fn clamp_rows_passthrough_when_unlimited() {
        let p = policy_with(None, None);
        assert_eq!(clamp_rows(&p, 1_000_000), 1_000_000);
    }

    #[tokio::test]
    async fn with_timeout_passes_through_when_no_limit() {
        let p = policy_with(None, None);
        let result: Result<i32, String> = with_timeout(&p, async { 42 }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn with_timeout_returns_value_within_budget() {
        let p = policy_with(None, Some(1000));
        let result: Result<i32, String> = with_timeout(&p, async { 7 }).await;
        assert_eq!(result.unwrap(), 7);
    }

    #[tokio::test]
    async fn with_timeout_fires_when_exceeded() {
        let p = policy_with(None, Some(20));
        let result: Result<(), String> = with_timeout(&p, async {
            tokio::time::sleep(Duration::from_millis(200)).await;
        })
        .await;
        let err = result.unwrap_err();
        assert!(err.contains("timed out"));
    }
}

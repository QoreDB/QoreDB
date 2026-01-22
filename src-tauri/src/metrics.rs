//! Lightweight in-memory metrics for dev builds.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use serde::Serialize;

#[derive(Default)]
struct QueryMetrics {
    total: AtomicU64,
    failed: AtomicU64,
    cancelled: AtomicU64,
    timeouts: AtomicU64,
    duration_total_ms: AtomicU64,
    duration_max_ms: AtomicU64,
}

static QUERY_METRICS: OnceLock<QueryMetrics> = OnceLock::new();

fn metrics() -> &'static QueryMetrics {
    QUERY_METRICS.get_or_init(QueryMetrics::default)
}

pub fn record_query(duration_ms: f64, success: bool) {
    let duration_ms = duration_ms.max(0.0) as u64;
    let metrics = metrics();
    metrics.total.fetch_add(1, Ordering::Relaxed);
    if !success {
        metrics.failed.fetch_add(1, Ordering::Relaxed);
    }
    metrics
        .duration_total_ms
        .fetch_add(duration_ms, Ordering::Relaxed);

    let mut current = metrics.duration_max_ms.load(Ordering::Relaxed);
    while duration_ms > current {
        match metrics.duration_max_ms.compare_exchange(
            current,
            duration_ms,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

pub fn record_cancel() {
    metrics().cancelled.fetch_add(1, Ordering::Relaxed);
}

pub fn record_timeout() {
    metrics().timeouts.fetch_add(1, Ordering::Relaxed);
}

#[derive(Debug, Serialize)]
pub struct QueryMetricsSnapshot {
    pub total: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub timeouts: u64,
    pub avg_ms: Option<f64>,
    pub max_ms: Option<u64>,
}

pub fn snapshot() -> QueryMetricsSnapshot {
    let metrics = metrics();
    let total = metrics.total.load(Ordering::Relaxed);
    let failed = metrics.failed.load(Ordering::Relaxed);
    let cancelled = metrics.cancelled.load(Ordering::Relaxed);
    let timeouts = metrics.timeouts.load(Ordering::Relaxed);
    let duration_total = metrics.duration_total_ms.load(Ordering::Relaxed);
    let max_ms = metrics.duration_max_ms.load(Ordering::Relaxed);

    let avg_ms = if total > 0 {
        Some(duration_total as f64 / total as f64)
    } else {
        None
    };

    QueryMetricsSnapshot {
        total,
        failed,
        cancelled,
        timeouts,
        avg_ms,
        max_ms: if max_ms > 0 { Some(max_ms) } else { None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_flow() {
        // Capture initial state
        let initial = snapshot();

        // 1. Record a successful query
        record_query(100.0, true);
        let s1 = snapshot();
        assert_eq!(s1.total, initial.total + 1);
        assert_eq!(s1.failed, initial.failed);
        // We can't strictly assert avg/max because other tests might run in parallel,
        // but we can check it updated logically if we were alone.
        // With deltas it is safer.

        // 2. Record a failed query
        record_query(50.0, false);
        let s2 = snapshot();
        assert_eq!(s2.total, s1.total + 1);
        assert_eq!(s2.failed, s1.failed + 1);

        // 3. Record cancel
        record_cancel();
        let s3 = snapshot();
        assert_eq!(s3.cancelled, initial.cancelled + 1);

        // 4. Record timeout
        record_timeout();
        let s4 = snapshot();
        assert_eq!(s4.timeouts, initial.timeouts + 1);

        // 5. Max duration update
        // We need to ensure this duration is larger than any previous max to test the update logic
        // But since we can't know previous max easily without race, we'll just record a large value
        // and hope it updates, or at least doesn't crash.
        // Actually, we can check if max_ms is at least what we sent.
        record_query(99999.0, true);
        let s5 = snapshot();
        assert!(s5.max_ms.unwrap() >= 99999);
    }
}

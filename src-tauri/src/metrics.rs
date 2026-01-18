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

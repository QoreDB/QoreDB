// SPDX-License-Identifier: BUSL-1.1

//! Profiling Store
//!
//! Extended profiling metrics for query performance analysis.
//! Tracks execution times, percentiles, and slow queries.

use std::collections::VecDeque;

use parking_lot::RwLock;

use chrono::Utc;
use tracing::{debug, info};

use super::types::{Environment, ProfilingMetrics, QueryOperationType, SlowQueryEntry};

/// Maximum number of execution times to track for percentile calculation
const MAX_EXECUTION_TIMES: usize = 10000;

/// Profiling store with performance metrics
pub struct ProfilingStore {
    metrics: RwLock<ProfilingMetrics>,
    /// Execution times for percentile calculation (insertion order)
    execution_times: RwLock<VecDeque<f64>>,
    slow_queries: RwLock<VecDeque<SlowQueryEntry>>,
    /// Slow query threshold in milliseconds
    slow_threshold_ms: RwLock<u64>,
    max_slow_queries: RwLock<usize>,
    enabled: RwLock<bool>,
}

impl ProfilingStore {
    pub fn new(slow_threshold_ms: u64, max_slow_queries: usize) -> Self {
        Self {
            metrics: RwLock::new(ProfilingMetrics::new()),
            execution_times: RwLock::new(VecDeque::with_capacity(MAX_EXECUTION_TIMES)),
            slow_queries: RwLock::new(VecDeque::with_capacity(max_slow_queries)),
            slow_threshold_ms: RwLock::new(slow_threshold_ms),
            max_slow_queries: RwLock::new(max_slow_queries),
            enabled: RwLock::new(true),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.write() = enabled;
        info!("Profiling {}", if enabled { "enabled" } else { "disabled" });
    }

    pub fn is_enabled(&self) -> bool {
        *self.enabled.read()
    }

    pub fn set_slow_threshold(&self, threshold_ms: u64) {
        *self.slow_threshold_ms.write() = threshold_ms;
        info!("Slow query threshold set to {}ms", threshold_ms);
    }

    pub fn set_max_slow_queries(&self, max_slow_queries: usize) {
        *self.max_slow_queries.write() = max_slow_queries;
        let mut slow_queries = self.slow_queries.write();
        while slow_queries.len() > max_slow_queries {
            slow_queries.pop_front();
        }
    }

    pub fn get_slow_threshold(&self) -> u64 {
        *self.slow_threshold_ms.read()
    }

    pub fn record(
        &self,
        execution_time_ms: f64,
        success: bool,
        blocked: bool,
        operation_type: QueryOperationType,
        environment: Environment,
        query: Option<&str>,
        database: Option<&str>,
        row_count: Option<i64>,
        driver_id: &str,
    ) {
        if !self.is_enabled() {
            return;
        }

        let threshold = *self.slow_threshold_ms.read();

        {
            let mut metrics = self.metrics.write();

            metrics.total_queries += 1;

            if blocked {
                metrics.blocked_queries += 1;
            } else if success {
                metrics.successful_queries += 1;
            } else {
                metrics.failed_queries += 1;
            }

            metrics.total_execution_time_ms += execution_time_ms;
            metrics.avg_execution_time_ms =
                metrics.total_execution_time_ms / metrics.total_queries as f64;

            if execution_time_ms < metrics.min_execution_time_ms {
                metrics.min_execution_time_ms = execution_time_ms;
            }
            if execution_time_ms > metrics.max_execution_time_ms {
                metrics.max_execution_time_ms = execution_time_ms;
            }

            if execution_time_ms >= threshold as f64 {
                metrics.slow_query_count += 1;
            }

            let op_key = format!("{:?}", operation_type).to_lowercase();
            *metrics.by_operation_type.entry(op_key).or_insert(0) += 1;

            let env_key = format!("{:?}", environment).to_lowercase();
            *metrics.by_environment.entry(env_key).or_insert(0) += 1;
        }

        // Insertion-order ring buffer; sorted on demand for percentiles.
        {
            let mut times = self.execution_times.write();
            if times.len() >= MAX_EXECUTION_TIMES {
                times.pop_front();
            }
            times.push_back(execution_time_ms);
        }

        if execution_time_ms >= threshold as f64 {
            if let Some(query_str) = query {
                self.record_slow_query(
                    query_str,
                    execution_time_ms,
                    environment,
                    database,
                    row_count,
                    driver_id,
                );
            }
        }

        // Recompute percentiles every 100 queries to amortise the sort cost.
        let total = self.metrics.read().total_queries;
        if total.is_multiple_of(100) {
            self.update_percentiles();
        }
    }

    fn record_slow_query(
        &self,
        query: &str,
        execution_time_ms: f64,
        environment: Environment,
        database: Option<&str>,
        row_count: Option<i64>,
        driver_id: &str,
    ) {
        use super::redaction::redact_query;

        let entry = SlowQueryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            query: redact_query(query, driver_id),
            execution_time_ms,
            environment,
            database: database.map(|s| s.to_string()),
            row_count,
            driver_id: driver_id.to_string(),
        };

        let max_slow_queries = *self.max_slow_queries.read();
        let mut slow_queries = self.slow_queries.write();
        if slow_queries.len() >= max_slow_queries {
            slow_queries.pop_front();
        }
        slow_queries.push_back(entry);

        debug!("Recorded slow query: {}ms", execution_time_ms);
    }

    fn update_percentiles(&self) {
        let times = self.execution_times.read();
        if times.is_empty() {
            return;
        }

        let mut sorted: Vec<f64> = times.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let len = sorted.len();
        let p50_idx = len * 50 / 100;
        let p95_idx = len * 95 / 100;
        let p99_idx = len * 99 / 100;

        let mut metrics = self.metrics.write();
        metrics.p50_execution_time_ms = sorted.get(p50_idx).copied().unwrap_or(0.0);
        metrics.p95_execution_time_ms = sorted.get(p95_idx).copied().unwrap_or(0.0);
        metrics.p99_execution_time_ms = sorted.get(p99_idx).copied().unwrap_or(0.0);
    }

    pub fn get_metrics(&self) -> ProfilingMetrics {
        self.update_percentiles();

        let metrics = self.metrics.read();
        metrics.clone()
    }

    pub fn get_slow_queries(&self, limit: usize, offset: usize) -> Vec<SlowQueryEntry> {
        let slow_queries = self.slow_queries.read();
        slow_queries
            .iter()
            .rev()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn clear_slow_queries(&self) {
        self.slow_queries.write().clear();
        info!("Slow queries cleared");
    }

    pub fn reset(&self) {
        *self.metrics.write() = ProfilingMetrics::new();
        self.execution_times.write().clear();
        self.slow_queries.write().clear();
        info!("Profiling metrics reset");
    }

    /// Export profiling data as JSON
    pub fn export(&self) -> String {
        #[derive(serde::Serialize)]
        struct ProfilingExport {
            metrics: ProfilingMetrics,
            slow_queries: Vec<SlowQueryEntry>,
        }

        let export = ProfilingExport {
            metrics: self.get_metrics(),
            slow_queries: self.slow_queries.read().iter().cloned().collect(),
        };

        serde_json::to_string_pretty(&export).unwrap_or_else(|_| "{}".to_string())
    }
}

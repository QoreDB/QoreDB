// SPDX-License-Identifier: BUSL-1.1

//! Profiling Store
//!
//! Extended profiling metrics for query performance analysis.
//! Tracks execution times, percentiles, and slow queries.

use std::collections::VecDeque;
use std::sync::RwLock;

use chrono::Utc;
use tracing::{debug, info};

use super::types::{Environment, ProfilingMetrics, QueryOperationType, SlowQueryEntry};

/// Maximum number of execution times to track for percentile calculation
const MAX_EXECUTION_TIMES: usize = 10000;

/// Profiling store with performance metrics
pub struct ProfilingStore {
    /// Current metrics
    metrics: RwLock<ProfilingMetrics>,
    /// Execution times for percentile calculation (sorted)
    execution_times: RwLock<Vec<f64>>,
    /// Slow query entries
    slow_queries: RwLock<VecDeque<SlowQueryEntry>>,
    /// Slow query threshold in milliseconds
    slow_threshold_ms: RwLock<u64>,
    /// Maximum slow queries to retain
    max_slow_queries: RwLock<usize>,
    /// Whether profiling is enabled
    enabled: RwLock<bool>,
}

impl ProfilingStore {
    /// Creates a new profiling store
    pub fn new(slow_threshold_ms: u64, max_slow_queries: usize) -> Self {
        Self {
            metrics: RwLock::new(ProfilingMetrics::new()),
            execution_times: RwLock::new(Vec::with_capacity(MAX_EXECUTION_TIMES)),
            slow_queries: RwLock::new(VecDeque::with_capacity(max_slow_queries)),
            slow_threshold_ms: RwLock::new(slow_threshold_ms),
            max_slow_queries: RwLock::new(max_slow_queries),
            enabled: RwLock::new(true),
        }
    }

    /// Enable or disable profiling
    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.write().unwrap() = enabled;
        info!("Profiling {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Check if profiling is enabled
    pub fn is_enabled(&self) -> bool {
        *self.enabled.read().unwrap()
    }

    /// Set the slow query threshold
    pub fn set_slow_threshold(&self, threshold_ms: u64) {
        *self.slow_threshold_ms.write().unwrap() = threshold_ms;
        info!("Slow query threshold set to {}ms", threshold_ms);
    }

    /// Set max slow queries to retain
    pub fn set_max_slow_queries(&self, max_slow_queries: usize) {
        *self.max_slow_queries.write().unwrap() = max_slow_queries;
        let mut slow_queries = self.slow_queries.write().unwrap();
        while slow_queries.len() > max_slow_queries {
            slow_queries.pop_front();
        }
    }

    /// Get the slow query threshold
    pub fn get_slow_threshold(&self) -> u64 {
        *self.slow_threshold_ms.read().unwrap()
    }

    /// Record a query execution
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

        let threshold = *self.slow_threshold_ms.read().unwrap();

        // Update metrics
        {
            let mut metrics = self.metrics.write().unwrap();

            metrics.total_queries += 1;

            if blocked {
                metrics.blocked_queries += 1;
            } else if success {
                metrics.successful_queries += 1;
            } else {
                metrics.failed_queries += 1;
            }

            // Update execution time stats
            metrics.total_execution_time_ms += execution_time_ms;
            metrics.avg_execution_time_ms =
                metrics.total_execution_time_ms / metrics.total_queries as f64;

            if execution_time_ms < metrics.min_execution_time_ms {
                metrics.min_execution_time_ms = execution_time_ms;
            }
            if execution_time_ms > metrics.max_execution_time_ms {
                metrics.max_execution_time_ms = execution_time_ms;
            }

            // Track slow queries
            if execution_time_ms >= threshold as f64 {
                metrics.slow_query_count += 1;
            }

            // Update by operation type
            let op_key = format!("{:?}", operation_type).to_lowercase();
            *metrics.by_operation_type.entry(op_key).or_insert(0) += 1;

            // Update by environment
            let env_key = format!("{:?}", environment).to_lowercase();
            *metrics.by_environment.entry(env_key).or_insert(0) += 1;
        }

        // Track execution time for percentiles
        {
            let mut times = self.execution_times.write().unwrap();
            if times.len() >= MAX_EXECUTION_TIMES {
                // Remove oldest (first) to make room
                times.remove(0);
            }
            // Insert in sorted order for efficient percentile calculation
            let pos = times.partition_point(|&t| t < execution_time_ms);
            times.insert(pos, execution_time_ms);
        }

        // Record slow query if above threshold
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

        // Update percentiles periodically (every 100 queries)
        let total = self.metrics.read().unwrap().total_queries;
        if total % 100 == 0 {
            self.update_percentiles();
        }
    }

    /// Record a slow query
    fn record_slow_query(
        &self,
        query: &str,
        execution_time_ms: f64,
        environment: Environment,
        database: Option<&str>,
        row_count: Option<i64>,
        driver_id: &str,
    ) {
        let entry = SlowQueryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            query: query.to_string(),
            execution_time_ms,
            environment,
            database: database.map(|s| s.to_string()),
            row_count,
            driver_id: driver_id.to_string(),
        };

        let max_slow_queries = *self.max_slow_queries.read().unwrap();
        let mut slow_queries = self.slow_queries.write().unwrap();
        if slow_queries.len() >= max_slow_queries {
            slow_queries.pop_front();
        }
        slow_queries.push_back(entry);

        debug!("Recorded slow query: {}ms", execution_time_ms);
    }

    /// Update percentile calculations
    fn update_percentiles(&self) {
        let times = self.execution_times.read().unwrap();
        if times.is_empty() {
            return;
        }

        let len = times.len();
        let p50_idx = len * 50 / 100;
        let p95_idx = len * 95 / 100;
        let p99_idx = len * 99 / 100;

        let mut metrics = self.metrics.write().unwrap();
        metrics.p50_execution_time_ms = times.get(p50_idx).copied().unwrap_or(0.0);
        metrics.p95_execution_time_ms = times.get(p95_idx).copied().unwrap_or(0.0);
        metrics.p99_execution_time_ms = times.get(p99_idx).copied().unwrap_or(0.0);
    }

    /// Get current profiling metrics
    pub fn get_metrics(&self) -> ProfilingMetrics {
        // Update percentiles before returning
        self.update_percentiles();

        let metrics = self.metrics.read().unwrap();
        metrics.clone()
    }

    /// Get slow query entries
    pub fn get_slow_queries(&self, limit: usize, offset: usize) -> Vec<SlowQueryEntry> {
        let slow_queries = self.slow_queries.read().unwrap();
        slow_queries
            .iter()
            .rev() // Most recent first
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Clear slow query entries
    pub fn clear_slow_queries(&self) {
        self.slow_queries.write().unwrap().clear();
        info!("Slow queries cleared");
    }

    /// Reset all profiling metrics
    pub fn reset(&self) {
        *self.metrics.write().unwrap() = ProfilingMetrics::new();
        self.execution_times.write().unwrap().clear();
        self.slow_queries.write().unwrap().clear();
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
            slow_queries: self.slow_queries.read().unwrap().iter().cloned().collect(),
        };

        serde_json::to_string_pretty(&export).unwrap_or_else(|_| "{}".to_string())
    }
}

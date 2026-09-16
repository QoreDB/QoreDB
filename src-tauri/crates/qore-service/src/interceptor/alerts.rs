// SPDX-License-Identifier: BUSL-1.1

//! Threshold alerts over a sliding 15-minute window: error rate and number of
//! slow queries. Each threshold fires at most once per window.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

pub const WINDOW: Duration = Duration::from_secs(15 * 60);
/// An error rate over a handful of queries is noise, not a signal.
pub const MIN_SAMPLES_FOR_ERROR_RATE: usize = 10;

#[derive(Debug, Clone, Copy, Default)]
pub struct Thresholds {
    pub error_rate_percent: Option<u32>,
    pub slow_queries_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Threshold {
    ErrorRate {
        percent: f64,
        threshold: u32,
        total: u64,
    },
    SlowQueries {
        count: u64,
        threshold: u32,
    },
}

#[derive(Default)]
struct Inner {
    samples: VecDeque<(Instant, bool, bool)>,
    last_error_alert: Option<Instant>,
    last_slow_alert: Option<Instant>,
}

#[derive(Default)]
pub struct ThresholdMonitor {
    inner: Mutex<Inner>,
}

impl ThresholdMonitor {
    pub fn observe(
        &self,
        thresholds: &Thresholds,
        success: bool,
        slow: bool,
        now: Instant,
    ) -> Vec<Threshold> {
        let mut inner = self.inner.lock();
        inner.samples.push_back((now, success, slow));
        while inner
            .samples
            .front()
            .is_some_and(|(at, _, _)| now.duration_since(*at) > WINDOW)
        {
            inner.samples.pop_front();
        }
        let mut fired = Vec::new();
        let armed = |last: Option<Instant>| last.is_none_or(|at| now.duration_since(at) >= WINDOW);

        if let Some(threshold) = thresholds.error_rate_percent.filter(|t| *t > 0) {
            let total = inner.samples.len();
            let failed = inner.samples.iter().filter(|(_, ok, _)| !ok).count();
            let percent = failed as f64 * 100.0 / total.max(1) as f64;
            if total >= MIN_SAMPLES_FOR_ERROR_RATE
                && percent >= threshold as f64
                && armed(inner.last_error_alert)
            {
                inner.last_error_alert = Some(now);
                fired.push(Threshold::ErrorRate {
                    percent,
                    threshold,
                    total: total as u64,
                });
            }
        }
        if let Some(threshold) = thresholds.slow_queries_count.filter(|t| *t > 0) {
            let count = inner.samples.iter().filter(|(_, _, s)| *s).count();
            if count >= threshold as usize && armed(inner.last_slow_alert) {
                inner.last_slow_alert = Some(now);
                fired.push(Threshold::SlowQueries {
                    count: count as u64,
                    threshold,
                });
            }
        }
        fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: Thresholds = Thresholds {
        error_rate_percent: Some(50),
        slow_queries_count: Some(3),
    };

    #[test]
    fn error_rate_needs_enough_samples_and_fires_once_per_window() {
        let monitor = ThresholdMonitor::default();
        let start = Instant::now();
        for _ in 0..9 {
            assert!(monitor.observe(&BOTH, false, false, start).is_empty());
        }
        let fired = monitor.observe(&BOTH, false, false, start);
        assert_eq!(
            fired,
            vec![Threshold::ErrorRate {
                percent: 100.0,
                threshold: 50,
                total: 10
            }]
        );
        assert!(monitor.observe(&BOTH, false, false, start).is_empty());
        assert!(
            !monitor
                .observe(&BOTH, false, false, start + WINDOW)
                .is_empty()
        );
    }

    #[test]
    fn slow_queries_count_within_the_window() {
        let monitor = ThresholdMonitor::default();
        let start = Instant::now();
        assert!(monitor.observe(&BOTH, true, true, start).is_empty());
        assert!(monitor.observe(&BOTH, true, true, start).is_empty());
        assert_eq!(
            monitor.observe(&BOTH, true, true, start),
            vec![Threshold::SlowQueries {
                count: 3,
                threshold: 3
            }]
        );
        // The window slides: old samples drop out and the count restarts.
        let later = start + WINDOW + Duration::from_secs(1);
        assert!(monitor.observe(&BOTH, true, true, later).is_empty());
    }

    #[test]
    fn disabled_thresholds_never_fire() {
        let monitor = ThresholdMonitor::default();
        let start = Instant::now();
        for _ in 0..20 {
            assert!(
                monitor
                    .observe(&Thresholds::default(), false, true, start)
                    .is_empty()
            );
        }
    }
}

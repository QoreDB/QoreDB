// SPDX-License-Identifier: BUSL-1.1

//! P95 regression rule: the last 24 h run at least twice slower than the
//! seven days before, on enough executions to mean something.

use chrono::{DateTime, Duration, Utc};

use super::trends::{Regression, Sample, percentile};

pub const MIN_RECENT_EXECUTIONS: usize = 20;
pub const FACTOR: f64 = 2.0;

pub fn detect(samples: &[Sample], now: DateTime<Utc>) -> Option<Regression> {
    let recent_start = now - Duration::hours(24);
    let baseline_start = recent_start - Duration::days(7);

    let mut recent: Vec<f64> = samples
        .iter()
        .filter(|s| s.timestamp >= recent_start)
        .map(|s| s.duration_ms)
        .collect();
    let mut baseline: Vec<f64> = samples
        .iter()
        .filter(|s| s.timestamp >= baseline_start && s.timestamp < recent_start)
        .map(|s| s.duration_ms)
        .collect();
    if recent.len() < MIN_RECENT_EXECUTIONS || baseline.is_empty() {
        return None;
    }
    recent.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    baseline.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let recent_p95 = percentile(&recent, 95);
    let baseline_p95 = percentile(&baseline, 95);
    (baseline_p95 > 0.0 && recent_p95 > FACTOR * baseline_p95).then_some(Regression {
        recent_p95_ms: recent_p95,
        baseline_p95_ms: baseline_p95,
        recent_count: recent.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn samples(recent: usize, recent_ms: f64, baseline: usize, baseline_ms: f64) -> Vec<Sample> {
        let now = Utc.with_ymd_and_hms(2026, 9, 6, 12, 0, 0).unwrap();
        let mut out: Vec<Sample> = (0..recent)
            .map(|_| Sample {
                timestamp: now - Duration::hours(1),
                duration_ms: recent_ms,
                success: true,
            })
            .collect();
        out.extend((0..baseline).map(|_| Sample {
            timestamp: now - Duration::days(3),
            duration_ms: baseline_ms,
            success: true,
        }));
        out
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 6, 12, 0, 0).unwrap()
    }

    #[test]
    fn flags_a_doubling_of_p95_on_enough_executions() {
        let regression = detect(&samples(20, 250.0, 5, 100.0), now()).unwrap();
        assert_eq!(regression.recent_p95_ms, 250.0);
        assert_eq!(regression.baseline_p95_ms, 100.0);
        assert_eq!(regression.recent_count, 20);
    }

    #[test]
    fn stays_quiet_below_the_thresholds() {
        assert!(detect(&samples(19, 250.0, 5, 100.0), now()).is_none());
        assert!(detect(&samples(20, 200.0, 5, 100.0), now()).is_none());
        assert!(detect(&samples(20, 250.0, 0, 100.0), now()).is_none());
    }
}

// SPDX-License-Identifier: Apache-2.0

//! Per-fingerprint trends computed from the audit log on disk: daily count,
//! P50, P95 and error rate, so a query's behaviour survives app restarts.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::types::AuditLogEntry;

pub const TOP_FINGERPRINTS: usize = 50;

#[derive(Debug, Clone, Deserialize)]
pub struct TrendFilter {
    pub days: u32,
    #[serde(default)]
    pub driver_id: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendPoint {
    /// Calendar day, `YYYY-MM-DD` in UTC.
    pub day: String,
    pub count: u64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub error_rate: f64,
}

/// A P95 regression: the last 24 h against the seven days before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Regression {
    pub recent_p95_ms: f64,
    pub baseline_p95_ms: f64,
    pub recent_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FingerprintTrend {
    pub fingerprint: String,
    pub query_preview: String,
    pub driver_id: String,
    pub database: Option<String>,
    pub count: u64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub error_rate: f64,
    pub points: Vec<TrendPoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regression: Option<Regression>,
}

pub struct Sample {
    pub timestamp: DateTime<Utc>,
    pub duration_ms: f64,
    pub success: bool,
}

/// Nearest-rank percentile on a sorted slice, the same rule as the live
/// profiling store so both screens agree.
pub fn percentile(sorted: &[f64], pct: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (sorted.len() * pct / 100).min(sorted.len() - 1);
    sorted[idx]
}

fn error_rate(samples: &[&Sample]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let failed = samples.iter().filter(|s| !s.success).count();
    failed as f64 / samples.len() as f64
}

fn sorted_durations(samples: &[&Sample]) -> Vec<f64> {
    let mut d: Vec<f64> = samples.iter().map(|s| s.duration_ms).collect();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    d
}

fn matches_filter(entry: &AuditLogEntry, filter: &TrendFilter) -> bool {
    let driver_ok = filter
        .driver_id
        .as_deref()
        .is_none_or(|d| entry.driver_id.eq_ignore_ascii_case(d));
    let database_ok = filter.database.as_deref().is_none_or(|db| {
        entry
            .database
            .as_deref()
            .is_some_and(|d| d.eq_ignore_ascii_case(db))
    });
    driver_ok && database_ok
}

/// Groups the entries by fingerprint and returns the most frequent ones with
/// one point per calendar day over the filter window. Blocked entries never
/// ran, so they are left out.
pub fn compute_trends(
    entries: &[AuditLogEntry],
    filter: &TrendFilter,
    now: DateTime<Utc>,
    detect_regression: impl Fn(&[Sample], DateTime<Utc>) -> Option<Regression>,
) -> Vec<FingerprintTrend> {
    let days = filter.days.max(1) as i64;
    let window_start = now - Duration::days(days);

    struct Group<'a> {
        first: &'a AuditLogEntry,
        samples: Vec<Sample>,
    }
    let mut groups: HashMap<&str, Group<'_>> = HashMap::new();
    for entry in entries {
        let Some(fingerprint) = entry.fingerprint.as_deref() else {
            continue;
        };
        if entry.blocked || entry.timestamp < window_start || !matches_filter(entry, filter) {
            continue;
        }
        let sample = Sample {
            timestamp: entry.timestamp,
            duration_ms: entry.execution_time_ms,
            success: entry.success,
        };
        groups
            .entry(fingerprint)
            .or_insert_with(|| Group {
                first: entry,
                samples: Vec::new(),
            })
            .samples
            .push(sample);
    }

    let mut trends: Vec<FingerprintTrend> = groups
        .into_iter()
        .map(|(fingerprint, group)| {
            let all: Vec<&Sample> = group.samples.iter().collect();
            let sorted = sorted_durations(&all);
            let points = (0..days)
                .map(|offset| {
                    let day = (window_start + Duration::days(offset + 1)).date_naive();
                    let of_day: Vec<&Sample> = group
                        .samples
                        .iter()
                        .filter(|s| s.timestamp.date_naive() == day)
                        .collect();
                    let sorted_day = sorted_durations(&of_day);
                    TrendPoint {
                        day: day.format("%Y-%m-%d").to_string(),
                        count: of_day.len() as u64,
                        p50_ms: percentile(&sorted_day, 50),
                        p95_ms: percentile(&sorted_day, 95),
                        error_rate: error_rate(&of_day),
                    }
                })
                .collect();
            FingerprintTrend {
                fingerprint: fingerprint.to_string(),
                query_preview: group.first.query_preview.clone(),
                driver_id: group.first.driver_id.clone(),
                database: group.first.database.clone(),
                count: group.samples.len() as u64,
                p50_ms: percentile(&sorted, 50),
                p95_ms: percentile(&sorted, 95),
                error_rate: error_rate(&all),
                points,
                regression: detect_regression(&group.samples, now),
            }
        })
        .collect();

    trends.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(a.fingerprint.cmp(&b.fingerprint))
    });
    trends.truncate(TOP_FINGERPRINTS);
    trends
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use crate::interceptor::types::Environment;

    pub fn entry(
        fingerprint: &str,
        timestamp: DateTime<Utc>,
        duration_ms: f64,
        success: bool,
    ) -> AuditLogEntry {
        let mut e = AuditLogEntry::new(
            "s1".into(),
            format!("SELECT {fingerprint}"),
            Environment::Development,
            "postgres".into(),
        );
        e.fingerprint = Some(fingerprint.to_string());
        e.timestamp = timestamp;
        e.execution_time_ms = duration_ms;
        e.success = success;
        e.database = Some("shop".into());
        e
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::entry;
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 6, 12, 0, 0).unwrap()
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        let sorted = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert_eq!(percentile(&sorted, 50), 6.0);
        assert_eq!(percentile(&sorted, 95), 10.0);
        assert_eq!(percentile(&[], 50), 0.0);
    }

    #[test]
    fn groups_by_fingerprint_with_one_point_per_day() {
        let now = now();
        let mut entries = Vec::new();
        for i in 0..10 {
            entries.push(entry(
                "aaa",
                now - Duration::hours(1),
                10.0 + i as f64,
                true,
            ));
        }
        entries.push(entry("aaa", now - Duration::days(1), 100.0, false));
        entries.push(entry("bbb", now - Duration::hours(2), 5.0, true));
        entries.push(entry("old", now - Duration::days(30), 5.0, true));
        let mut blocked = entry("blocked", now, 1.0, true);
        blocked.blocked = true;
        entries.push(blocked);

        let filter = TrendFilter {
            days: 3,
            driver_id: None,
            database: None,
        };
        let trends = compute_trends(&entries, &filter, now, |_, _| None);

        assert_eq!(trends.len(), 2);
        let aaa = &trends[0];
        assert_eq!(aaa.fingerprint, "aaa");
        assert_eq!(aaa.count, 11);
        assert_eq!(aaa.points.len(), 3);
        assert_eq!(aaa.points[2].day, "2026-09-06");
        assert_eq!(aaa.points[2].count, 10);
        assert_eq!(aaa.points[1].count, 1);
        assert_eq!(aaa.points[1].error_rate, 1.0);
        assert_eq!(aaa.points[0].count, 0);
        assert!((aaa.error_rate - 1.0 / 11.0).abs() < 1e-9);
        assert_eq!(aaa.p95_ms, 100.0);
        assert_eq!(aaa.database.as_deref(), Some("shop"));
    }

    #[test]
    fn filters_by_driver_and_database() {
        let now = now();
        let mut pg = entry("aaa", now, 1.0, true);
        pg.driver_id = "postgres".into();
        let mut my = entry("bbb", now, 1.0, true);
        my.driver_id = "mysql".into();
        my.database = Some("other".into());
        let entries = vec![pg, my];

        let by_driver = TrendFilter {
            days: 1,
            driver_id: Some("MySQL".into()),
            database: None,
        };
        let trends = compute_trends(&entries, &by_driver, now, |_, _| None);
        assert_eq!(trends.len(), 1);
        assert_eq!(trends[0].fingerprint, "bbb");

        let by_db = TrendFilter {
            days: 1,
            driver_id: None,
            database: Some("shop".into()),
        };
        let trends = compute_trends(&entries, &by_db, now, |_, _| None);
        assert_eq!(trends.len(), 1);
        assert_eq!(trends[0].fingerprint, "aaa");
    }

    #[test]
    fn keeps_the_most_frequent_fingerprints_only() {
        let now = now();
        let mut entries = Vec::new();
        for fp in 0..(TOP_FINGERPRINTS + 5) {
            for _ in 0..=fp {
                entries.push(entry(&format!("fp{fp:03}"), now, 1.0, true));
            }
        }
        let filter = TrendFilter {
            days: 1,
            driver_id: None,
            database: None,
        };
        let trends = compute_trends(&entries, &filter, now, |_, _| None);
        assert_eq!(trends.len(), TOP_FINGERPRINTS);
        assert_eq!(trends[0].count, (TOP_FINGERPRINTS + 5) as u64);
        assert!(!trends.iter().any(|t| t.fingerprint == "fp000"));
    }
}

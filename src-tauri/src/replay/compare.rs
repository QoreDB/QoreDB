// SPDX-License-Identifier: BUSL-1.1

//! Classification of a replayed entry against its recorded expectation.

use super::types::{ExpectedOutcome, ReplaySummary, ReplayVerdict};

pub struct Observed {
    pub success: bool,
    pub execution_time_ms: f64,
    pub row_count: Option<i64>,
    pub digest: Option<String>,
}

pub struct SlowerThresholds {
    pub ratio: f64,
    pub min_delta_ms: f64,
}

/// A run is only slower when it is both relatively and absolutely worse:
/// a 4 ms query at 9 ms is scheduler noise, not a regression.
fn is_slower(expected_ms: f64, observed_ms: f64, thresholds: &SlowerThresholds) -> bool {
    if expected_ms <= 0.0 {
        return false;
    }
    observed_ms - expected_ms >= thresholds.min_delta_ms
        && observed_ms >= expected_ms * thresholds.ratio
}

pub fn classify(
    expected: &ExpectedOutcome,
    observed: &Observed,
    thresholds: &SlowerThresholds,
) -> ReplayVerdict {
    if expected.success != observed.success {
        return ReplayVerdict::Broken;
    }

    // Both sides failed: the query was already broken at recording time, so
    // this run introduces nothing.
    if !observed.success {
        return ReplayVerdict::Match;
    }

    if let (Some(expected_rows), Some(observed_rows)) = (expected.row_count, observed.row_count)
        && expected_rows != observed_rows
    {
        return ReplayVerdict::RowCountDiff;
    }

    if let (Some(expected_digest), Some(observed_digest)) =
        (expected.result_digest.as_ref(), observed.digest.as_ref())
        && expected_digest != observed_digest
    {
        return ReplayVerdict::DigestDiff;
    }

    if is_slower(
        expected.execution_time_ms,
        observed.execution_time_ms,
        thresholds,
    ) {
        return ReplayVerdict::Slower;
    }

    ReplayVerdict::Match
}

pub fn summarize(verdicts: impl IntoIterator<Item = ReplayVerdict>) -> ReplaySummary {
    let mut summary = ReplaySummary::default();
    for verdict in verdicts {
        summary.total += 1;
        match verdict {
            ReplayVerdict::Match => summary.matched += 1,
            ReplayVerdict::Broken => summary.broken += 1,
            ReplayVerdict::RowCountDiff => summary.row_count_diff += 1,
            ReplayVerdict::DigestDiff => summary.digest_diff += 1,
            ReplayVerdict::Slower => summary.slower += 1,
            ReplayVerdict::Skipped => summary.skipped += 1,
        }
    }
    summary
}

/// Replaces the recorded expectations with what a previous run observed.
///
/// Without this, picking "compare to run X" would still classify against the
/// original recording while the diff opened X — a report saying "identical"
/// next to a diff showing changes.
pub fn rebase_expectations(
    set: &super::types::ReplaySet,
    report: &super::types::ReplayReport,
) -> super::types::ReplaySet {
    use std::collections::HashMap;

    let observed: HashMap<&str, &super::types::ReplayEntryResult> = report
        .results
        .iter()
        .map(|r| (r.entry_id.as_str(), r))
        .collect();

    let mut rebased = set.clone();
    for entry in &mut rebased.entries {
        // An entry the chosen run never executed keeps its recorded
        // expectation: there is nothing better to compare against.
        let Some(result) = observed.get(entry.id.as_str()) else {
            continue;
        };
        if result.verdict == ReplayVerdict::Skipped {
            continue;
        }
        entry.expected.execution_time_ms = result.execution_time_ms;
        entry.expected.row_count = result.row_count;
        entry.expected.success = result.success;
        entry.expected.result_digest = result.digest.clone();
    }
    rebased
}

/// Rebuilds an entry result by treating the left run as the expectation, so a
/// two-connection comparison reuses the same classification as a replay.
pub fn compare_sides(
    left: &super::types::ReplayEntryResult,
    right: &super::types::ReplayEntryResult,
    thresholds: &SlowerThresholds,
) -> super::types::ReplayEntryResult {
    use super::types::{ExpectedOutcome, ReplayEntryResult};

    // A side that never ran carries nothing to compare.
    if left.verdict == ReplayVerdict::Skipped || right.verdict == ReplayVerdict::Skipped {
        return ReplayEntryResult {
            verdict: ReplayVerdict::Skipped,
            skip_reason: left
                .skip_reason
                .clone()
                .or_else(|| right.skip_reason.clone()),
            ..right.clone()
        };
    }

    let expected = ExpectedOutcome {
        execution_time_ms: left.execution_time_ms,
        row_count: left.row_count,
        success: left.success,
        fingerprint: None,
        result_digest: left.digest.clone(),
    };
    let observed = Observed {
        success: right.success,
        execution_time_ms: right.execution_time_ms,
        row_count: right.row_count,
        digest: right.digest.clone(),
    };

    ReplayEntryResult {
        verdict: classify(&expected, &observed, thresholds),
        expected_execution_time_ms: left.execution_time_ms,
        expected_row_count: left.row_count,
        expected_digest: left.digest.clone(),
        // Both sides must be on disk for the diff to open.
        captured: left.captured && right.captured,
        partial_comparison: left.partial_comparison || right.partial_comparison,
        ..right.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::types::{DEFAULT_SLOWER_MIN_DELTA_MS, DEFAULT_SLOWER_RATIO};

    fn thresholds() -> SlowerThresholds {
        SlowerThresholds {
            ratio: DEFAULT_SLOWER_RATIO,
            min_delta_ms: DEFAULT_SLOWER_MIN_DELTA_MS,
        }
    }

    fn expected(rows: i64, ms: f64, digest: &str) -> ExpectedOutcome {
        ExpectedOutcome {
            execution_time_ms: ms,
            row_count: Some(rows),
            success: true,
            fingerprint: None,
            result_digest: Some(digest.to_string()),
        }
    }

    fn observed(rows: i64, ms: f64, digest: &str) -> Observed {
        Observed {
            success: true,
            execution_time_ms: ms,
            row_count: Some(rows),
            digest: Some(digest.to_string()),
        }
    }

    #[test]
    fn identical_run_matches() {
        let verdict = classify(
            &expected(42, 12.0, "sha256:a"),
            &observed(42, 13.0, "sha256:a"),
            &thresholds(),
        );
        assert_eq!(verdict, ReplayVerdict::Match);
    }

    #[test]
    fn failure_after_success_is_broken() {
        let mut obs = observed(0, 3.0, "sha256:a");
        obs.success = false;
        assert_eq!(
            classify(&expected(42, 12.0, "sha256:a"), &obs, &thresholds()),
            ReplayVerdict::Broken
        );
    }

    #[test]
    fn two_failures_are_not_a_regression() {
        let mut exp = expected(0, 5.0, "sha256:a");
        exp.success = false;
        exp.result_digest = None;
        let obs = Observed {
            success: false,
            execution_time_ms: 6.0,
            row_count: None,
            digest: None,
        };
        assert_eq!(classify(&exp, &obs, &thresholds()), ReplayVerdict::Match);
    }

    #[test]
    fn row_count_wins_over_digest() {
        assert_eq!(
            classify(
                &expected(42, 12.0, "sha256:a"),
                &observed(41, 12.0, "sha256:b"),
                &thresholds()
            ),
            ReplayVerdict::RowCountDiff
        );
    }

    #[test]
    fn digest_diff_is_reported_when_counts_agree() {
        assert_eq!(
            classify(
                &expected(42, 12.0, "sha256:a"),
                &observed(42, 12.0, "sha256:b"),
                &thresholds()
            ),
            ReplayVerdict::DigestDiff
        );
    }

    #[test]
    fn missing_digest_falls_back_to_metadata_only_comparison() {
        let mut exp = expected(42, 12.0, "sha256:a");
        exp.result_digest = None;
        let mut obs = observed(42, 12.0, "sha256:b");
        obs.digest = None;
        assert_eq!(classify(&exp, &obs, &thresholds()), ReplayVerdict::Match);
    }

    #[test]
    fn small_absolute_slowdown_is_noise() {
        assert_eq!(
            classify(
                &expected(42, 4.0, "sha256:a"),
                &observed(42, 40.0, "sha256:a"),
                &thresholds()
            ),
            ReplayVerdict::Match
        );
    }

    #[test]
    fn large_slowdown_is_reported() {
        assert_eq!(
            classify(
                &expected(42, 120.0, "sha256:a"),
                &observed(42, 400.0, "sha256:a"),
                &thresholds()
            ),
            ReplayVerdict::Slower
        );
    }

    #[test]
    fn slowdown_needs_the_ratio_too() {
        assert_eq!(
            classify(
                &expected(42, 1000.0, "sha256:a"),
                &observed(42, 1150.0, "sha256:a"),
                &thresholds()
            ),
            ReplayVerdict::Match
        );
    }

    fn side(
        rows: i64,
        ms: f64,
        digest: Option<&str>,
        captured: bool,
    ) -> super::super::types::ReplayEntryResult {
        super::super::types::ReplayEntryResult {
            entry_id: "e".into(),
            order: 1,
            query_preview: "SELECT 1".into(),
            verdict: ReplayVerdict::Match,
            success: true,
            error: None,
            skip_reason: None,
            execution_time_ms: ms,
            expected_execution_time_ms: 0.0,
            row_count: Some(rows),
            expected_row_count: None,
            digest: digest.map(|d| d.to_string()),
            expected_digest: None,
            captured,
            partial_comparison: false,
        }
    }

    fn report_with(
        results: Vec<super::super::types::ReplayEntryResult>,
    ) -> super::super::types::ReplayReport {
        use super::super::types::{CaptureMode, ReplayReport, RunMeta};
        ReplayReport {
            run: RunMeta {
                run_id: "run".into(),
                project_id: "default".into(),
                set_slug: "s".into(),
                set_name: "s".into(),
                started_at: String::new(),
                finished_at: None,
                connection_label: None,
                driver_id: "postgres".into(),
                environment: "staging".into(),
                capture_mode: CaptureMode::Full,
                capture_stopped_reason: None,
                is_baseline: false,
                captured_bytes: 0,
                entry_count: results.len(),
            },
            baseline_run_id: None,
            summary: summarize(results.iter().map(|r| r.verdict)),
            results,
        }
    }

    /// Picking "compare to run X" must classify against X, not against the
    /// original recording.
    #[test]
    fn rebasing_takes_the_expectations_from_the_chosen_run() {
        use super::super::types::{
            ExpectedOutcome, REPLAY_SET_VERSION, ReplayEntry, ReplaySet, ReplaySource,
        };

        let mut observed = side(99, 250.0, Some("sha256:new"), true);
        observed.entry_id = "e1".into();

        let set = ReplaySet {
            version: REPLAY_SET_VERSION,
            name: "s".into(),
            created_at: String::new(),
            source: ReplaySource {
                driver_id: "postgres".into(),
                connection_label: None,
                environment: "staging".into(),
            },
            ignored_columns: Vec::new(),
            entries: vec![ReplayEntry {
                id: "e1".into(),
                order: 1,
                query: "SELECT 1".into(),
                driver_id: "postgres".into(),
                namespace: None,
                operation_type: "select".into(),
                is_mutation: false,
                expected: ExpectedOutcome {
                    execution_time_ms: 10.0,
                    row_count: Some(42),
                    success: true,
                    fingerprint: None,
                    result_digest: Some("sha256:recorded".into()),
                },
            }],
        };

        let rebased = rebase_expectations(&set, &report_with(vec![observed]));
        assert_eq!(rebased.entries[0].expected.row_count, Some(99));
        assert_eq!(rebased.entries[0].expected.execution_time_ms, 250.0);
        assert_eq!(
            rebased.entries[0].expected.result_digest.as_deref(),
            Some("sha256:new")
        );

        // An entry the chosen run skipped keeps its recorded expectation.
        let mut skipped = side(0, 0.0, None, false);
        skipped.entry_id = "e1".into();
        skipped.verdict = ReplayVerdict::Skipped;
        let kept = rebase_expectations(&set, &report_with(vec![skipped]));
        assert_eq!(kept.entries[0].expected.row_count, Some(42));
        assert_eq!(
            kept.entries[0].expected.result_digest.as_deref(),
            Some("sha256:recorded")
        );
    }

    #[test]
    fn ab_compares_the_two_live_sides() {
        let left = side(42, 10.0, Some("sha256:a"), true);
        let right = side(42, 11.0, Some("sha256:b"), true);
        let merged = compare_sides(&left, &right, &thresholds());
        assert_eq!(merged.verdict, ReplayVerdict::DigestDiff);
        assert_eq!(merged.expected_row_count, Some(42));
        assert_eq!(merged.expected_digest.as_deref(), Some("sha256:a"));
        assert!(merged.captured, "both sides captured, the diff can open");
    }

    #[test]
    fn ab_needs_both_sides_captured_to_offer_a_diff() {
        let left = side(42, 10.0, Some("sha256:a"), false);
        let right = side(42, 10.0, Some("sha256:a"), true);
        assert!(!compare_sides(&left, &right, &thresholds()).captured);
    }

    #[test]
    fn ab_skips_when_either_side_was_skipped() {
        let mut left = side(0, 0.0, None, false);
        left.verdict = ReplayVerdict::Skipped;
        left.skip_reason = Some("Mutation excluded from replay".to_string());
        let right = side(42, 10.0, Some("sha256:a"), true);

        let merged = compare_sides(&left, &right, &thresholds());
        assert_eq!(merged.verdict, ReplayVerdict::Skipped);
        assert_eq!(
            merged.skip_reason.as_deref(),
            Some("Mutation excluded from replay")
        );
    }

    #[test]
    fn summary_counts_each_verdict() {
        let summary = summarize([
            ReplayVerdict::Match,
            ReplayVerdict::Match,
            ReplayVerdict::Broken,
            ReplayVerdict::Skipped,
        ]);
        assert_eq!(summary.total, 4);
        assert_eq!(summary.matched, 2);
        assert_eq!(summary.broken, 1);
        assert_eq!(summary.skipped, 1);
    }
}

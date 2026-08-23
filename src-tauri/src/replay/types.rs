// SPDX-License-Identifier: BUSL-1.1

use serde::{Deserialize, Serialize};

use crate::engine::types::Namespace;

pub const REPLAY_SET_VERSION: u32 = 1;

/// Rows captured per entry. Same default as the Visual Data Diff, so a report
/// row can be opened in the diff viewer without a second bound to explain.
pub const DEFAULT_MAX_CAPTURED_ROWS: usize = 1000;

/// Bytes a single run may write before capture stops. The run keeps going —
/// only the row capture is dropped, and the report says so.
pub const DEFAULT_CAPTURE_BUDGET_BYTES: u64 = 64 * 1024 * 1024;

/// Runs kept per set before the oldest are purged.
pub const DEFAULT_RUN_RETENTION: usize = 10;

/// A run is slower only when it is both relatively and absolutely worse:
/// a 4 ms query going to 9 ms is noise, not a regression.
pub const DEFAULT_SLOWER_RATIO: f64 = 2.0;
pub const DEFAULT_SLOWER_MIN_DELTA_MS: f64 = 100.0;

/// Whether a run persists result rows or only metadata and digests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    #[default]
    Full,
    MetadataOnly,
}

/// Why a run stopped persisting rows. Reported so a partial capture never
/// reads as a complete one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureStopReason {
    BudgetExceeded,
    MetadataOnly,
    ProductionPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySource {
    pub driver_id: String,
    #[serde(default)]
    pub connection_label: Option<String>,
    pub environment: String,
}

/// What the recording observed, and what a replay is compared against.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedOutcome {
    pub execution_time_ms: f64,
    #[serde(default)]
    pub row_count: Option<i64>,
    pub success: bool,
    #[serde(default)]
    pub fingerprint: Option<String>,
    /// `None` when the recording ran in metadata-only mode.
    #[serde(default)]
    pub result_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayEntry {
    pub id: String,
    pub order: u32,
    pub query: String,
    pub driver_id: String,
    #[serde(default)]
    pub namespace: Option<Namespace>,
    pub operation_type: String,
    #[serde(default)]
    pub is_mutation: bool,
    pub expected: ExpectedOutcome,
}

/// The shareable half of the feature: queries and expectations, no data.
/// Lives in `.qoredb/replays/<slug>.qreplay.json` and is meant to be committed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySet {
    pub version: u32,
    pub name: String,
    pub created_at: String,
    pub source: ReplaySource,
    /// Column names excluded from the digest. Without this, any table with an
    /// `updated_at` reports a diff on every run.
    #[serde(default)]
    pub ignored_columns: Vec<String>,
    /// Query text was redacted on the way in. Shareable without reservation,
    /// and no longer replayable — a redacted literal does not match anything.
    #[serde(default)]
    pub redacted: bool,
    pub entries: Vec<ReplayEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplaySetSummary {
    pub slug: String,
    pub name: String,
    pub created_at: String,
    pub driver_id: String,
    pub environment: String,
    pub entry_count: usize,
    pub redacted: bool,
}

/// The local half: one directory per run under `data_dir/replays/<run_id>/`,
/// never versioned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMeta {
    pub run_id: String,
    /// Workspace the run belongs to; also its directory under `data_dir`.
    #[serde(default)]
    pub project_id: String,
    pub set_slug: String,
    pub set_name: String,
    pub started_at: String,
    #[serde(default)]
    pub finished_at: Option<String>,
    #[serde(default)]
    pub connection_label: Option<String>,
    pub driver_id: String,
    pub environment: String,
    pub capture_mode: CaptureMode,
    #[serde(default)]
    pub capture_stopped_reason: Option<CaptureStopReason>,
    /// The recording run, used as the left side of a comparison.
    #[serde(default)]
    pub is_baseline: bool,
    #[serde(default)]
    pub captured_bytes: u64,
    #[serde(default)]
    pub entry_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayVerdict {
    Match,
    /// Succeeded at recording, fails now (or the reverse).
    Broken,
    RowCountDiff,
    DigestDiff,
    Slower,
    /// Not run: mutation excluded, or blocked by the production policy.
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayEntryResult {
    pub entry_id: String,
    pub order: u32,
    pub query_preview: String,
    pub verdict: ReplayVerdict,
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub skip_reason: Option<String>,
    pub execution_time_ms: f64,
    pub expected_execution_time_ms: f64,
    #[serde(default)]
    pub row_count: Option<i64>,
    #[serde(default)]
    pub expected_row_count: Option<i64>,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub expected_digest: Option<String>,
    /// Rows for this entry are on disk for this run, so the diff can be opened.
    #[serde(default)]
    pub captured: bool,
    /// Comparison covered only the first `max_captured_rows` rows.
    #[serde(default)]
    pub partial_comparison: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplaySummary {
    pub total: usize,
    pub matched: usize,
    pub broken: usize,
    pub row_count_diff: usize,
    pub digest_diff: usize,
    pub slower: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayReport {
    pub run: RunMeta,
    #[serde(default)]
    pub baseline_run_id: Option<String>,
    pub results: Vec<ReplayEntryResult>,
    pub summary: ReplaySummary,
}

/// Two runs of the same set on two connections, compared against each other
/// rather than against the recording. Both sides are live, so the comparison
/// holds even when neither side has captured rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayAbReport {
    pub left: RunMeta,
    pub right: RunMeta,
    pub results: Vec<ReplayEntryResult>,
    pub summary: ReplaySummary,
}

/// Knobs a run reads. Persisted with the report so a run can be read back
/// with the thresholds it was judged under.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayRunOptions {
    #[serde(default)]
    pub capture_mode: CaptureMode,
    /// Off by default: a replay is a read.
    #[serde(default)]
    pub allow_mutations: bool,
    #[serde(default = "default_max_rows")]
    pub max_captured_rows: usize,
    #[serde(default = "default_budget")]
    pub capture_budget_bytes: u64,
    #[serde(default = "default_ratio")]
    pub slower_ratio: f64,
    #[serde(default = "default_min_delta")]
    pub slower_min_delta_ms: f64,
    #[serde(default = "default_retention")]
    pub run_retention: usize,
}

fn default_max_rows() -> usize {
    DEFAULT_MAX_CAPTURED_ROWS
}
fn default_budget() -> u64 {
    DEFAULT_CAPTURE_BUDGET_BYTES
}
fn default_ratio() -> f64 {
    DEFAULT_SLOWER_RATIO
}
fn default_min_delta() -> f64 {
    DEFAULT_SLOWER_MIN_DELTA_MS
}
fn default_retention() -> usize {
    DEFAULT_RUN_RETENTION
}

impl Default for ReplayRunOptions {
    fn default() -> Self {
        Self {
            capture_mode: CaptureMode::Full,
            allow_mutations: false,
            max_captured_rows: DEFAULT_MAX_CAPTURED_ROWS,
            capture_budget_bytes: DEFAULT_CAPTURE_BUDGET_BYTES,
            slower_ratio: DEFAULT_SLOWER_RATIO,
            slower_min_delta_ms: DEFAULT_SLOWER_MIN_DELTA_MS,
            run_retention: DEFAULT_RUN_RETENTION,
        }
    }
}

/// Progress event emitted while a run is in flight.
#[derive(Debug, Clone, Serialize)]
pub struct ReplayProgress {
    pub run_id: String,
    pub completed: usize,
    pub total: usize,
    pub current_query_preview: String,
}

pub fn query_preview(query: &str) -> String {
    let mut preview = query.chars().take(120).collect::<String>();
    if query.chars().nth(120).is_some() {
        preview.push('…');
    }
    preview
}

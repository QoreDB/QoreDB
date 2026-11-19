// SPDX-License-Identifier: BUSL-1.1

//! Query Replay Lab desktop runtime (capture, recorder, runner) + shared store/types.

pub mod capture;
pub mod recorder;
pub mod runner;

pub use qore_service::replay::{
    CaptureMode, CaptureStopReason, DEFAULT_CAPTURE_BUDGET_BYTES, DEFAULT_MAX_CAPTURED_ROWS,
    DEFAULT_RUN_RETENTION, DEFAULT_SLOWER_MIN_DELTA_MS, DEFAULT_SLOWER_RATIO, ExpectedOutcome,
    REPLAY_SET_VERSION, ReplayAbReport, ReplayEntry, ReplayEntryResult, ReplayProgress,
    ReplayReport, ReplayRunOptions, ReplaySet, ReplaySetStore, ReplaySetSummary, ReplaySource,
    ReplaySummary, ReplayVerdict, RunMeta, SecretPolicy, compare, digest, looks_like_secret,
    query_preview, secrets, slugify, store, types, validate_slug,
};

pub use capture::CaptureStore;
pub use recorder::{Recorder, RecordingOptions, RecordingStatus};

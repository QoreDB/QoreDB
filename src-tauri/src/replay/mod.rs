// SPDX-License-Identifier: BUSL-1.1

//! Query Replay Lab: record a set of queries, replay it against another
//! connection or after a migration, and report what changed.
//!
//! The set (`.qoredb/replays/`) holds queries and expectations and is meant to
//! be committed; captured rows (`data_dir/replays/`) are the local evidence and
//! never leave the machine.

pub mod capture;
pub mod compare;
pub mod digest;
pub mod recorder;
pub mod runner;
pub mod store;
pub mod types;

pub use capture::CaptureStore;
pub use recorder::{Recorder, RecordingOptions, RecordingStatus};
pub use store::{ReplaySetStore, slugify, validate_slug};
pub use types::*;

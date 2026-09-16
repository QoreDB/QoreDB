// SPDX-License-Identifier: Apache-2.0

//! Universal Query Interceptor
//!
//! A comprehensive query interception system for:
//! - **Audit Logging**: Persistent logging of all query executions
//! - **Profiling**: Performance metrics, percentiles, and slow query detection
//! - **Safety Net**: Rule-based blocking and warning for dangerous queries
//!
//! This module implements the interceptor in the Rust backend for maximum security.
//! The frontend only displays and configures what the backend provides.

pub mod alerts;
pub mod audit;
pub mod export;
pub mod fingerprint;
pub mod n_plus_one;
pub mod pipeline;
pub mod profiling;
pub mod redaction;
pub mod regression;
pub mod safety;
pub mod trends;
pub mod types;

pub use audit::{AuditStats, AuditStore};
pub use export::{AuditExportFormat, export_entries};
pub use fingerprint::fingerprint_query;
pub use pipeline::InterceptorPipeline;
pub use profiling::ProfilingStore;
pub use redaction::redact_query_forced;
pub use safety::SafetyEngine;
pub use trends::{FingerprintTrend, Regression, TrendFilter, TrendPoint};
pub use types::*;

//! Universal Query Interceptor
//!
//! A comprehensive query interception system for:
//! - **Audit Logging**: Persistent logging of all query executions
//! - **Profiling**: Performance metrics, percentiles, and slow query detection
//! - **Safety Net**: Rule-based blocking and warning for dangerous queries
//!
//! This module implements the interceptor in the Rust backend for maximum security.
//! The frontend only displays and configures what the backend provides.

pub mod audit;
pub mod pipeline;
pub mod profiling;
pub mod safety;
pub mod types;

pub use audit::{AuditStats, AuditStore};
pub use pipeline::InterceptorPipeline;
pub use profiling::ProfilingStore;
pub use safety::SafetyEngine;
pub use types::*;

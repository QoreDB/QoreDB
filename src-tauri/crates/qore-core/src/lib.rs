// SPDX-License-Identifier: Apache-2.0

//! QoreCore — Universal database engine abstraction.
//!
//! Types, traits, and error handling for the QoreDB engine.

pub mod error;
pub mod registry;
pub mod traits;
pub mod types;

// Re-exports for convenience
pub use error::{sanitize_error_message, EngineError, EngineResult};
pub use registry::DriverRegistry;
pub use traits::{DataEngine, StreamEvent, StreamSender};
pub use types::*;

// SPDX-License-Identifier: Apache-2.0

//! QoreCore — universal database engine abstraction: types, traits, errors.

pub mod error;
pub mod registry;
pub mod traits;
pub mod types;

pub use error::{sanitize_error_message, EngineError, EngineResult};
pub use registry::DriverRegistry;
pub use traits::{DataEngine, StreamEvent, StreamSender};
pub use types::*;

// Data Engine Module
// Universal abstraction layer for all database engines

pub mod drivers;
pub mod error;
pub mod registry;
pub mod traits;
pub mod types;

pub use error::EngineError;
pub use registry::DriverRegistry;
pub use traits::DataEngine;
pub use types::*;

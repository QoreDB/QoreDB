// Data Engine Module
// Universal abstraction layer for all database engines

pub mod connection_url;
pub mod drivers;
pub mod error;
pub mod fulltext_strategy;
pub mod mongo_safety;
pub mod query_manager;
pub mod registry;
pub mod sql_generator;
pub mod sql_safety;
pub mod session_manager;
pub mod ssh_tunnel;
pub mod traits;
pub mod types;

pub use error::EngineError;
pub use query_manager::QueryManager;
pub use registry::DriverRegistry;
pub use session_manager::SessionManager;
pub use traits::DataEngine;
pub use types::*;


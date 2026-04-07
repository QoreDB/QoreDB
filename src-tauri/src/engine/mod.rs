// SPDX-License-Identifier: Apache-2.0

// Data Engine Module — Facade
//
// All engine code now lives in the qore-core, qore-sql, and qore-drivers crates.
// This module re-exports everything for backwards compatibility so that existing
// `use crate::engine::*` imports continue to work without changes.

// ── Re-exports from qore-core ──────────────────────────────────────
pub mod error {
    pub use qore_core::error::*;
}
pub mod traits {
    pub use qore_core::traits::*;
}
pub mod types {
    pub use qore_core::types::*;
}
pub mod registry {
    pub use qore_core::registry::*;
}

// ── Re-exports from qore-sql ──────────────────────────────────────
pub mod sql_safety {
    pub use qore_sql::safety::*;
}
pub mod sql_generator {
    pub use qore_sql::generator::*;
}
pub mod connection_url {
    pub use qore_sql::connection_url::*;
}

// ── Re-exports from qore-drivers ──────────────────────────────────
pub mod drivers {
    pub use qore_drivers::drivers::*;
}
pub mod fulltext_strategy {
    pub use qore_drivers::fulltext_strategy::*;
}
pub mod mongo_safety {
    pub use qore_drivers::mongo_safety::*;
}
pub mod proxy {
    pub use qore_drivers::proxy::*;
}
pub mod query_manager {
    pub use qore_drivers::query_manager::*;
}
pub mod redis_safety {
    pub use qore_drivers::redis_safety::*;
}
pub mod schema_export {
    pub use qore_drivers::schema_export::*;
}
pub mod session_manager {
    pub use qore_drivers::session_manager::*;
}
pub mod ssh_tunnel {
    pub use qore_drivers::ssh_tunnel::*;
}

// ── Convenience re-exports ────────────────────────────────────────
pub use qore_core::error::EngineError;
pub use qore_core::registry::DriverRegistry;
pub use qore_core::traits::DataEngine;
pub use qore_core::types::*;
pub use qore_drivers::query_manager::QueryManager;
pub use qore_drivers::session_manager::SessionManager;

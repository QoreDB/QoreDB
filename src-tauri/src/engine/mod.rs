// SPDX-License-Identifier: Apache-2.0

// Data Engine Module
// Universal abstraction layer for all database engines
//
// Core types, traits, errors, and registry are provided by the `qore-core` crate.
// SQL utilities (safety, generator, connection URLs) are provided by `qore-sql`.
// This module re-exports them for backwards compatibility and hosts the
// remaining modules (drivers, session management, etc.) that haven't been
// extracted yet.

// ── Re-exports from qore-core ──────────────────────────────────────
pub mod error {
    pub use qore_core::error::*;
}
pub mod traits {
    pub use qore_core::traits::*;
}
pub mod types {
    pub use qore_core::types::*;

    // Sub-module re-export
    pub mod collection_list {
        // CollectionList and CollectionListOptions are defined in qore_core::types directly
    }
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

// ── Modules still in this crate ───────────────────────────────────
pub mod drivers;
pub mod fulltext_strategy;
pub mod mongo_safety;
pub mod proxy;
pub mod query_manager;
pub mod redis_safety;
pub mod schema_export;
pub mod session_manager;
pub mod ssh_tunnel;

// ── Convenience re-exports ────────────────────────────────────────
pub use qore_core::error::EngineError;
pub use qore_core::registry::DriverRegistry;
pub use qore_core::traits::DataEngine;
pub use qore_core::types::*;
pub use query_manager::QueryManager;
pub use session_manager::SessionManager;

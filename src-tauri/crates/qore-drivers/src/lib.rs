// SPDX-License-Identifier: Apache-2.0

//! QoreDrivers — Database driver implementations for QoreCore.
//!
//! Contains all driver implementations (PostgreSQL, MySQL, SQLite, MongoDB,
//! Redis, DuckDB, SQL Server, CockroachDB, MariaDB), session management,
//! SSH tunneling, and query tracking.

pub mod drivers;
pub mod fulltext_strategy;
pub mod mongo_safety;
pub mod proxy;
pub mod query_manager;
pub mod redis_safety;
pub mod schema_export;
pub mod session_manager;
pub mod ssh_tunnel;

// Re-exports for convenience
pub use query_manager::QueryManager;
pub use session_manager::SessionManager;

// SPDX-License-Identifier: BUSL-1.1

//! Data Time-Travel Module
//!
//! "Git blame" for database data — captures before/after images of mutations
//! made through QoreDB's DataGrid, enabling timeline visualization,
//! temporal diffs, and rollback SQL generation.

pub mod capture;
pub mod rollback;
pub mod store;
pub mod types;

pub use store::ChangelogStore;
pub use types::*;

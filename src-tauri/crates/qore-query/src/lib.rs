// SPDX-License-Identifier: Apache-2.0

//! QoreQuery — Type-safe multi-dialect SQL query builder.
//!
//! Built on top of [`qore_core`] (universal value/row types) and
//! [`qore_sql`] (dialect quoting and formatting).
//!
//! See `doc/QoreQuery_Builder_Plan.md` for the full Phase 2 plan.

pub mod built;
pub mod compiler;
pub mod error;
pub mod expr;
pub mod ident;
pub mod prelude;
pub mod query;

pub use built::BuiltQuery;
pub use error::{QueryError, QueryResult};
pub use ident::{col, Column};

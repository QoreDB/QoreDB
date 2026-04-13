// SPDX-License-Identifier: Apache-2.0

//! Typical import for query building:
//!
//! ```ignore
//! use qore_query::prelude::*;
//! ```

pub use crate::built::BuiltQuery;
pub use crate::error::{QueryError, QueryResult};
pub use crate::expr::{BinOp, Expr, UnOp};
pub use crate::ident::{col, Column};
pub use qore_sql::generator::SqlDialect as Dialect;

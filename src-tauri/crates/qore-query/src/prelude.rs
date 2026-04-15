// SPDX-License-Identifier: Apache-2.0

//! Typical import for query building:
//!
//! ```ignore
//! use qore_query::prelude::*;
//! ```

pub use crate::built::BuiltQuery;
pub use crate::dialect::Dialect;
pub use crate::error::{QueryError, QueryResult};
pub use crate::expr::{
    avg, cast, coalesce, count, count_all, count_distinct, exists, max, min, not_exists, sum,
    AggFn, BinOp, Expr, UnOp,
};
pub use crate::ident::{col, tcol, Column, IntoOperand};
pub use crate::sql_type::SqlType;

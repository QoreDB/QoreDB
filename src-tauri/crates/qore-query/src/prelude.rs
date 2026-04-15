// SPDX-License-Identifier: Apache-2.0

//! Typical import for query building:
//!
//! ```ignore
//! use qore_query::prelude::*;
//! ```

pub use crate::built::BuiltQuery;
pub use crate::dialect::Dialect;
pub use crate::error::{QueryError, QueryResult};
pub use crate::expr::{cast, coalesce, exists, not_exists, BinOp, Expr, UnOp};
pub use crate::ident::{col, tcol, Column, IntoOperand};
pub use crate::sql_type::SqlType;

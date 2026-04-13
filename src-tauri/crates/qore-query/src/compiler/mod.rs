// SPDX-License-Identifier: Apache-2.0

//! Query → target-language compilation.

pub mod sql;

use crate::built::BuiltQuery;
use crate::error::QueryResult;
use crate::query::SelectQuery;

pub trait QueryCompiler {
    fn compile_select(&self, q: &SelectQuery) -> QueryResult<BuiltQuery>;
}

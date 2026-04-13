// SPDX-License-Identifier: Apache-2.0

//! Generic SQL compiler — shared implementation across dialects.
//! Dialect-specific overrides (postgres/mysql/sqlite/mssql) land in Semaine 3.

use qore_sql::generator::SqlDialect;

use crate::built::BuiltQuery;
use crate::compiler::QueryCompiler;
use crate::error::QueryResult;
use crate::query::SelectQuery;

pub struct SqlCompiler {
    pub dialect: SqlDialect,
}

impl SqlCompiler {
    pub fn new(dialect: SqlDialect) -> Self {
        Self { dialect }
    }
}

impl QueryCompiler for SqlCompiler {
    fn compile_select(&self, _q: &SelectQuery) -> QueryResult<BuiltQuery> {
        // Filled in Semaine 1-2 — skeleton only.
        Ok(BuiltQuery {
            sql: String::new(),
            params: Vec::new(),
        })
    }
}

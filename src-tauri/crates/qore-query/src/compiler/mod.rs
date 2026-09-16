// SPDX-License-Identifier: Apache-2.0

//! Query → target-language compilation.
//!
//! The compilation pipeline is split into two layers:
//!
//! - [`DialectOps`] — per-dialect behaviour (quoting, placeholders,
//!   LIMIT style, `ILIKE` support). One zero-sized implementor per dialect.
//! - [`sql::SqlCompiler`] — dialect-neutral traversal of the query AST
//!   that delegates to a `&dyn DialectOps`.

pub mod bigquery;
pub mod duckdb;
pub mod mssql;
pub mod mysql;
pub mod postgres;
pub mod snowflake;
pub mod sql;
pub mod sqlite;

use std::fmt::Write;

use crate::built::BuiltQuery;
use crate::error::QueryResult;
use crate::query::SelectQuery;
use crate::sql_type::SqlType;

/// Maximum nesting depth of the expression tree.
/// Guards against stack overflow on pathological inputs.
pub const MAX_AST_DEPTH: u32 = 1024;

/// Maximum number of bound parameters per query.
///
/// Matches the common driver ceiling: Postgres (65535 via protocol),
/// MySQL (65535 via prepared statement), MSSQL (2100).
pub const MAX_PARAMS: usize = 65_535;

pub trait QueryCompiler {
    fn compile_select(&self, q: &SelectQuery) -> QueryResult<BuiltQuery>;
}

/// Per-dialect behaviour. All methods take `&self` so a single static
/// instance can serve every query compiled for its dialect.
pub(crate) trait DialectOps: Sync + Send {
    /// Write an identifier (table or column name) with the dialect's
    /// quoting rules applied to the given buffer.
    fn quote_ident(&self, out: &mut String, name: &str);

    /// Write the parameter placeholder for the `n`-th bound value
    /// (1-indexed) to `out`.
    fn write_placeholder(&self, out: &mut String, n: usize);

    fn supports_ilike(&self) -> bool {
        false
    }

    fn supports_right_join(&self) -> bool {
        true
    }

    fn supports_full_join(&self) -> bool {
        true
    }

    fn supports_nulls_ordering(&self) -> bool {
        false
    }

    /// How to render LIMIT/OFFSET.
    fn limit_style(&self) -> LimitStyle {
        LimitStyle::LimitOffset
    }

    /// Render a [`SqlType`] target for `CAST(... AS type)`. Default is
    /// an ANSI-leaning mapping; dialects with quirks (MySQL CAST targets,
    /// SQLite type affinities, MSSQL `BIT`/`NVARCHAR(MAX)`, …) override.
    fn write_sql_type(&self, out: &mut String, ty: SqlType) {
        out.push_str(match ty {
            SqlType::Int => "INT",
            SqlType::BigInt => "BIGINT",
            SqlType::Real => "REAL",
            SqlType::Double => "DOUBLE PRECISION",
            SqlType::Text => "TEXT",
            SqlType::Bool => "BOOLEAN",
            SqlType::Date => "DATE",
            SqlType::Timestamp => "TIMESTAMP",
            SqlType::Blob => "BLOB",
        });
    }
}

/// LIMIT/OFFSET syntax variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LimitStyle {
    /// `LIMIT n [OFFSET m]` — Postgres, MySQL, SQLite, DuckDB.
    LimitOffset,
    /// `OFFSET m ROWS [FETCH NEXT n ROWS ONLY]` — MSSQL. Requires ORDER BY.
    OffsetFetch,
}

/// Shared helper: the standard quoting pattern "wrap in pair + double the
/// escape char". Used by Postgres, SQLite, DuckDB (`"..."`), MySQL
/// (`` `...` ``). MSSQL has an asymmetric pair `[...]` and is handled
/// inline in its own impl.
pub(crate) fn write_quoted_symmetric(out: &mut String, name: &str, delim: char) {
    out.push(delim);
    for ch in name.chars() {
        if ch == delim {
            out.push(ch);
        }
        out.push(ch);
    }
    out.push(delim);
}

/// MSSQL's `[name]` quoting: the closing bracket is the only character to
/// escape (by doubling); the opening bracket is left untouched.
pub(crate) fn write_quoted_mssql(out: &mut String, name: &str) {
    out.push('[');
    for ch in name.chars() {
        if ch == ']' {
            out.push(']');
        }
        out.push(ch);
    }
    out.push(']');
}

/// Numeric placeholder helper — writes `{prefix}{n}` without any allocation.
pub(crate) fn write_numeric_placeholder(out: &mut String, prefix: &str, n: usize) {
    out.push_str(prefix);
    // `write!` into a `String` is infallible.
    let _ = write!(out, "{}", n);
}

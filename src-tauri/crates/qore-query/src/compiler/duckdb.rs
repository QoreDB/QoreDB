// SPDX-License-Identifier: Apache-2.0

//! DuckDB dialect operations.
//!
//! DuckDB's SQL surface is largely Postgres-compatible with additional
//! analytical features. For the query builder MVP we only care about the
//! baseline:
//!
//! - Double-quoted identifiers (ANSI), same as Postgres / SQLite.
//! - Positional `?` placeholders (DuckDB also supports `$n`, but `?`
//!   matches the prepared-statement API exposed by the DuckDB Rust crate).
//! - Native `ILIKE`.
//! - Suffix `LIMIT n OFFSET m`.

use super::{write_quoted_symmetric, DialectOps};

pub(crate) struct DuckDbOps;

impl DialectOps for DuckDbOps {
    fn quote_ident(&self, out: &mut String, name: &str) {
        write_quoted_symmetric(out, name, '"');
    }

    fn write_placeholder(&self, out: &mut String, _n: usize) {
        out.push('?');
    }

    fn supports_ilike(&self) -> bool {
        true
    }

    fn supports_nulls_ordering(&self) -> bool {
        true
    }
}

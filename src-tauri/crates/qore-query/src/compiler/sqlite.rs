// SPDX-License-Identifier: Apache-2.0

//! SQLite dialect operations.

use crate::sql_type::SqlType;

use super::{write_quoted_symmetric, DialectOps};

pub(crate) struct SqliteOps;

impl DialectOps for SqliteOps {
    fn quote_ident(&self, out: &mut String, name: &str) {
        write_quoted_symmetric(out, name, '"');
    }

    fn write_placeholder(&self, out: &mut String, _n: usize) {
        out.push('?');
    }

    fn supports_right_join(&self) -> bool {
        // Conservative: SQLite only supports RIGHT/FULL JOIN since 3.39
        // (Feb 2022). Reject them until we gate behind a runtime version
        // check or a compile flag.
        false
    }

    fn supports_full_join(&self) -> bool {
        false
    }

    fn supports_nulls_ordering(&self) -> bool {
        true
    }

    /// SQLite has type *affinities*, not strict types. We map to the
    /// five storage classes the engine actually recognises.
    fn write_sql_type(&self, out: &mut String, ty: SqlType) {
        out.push_str(match ty {
            SqlType::Int | SqlType::BigInt | SqlType::Bool => "INTEGER",
            SqlType::Real | SqlType::Double => "REAL",
            SqlType::Text | SqlType::Date | SqlType::Timestamp => "TEXT",
            SqlType::Blob => "BLOB",
        });
    }
}

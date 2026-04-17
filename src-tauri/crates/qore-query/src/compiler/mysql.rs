// SPDX-License-Identifier: Apache-2.0

//! MySQL / MariaDB dialect operations.

use crate::sql_type::SqlType;

use super::{write_quoted_symmetric, DialectOps};

pub(crate) struct MySqlOps;

impl DialectOps for MySqlOps {
    fn quote_ident(&self, out: &mut String, name: &str) {
        write_quoted_symmetric(out, name, '`');
    }

    fn write_placeholder(&self, out: &mut String, _n: usize) {
        out.push('?');
    }

    fn supports_full_join(&self) -> bool {
        // MySQL has no FULL OUTER JOIN; emulation via UNION of LEFT/RIGHT
        // is not attempted — caller error.
        false
    }

    /// MySQL's `CAST` accepts a restricted set of target types —
    /// notably `SIGNED`/`UNSIGNED` instead of `INT`/`BIGINT`, and
    /// `CHAR` instead of `TEXT`. We map to what actually compiles.
    fn write_sql_type(&self, out: &mut String, ty: SqlType) {
        out.push_str(match ty {
            SqlType::Int | SqlType::BigInt | SqlType::Bool => "SIGNED",
            SqlType::Real => "FLOAT",
            SqlType::Double => "DOUBLE",
            SqlType::Text => "CHAR",
            SqlType::Date => "DATE",
            SqlType::Timestamp => "DATETIME",
            SqlType::Blob => "BINARY",
        });
    }
}

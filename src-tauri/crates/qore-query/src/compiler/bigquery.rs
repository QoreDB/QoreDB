// SPDX-License-Identifier: Apache-2.0

//! BigQuery dialect operations: backtick quoting, named `@p<n>` parameters,
//! no `ILIKE`.

use crate::sql_type::SqlType;

use super::{DialectOps, write_numeric_placeholder, write_quoted_symmetric};

pub(crate) struct BigQueryOps;

impl DialectOps for BigQueryOps {
    fn quote_ident(&self, out: &mut String, name: &str) {
        write_quoted_symmetric(out, name, '`');
    }

    fn write_placeholder(&self, out: &mut String, n: usize) {
        write_numeric_placeholder(out, "@p", n);
    }

    fn supports_nulls_ordering(&self) -> bool {
        true
    }

    fn write_sql_type(&self, out: &mut String, ty: SqlType) {
        out.push_str(match ty {
            SqlType::Int | SqlType::BigInt => "INT64",
            SqlType::Real | SqlType::Double => "FLOAT64",
            SqlType::Text => "STRING",
            SqlType::Bool => "BOOL",
            SqlType::Date => "DATE",
            SqlType::Timestamp => "TIMESTAMP",
            SqlType::Blob => "BYTES",
        });
    }
}

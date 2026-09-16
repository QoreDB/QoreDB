// SPDX-License-Identifier: Apache-2.0

//! Snowflake dialect operations. Unquoted identifiers fold to upper case,
//! the opposite of Postgres, so every identifier is quoted.

use crate::sql_type::SqlType;

use super::{DialectOps, write_quoted_symmetric};

pub(crate) struct SnowflakeOps;

impl DialectOps for SnowflakeOps {
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

    fn write_sql_type(&self, out: &mut String, ty: SqlType) {
        out.push_str(match ty {
            SqlType::Int | SqlType::BigInt => "NUMBER(38,0)",
            SqlType::Real | SqlType::Double => "FLOAT",
            SqlType::Text => "VARCHAR",
            SqlType::Bool => "BOOLEAN",
            SqlType::Date => "DATE",
            SqlType::Timestamp => "TIMESTAMP_NTZ",
            SqlType::Blob => "BINARY",
        });
    }
}

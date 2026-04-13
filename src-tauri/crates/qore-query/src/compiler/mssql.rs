// SPDX-License-Identifier: Apache-2.0

//! Microsoft SQL Server dialect operations.

use super::{write_numeric_placeholder, write_quoted_mssql, DialectOps, LimitStyle};

pub(crate) struct SqlServerOps;

impl DialectOps for SqlServerOps {
    fn quote_ident(&self, out: &mut String, name: &str) {
        write_quoted_mssql(out, name);
    }

    fn write_placeholder(&self, out: &mut String, n: usize) {
        write_numeric_placeholder(out, "@p", n);
    }

    fn limit_style(&self) -> LimitStyle {
        LimitStyle::OffsetFetch
    }
}

// SPDX-License-Identifier: Apache-2.0

//! Postgres dialect operations. CockroachDB users should also pick
//! [`crate::Dialect::Postgres`] — it is wire-compatible.

use super::{write_numeric_placeholder, write_quoted_symmetric, DialectOps};

pub(crate) struct PostgresOps;

impl DialectOps for PostgresOps {
    fn quote_ident(&self, out: &mut String, name: &str) {
        write_quoted_symmetric(out, name, '"');
    }

    fn write_placeholder(&self, out: &mut String, n: usize) {
        write_numeric_placeholder(out, "$", n);
    }

    fn supports_ilike(&self) -> bool {
        true
    }
}

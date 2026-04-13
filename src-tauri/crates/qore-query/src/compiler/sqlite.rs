// SPDX-License-Identifier: Apache-2.0

//! SQLite dialect operations.

use super::{write_quoted_symmetric, DialectOps};

pub(crate) struct SqliteOps;

impl DialectOps for SqliteOps {
    fn quote_ident(&self, out: &mut String, name: &str) {
        write_quoted_symmetric(out, name, '"');
    }

    fn write_placeholder(&self, out: &mut String, _n: usize) {
        out.push('?');
    }
}

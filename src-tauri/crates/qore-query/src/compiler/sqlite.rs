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
}

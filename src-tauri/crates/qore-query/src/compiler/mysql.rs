// SPDX-License-Identifier: Apache-2.0

//! MySQL / MariaDB dialect operations.

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
}

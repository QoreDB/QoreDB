// SPDX-License-Identifier: Apache-2.0

//! SELECT query — builder and AST. Filled in Semaine 1-2.

#[derive(Debug, Default)]
pub struct SelectQuery {
    // populated progressively: from, columns, where_, joins, order_by, limit, offset, group_by
}

impl SelectQuery {
    pub fn new() -> Self {
        Self::default()
    }
}

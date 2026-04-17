// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;

use crate::ident::ColumnRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Asc,
    Desc,
}

/// NULL sorting behaviour for an ORDER BY clause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nulls {
    First,
    Last,
}

#[derive(Debug, Clone)]
pub struct OrderItem {
    pub column: ColumnRef,
    pub order: Order,
    pub nulls: Option<Nulls>,
}

impl OrderItem {
    pub fn new(name: impl Into<Cow<'static, str>>, order: Order) -> Self {
        Self {
            column: ColumnRef {
                name: name.into(),
                table: None,
            },
            order,
            nulls: None,
        }
    }

    pub fn qualified(
        table: impl Into<Cow<'static, str>>,
        name: impl Into<Cow<'static, str>>,
        order: Order,
    ) -> Self {
        Self {
            column: ColumnRef {
                name: name.into(),
                table: Some(table.into()),
            },
            order,
            nulls: None,
        }
    }

    pub fn with_nulls(mut self, nulls: Nulls) -> Self {
        self.nulls = Some(nulls);
        self
    }
}

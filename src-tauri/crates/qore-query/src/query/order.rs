// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;

use crate::ident::ColumnRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Asc,
    Desc,
}

#[derive(Debug, Clone)]
pub struct OrderItem {
    pub column: ColumnRef,
    pub order: Order,
}

impl OrderItem {
    pub fn new(name: impl Into<Cow<'static, str>>, order: Order) -> Self {
        Self {
            column: ColumnRef {
                name: name.into(),
                table: None,
            },
            order,
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
        }
    }
}

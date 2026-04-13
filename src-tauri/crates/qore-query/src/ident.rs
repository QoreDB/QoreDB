// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::marker::PhantomData;

use qore_core::Value;

/// A typed reference to a database column.
///
/// `T` defaults to [`Value`] for untyped usage in the MVP. Phase 3
/// `#[derive(Model)]` macros will generate specialised `Column<i64>`,
/// `Column<String>`, etc., without changing this surface.
#[derive(Debug, Clone)]
pub struct Column<T = Value> {
    pub(crate) name: Cow<'static, str>,
    pub(crate) table: Option<Cow<'static, str>>,
    _marker: PhantomData<T>,
}

impl<T> Column<T> {
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            name: name.into(),
            table: None,
            _marker: PhantomData,
        }
    }

    pub fn qualified(
        table: impl Into<Cow<'static, str>>,
        name: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            name: name.into(),
            table: Some(table.into()),
            _marker: PhantomData,
        }
    }
}

/// Shorthand constructor for an untyped column reference.
pub fn col(name: impl Into<Cow<'static, str>>) -> Column<Value> {
    Column::new(name)
}

// SPDX-License-Identifier: Apache-2.0

//! SELECT query — AST and fluent builder.

use std::borrow::Cow;

use crate::built::BuiltQuery;
use crate::compiler::{sql::SqlCompiler, QueryCompiler};
use crate::dialect::Dialect;
use crate::error::QueryResult;
use crate::expr::Expr;
use crate::ident::ColumnRef;

use super::order::{Order, OrderItem};

/// Projected item in the SELECT list.
#[derive(Debug, Clone)]
pub enum SelectItem {
    /// `SELECT *`
    All,
    /// A column reference — possibly table-qualified.
    Column(ColumnRef),
}

/// SELECT query AST.
#[derive(Debug, Clone, Default)]
pub struct SelectQuery {
    pub(crate) table: Option<Cow<'static, str>>,
    pub(crate) columns: Vec<SelectItem>,
    pub(crate) where_: Option<Expr>,
    pub(crate) order_by: Vec<OrderItem>,
    pub(crate) limit: Option<u64>,
    pub(crate) offset: Option<u64>,
}

impl SelectQuery {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the source table (FROM clause). Overwrites any previous value.
    pub fn from(mut self, table: impl Into<Cow<'static, str>>) -> Self {
        self.table = Some(table.into());
        self
    }

    /// Set the projected columns. Overwrites any previous set.
    pub fn columns<I, C>(mut self, cols: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<Cow<'static, str>>,
    {
        self.columns = cols
            .into_iter()
            .map(|c| {
                SelectItem::Column(ColumnRef {
                    name: c.into(),
                    table: None,
                })
            })
            .collect();
        self
    }

    /// Select all columns (`SELECT *`).
    pub fn all(mut self) -> Self {
        self.columns = vec![SelectItem::All];
        self
    }

    /// Add a WHERE predicate. Subsequent calls combine with AND.
    pub fn filter(mut self, expr: Expr) -> Self {
        self.where_ = Some(match self.where_ {
            Some(prev) => prev.and(expr),
            None => expr,
        });
        self
    }

    /// Add an ORDER BY clause (chainable for multi-column ordering).
    pub fn order_by(mut self, name: impl Into<Cow<'static, str>>, order: Order) -> Self {
        self.order_by.push(OrderItem::new(name, order));
        self
    }

    pub fn limit(mut self, n: u64) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn offset(mut self, n: u64) -> Self {
        self.offset = Some(n);
        self
    }

    /// Compile to a ready-to-execute [`BuiltQuery`] for the given dialect.
    pub fn build(self, dialect: Dialect) -> QueryResult<BuiltQuery> {
        SqlCompiler::new(dialect).compile_select(&self)
    }
}

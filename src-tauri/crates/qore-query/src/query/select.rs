// SPDX-License-Identifier: Apache-2.0

//! SELECT query — AST and fluent builder.

use std::borrow::Cow;

use crate::built::BuiltQuery;
use crate::compiler::{sql::SqlCompiler, QueryCompiler};
use crate::dialect::Dialect;
use crate::error::QueryResult;
use crate::expr::Expr;
use crate::ident::ColumnRef;

use super::join::{Join, JoinKind};
use super::order::{Nulls, Order, OrderItem};

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
    pub(crate) table_alias: Option<Cow<'static, str>>,
    pub(crate) columns: Vec<SelectItem>,
    pub(crate) joins: Vec<Join>,
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
        self.table_alias = None;
        self
    }

    /// Set the source table with an alias — `FROM table AS alias`.
    pub fn from_as(
        mut self,
        table: impl Into<Cow<'static, str>>,
        alias: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.table = Some(table.into());
        self.table_alias = Some(alias.into());
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

    /// Add an ORDER BY clause on a table-qualified column.
    pub fn order_by_qualified(
        mut self,
        table: impl Into<Cow<'static, str>>,
        name: impl Into<Cow<'static, str>>,
        order: Order,
    ) -> Self {
        self.order_by
            .push(OrderItem::qualified(table, name, order));
        self
    }

    /// Add an ORDER BY clause with explicit NULL placement.
    pub fn order_by_nulls(
        mut self,
        name: impl Into<Cow<'static, str>>,
        order: Order,
        nulls: Nulls,
    ) -> Self {
        self.order_by
            .push(OrderItem::new(name, order).with_nulls(nulls));
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

    fn push_join(
        mut self,
        kind: JoinKind,
        table: impl Into<Cow<'static, str>>,
        alias: Option<Cow<'static, str>>,
        on: Expr,
    ) -> Self {
        self.joins.push(Join {
            kind,
            table: table.into(),
            alias,
            on,
        });
        self
    }

    pub fn inner_join(self, table: impl Into<Cow<'static, str>>, on: Expr) -> Self {
        self.push_join(JoinKind::Inner, table, None, on)
    }
    pub fn left_join(self, table: impl Into<Cow<'static, str>>, on: Expr) -> Self {
        self.push_join(JoinKind::Left, table, None, on)
    }
    pub fn right_join(self, table: impl Into<Cow<'static, str>>, on: Expr) -> Self {
        self.push_join(JoinKind::Right, table, None, on)
    }
    pub fn full_join(self, table: impl Into<Cow<'static, str>>, on: Expr) -> Self {
        self.push_join(JoinKind::Full, table, None, on)
    }

    pub fn inner_join_as(
        self,
        table: impl Into<Cow<'static, str>>,
        alias: impl Into<Cow<'static, str>>,
        on: Expr,
    ) -> Self {
        self.push_join(JoinKind::Inner, table, Some(alias.into()), on)
    }
    pub fn left_join_as(
        self,
        table: impl Into<Cow<'static, str>>,
        alias: impl Into<Cow<'static, str>>,
        on: Expr,
    ) -> Self {
        self.push_join(JoinKind::Left, table, Some(alias.into()), on)
    }
    pub fn right_join_as(
        self,
        table: impl Into<Cow<'static, str>>,
        alias: impl Into<Cow<'static, str>>,
        on: Expr,
    ) -> Self {
        self.push_join(JoinKind::Right, table, Some(alias.into()), on)
    }
    pub fn full_join_as(
        self,
        table: impl Into<Cow<'static, str>>,
        alias: impl Into<Cow<'static, str>>,
        on: Expr,
    ) -> Self {
        self.push_join(JoinKind::Full, table, Some(alias.into()), on)
    }

    /// Compile to a ready-to-execute [`BuiltQuery`] for the given dialect.
    pub fn build(self, dialect: Dialect) -> QueryResult<BuiltQuery> {
        SqlCompiler::new(dialect).compile_select(&self)
    }
}

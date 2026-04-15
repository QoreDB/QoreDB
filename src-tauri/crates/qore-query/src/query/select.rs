// SPDX-License-Identifier: Apache-2.0

//! SELECT query — AST and fluent builder.

use std::borrow::Cow;

use crate::built::BuiltQuery;
use crate::compiler::{sql::SqlCompiler, QueryCompiler};
use crate::dialect::Dialect;
use crate::error::QueryResult;
use crate::expr::Expr;
use crate::ident::{ColumnRef, IntoOperand};

use super::join::{Join, JoinKind};
use super::order::{Nulls, Order, OrderItem};

/// Source of a SELECT's FROM clause.
///
/// Carries the (optional for tables, mandatory for subqueries) alias so
/// that the compiler has a single source of truth for how the main
/// relation is introduced.
#[derive(Debug, Clone)]
pub enum FromSource {
    Table {
        table: Cow<'static, str>,
        alias: Option<Cow<'static, str>>,
    },
    Subquery {
        subquery: Box<SelectQuery>,
        alias: Cow<'static, str>,
    },
}

/// Projected item in the SELECT list.
///
/// - [`SelectItem::All`] renders `*`
/// - [`SelectItem::Projection`] renders `expr` or `expr AS alias`
///
/// A bare column reference is a `Projection` whose `expr` is
/// `Expr::Column(..)` with no alias. Keeping a single projection variant
/// simplifies the compiler — every projected item is handled uniformly.
#[derive(Debug, Clone)]
pub enum SelectItem {
    All,
    Projection {
        expr: Box<Expr>,
        alias: Option<Cow<'static, str>>,
    },
}

impl SelectItem {
    fn column(name: impl Into<Cow<'static, str>>) -> Self {
        SelectItem::Projection {
            expr: Box::new(Expr::Column(ColumnRef {
                name: name.into(),
                table: None,
            })),
            alias: None,
        }
    }
}

/// SELECT query AST.
#[derive(Debug, Clone, Default)]
pub struct SelectQuery {
    pub(crate) from: Option<FromSource>,
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

    /// Set the source table (FROM clause). Overwrites any previous source.
    pub fn from(mut self, table: impl Into<Cow<'static, str>>) -> Self {
        self.from = Some(FromSource::Table {
            table: table.into(),
            alias: None,
        });
        self
    }

    /// Set the source table with an alias — `FROM table AS alias`.
    pub fn from_as(
        mut self,
        table: impl Into<Cow<'static, str>>,
        alias: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.from = Some(FromSource::Table {
            table: table.into(),
            alias: Some(alias.into()),
        });
        self
    }

    /// Set the source to a subquery — `FROM (SELECT …) AS alias`.
    ///
    /// Alias is mandatory: every supported dialect requires an alias
    /// on a subquery in the FROM clause.
    pub fn from_subquery(
        mut self,
        subquery: SelectQuery,
        alias: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.from = Some(FromSource::Subquery {
            subquery: Box::new(subquery),
            alias: alias.into(),
        });
        self
    }

    /// Set the projected columns (simple unaliased names). Overwrites
    /// any previous set.
    pub fn columns<I, C>(mut self, cols: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<Cow<'static, str>>,
    {
        self.columns = cols.into_iter().map(SelectItem::column).collect();
        self
    }

    /// Select all columns (`SELECT *`).
    pub fn all(mut self) -> Self {
        self.columns = vec![SelectItem::All];
        self
    }

    /// Append a single column to the projection list.
    pub fn column(mut self, name: impl Into<Cow<'static, str>>) -> Self {
        self.columns.push(SelectItem::column(name));
        self
    }

    /// Append a column with an alias: `name AS alias`.
    pub fn column_as(
        mut self,
        name: impl Into<Cow<'static, str>>,
        alias: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.columns.push(SelectItem::Projection {
            expr: Box::new(Expr::Column(ColumnRef {
                name: name.into(),
                table: None,
            })),
            alias: Some(alias.into()),
        });
        self
    }

    /// Append an arbitrary expression to the projection list (no alias).
    /// Accepts anything convertible via [`IntoOperand`] — columns,
    /// literals, function calls, subqueries, CASTs.
    pub fn select_expr(mut self, expr: impl IntoOperand) -> Self {
        self.columns.push(SelectItem::Projection {
            expr: Box::new(expr.into_operand()),
            alias: None,
        });
        self
    }

    /// Append an arbitrary expression with an alias: `expr AS alias`.
    pub fn select_expr_as(
        mut self,
        expr: impl IntoOperand,
        alias: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.columns.push(SelectItem::Projection {
            expr: Box::new(expr.into_operand()),
            alias: Some(alias.into()),
        });
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

// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::marker::PhantomData;

use qore_core::Value;

use crate::expr::{BinOp, Expr, UnOp};

/// A typed reference to a database column.
///
/// `T` defaults to [`Value`] for untyped usage in the MVP. Phase 3
/// `#[derive(Model)]` macros will generate specialised `Column<i64>`,
/// `Column<String>`, etc., without changing this surface.
#[derive(Debug, Clone)]
pub struct Column<T = Value> {
    pub(crate) name: Cow<'static, str>,
    pub(crate) table: Option<Cow<'static, str>>,
    _marker: PhantomData<fn() -> T>,
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

    fn as_ref_expr(&self) -> Expr {
        Expr::Column(ColumnRef {
            name: self.name.clone(),
            table: self.table.clone(),
        })
    }

    fn binop(self, op: BinOp, rhs: impl Into<Value>) -> Expr {
        Expr::binary(self.as_ref_expr(), op, Expr::Literal(rhs.into()))
    }

    pub fn eq(self, v: impl Into<Value>) -> Expr {
        self.binop(BinOp::Eq, v)
    }
    pub fn ne(self, v: impl Into<Value>) -> Expr {
        self.binop(BinOp::Ne, v)
    }
    pub fn gt(self, v: impl Into<Value>) -> Expr {
        self.binop(BinOp::Gt, v)
    }
    pub fn ge(self, v: impl Into<Value>) -> Expr {
        self.binop(BinOp::Ge, v)
    }
    pub fn lt(self, v: impl Into<Value>) -> Expr {
        self.binop(BinOp::Lt, v)
    }
    pub fn le(self, v: impl Into<Value>) -> Expr {
        self.binop(BinOp::Le, v)
    }
    pub fn like(self, pat: impl Into<Value>) -> Expr {
        self.binop(BinOp::Like, pat)
    }
    pub fn ilike(self, pat: impl Into<Value>) -> Expr {
        self.binop(BinOp::ILike, pat)
    }

    pub fn is_null(self) -> Expr {
        Expr::unary(UnOp::IsNull, self.as_ref_expr())
    }
    pub fn is_not_null(self) -> Expr {
        Expr::unary(UnOp::IsNotNull, self.as_ref_expr())
    }

    pub fn in_<I, V>(self, values: I) -> Expr
    where
        I: IntoIterator<Item = V>,
        V: Into<Value>,
    {
        let values = values
            .into_iter()
            .map(|v| Expr::Literal(v.into()))
            .collect();
        Expr::InList {
            expr: Box::new(self.as_ref_expr()),
            values,
            negated: false,
        }
    }

    pub fn not_in<I, V>(self, values: I) -> Expr
    where
        I: IntoIterator<Item = V>,
        V: Into<Value>,
    {
        let values = values
            .into_iter()
            .map(|v| Expr::Literal(v.into()))
            .collect();
        Expr::InList {
            expr: Box::new(self.as_ref_expr()),
            values,
            negated: true,
        }
    }

    pub fn between(self, low: impl Into<Value>, high: impl Into<Value>) -> Expr {
        Expr::Between {
            expr: Box::new(self.as_ref_expr()),
            low: Box::new(Expr::Literal(low.into())),
            high: Box::new(Expr::Literal(high.into())),
        }
    }
}

/// Untyped column reference — the MVP entry point.
pub fn col(name: impl Into<Cow<'static, str>>) -> Column<Value> {
    Column::new(name)
}

/// Table-qualified untyped column reference.
pub fn tcol(
    table: impl Into<Cow<'static, str>>,
    name: impl Into<Cow<'static, str>>,
) -> Column<Value> {
    Column::qualified(table, name)
}

/// Dialect-neutral reference to a column, used inside [`Expr`].
#[derive(Debug, Clone)]
pub struct ColumnRef {
    pub name: Cow<'static, str>,
    pub table: Option<Cow<'static, str>>,
}

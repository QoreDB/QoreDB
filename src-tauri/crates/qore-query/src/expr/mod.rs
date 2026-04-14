// SPDX-License-Identifier: Apache-2.0

//! Expression tree used inside WHERE / HAVING / ON clauses.
//!
//! The tree is dialect-neutral: it is compiled to SQL by the
//! [`crate::compiler::QueryCompiler`] implementation of the target dialect.

use qore_core::Value;

use crate::ident::ColumnRef;

pub mod ops;

/// Expression AST node.
#[derive(Debug, Clone)]
pub enum Expr {
    Column(ColumnRef),
    Literal(Value),
    Binary {
        lhs: Box<Expr>,
        op: BinOp,
        rhs: Box<Expr>,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    InList {
        expr: Box<Expr>,
        values: Vec<Expr>,
        negated: bool,
    },
    Between {
        expr: Box<Expr>,
        low: Box<Expr>,
        high: Box<Expr>,
    },
    /// Pattern matching — `LIKE` / `ILIKE` with optional `ESCAPE`.
    ///
    /// Carried as a dedicated variant rather than a [`BinOp`] because
    /// (a) the optional `escape` character requires storage, and
    /// (b) `ILIKE` needs a per-dialect fallback to `LOWER(x) LIKE LOWER(y)`
    /// that is cleaner to isolate from generic binary handling.
    Like {
        expr: Box<Expr>,
        pattern: Box<Expr>,
        case_insensitive: bool,
        escape: Option<char>,
    },
}

impl Expr {
    pub(crate) fn binary(lhs: Expr, op: BinOp, rhs: Expr) -> Expr {
        Expr::Binary {
            lhs: Box::new(lhs),
            op,
            rhs: Box::new(rhs),
        }
    }

    pub(crate) fn unary(op: UnOp, expr: Expr) -> Expr {
        Expr::Unary {
            op,
            expr: Box::new(expr),
        }
    }

    /// Logical AND — `self AND rhs`.
    pub fn and(self, rhs: Expr) -> Expr {
        Expr::binary(self, BinOp::And, rhs)
    }

    /// Logical OR — `self OR rhs`.
    pub fn or(self, rhs: Expr) -> Expr {
        Expr::binary(self, BinOp::Or, rhs)
    }

    /// Logical NOT — `NOT self`.
    ///
    /// Inherent method to match the chaining style of [`Expr::and`] and
    /// [`Expr::or`]. We don't implement [`std::ops::Not`] because `!expr`
    /// would break method chains like `col("x").eq(1).not().and(...)`.
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Expr {
        Expr::unary(UnOp::Not, self)
    }

    /// AND-fold an iterator of expressions into a single expression.
    /// Returns `None` if the iterator is empty — callers decide whether
    /// to treat that as "no filter" or an error.
    ///
    /// ```
    /// use qore_query::prelude::*;
    /// let parts = vec![col("a").eq(1i64), col("b").eq(2i64), col("c").eq(3i64)];
    /// let combined = Expr::and_all(parts).unwrap();
    /// // equivalent to col("a").eq(1).and(col("b").eq(2)).and(col("c").eq(3))
    /// # let _ = combined;
    /// ```
    pub fn and_all<I: IntoIterator<Item = Expr>>(items: I) -> Option<Expr> {
        items.into_iter().reduce(Expr::and)
    }

    /// OR-fold an iterator of expressions into a single expression.
    /// Returns `None` if the iterator is empty.
    pub fn or_any<I: IntoIterator<Item = Expr>>(items: I) -> Option<Expr> {
        items.into_iter().reduce(Expr::or)
    }
}

/// Binary operators over two expressions.
///
/// Pattern matching (`LIKE`/`ILIKE`) is intentionally **not** here — it
/// lives in [`Expr::Like`] to carry the optional escape character and
/// the case-insensitive flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// Unary operators over a single expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    IsNull,
    IsNotNull,
}

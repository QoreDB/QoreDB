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
}

/// Binary operators over two expressions.
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
    Like,
    ILike,
}

/// Unary operators over a single expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    IsNull,
    IsNotNull,
}

// SPDX-License-Identifier: Apache-2.0

//! Expression tree used inside WHERE / HAVING / ON clauses.
//!
//! The tree is dialect-neutral: it is compiled to SQL by the
//! [`crate::compiler::QueryCompiler`] implementation of the target dialect.

use qore_core::Value;

use crate::ident::ColumnRef;
use crate::query::SelectQuery;
use crate::sql_type::SqlType;

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
    /// Scalar subquery — can appear anywhere an expression is expected.
    /// Compiled inside parentheses; parameters flow into the outer query.
    Subquery(Box<SelectQuery>),
    /// `expr [NOT] IN (SELECT …)`.
    InSubquery {
        expr: Box<Expr>,
        subquery: Box<SelectQuery>,
        negated: bool,
    },
    /// `[NOT] EXISTS (SELECT …)`.
    Exists {
        subquery: Box<SelectQuery>,
        negated: bool,
    },
    /// `CAST(expr AS type)` — dialect-specific type name rendering.
    Cast {
        expr: Box<Expr>,
        ty: SqlType,
    },
    /// `COALESCE(a, b, c, …)` — returns the first non-null operand.
    /// Compiler rejects lengths `< 2` because single-operand COALESCE is
    /// useless and zero-operand is a SQL error.
    Coalesce(Vec<Expr>),
    /// Aggregate function call over a single expression with optional
    /// `DISTINCT` — `SUM(col)`, `COUNT(DISTINCT col)`, etc.
    Aggregate {
        func: AggFn,
        arg: Box<Expr>,
        distinct: bool,
    },
    /// `COUNT(*)` — carried separately because `*` is not an `Expr`.
    CountStar,
}

/// Aggregate function kinds supported as first-class AST nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggFn {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

impl AggFn {
    pub(crate) fn sql_name(self) -> &'static str {
        match self {
            AggFn::Count => "COUNT",
            AggFn::Sum => "SUM",
            AggFn::Avg => "AVG",
            AggFn::Min => "MIN",
            AggFn::Max => "MAX",
        }
    }
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

    /// `CAST(self AS ty)` — coerce the expression to another SQL type.
    pub fn cast(self, ty: SqlType) -> Expr {
        Expr::Cast {
            expr: Box::new(self),
            ty,
        }
    }

    // Comparison methods on arbitrary expressions (not only `Column`).
    // Needed for predicates on aggregates (e.g. `count_all().gt(5)` for
    // a HAVING clause) and function-call results.

    pub fn eq(self, rhs: impl crate::ident::IntoOperand) -> Expr {
        Expr::binary(self, BinOp::Eq, rhs.into_operand())
    }
    pub fn ne(self, rhs: impl crate::ident::IntoOperand) -> Expr {
        Expr::binary(self, BinOp::Ne, rhs.into_operand())
    }
    pub fn gt(self, rhs: impl crate::ident::IntoOperand) -> Expr {
        Expr::binary(self, BinOp::Gt, rhs.into_operand())
    }
    pub fn ge(self, rhs: impl crate::ident::IntoOperand) -> Expr {
        Expr::binary(self, BinOp::Ge, rhs.into_operand())
    }
    pub fn lt(self, rhs: impl crate::ident::IntoOperand) -> Expr {
        Expr::binary(self, BinOp::Lt, rhs.into_operand())
    }
    pub fn le(self, rhs: impl crate::ident::IntoOperand) -> Expr {
        Expr::binary(self, BinOp::Le, rhs.into_operand())
    }
}

/// `COALESCE(a, b, c, …)` — returns the first non-null operand.
///
/// Accepts any iterable of items convertible via
/// [`crate::ident::IntoOperand`]. For **heterogeneous** argument types
/// (e.g. mixing [`Column`](crate::Column) and literals), use the
/// [`coalesce!`](crate::coalesce!) macro instead — Rust's homogeneous
/// array typing can't unify `Column<T>` with a scalar literal through
/// the generic bound.
///
/// The compiler surfaces a [`crate::error::QueryError::InvalidExpr`] at
/// `.build()` time if fewer than two operands are supplied.
pub fn coalesce<I, O>(items: I) -> Expr
where
    I: IntoIterator<Item = O>,
    O: crate::ident::IntoOperand,
{
    Expr::Coalesce(items.into_iter().map(|o| o.into_operand()).collect())
}

/// Variadic form of [`coalesce`] that accepts heterogeneous argument
/// types. Each argument is converted through
/// [`crate::ident::IntoOperand`] so columns, literals, subqueries and
/// pre-built expressions can be mixed.
///
/// ```
/// use qore_query::prelude::*;
/// let e = qore_query::coalesce![col("a"), col("b"), 0i64];
/// # let _ = e;
/// ```
#[macro_export]
macro_rules! coalesce {
    ($($arg:expr),+ $(,)?) => {
        $crate::expr::Expr::Coalesce(vec![
            $( $crate::ident::IntoOperand::into_operand($arg) ),+
        ])
    };
}

/// `EXISTS (SELECT …)`.
pub fn exists(subquery: SelectQuery) -> Expr {
    Expr::Exists {
        subquery: Box::new(subquery),
        negated: false,
    }
}

/// `NOT EXISTS (SELECT …)`.
pub fn not_exists(subquery: SelectQuery) -> Expr {
    Expr::Exists {
        subquery: Box::new(subquery),
        negated: true,
    }
}

/// `CAST(expr AS ty)` — free-function form of [`Expr::cast`].
pub fn cast(expr: impl crate::ident::IntoOperand, ty: SqlType) -> Expr {
    Expr::Cast {
        expr: Box::new(expr.into_operand()),
        ty,
    }
}

// ============================================================================
// Aggregate function constructors
// ============================================================================

fn agg(func: AggFn, arg: impl crate::ident::IntoOperand, distinct: bool) -> Expr {
    Expr::Aggregate {
        func,
        arg: Box::new(arg.into_operand()),
        distinct,
    }
}

/// `COUNT(expr)` — count non-null values of `expr`.
pub fn count(expr: impl crate::ident::IntoOperand) -> Expr {
    agg(AggFn::Count, expr, false)
}

/// `COUNT(*)` — count rows including those with NULL in every column.
pub fn count_all() -> Expr {
    Expr::CountStar
}

/// `COUNT(DISTINCT expr)`.
pub fn count_distinct(expr: impl crate::ident::IntoOperand) -> Expr {
    agg(AggFn::Count, expr, true)
}

/// `SUM(expr)`.
pub fn sum(expr: impl crate::ident::IntoOperand) -> Expr {
    agg(AggFn::Sum, expr, false)
}

/// `AVG(expr)`.
pub fn avg(expr: impl crate::ident::IntoOperand) -> Expr {
    agg(AggFn::Avg, expr, false)
}

/// `MIN(expr)`.
pub fn min(expr: impl crate::ident::IntoOperand) -> Expr {
    agg(AggFn::Min, expr, false)
}

/// `MAX(expr)`.
pub fn max(expr: impl crate::ident::IntoOperand) -> Expr {
    agg(AggFn::Max, expr, false)
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

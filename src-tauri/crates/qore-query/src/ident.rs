// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::marker::PhantomData;

use qore_core::Value;

use crate::expr::{BinOp, Expr, UnOp};
use crate::query::SelectQuery;
use crate::sql_type::SqlType;

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

    fn col_expr(&self) -> Expr {
        Expr::Column(ColumnRef {
            name: self.name.clone(),
            table: self.table.clone(),
        })
    }

    fn binop(self, op: BinOp, rhs: impl IntoOperand) -> Expr {
        Expr::binary(self.col_expr(), op, rhs.into_operand())
    }

    pub fn eq(self, v: impl IntoOperand) -> Expr {
        self.binop(BinOp::Eq, v)
    }
    pub fn ne(self, v: impl IntoOperand) -> Expr {
        self.binop(BinOp::Ne, v)
    }
    pub fn gt(self, v: impl IntoOperand) -> Expr {
        self.binop(BinOp::Gt, v)
    }
    pub fn ge(self, v: impl IntoOperand) -> Expr {
        self.binop(BinOp::Ge, v)
    }
    pub fn lt(self, v: impl IntoOperand) -> Expr {
        self.binop(BinOp::Lt, v)
    }
    pub fn le(self, v: impl IntoOperand) -> Expr {
        self.binop(BinOp::Le, v)
    }
    /// `col LIKE pattern` — case-sensitive. The pattern is passed
    /// through unescaped; use [`Self::starts_with`]/[`Self::ends_with`]/
    /// [`Self::contains`] when the pattern comes from untrusted input.
    pub fn like(self, pat: impl IntoOperand) -> Expr {
        Expr::Like {
            expr: Box::new(self.col_expr()),
            pattern: Box::new(pat.into_operand()),
            case_insensitive: false,
            escape: None,
        }
    }

    /// `col ILIKE pattern` — case-insensitive. Native on Postgres and
    /// DuckDB; emulated elsewhere via `LOWER(col) LIKE LOWER(pattern)`.
    pub fn ilike(self, pat: impl IntoOperand) -> Expr {
        Expr::Like {
            expr: Box::new(self.col_expr()),
            pattern: Box::new(pat.into_operand()),
            case_insensitive: true,
            escape: None,
        }
    }

    /// Match values starting with `prefix`. Wildcard characters (`%`,
    /// `_`, `\`) in `prefix` are escaped so they match literally; the
    /// emitted SQL is `col LIKE '<escaped-prefix>%' ESCAPE '\'` for
    /// portable wildcard semantics across all dialects.
    pub fn starts_with(self, prefix: impl AsRef<str>) -> Expr {
        let mut pat = String::with_capacity(prefix.as_ref().len() + 1);
        escape_like_pattern(prefix.as_ref(), &mut pat);
        pat.push('%');
        self.like_with_escape(pat, false)
    }

    /// Match values ending with `suffix`. See [`Self::starts_with`].
    pub fn ends_with(self, suffix: impl AsRef<str>) -> Expr {
        let mut pat = String::with_capacity(suffix.as_ref().len() + 1);
        pat.push('%');
        escape_like_pattern(suffix.as_ref(), &mut pat);
        self.like_with_escape(pat, false)
    }

    /// Match values containing `substr` anywhere. See [`Self::starts_with`].
    pub fn contains(self, substr: impl AsRef<str>) -> Expr {
        let mut pat = String::with_capacity(substr.as_ref().len() + 2);
        pat.push('%');
        escape_like_pattern(substr.as_ref(), &mut pat);
        pat.push('%');
        self.like_with_escape(pat, false)
    }

    fn like_with_escape(self, pattern: String, case_insensitive: bool) -> Expr {
        Expr::Like {
            expr: Box::new(self.col_expr()),
            pattern: Box::new(Expr::Literal(Value::Text(pattern))),
            case_insensitive,
            escape: Some(LIKE_ESCAPE_CHAR),
        }
    }

    pub fn is_null(self) -> Expr {
        Expr::unary(UnOp::IsNull, self.col_expr())
    }
    pub fn is_not_null(self) -> Expr {
        Expr::unary(UnOp::IsNotNull, self.col_expr())
    }

    pub fn in_<I, V>(self, values: I) -> Expr
    where
        I: IntoIterator<Item = V>,
        V: IntoOperand,
    {
        let values = values.into_iter().map(|v| v.into_operand()).collect();
        Expr::InList {
            expr: Box::new(self.col_expr()),
            values,
            negated: false,
        }
    }

    pub fn not_in<I, V>(self, values: I) -> Expr
    where
        I: IntoIterator<Item = V>,
        V: IntoOperand,
    {
        let values = values.into_iter().map(|v| v.into_operand()).collect();
        Expr::InList {
            expr: Box::new(self.col_expr()),
            values,
            negated: true,
        }
    }

    pub fn between(self, low: impl IntoOperand, high: impl IntoOperand) -> Expr {
        Expr::Between {
            expr: Box::new(self.col_expr()),
            low: Box::new(low.into_operand()),
            high: Box::new(high.into_operand()),
        }
    }

    /// `col IN (SELECT …)` — subquery membership test.
    pub fn in_sub(self, subquery: SelectQuery) -> Expr {
        Expr::InSubquery {
            expr: Box::new(self.col_expr()),
            subquery: Box::new(subquery),
            negated: false,
        }
    }

    /// `col NOT IN (SELECT …)`.
    pub fn not_in_sub(self, subquery: SelectQuery) -> Expr {
        Expr::InSubquery {
            expr: Box::new(self.col_expr()),
            subquery: Box::new(subquery),
            negated: true,
        }
    }

    /// `CAST(col AS ty)` — shorthand for [`Expr::cast`] on a column.
    pub fn cast(self, ty: SqlType) -> Expr {
        Expr::Cast {
            expr: Box::new(self.col_expr()),
            ty,
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

/// Escape character used by [`Column::starts_with`] and friends to
/// neutralise `%` / `_` in user-supplied search terms. Matches the
/// default on Postgres/MySQL/DuckDB; on SQLite/MSSQL the compiler
/// emits an explicit `ESCAPE '\'` clause.
pub(crate) const LIKE_ESCAPE_CHAR: char = '\\';

/// Append `s` to `out`, doubling any `%`, `_`, or `\\` so they are
/// matched literally when the pattern is used with
/// `LIKE ... ESCAPE '\\'`.
pub(crate) fn escape_like_pattern(s: &str, out: &mut String) {
    for ch in s.chars() {
        if ch == '%' || ch == '_' || ch == LIKE_ESCAPE_CHAR {
            out.push(LIKE_ESCAPE_CHAR);
        }
        out.push(ch);
    }
}

// ============================================================================
// IntoOperand — accepts literals, column refs, or raw expressions
// ============================================================================

/// Anything that can serve as the right-hand side of a comparison:
/// a literal (any scalar type), another [`Column`], or a pre-built
/// [`Expr`]. Enables `col("u").id().eq(col("o").user_id())` for JOIN
/// ON clauses alongside the usual `col("age").gt(18)`.
///
/// A blanket `impl<V: Into<Value>> IntoOperand for V` would conflict
/// with the [`Column`] / [`Expr`] impls on coherence grounds, so we
/// list the literal types explicitly via a macro.
pub trait IntoOperand {
    fn into_operand(self) -> Expr;
}

impl IntoOperand for Expr {
    fn into_operand(self) -> Expr {
        self
    }
}

impl<T> IntoOperand for Column<T> {
    fn into_operand(self) -> Expr {
        Expr::Column(ColumnRef {
            name: self.name,
            table: self.table,
        })
    }
}

impl IntoOperand for SelectQuery {
    /// A `SelectQuery` used as an operand is a scalar subquery —
    /// `(SELECT …)`. The SQL context must accept a single row / single
    /// column result; that is the caller's responsibility.
    fn into_operand(self) -> Expr {
        Expr::Subquery(Box::new(self))
    }
}

impl IntoOperand for Value {
    fn into_operand(self) -> Expr {
        Expr::Literal(self)
    }
}

impl<T> IntoOperand for Option<T>
where
    T: Into<Value>,
{
    fn into_operand(self) -> Expr {
        Expr::Literal(self.map(Into::into).unwrap_or(Value::Null))
    }
}

impl IntoOperand for &str {
    fn into_operand(self) -> Expr {
        Expr::Literal(Value::Text(self.to_string()))
    }
}

impl IntoOperand for String {
    fn into_operand(self) -> Expr {
        Expr::Literal(Value::Text(self))
    }
}

impl IntoOperand for &String {
    fn into_operand(self) -> Expr {
        Expr::Literal(Value::Text(self.clone()))
    }
}

macro_rules! impl_operand_via_into_value {
    ($($t:ty),* $(,)?) => {
        $(
            impl IntoOperand for $t {
                fn into_operand(self) -> Expr { Expr::Literal(Value::from(self)) }
            }
            impl IntoOperand for &$t {
                fn into_operand(self) -> Expr { Expr::Literal(Value::from(self)) }
            }
        )*
    };
}
impl_operand_via_into_value!(bool, i8, i16, i32, i64, u8, u16, u32, f32, f64);

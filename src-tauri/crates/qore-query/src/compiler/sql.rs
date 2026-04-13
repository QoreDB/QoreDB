// SPDX-License-Identifier: Apache-2.0

//! Generic SQL compiler — shared across Postgres / MySQL / SQLite / MSSQL.
//!
//! Dialect-specific overrides (placeholders, `ILIKE` fallbacks, MSSQL
//! `OFFSET..FETCH`) are selected inline via [`SqlDialect`] rather than
//! through trait dispatch: the surface area is small enough that inline
//! branching stays readable and keeps the compiler monomorphic.
//!
//! When the number of divergences grows (RETURNING, array types, JSON ops,
//! ON CONFLICT variants), this is planned to refactor to a `DialectOps`
//! trait — see `doc/QoreQuery_Builder_Plan.md` §4.3.

use std::fmt::Write as _;

use qore_core::Value;
use qore_sql::generator::SqlDialect;

use crate::built::BuiltQuery;
use crate::compiler::QueryCompiler;
use crate::error::{QueryError, QueryResult};
use crate::expr::{BinOp, Expr, UnOp};
use crate::ident::ColumnRef;
use crate::query::select::{SelectItem, SelectQuery};
use crate::query::OrderItem;

pub struct SqlCompiler {
    pub dialect: SqlDialect,
}

impl SqlCompiler {
    pub fn new(dialect: SqlDialect) -> Self {
        Self { dialect }
    }
}

impl QueryCompiler for SqlCompiler {
    fn compile_select(&self, q: &SelectQuery) -> QueryResult<BuiltQuery> {
        let table = q.table.as_ref().ok_or(QueryError::MissingFrom)?;

        let mut ctx = Ctx::new(self.dialect);
        let mut sql = String::with_capacity(128);

        sql.push_str("SELECT ");
        write_select_list(&mut sql, &ctx, &q.columns)?;

        write!(&mut sql, " FROM {}", ctx.dialect.quote_ident(table))
            .expect("writing to String never fails");

        if let Some(where_) = &q.where_ {
            sql.push_str(" WHERE ");
            ctx.write_expr(&mut sql, where_)?;
        }

        if !q.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            write_order_list(&mut sql, &ctx, &q.order_by);
        }

        write_limit_offset(
            &mut sql,
            ctx.dialect,
            q.limit,
            q.offset,
            !q.order_by.is_empty(),
        )?;

        Ok(BuiltQuery {
            sql,
            params: ctx.into_params(),
        })
    }
}

/// Compilation context — owns the parameter buffer and placeholder numbering.
struct Ctx {
    dialect: SqlDialect,
    params: Vec<Value>,
}

impl Ctx {
    fn new(dialect: SqlDialect) -> Self {
        Self {
            dialect,
            params: Vec::new(),
        }
    }

    fn into_params(self) -> Vec<Value> {
        self.params
    }

    /// Validate a [`Value`] before it becomes a bound parameter.
    ///
    /// `NaN` and `±Infinity` have no portable SQL representation: MySQL
    /// rejects them outright, Postgres accepts them only in specific
    /// contexts, and comparison with `NaN` yields undefined semantics on
    /// every SGBD. We refuse them up-front rather than passing garbage to
    /// the driver.
    fn validate_literal(v: &Value) -> QueryResult<()> {
        if let Value::Float(f) = v {
            if !f.is_finite() {
                return Err(QueryError::InvalidLiteral(
                    "non-finite float (NaN or Infinity)",
                ));
            }
        }
        Ok(())
    }

    fn push_placeholder(&mut self, out: &mut String, v: Value) -> QueryResult<()> {
        Self::validate_literal(&v)?;
        self.params.push(v);
        let n = self.params.len();
        match self.dialect {
            SqlDialect::Postgres => write!(out, "${}", n).expect("infallible"),
            SqlDialect::MySql | SqlDialect::Sqlite => out.push('?'),
            SqlDialect::SqlServer => write!(out, "@p{}", n).expect("infallible"),
        }
        Ok(())
    }

    fn write_col_ref(&self, out: &mut String, c: &ColumnRef) {
        if let Some(t) = &c.table {
            out.push_str(&self.dialect.quote_ident(t));
            out.push('.');
        }
        out.push_str(&self.dialect.quote_ident(&c.name));
    }

    fn write_expr(&mut self, out: &mut String, expr: &Expr) -> QueryResult<()> {
        match expr {
            Expr::Column(c) => {
                self.write_col_ref(out, c);
                Ok(())
            }
            Expr::Literal(v) => self.push_placeholder(out, v.clone()),
            Expr::Binary { lhs, op, rhs } => self.write_binary(out, lhs, *op, rhs),
            Expr::Unary { op, expr } => self.write_unary(out, *op, expr),
            Expr::InList {
                expr,
                values,
                negated,
            } => self.write_in_list(out, expr, values, *negated),
            Expr::Between { expr, low, high } => self.write_between(out, expr, low, high),
        }
    }

    fn write_binary(
        &mut self,
        out: &mut String,
        lhs: &Expr,
        op: BinOp,
        rhs: &Expr,
    ) -> QueryResult<()> {
        // ILIKE is native on Postgres; on MySQL/SQLite/MSSQL we fall back to
        // `LOWER(lhs) LIKE LOWER(rhs)`. MSSQL is case-insensitive by default
        // on most collations — the fallback still gives deterministic semantics.
        let needs_ilike_fallback =
            op == BinOp::ILike && !matches!(self.dialect, SqlDialect::Postgres);

        out.push('(');
        if needs_ilike_fallback {
            out.push_str("LOWER(");
            self.write_expr(out, lhs)?;
            out.push_str(") LIKE LOWER(");
            self.write_expr(out, rhs)?;
            out.push(')');
        } else {
            self.write_expr(out, lhs)?;
            out.push(' ');
            out.push_str(binop_sql(op));
            out.push(' ');
            self.write_expr(out, rhs)?;
        }
        out.push(')');
        Ok(())
    }

    fn write_unary(&mut self, out: &mut String, op: UnOp, expr: &Expr) -> QueryResult<()> {
        match op {
            UnOp::Not => {
                out.push_str("NOT (");
                self.write_expr(out, expr)?;
                out.push(')');
            }
            UnOp::IsNull => {
                out.push('(');
                self.write_expr(out, expr)?;
                out.push_str(" IS NULL)");
            }
            UnOp::IsNotNull => {
                out.push('(');
                self.write_expr(out, expr)?;
                out.push_str(" IS NOT NULL)");
            }
        }
        Ok(())
    }

    fn write_in_list(
        &mut self,
        out: &mut String,
        expr: &Expr,
        values: &[Expr],
        negated: bool,
    ) -> QueryResult<()> {
        if values.is_empty() {
            // An empty IN list is portable as an always-false predicate.
            // `x IN ()` is a syntax error on every dialect we target.
            out.push_str(if negated { "(1 = 1)" } else { "(1 = 0)" });
            return Ok(());
        }
        out.push('(');
        self.write_expr(out, expr)?;
        out.push_str(if negated { " NOT IN (" } else { " IN (" });
        for (i, v) in values.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            self.write_expr(out, v)?;
        }
        out.push_str("))");
        Ok(())
    }

    fn write_between(
        &mut self,
        out: &mut String,
        expr: &Expr,
        low: &Expr,
        high: &Expr,
    ) -> QueryResult<()> {
        out.push('(');
        self.write_expr(out, expr)?;
        out.push_str(" BETWEEN ");
        self.write_expr(out, low)?;
        out.push_str(" AND ");
        self.write_expr(out, high)?;
        out.push(')');
        Ok(())
    }
}

fn binop_sql(op: BinOp) -> &'static str {
    match op {
        BinOp::Eq => "=",
        BinOp::Ne => "<>",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "AND",
        BinOp::Or => "OR",
        BinOp::Like => "LIKE",
        BinOp::ILike => "ILIKE",
    }
}

fn write_select_list(out: &mut String, ctx: &Ctx, items: &[SelectItem]) -> QueryResult<()> {
    if items.is_empty() {
        return Err(QueryError::EmptyProjection);
    }
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        match item {
            SelectItem::All => out.push('*'),
            SelectItem::Column(c) => ctx.write_col_ref(out, c),
        }
    }
    Ok(())
}

fn write_order_list(out: &mut String, ctx: &Ctx, items: &[OrderItem]) {
    use crate::query::order::Order;
    for (i, o) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        ctx.write_col_ref(out, &o.column);
        out.push_str(match o.order {
            Order::Asc => " ASC",
            Order::Desc => " DESC",
        });
    }
}

fn write_limit_offset(
    out: &mut String,
    dialect: SqlDialect,
    limit: Option<u64>,
    offset: Option<u64>,
    has_order_by: bool,
) -> QueryResult<()> {
    match dialect {
        SqlDialect::Postgres | SqlDialect::MySql | SqlDialect::Sqlite => {
            if let Some(n) = limit {
                write!(out, " LIMIT {}", n).expect("infallible");
            }
            if let Some(n) = offset {
                write!(out, " OFFSET {}", n).expect("infallible");
            }
        }
        SqlDialect::SqlServer => {
            if limit.is_none() && offset.is_none() {
                return Ok(());
            }
            if !has_order_by {
                return Err(QueryError::MssqlOffsetRequiresOrderBy);
            }
            let off = offset.unwrap_or(0);
            write!(out, " OFFSET {} ROWS", off).expect("infallible");
            if let Some(n) = limit {
                write!(out, " FETCH NEXT {} ROWS ONLY", n).expect("infallible");
            }
        }
    }
    Ok(())
}

// SPDX-License-Identifier: Apache-2.0

//! Generic SQL compiler — shared across all SQL dialects.
//!
//! All per-dialect behaviour (quoting, placeholders, LIMIT style,
//! `ILIKE` support, JOIN/NULLS capabilities) is delegated to a
//! [`DialectOps`] implementation resolved from the [`crate::Dialect`]
//! passed at compile time.

use qore_core::Value;

use crate::built::BuiltQuery;
use crate::compiler::{DialectOps, LimitStyle, QueryCompiler, MAX_AST_DEPTH, MAX_PARAMS};
use crate::dialect::Dialect;
use crate::error::{QueryError, QueryResult};
use crate::expr::{BinOp, Expr, UnOp};
use crate::ident::ColumnRef;
use crate::query::join::{Join, JoinKind};
use crate::query::order::{Nulls, Order, OrderItem};
use crate::query::select::{SelectItem, SelectQuery};

pub struct SqlCompiler {
    dialect: Dialect,
}

impl SqlCompiler {
    pub fn new(dialect: Dialect) -> Self {
        Self { dialect }
    }
}

impl QueryCompiler for SqlCompiler {
    fn compile_select(&self, q: &SelectQuery) -> QueryResult<BuiltQuery> {
        let table = q.table.as_ref().ok_or(QueryError::MissingFrom)?;

        let mut ctx = Ctx::new(self.dialect.ops());
        let mut sql = String::with_capacity(128);

        sql.push_str("SELECT ");
        write_select_list(&mut sql, &ctx, &q.columns)?;

        sql.push_str(" FROM ");
        ctx.ops.quote_ident(&mut sql, table);
        if let Some(alias) = &q.table_alias {
            sql.push_str(" AS ");
            ctx.ops.quote_ident(&mut sql, alias);
        }

        for join in &q.joins {
            ctx.write_join(&mut sql, join)?;
        }

        if let Some(where_) = &q.where_ {
            sql.push_str(" WHERE ");
            ctx.write_expr(&mut sql, where_, 0)?;
        }

        if !q.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            write_order_list(&mut sql, &ctx, &q.order_by);
        }

        write_limit_offset(&mut sql, ctx.ops, q.limit, q.offset, !q.order_by.is_empty())?;

        Ok(BuiltQuery {
            sql,
            params: ctx.into_params(),
        })
    }
}

/// Compilation context — owns the parameter buffer and a dialect handle.
struct Ctx {
    ops: &'static dyn DialectOps,
    params: Vec<Value>,
}

impl Ctx {
    fn new(ops: &'static dyn DialectOps) -> Self {
        Self {
            ops,
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
        if self.params.len() >= MAX_PARAMS {
            return Err(QueryError::TooManyParameters(MAX_PARAMS));
        }
        self.params.push(v);
        self.ops.write_placeholder(out, self.params.len());
        Ok(())
    }

    fn write_col_ref(&self, out: &mut String, c: &ColumnRef) {
        if let Some(t) = &c.table {
            self.ops.quote_ident(out, t);
            out.push('.');
        }
        self.ops.quote_ident(out, &c.name);
    }

    fn write_expr(&mut self, out: &mut String, expr: &Expr, depth: u32) -> QueryResult<()> {
        if depth >= MAX_AST_DEPTH {
            return Err(QueryError::AstTooDeep(MAX_AST_DEPTH));
        }
        match expr {
            Expr::Column(c) => {
                self.write_col_ref(out, c);
                Ok(())
            }
            Expr::Literal(v) => self.push_placeholder(out, v.clone()),
            Expr::Binary { lhs, op, rhs } => self.write_binary(out, lhs, *op, rhs, depth + 1),
            Expr::Unary { op, expr } => self.write_unary(out, *op, expr, depth + 1),
            Expr::InList {
                expr,
                values,
                negated,
            } => self.write_in_list(out, expr, values, *negated, depth + 1),
            Expr::Between { expr, low, high } => {
                self.write_between(out, expr, low, high, depth + 1)
            }
            Expr::Like {
                expr,
                pattern,
                case_insensitive,
                escape,
            } => self.write_like(out, expr, pattern, *case_insensitive, *escape, depth + 1),
        }
    }

    fn write_join(&mut self, out: &mut String, join: &Join) -> QueryResult<()> {
        match join.kind {
            JoinKind::Right if !self.ops.supports_right_join() => {
                return Err(QueryError::Unsupported(
                    "RIGHT JOIN on this dialect (use LEFT JOIN with swapped tables)",
                ));
            }
            JoinKind::Full if !self.ops.supports_full_join() => {
                return Err(QueryError::Unsupported(
                    "FULL JOIN on this dialect (emulate via UNION of LEFT and RIGHT)",
                ));
            }
            _ => {}
        }
        out.push(' ');
        out.push_str(join.kind.sql_keyword());
        out.push(' ');
        self.ops.quote_ident(out, &join.table);
        if let Some(alias) = &join.alias {
            out.push_str(" AS ");
            self.ops.quote_ident(out, alias);
        }
        out.push_str(" ON ");
        self.write_expr(out, &join.on, 0)?;
        Ok(())
    }

    fn write_binary(
        &mut self,
        out: &mut String,
        lhs: &Expr,
        op: BinOp,
        rhs: &Expr,
        depth: u32,
    ) -> QueryResult<()> {
        out.push('(');
        self.write_expr(out, lhs, depth)?;
        out.push(' ');
        out.push_str(binop_sql(op));
        out.push(' ');
        self.write_expr(out, rhs, depth)?;
        out.push(')');
        Ok(())
    }

    fn write_like(
        &mut self,
        out: &mut String,
        expr: &Expr,
        pattern: &Expr,
        case_insensitive: bool,
        escape: Option<char>,
        depth: u32,
    ) -> QueryResult<()> {
        // ILIKE: emit natively when the dialect supports it, otherwise fall
        // back to `LOWER(expr) LIKE LOWER(pattern)`. The fallback loses any
        // functional index on the column but is semantically equivalent for
        // ASCII-dominant text — that tradeoff is documented in the plan.
        let fallback = case_insensitive && !self.ops.supports_ilike();

        out.push('(');
        if fallback {
            out.push_str("LOWER(");
            self.write_expr(out, expr, depth)?;
            out.push_str(") LIKE LOWER(");
            self.write_expr(out, pattern, depth)?;
            out.push(')');
        } else {
            self.write_expr(out, expr, depth)?;
            out.push(' ');
            out.push_str(if case_insensitive { "ILIKE" } else { "LIKE" });
            out.push(' ');
            self.write_expr(out, pattern, depth)?;
        }
        if let Some(ch) = escape {
            // Render as SQL literal: ESCAPE 'X' with ' doubled inside the pair.
            out.push_str(" ESCAPE '");
            if ch == '\'' {
                out.push('\'');
            }
            out.push(ch);
            out.push('\'');
        }
        out.push(')');
        Ok(())
    }

    fn write_unary(
        &mut self,
        out: &mut String,
        op: UnOp,
        expr: &Expr,
        depth: u32,
    ) -> QueryResult<()> {
        match op {
            UnOp::Not => {
                out.push_str("NOT (");
                self.write_expr(out, expr, depth)?;
                out.push(')');
            }
            UnOp::IsNull => {
                out.push('(');
                self.write_expr(out, expr, depth)?;
                out.push_str(" IS NULL)");
            }
            UnOp::IsNotNull => {
                out.push('(');
                self.write_expr(out, expr, depth)?;
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
        depth: u32,
    ) -> QueryResult<()> {
        if values.is_empty() {
            // `x IN ()` is a syntax error on every dialect we target.
            // Emit a portable always-false / always-true instead.
            out.push_str(if negated { "(1 = 1)" } else { "(1 = 0)" });
            return Ok(());
        }
        out.push('(');
        self.write_expr(out, expr, depth)?;
        out.push_str(if negated { " NOT IN (" } else { " IN (" });
        for (i, v) in values.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            self.write_expr(out, v, depth)?;
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
        depth: u32,
    ) -> QueryResult<()> {
        out.push('(');
        self.write_expr(out, expr, depth)?;
        out.push_str(" BETWEEN ");
        self.write_expr(out, low, depth)?;
        out.push_str(" AND ");
        self.write_expr(out, high, depth)?;
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
    for (i, o) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_order_item(out, ctx, o);
    }
}

fn write_order_item(out: &mut String, ctx: &Ctx, o: &OrderItem) {
    match (o.nulls, ctx.ops.supports_nulls_ordering()) {
        (Some(nulls), false) => {
            // Portable emulation: CASE WHEN col IS NULL THEN N ELSE M END
            // used as a leading sort key. Non-NULLs group as 0, NULLs as 1
            // for NULLS LAST; inverted for NULLS FIRST. This is stable on
            // MySQL and MSSQL, the two dialects that lack the native
            // syntax in our target set.
            let (null_key, nonnull_key) = match nulls {
                Nulls::First => (0, 1),
                Nulls::Last => (1, 0),
            };
            out.push_str("CASE WHEN ");
            ctx.write_col_ref(out, &o.column);
            out.push_str(" IS NULL THEN ");
            // These are integer literals we control — safe to inline.
            push_u8(out, null_key);
            out.push_str(" ELSE ");
            push_u8(out, nonnull_key);
            out.push_str(" END, ");
            ctx.write_col_ref(out, &o.column);
            out.push_str(order_keyword(o.order));
        }
        _ => {
            ctx.write_col_ref(out, &o.column);
            out.push_str(order_keyword(o.order));
            if let Some(nulls) = o.nulls {
                out.push_str(match nulls {
                    Nulls::First => " NULLS FIRST",
                    Nulls::Last => " NULLS LAST",
                });
            }
        }
    }
}

fn order_keyword(o: Order) -> &'static str {
    match o {
        Order::Asc => " ASC",
        Order::Desc => " DESC",
    }
}

fn push_u8(out: &mut String, n: u8) {
    // Avoid `write!` formatting overhead for single-digit integers we
    // generate ourselves.
    out.push(char::from(b'0' + n));
}

fn write_limit_offset(
    out: &mut String,
    ops: &'static dyn DialectOps,
    limit: Option<u64>,
    offset: Option<u64>,
    has_order_by: bool,
) -> QueryResult<()> {
    use std::fmt::Write as _;
    match ops.limit_style() {
        LimitStyle::LimitOffset => {
            if let Some(n) = limit {
                let _ = write!(out, " LIMIT {}", n);
            }
            if let Some(n) = offset {
                let _ = write!(out, " OFFSET {}", n);
            }
        }
        LimitStyle::OffsetFetch => {
            if limit.is_none() && offset.is_none() {
                return Ok(());
            }
            if !has_order_by {
                return Err(QueryError::MssqlOffsetRequiresOrderBy);
            }
            let off = offset.unwrap_or(0);
            let _ = write!(out, " OFFSET {} ROWS", off);
            if let Some(n) = limit {
                let _ = write!(out, " FETCH NEXT {} ROWS ONLY", n);
            }
        }
    }
    Ok(())
}

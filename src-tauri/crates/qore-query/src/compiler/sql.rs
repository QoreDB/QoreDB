// SPDX-License-Identifier: Apache-2.0

//! Generic SQL compiler — shared across all SQL dialects.
//!
//! All per-dialect behaviour (quoting, placeholders, LIMIT style,
//! `ILIKE` support, JOIN/NULLS capabilities, CAST target names) is
//! delegated to a [`DialectOps`] implementation resolved from the
//! [`crate::Dialect`] passed at compile time.

use qore_core::Value;

use crate::built::BuiltQuery;
use crate::compiler::{DialectOps, LimitStyle, QueryCompiler, MAX_AST_DEPTH, MAX_PARAMS};
use crate::dialect::Dialect;
use crate::error::{QueryError, QueryResult};
use crate::expr::{AggFn, BinOp, Expr, UnOp};
use crate::ident::ColumnRef;
use crate::query::join::{Join, JoinKind};
use crate::query::order::{Nulls, Order, OrderItem};
use crate::query::select::{FromSource, SelectItem, SelectQuery};

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
        let mut ctx = Ctx::new(self.dialect.ops());
        let mut sql = String::with_capacity(128);
        ctx.compile_select_into(q, &mut sql, 0)?;
        Ok(BuiltQuery {
            sql,
            params: ctx.into_params(),
        })
    }
}

/// Compilation context — owns the parameter buffer and a dialect handle.
///
/// The same `Ctx` is threaded through nested subqueries so that bound
/// parameters are numbered contiguously across the whole compiled SQL.
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

    /// Compile a SELECT into `out`, reusing this context's param buffer.
    ///
    /// `depth` guards against pathological nesting of expressions /
    /// subqueries. Each recursion increases it by one.
    fn compile_select_into(
        &mut self,
        q: &SelectQuery,
        out: &mut String,
        depth: u32,
    ) -> QueryResult<()> {
        if depth >= MAX_AST_DEPTH {
            return Err(QueryError::AstTooDeep(MAX_AST_DEPTH));
        }

        let from = q.from.as_ref().ok_or(QueryError::MissingFrom)?;

        out.push_str("SELECT ");
        self.write_select_list(out, &q.columns, depth)?;

        out.push_str(" FROM ");
        match from {
            FromSource::Table { table, alias } => {
                self.ops.quote_ident(out, table);
                if let Some(a) = alias {
                    out.push_str(" AS ");
                    self.ops.quote_ident(out, a);
                }
            }
            FromSource::Subquery { subquery, alias } => {
                out.push('(');
                self.compile_select_into(subquery, out, depth + 1)?;
                out.push_str(") AS ");
                self.ops.quote_ident(out, alias);
            }
        }

        for join in &q.joins {
            self.write_join(out, join, depth)?;
        }

        if let Some(where_) = &q.where_ {
            out.push_str(" WHERE ");
            self.write_expr(out, where_, depth)?;
        }

        if !q.group_by.is_empty() {
            out.push_str(" GROUP BY ");
            for (i, item) in q.group_by.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                self.write_expr(out, item, depth)?;
            }
        }

        if let Some(having) = &q.having {
            if q.group_by.is_empty() {
                return Err(QueryError::InvalidExpr(
                    "HAVING without GROUP BY is only valid when the SELECT list is all aggregates",
                ));
            }
            out.push_str(" HAVING ");
            self.write_expr(out, having, depth)?;
        }

        if !q.order_by.is_empty() {
            out.push_str(" ORDER BY ");
            self.write_order_list(out, &q.order_by);
        }

        self.write_limit_offset(out, q.limit, q.offset, !q.order_by.is_empty())?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Literals & placeholders
    // ------------------------------------------------------------------

    /// `NaN`/`±Infinity` have no portable SQL representation: MySQL
    /// rejects them, Postgres accepts them only in specific contexts,
    /// and comparisons with `NaN` yield undefined semantics. We refuse
    /// them up-front rather than passing garbage to the driver.
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

    // ------------------------------------------------------------------
    // Expression walker
    // ------------------------------------------------------------------

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
            Expr::Subquery(sq) => {
                out.push('(');
                self.compile_select_into(sq, out, depth + 1)?;
                out.push(')');
                Ok(())
            }
            Expr::InSubquery {
                expr,
                subquery,
                negated,
            } => self.write_in_subquery(out, expr, subquery, *negated, depth + 1),
            Expr::Exists { subquery, negated } => {
                self.write_exists(out, subquery, *negated, depth + 1)
            }
            Expr::Cast { expr, ty } => {
                out.push_str("CAST(");
                self.write_expr(out, expr, depth + 1)?;
                out.push_str(" AS ");
                self.ops.write_sql_type(out, *ty);
                out.push(')');
                Ok(())
            }
            Expr::Coalesce(items) => {
                if items.len() < 2 {
                    return Err(QueryError::InvalidExpr(
                        "COALESCE requires at least 2 arguments",
                    ));
                }
                out.push_str("COALESCE(");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    self.write_expr(out, item, depth + 1)?;
                }
                out.push(')');
                Ok(())
            }
            Expr::Aggregate {
                func,
                arg,
                distinct,
            } => self.write_aggregate(out, *func, arg, *distinct, depth + 1),
            Expr::CountStar => {
                out.push_str("COUNT(*)");
                Ok(())
            }
        }
    }

    fn write_aggregate(
        &mut self,
        out: &mut String,
        func: AggFn,
        arg: &Expr,
        distinct: bool,
        depth: u32,
    ) -> QueryResult<()> {
        // DISTINCT is only meaningful on COUNT/SUM/AVG for our targets.
        // Most dialects accept it on MIN/MAX too (no-op) but reject it
        // on some analytical functions we don't yet expose — so we
        // allow it uniformly here.
        out.push_str(func.sql_name());
        out.push('(');
        if distinct {
            out.push_str("DISTINCT ");
        }
        self.write_expr(out, arg, depth)?;
        out.push(')');
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

    fn write_in_subquery(
        &mut self,
        out: &mut String,
        expr: &Expr,
        subquery: &SelectQuery,
        negated: bool,
        depth: u32,
    ) -> QueryResult<()> {
        out.push('(');
        self.write_expr(out, expr, depth)?;
        out.push_str(if negated { " NOT IN (" } else { " IN (" });
        self.compile_select_into(subquery, out, depth)?;
        out.push_str("))");
        Ok(())
    }

    fn write_exists(
        &mut self,
        out: &mut String,
        subquery: &SelectQuery,
        negated: bool,
        depth: u32,
    ) -> QueryResult<()> {
        out.push('(');
        if negated {
            out.push_str("NOT ");
        }
        out.push_str("EXISTS (");
        self.compile_select_into(subquery, out, depth)?;
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

    // ------------------------------------------------------------------
    // SELECT list, JOINs, ORDER BY, LIMIT/OFFSET
    // ------------------------------------------------------------------

    fn write_select_list(
        &mut self,
        out: &mut String,
        items: &[SelectItem],
        depth: u32,
    ) -> QueryResult<()> {
        if items.is_empty() {
            return Err(QueryError::EmptyProjection);
        }
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            match item {
                SelectItem::All => out.push('*'),
                SelectItem::Projection { expr, alias } => {
                    self.write_expr(out, expr, depth + 1)?;
                    if let Some(a) = alias {
                        out.push_str(" AS ");
                        self.ops.quote_ident(out, a);
                    }
                }
            }
        }
        Ok(())
    }

    fn write_join(&mut self, out: &mut String, join: &Join, depth: u32) -> QueryResult<()> {
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
        self.write_expr(out, &join.on, depth)?;
        Ok(())
    }

    fn write_order_list(&self, out: &mut String, items: &[OrderItem]) {
        for (i, o) in items.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            self.write_order_item(out, o);
        }
    }

    fn write_order_item(&self, out: &mut String, o: &OrderItem) {
        match (o.nulls, self.ops.supports_nulls_ordering()) {
            (Some(nulls), false) => {
                // Portable emulation: CASE WHEN col IS NULL THEN N ELSE M END
                // used as a leading sort key.
                let (null_key, nonnull_key) = match nulls {
                    Nulls::First => (0, 1),
                    Nulls::Last => (1, 0),
                };
                out.push_str("CASE WHEN ");
                self.write_col_ref(out, &o.column);
                out.push_str(" IS NULL THEN ");
                out.push(char::from(b'0' + null_key));
                out.push_str(" ELSE ");
                out.push(char::from(b'0' + nonnull_key));
                out.push_str(" END, ");
                self.write_col_ref(out, &o.column);
                out.push_str(order_keyword(o.order));
            }
            _ => {
                self.write_col_ref(out, &o.column);
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

    fn write_limit_offset(
        &self,
        out: &mut String,
        limit: Option<u64>,
        offset: Option<u64>,
        has_order_by: bool,
    ) -> QueryResult<()> {
        use std::fmt::Write as _;
        match self.ops.limit_style() {
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

fn order_keyword(o: Order) -> &'static str {
    match o {
        Order::Asc => " ASC",
        Order::Desc => " DESC",
    }
}

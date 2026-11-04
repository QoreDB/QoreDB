// SPDX-License-Identifier: BUSL-1.1

//! Federation query parser.
//!
//! Detects cross-database table references in SQL queries (3-part identifiers
//! like `connection.schema.table`) and rewrites them to DuckDB temp table names.

use std::collections::{HashMap, HashSet};

use sqlparser::ast::{
    BinaryOperator, Distinct, Expr, FunctionArg, FunctionArgExpr, FunctionArguments, GroupByExpr,
    JoinConstraint, JoinOperator, ObjectNamePart, OrderByKind, Query, Select, SelectItem,
    SelectItemQualifiedWildcardKind, SetExpr, Statement, TableFactor, TableWithJoins,
    UnaryOperator, Value as AstValue,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use qore_core::error::{EngineError, EngineResult};
use qore_core::types::Namespace;

use super::types::FederatedTableRef;

/// Extracts the string value from an `ObjectNamePart`.
fn part_value(part: &ObjectNamePart) -> String {
    match part {
        ObjectNamePart::Identifier(ident) => ident.value.clone(),
        _ => String::new(),
    }
}

/// Converts `ObjectName.0` parts into a `Vec<String>`.
fn name_parts(name: &sqlparser::ast::ObjectName) -> Vec<String> {
    name.0.iter().map(part_value).collect()
}

/// Checks whether a SQL query contains cross-database federation references.
pub fn is_federation_query(sql: &str, known_aliases: &HashSet<String>) -> bool {
    let Ok(statements) = Parser::parse_sql(&GenericDialect {}, sql.trim()) else {
        return false;
    };

    for statement in &statements {
        let refs = extract_table_refs(statement);
        for (parts, _) in &refs {
            if parts.len() >= 3 {
                let alias_candidate = parts[0].to_lowercase();
                if known_aliases.contains(&alias_candidate) {
                    return true;
                }
            }
        }
    }
    false
}

/// Parses a federation query and extracts all cross-database table references.
pub fn parse_federation_refs(
    sql: &str,
    known_aliases: &HashSet<String>,
) -> EngineResult<Vec<FederatedTableRef>> {
    let statements = Parser::parse_sql(&GenericDialect {}, sql.trim())
        .map_err(|e| EngineError::syntax_error(format!("Failed to parse federation query: {e}")))?;

    if statements.len() != 1 {
        return Err(EngineError::validation(
            "Federation queries must be a single statement",
        ));
    }

    let statement = &statements[0];

    if !matches!(statement, Statement::Query(_)) {
        return Err(EngineError::validation(
            "Federation queries must be SELECT statements",
        ));
    }

    let refs = extract_table_refs(statement);
    let mut federated_refs = Vec::new();
    let mut counter = 0u32;

    for (parts, sql_alias) in refs {
        if parts.len() >= 3 {
            let alias_candidate = parts[0].to_lowercase();
            if known_aliases.contains(&alias_candidate) {
                // 3-part: connection.database.table; 4-part: connection.database.schema.table.
                let (namespace, table) = if parts.len() == 3 {
                    (
                        Namespace {
                            database: parts[1].clone(),
                            schema: None,
                        },
                        parts[2].clone(),
                    )
                } else {
                    (
                        Namespace::with_schema(parts[1].clone(), parts[2].clone()),
                        parts[parts.len() - 1].clone(),
                    )
                };

                let local_alias = format!("__fed_{}_{}", sanitize_identifier(&table), counter);
                counter += 1;

                federated_refs.push(FederatedTableRef {
                    connection_alias: alias_candidate,
                    namespace,
                    table,
                    local_alias,
                    sql_alias,
                });
            }
        }
    }

    if federated_refs.is_empty() {
        return Err(EngineError::validation(
            "No cross-database table references found in query",
        ));
    }

    Ok(federated_refs)
}

/// Rewrites a SQL query, replacing 3-part table references with local DuckDB temp table names.
///
/// `mappings` maps the original dotted name (e.g., "prod_pg.public.users") to the local alias.
pub fn rewrite_query(sql: &str, mappings: &HashMap<String, String>) -> EngineResult<String> {
    let mut statements = Parser::parse_sql(&GenericDialect {}, sql.trim())
        .map_err(|e| EngineError::syntax_error(format!("Failed to parse federation query: {e}")))?;

    if statements.len() != 1 {
        return Err(EngineError::validation(
            "Federation queries must be a single statement",
        ));
    }

    let statement = &mut statements[0];
    rewrite_statement(statement, mappings);

    Ok(statement.to_string())
}

/// Builds the original dotted name from parts for mapping lookup.
pub fn build_dotted_name(parts: &[String]) -> String {
    parts
        .iter()
        .map(|p| p.to_lowercase())
        .collect::<Vec<_>>()
        .join(".")
}

/// Extracts all table references from a statement as (parts, optional_alias) pairs.
/// Walks the AST manually to find all table references in FROM/JOIN clauses.
fn extract_table_refs(statement: &Statement) -> Vec<(Vec<String>, Option<String>)> {
    let mut refs = Vec::new();
    if let Statement::Query(query) = statement {
        collect_query_refs(query, &mut refs);
    }
    refs
}

fn collect_query_refs(query: &Query, refs: &mut Vec<(Vec<String>, Option<String>)>) {
    collect_set_expr_refs(&query.body, refs);
    if let Some(ref with) = query.with {
        for cte in &with.cte_tables {
            collect_query_refs(&cte.query, refs);
        }
    }
}

fn collect_set_expr_refs(set_expr: &SetExpr, refs: &mut Vec<(Vec<String>, Option<String>)>) {
    match set_expr {
        SetExpr::Select(select) => collect_select_refs(select, refs),
        SetExpr::Query(query) => collect_query_refs(query, refs),
        SetExpr::SetOperation { left, right, .. } => {
            collect_set_expr_refs(left, refs);
            collect_set_expr_refs(right, refs);
        }
        _ => {}
    }
}

fn collect_select_refs(select: &Select, refs: &mut Vec<(Vec<String>, Option<String>)>) {
    for twj in &select.from {
        collect_table_factor_refs(&twj.relation, refs);
        for join in &twj.joins {
            collect_table_factor_refs(&join.relation, refs);
        }
    }
}

fn collect_table_factor_refs(factor: &TableFactor, refs: &mut Vec<(Vec<String>, Option<String>)>) {
    match factor {
        TableFactor::Table { name, alias, .. } => {
            let parts = name_parts(name);
            if parts.len() >= 3 {
                let sql_alias = alias.as_ref().map(|a| a.name.value.clone());
                refs.push((parts, sql_alias));
            }
        }
        TableFactor::Derived { subquery, .. } => {
            collect_query_refs(subquery, refs);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_table_factor_refs(&table_with_joins.relation, refs);
            for join in &table_with_joins.joins {
                collect_table_factor_refs(&join.relation, refs);
            }
        }
        _ => {}
    }
}

/// Recursively rewrites table references in a statement.
fn rewrite_statement(statement: &mut Statement, mappings: &HashMap<String, String>) {
    if let Statement::Query(query) = statement {
        rewrite_query_ast(query, mappings);
    }
}

fn rewrite_query_ast(query: &mut Query, mappings: &HashMap<String, String>) {
    rewrite_set_expr(&mut query.body, mappings);

    if let Some(ref mut with) = query.with {
        for cte in &mut with.cte_tables {
            rewrite_query_ast(&mut cte.query, mappings);
        }
    }

    if let Some(ref mut order_by) = query.order_by {
        if let OrderByKind::Expressions(ref mut exprs) = order_by.kind {
            for expr_with_alias in exprs {
                rewrite_expr(&mut expr_with_alias.expr, mappings);
            }
        }
    }
}

fn rewrite_set_expr(set_expr: &mut SetExpr, mappings: &HashMap<String, String>) {
    match set_expr {
        SetExpr::Select(select) => rewrite_select(select, mappings),
        SetExpr::Query(query) => rewrite_query_ast(query, mappings),
        SetExpr::SetOperation { left, right, .. } => {
            rewrite_set_expr(left, mappings);
            rewrite_set_expr(right, mappings);
        }
        _ => {}
    }
}

fn rewrite_select(select: &mut Select, mappings: &HashMap<String, String>) {
    for table_with_joins in &mut select.from {
        rewrite_table_with_joins(table_with_joins, mappings);
    }

    for item in &mut select.projection {
        if let SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } = item {
            rewrite_expr(expr, mappings);
        }
    }

    if let Some(ref mut selection) = select.selection {
        rewrite_expr(selection, mappings);
    }

    if let sqlparser::ast::GroupByExpr::Expressions(ref mut exprs, _) = select.group_by {
        for expr in exprs {
            rewrite_expr(expr, mappings);
        }
    }

    if let Some(ref mut having) = select.having {
        rewrite_expr(having, mappings);
    }
}

fn rewrite_table_with_joins(twj: &mut TableWithJoins, mappings: &HashMap<String, String>) {
    rewrite_table_factor(&mut twj.relation, mappings);
    for join in &mut twj.joins {
        rewrite_table_factor(&mut join.relation, mappings);
        match &mut join.join_operator {
            JoinOperator::Join(constraint)
            | JoinOperator::Inner(constraint)
            | JoinOperator::Left(constraint)
            | JoinOperator::LeftOuter(constraint)
            | JoinOperator::Right(constraint)
            | JoinOperator::RightOuter(constraint)
            | JoinOperator::FullOuter(constraint)
            | JoinOperator::Semi(constraint)
            | JoinOperator::LeftSemi(constraint)
            | JoinOperator::RightSemi(constraint)
            | JoinOperator::Anti(constraint)
            | JoinOperator::LeftAnti(constraint)
            | JoinOperator::RightAnti(constraint) => {
                if let JoinConstraint::On(expr) = constraint {
                    rewrite_expr(expr, mappings);
                }
            }
            _ => {}
        }
    }
}

fn rewrite_table_factor(factor: &mut TableFactor, mappings: &HashMap<String, String>) {
    match factor {
        TableFactor::Table { name, .. } => {
            let parts = name_parts(name);
            if parts.len() >= 3 {
                let dotted = build_dotted_name(&parts);
                if let Some(local_alias) = mappings.get(&dotted) {
                    name.0 = vec![ObjectNamePart::Identifier(sqlparser::ast::Ident::new(
                        local_alias.clone(),
                    ))];
                }
            }
        }
        TableFactor::Derived { subquery, .. } => {
            rewrite_query_ast(subquery, mappings);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            rewrite_table_with_joins(table_with_joins, mappings);
        }
        _ => {}
    }
}

fn rewrite_expr(expr: &mut Expr, mappings: &HashMap<String, String>) {
    match expr {
        Expr::CompoundIdentifier(idents) => {
            // Qualified column reference: alias.db[.schema].table.col (4 or 5 parts).
            // Try the longest table prefix first so PostgreSQL 4-part names win.
            if idents.len() >= 4 {
                let max_prefix = idents.len() - 1;
                let min_prefix = 3usize;

                for prefix_len in (min_prefix..=max_prefix).rev() {
                    let candidate: Vec<String> = idents[..prefix_len]
                        .iter()
                        .map(|i| i.value.to_lowercase())
                        .collect();
                    let dotted = candidate.join(".");

                    if let Some(local_alias) = mappings.get(&dotted) {
                        let mut new_idents = vec![sqlparser::ast::Ident::new(local_alias.clone())];
                        new_idents.extend(idents[prefix_len..].iter().cloned());
                        *idents = new_idents;
                        break;
                    }
                }
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            rewrite_expr(left, mappings);
            rewrite_expr(right, mappings);
        }
        Expr::UnaryOp { expr: inner, .. } => {
            rewrite_expr(inner, mappings);
        }
        Expr::Nested(inner) => {
            rewrite_expr(inner, mappings);
        }
        Expr::Function(func) => {
            if let FunctionArguments::List(ref mut arg_list) = func.args {
                for arg in &mut arg_list.args {
                    if let sqlparser::ast::FunctionArg::Unnamed(
                        sqlparser::ast::FunctionArgExpr::Expr(e),
                    ) = arg
                    {
                        rewrite_expr(e, mappings);
                    }
                }
            }
        }
        Expr::Cast { expr: inner, .. } => {
            rewrite_expr(inner, mappings);
        }
        Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::IsTrue(inner)
        | Expr::IsFalse(inner) => {
            rewrite_expr(inner, mappings);
        }
        Expr::Between {
            expr: inner,
            low,
            high,
            ..
        } => {
            rewrite_expr(inner, mappings);
            rewrite_expr(low, mappings);
            rewrite_expr(high, mappings);
        }
        Expr::InList {
            expr: inner, list, ..
        } => {
            rewrite_expr(inner, mappings);
            for item in list {
                rewrite_expr(item, mappings);
            }
        }
        Expr::InSubquery {
            expr: inner,
            subquery,
            ..
        } => {
            rewrite_expr(inner, mappings);
            rewrite_query_ast(subquery, mappings);
        }
        Expr::Subquery(subquery) => {
            rewrite_query_ast(subquery, mappings);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                rewrite_expr(op, mappings);
            }
            for case_when in conditions {
                rewrite_expr(&mut case_when.condition, mappings);
                rewrite_expr(&mut case_when.result, mappings);
            }
            if let Some(else_r) = else_result {
                rewrite_expr(else_r, mappings);
            }
        }
        Expr::Like {
            expr: inner,
            pattern,
            ..
        }
        | Expr::ILike {
            expr: inner,
            pattern,
            ..
        } => {
            rewrite_expr(inner, mappings);
            rewrite_expr(pattern, mappings);
        }
        _ => {}
    }
}

/// Sanitizes a string for use as a DuckDB identifier part.
fn sanitize_identifier(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

/// A simple single-column predicate candidate for source pushdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidatePredicate {
    pub column: String,
    pub kind: PredicateKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PredicateKind {
    Comparison {
        op: &'static str,
        literal_sql: String,
    },
    IsNull,
    IsNotNull,
    InList {
        negated: bool,
        literals_sql: Vec<String>,
    },
}

/// Extracted per-source optimization hints from the AST.
#[derive(Debug, Clone, Default)]
pub struct SourceOptimizationAnalysis {
    /// Projected columns needed from this source (`None` means `SELECT *`).
    pub columns: Option<Vec<String>>,
    /// Candidate simple predicates that reference only this source.
    pub predicates: Vec<CandidatePredicate>,
}

/// Analyzes a federation SQL query and returns per-source column projections
/// and candidate pushdown predicates aligned with `refs`.
///
/// Falls back conservatively to `columns: None` (`SELECT *`) and empty predicates
/// whenever the query uses CTEs, set operations (`UNION`, etc.), subqueries,
/// wildcards (`*` / `alias.*`), or ambiguous unqualified columns.
pub fn analyze_source_optimizations(
    sql: &str,
    refs: &[FederatedTableRef],
) -> Vec<SourceOptimizationAnalysis> {
    let default_analysis = || vec![SourceOptimizationAnalysis::default(); refs.len()];

    if refs.is_empty() {
        return Vec::new();
    }

    let Ok(statements) = Parser::parse_sql(&GenericDialect {}, sql.trim()) else {
        return default_analysis();
    };
    if statements.len() != 1 {
        return default_analysis();
    }
    let Statement::Query(query) = &statements[0] else {
        return default_analysis();
    };

    if query.with.is_some() {
        return default_analysis();
    }

    let SetExpr::Select(select) = &*query.body else {
        return default_analysis();
    };

    // Inspect FROM / JOIN relations to verify they match `refs` 1-to-1 and
    // track which sources sit on the nullable side of an outer join.
    let mut nullable_in_outer_join = Vec::with_capacity(refs.len());
    let mut fallback_all_columns = false;

    for twj in &select.from {
        if !matches!(twj.relation, TableFactor::Table { .. }) {
            return default_analysis();
        }
        let has_right_or_full = twj.joins.iter().any(|j| {
            matches!(
                j.join_operator,
                JoinOperator::Right(_)
                    | JoinOperator::RightOuter(_)
                    | JoinOperator::RightSemi(_)
                    | JoinOperator::RightAnti(_)
                    | JoinOperator::FullOuter(_)
            )
        });
        nullable_in_outer_join.push(has_right_or_full);

        for join in &twj.joins {
            if !matches!(join.relation, TableFactor::Table { .. }) {
                return default_analysis();
            }
            let join_nullable = has_right_or_full
                || !matches!(
                    join.join_operator,
                    JoinOperator::Join(_) | JoinOperator::Inner(_) | JoinOperator::CrossJoin(_)
                );
            nullable_in_outer_join.push(join_nullable);

            if let Some(constraint) = extract_join_constraint(&join.join_operator) {
                if matches!(
                    constraint,
                    JoinConstraint::Using(_) | JoinConstraint::Natural
                ) {
                    fallback_all_columns = true;
                }
            }
        }
    }

    if nullable_in_outer_join.len() != refs.len() {
        return default_analysis();
    }

    let mut columns_per_source: Vec<Vec<String>> = vec![Vec::new(); refs.len()];
    let mut wildcard_source: Vec<bool> = vec![false; refs.len()];
    let mut select_aliases: HashSet<String> = HashSet::new();
    for item in &select.projection {
        if let SelectItem::ExprWithAlias { alias, .. } = item {
            select_aliases.insert(alias.value.to_lowercase());
        }
    }

    let mut ctx = ColumnCollector {
        refs,
        columns_per_source: &mut columns_per_source,
        wildcard_source: &mut wildcard_source,
        fallback_all_columns: &mut fallback_all_columns,
        select_aliases: &select_aliases,
        allow_select_alias: false,
    };

    // 1. SELECT projection
    for item in &select.projection {
        match item {
            SelectItem::Wildcard(_) => {
                *ctx.fallback_all_columns = true;
            }
            SelectItem::QualifiedWildcard(kind, _) => {
                if let SelectItemQualifiedWildcardKind::ObjectName(obj_name) = kind {
                    let parts = name_parts(obj_name);
                    if let Some(src_idx) = resolve_table_qualifier(&parts, refs) {
                        ctx.wildcard_source[src_idx] = true;
                    } else {
                        *ctx.fallback_all_columns = true;
                    }
                } else {
                    *ctx.fallback_all_columns = true;
                }
            }
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                ctx.visit_expr(expr);
            }
        }
    }

    // 2. JOIN ON conditions
    for twj in &select.from {
        for join in &twj.joins {
            if let Some(JoinConstraint::On(expr)) = extract_join_constraint(&join.join_operator) {
                ctx.visit_expr(expr);
            }
        }
    }

    // 3. WHERE selection
    if let Some(ref selection) = select.selection {
        ctx.visit_expr(selection);
    }

    // 4. GROUP BY / HAVING / ORDER BY / DISTINCT ON (SELECT aliases are in scope)
    ctx.allow_select_alias = true;

    match &select.group_by {
        GroupByExpr::All(_) => {
            *ctx.fallback_all_columns = true;
        }
        GroupByExpr::Expressions(exprs, _) => {
            for expr in exprs {
                ctx.visit_expr(expr);
            }
        }
    }

    if let Some(ref having) = select.having {
        ctx.visit_expr(having);
    }

    if let Some(ref order_by) = query.order_by {
        match &order_by.kind {
            OrderByKind::All(_) => {
                *ctx.fallback_all_columns = true;
            }
            OrderByKind::Expressions(exprs) => {
                for expr_with_order in exprs {
                    ctx.visit_expr(&expr_with_order.expr);
                }
            }
        }
    }

    if let Some(Distinct::On(ref exprs)) = select.distinct {
        for expr in exprs {
            ctx.visit_expr(expr);
        }
    }

    // 5. Extract simple pushdown predicates from top-level WHERE conjunctions
    let mut predicates_per_source: Vec<Vec<CandidatePredicate>> = vec![Vec::new(); refs.len()];
    if let Some(ref selection) = select.selection {
        let mut conjuncts = Vec::new();
        collect_and_conjuncts(selection, &mut conjuncts);
        for conjunct in conjuncts {
            if let Some((src_idx, predicate)) = extract_simple_predicate(conjunct, refs) {
                if !nullable_in_outer_join[src_idx] {
                    predicates_per_source[src_idx].push(predicate);
                }
            }
        }
    }

    let mut results = Vec::with_capacity(refs.len());
    for i in 0..refs.len() {
        let columns = if fallback_all_columns || wildcard_source[i] || columns_per_source[i].is_empty()
        {
            None
        } else {
            Some(columns_per_source[i].clone())
        };
        results.push(SourceOptimizationAnalysis {
            columns,
            predicates: predicates_per_source[i].clone(),
        });
    }

    results
}

struct ColumnCollector<'a> {
    refs: &'a [FederatedTableRef],
    columns_per_source: &'a mut [Vec<String>],
    wildcard_source: &'a mut [bool],
    fallback_all_columns: &'a mut bool,
    select_aliases: &'a HashSet<String>,
    allow_select_alias: bool,
}

impl ColumnCollector<'_> {
    fn add_column(&mut self, source_idx: usize, column: &str) {
        let list = &mut self.columns_per_source[source_idx];
        if !list.iter().any(|c| c.eq_ignore_ascii_case(column)) {
            list.push(column.to_string());
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if *self.fallback_all_columns {
            return;
        }
        match expr {
            Expr::Identifier(ident) => {
                if self.allow_select_alias
                    && self.select_aliases.contains(&ident.value.to_lowercase())
                {
                    return;
                }
                if self.refs.len() == 1 {
                    self.add_column(0, &ident.value);
                } else {
                    *self.fallback_all_columns = true;
                }
            }
            Expr::CompoundIdentifier(idents) => {
                let parts: Vec<String> = idents.iter().map(|i| i.value.clone()).collect();
                if let Some((src_idx, col)) = resolve_compound_column(&parts, self.refs) {
                    self.add_column(src_idx, &col);
                } else {
                    *self.fallback_all_columns = true;
                }
            }
            Expr::Value(_) | Expr::TypedString { .. } => {}
            Expr::BinaryOp { left, right, .. }
            | Expr::AnyOp { left, right, .. }
            | Expr::AllOp { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            Expr::UnaryOp { expr: inner, .. }
            | Expr::Nested(inner)
            | Expr::Cast { expr: inner, .. }
            | Expr::IsNull(inner)
            | Expr::IsNotNull(inner)
            | Expr::IsTrue(inner)
            | Expr::IsFalse(inner)
            | Expr::IsUnknown(inner)
            | Expr::IsNotUnknown(inner)
            | Expr::Extract { expr: inner, .. }
            | Expr::Ceil { expr: inner, .. }
            | Expr::Floor { expr: inner, .. }
            | Expr::Collate { expr: inner, .. }
            | Expr::AtTimeZone {
                timestamp: inner, ..
            } => {
                self.visit_expr(inner);
            }
            Expr::Between {
                expr: inner,
                low,
                high,
                ..
            } => {
                self.visit_expr(inner);
                self.visit_expr(low);
                self.visit_expr(high);
            }
            Expr::InList {
                expr: inner, list, ..
            } => {
                self.visit_expr(inner);
                for item in list {
                    self.visit_expr(item);
                }
            }
            Expr::Like {
                expr: inner,
                pattern,
                ..
            }
            | Expr::ILike {
                expr: inner,
                pattern,
                ..
            }
            | Expr::SimilarTo {
                expr: inner,
                pattern,
                ..
            } => {
                self.visit_expr(inner);
                self.visit_expr(pattern);
            }
            Expr::Case {
                operand,
                conditions,
                else_result,
                ..
            } => {
                if let Some(op) = operand {
                    self.visit_expr(op);
                }
                for cw in conditions {
                    self.visit_expr(&cw.condition);
                    self.visit_expr(&cw.result);
                }
                if let Some(else_r) = else_result {
                    self.visit_expr(else_r);
                }
            }
            Expr::Function(func) => {
                if func.over.is_some() || !func.within_group.is_empty() || func.filter.is_some() {
                    *self.fallback_all_columns = true;
                    return;
                }
                if let FunctionArguments::List(ref arg_list) = func.args {
                    for arg in &arg_list.args {
                        match arg {
                            FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
                            | FunctionArg::Named {
                                arg: FunctionArgExpr::Expr(e),
                                ..
                            }
                            | FunctionArg::ExprNamed {
                                arg: FunctionArgExpr::Expr(e),
                                ..
                            } => {
                                self.visit_expr(e);
                            }
                            FunctionArg::Unnamed(FunctionArgExpr::QualifiedWildcard(obj_name)) => {
                                let parts = name_parts(obj_name);
                                if let Some(src_idx) = resolve_table_qualifier(&parts, self.refs) {
                                    self.wildcard_source[src_idx] = true;
                                } else {
                                    *self.fallback_all_columns = true;
                                }
                            }
                            FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => {
                                // `COUNT(*)` does not require extra columns if at least one column
                                // is already projected; if a source has 0 columns at the end, it
                                // automatically falls back to `None` (`SELECT *`).
                            }
                            _ => {
                                *self.fallback_all_columns = true;
                            }
                        }
                    }
                }
            }
            Expr::Tuple(exprs) => {
                for e in exprs {
                    self.visit_expr(e);
                }
            }
            _ => {
                *self.fallback_all_columns = true;
            }
        }
    }
}

fn extract_join_constraint(op: &JoinOperator) -> Option<&JoinConstraint> {
    match op {
        JoinOperator::Join(constraint)
        | JoinOperator::Inner(constraint)
        | JoinOperator::Left(constraint)
        | JoinOperator::LeftOuter(constraint)
        | JoinOperator::Right(constraint)
        | JoinOperator::RightOuter(constraint)
        | JoinOperator::FullOuter(constraint)
        | JoinOperator::Semi(constraint)
        | JoinOperator::LeftSemi(constraint)
        | JoinOperator::RightSemi(constraint)
        | JoinOperator::Anti(constraint)
        | JoinOperator::LeftAnti(constraint)
        | JoinOperator::RightAnti(constraint) => Some(constraint),
        _ => None,
    }
}

fn ref_dotted_name(r: &FederatedTableRef) -> String {
    if let Some(ref schema) = r.namespace.schema {
        build_dotted_name(&[
            r.connection_alias.clone(),
            r.namespace.database.clone(),
            schema.clone(),
            r.table.clone(),
        ])
    } else {
        build_dotted_name(&[
            r.connection_alias.clone(),
            r.namespace.database.clone(),
            r.table.clone(),
        ])
    }
}

/// Resolves a table qualifier (from `alias.*` or `conn.db.table.*`) to a unique source index.
fn resolve_table_qualifier(parts: &[String], refs: &[FederatedTableRef]) -> Option<usize> {
    if parts.len() >= 3 {
        let dotted = build_dotted_name(parts);
        let matches: Vec<usize> = refs
            .iter()
            .enumerate()
            .filter(|(_, r)| ref_dotted_name(r) == dotted)
            .map(|(i, _)| i)
            .collect();
        if matches.len() == 1 {
            return Some(matches[0]);
        }
    }
    if parts.len() == 1 {
        let qual = parts[0].to_lowercase();
        let matches: Vec<usize> = refs
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.sql_alias
                    .as_ref()
                    .is_some_and(|a| a.eq_ignore_ascii_case(&qual))
                    || (r.sql_alias.is_none() && r.table.eq_ignore_ascii_case(&qual))
            })
            .map(|(i, _)| i)
            .collect();
        if matches.len() == 1 {
            return Some(matches[0]);
        }
    }
    None
}

/// Resolves a compound identifier (`u.col` or `conn.db.table.col`) to `(source_idx, column_name)`.
fn resolve_compound_column(
    parts: &[String],
    refs: &[FederatedTableRef],
) -> Option<(usize, String)> {
    if parts.len() >= 4 {
        let max_prefix = parts.len() - 1;
        for prefix_len in (3..=max_prefix).rev() {
            if let Some(src_idx) = resolve_table_qualifier(&parts[..prefix_len], refs) {
                return Some((src_idx, parts[prefix_len].clone()));
            }
        }
    }
    if parts.len() >= 2 {
        if let Some(src_idx) = resolve_table_qualifier(&parts[..1], refs) {
            return Some((src_idx, parts[1].clone()));
        }
    }
    None
}

/// Resolves an AST expression that is a simple column reference to `(source_idx, column_name)`.
fn resolve_simple_column_expr(
    expr: &Expr,
    refs: &[FederatedTableRef],
) -> Option<(usize, String)> {
    match expr {
        Expr::Nested(inner) => resolve_simple_column_expr(inner, refs),
        Expr::Identifier(ident) if refs.len() == 1 => Some((0, ident.value.clone())),
        Expr::CompoundIdentifier(idents) => {
            if idents.len() == 2 {
                let parts = vec![idents[0].value.clone(), idents[1].value.clone()];
                resolve_compound_column(&parts, refs)
            } else if idents.len() == 4 || idents.len() == 5 {
                let parts: Vec<String> = idents.iter().map(|i| i.value.clone()).collect();
                // Only match when the column is the final part (not nested struct/JSON path).
                let prefix_len = parts.len() - 1;
                resolve_table_qualifier(&parts[..prefix_len], refs)
                    .map(|src_idx| (src_idx, parts[prefix_len].clone()))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn collect_and_conjuncts<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
    match expr {
        Expr::Nested(inner) => collect_and_conjuncts(inner, out),
        Expr::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } => {
            collect_and_conjuncts(left, out);
            collect_and_conjuncts(right, out);
        }
        other => out.push(other),
    }
}

fn extract_simple_predicate(
    expr: &Expr,
    refs: &[FederatedTableRef],
) -> Option<(usize, CandidatePredicate)> {
    match expr {
        Expr::Nested(inner) => extract_simple_predicate(inner, refs),
        Expr::BinaryOp { left, op, right } => {
            if let Some((src_idx, column)) = resolve_simple_column_expr(left, refs) {
                let op_str = comparison_op_str(op, false)?;
                let literal_sql = render_sql_literal(right)?;
                return Some((
                    src_idx,
                    CandidatePredicate {
                        column,
                        kind: PredicateKind::Comparison {
                            op: op_str,
                            literal_sql,
                        },
                    },
                ));
            }
            if let Some((src_idx, column)) = resolve_simple_column_expr(right, refs) {
                let op_str = comparison_op_str(op, true)?;
                let literal_sql = render_sql_literal(left)?;
                return Some((
                    src_idx,
                    CandidatePredicate {
                        column,
                        kind: PredicateKind::Comparison {
                            op: op_str,
                            literal_sql,
                        },
                    },
                ));
            }
            None
        }
        Expr::IsNull(inner) => {
            let (src_idx, column) = resolve_simple_column_expr(inner, refs)?;
            Some((
                src_idx,
                CandidatePredicate {
                    column,
                    kind: PredicateKind::IsNull,
                },
            ))
        }
        Expr::IsNotNull(inner) => {
            let (src_idx, column) = resolve_simple_column_expr(inner, refs)?;
            Some((
                src_idx,
                CandidatePredicate {
                    column,
                    kind: PredicateKind::IsNotNull,
                },
            ))
        }
        Expr::InList {
            expr: inner,
            list,
            negated,
        } => {
            if list.is_empty() {
                return None;
            }
            let (src_idx, column) = resolve_simple_column_expr(inner, refs)?;
            let mut literals_sql = Vec::with_capacity(list.len());
            for item in list {
                literals_sql.push(render_sql_literal(item)?);
            }
            Some((
                src_idx,
                CandidatePredicate {
                    column,
                    kind: PredicateKind::InList {
                        negated: *negated,
                        literals_sql,
                    },
                },
            ))
        }
        _ => None,
    }
}

fn comparison_op_str(op: &BinaryOperator, flipped: bool) -> Option<&'static str> {
    match (op, flipped) {
        (BinaryOperator::Eq, _) => Some("="),
        (BinaryOperator::NotEq, _) => Some("!="),
        (BinaryOperator::Lt, false) | (BinaryOperator::Gt, true) => Some("<"),
        (BinaryOperator::LtEq, false) | (BinaryOperator::GtEq, true) => Some("<="),
        (BinaryOperator::Gt, false) | (BinaryOperator::Lt, true) => Some(">"),
        (BinaryOperator::GtEq, false) | (BinaryOperator::LtEq, true) => Some(">="),
        _ => None,
    }
}

fn render_sql_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Nested(inner) => render_sql_literal(inner),
        Expr::Value(val) => match &val.value {
            AstValue::Number(n, _) => {
                if !n.is_empty()
                    && n.chars()
                        .all(|c| c.is_ascii_digit() || matches!(c, '.' | '+' | '-' | 'e' | 'E'))
                {
                    Some(n.clone())
                } else {
                    None
                }
            }
            AstValue::SingleQuotedString(s) => Some(format!("'{}'", s.replace('\'', "''"))),
            AstValue::Boolean(b) => Some(if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }),
            _ => None,
        },
        Expr::UnaryOp {
            op: UnaryOperator::Minus,
            expr: inner,
        } => {
            if let Expr::Value(val) = &**inner {
                if let AstValue::Number(n, _) = &val.value {
                    if !n.is_empty()
                        && n.chars()
                            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '+' | '-' | 'e' | 'E'))
                    {
                        return Some(format!("-{n}"));
                    }
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aliases() -> HashSet<String> {
        let mut set = HashSet::new();
        set.insert("prod_pg".to_string());
        set.insert("analytics_mongo".to_string());
        set.insert("local_sqlite".to_string());
        set
    }

    #[test]
    fn detects_federation_query() {
        let sql = "SELECT u.email FROM prod_pg.public.users u";
        assert!(is_federation_query(sql, &aliases()));
    }

    #[test]
    fn rejects_non_federation_query() {
        let sql = "SELECT * FROM users WHERE id = 1";
        assert!(!is_federation_query(sql, &aliases()));
    }

    #[test]
    fn rejects_unknown_alias() {
        let sql = "SELECT * FROM unknown_db.public.users";
        assert!(!is_federation_query(sql, &aliases()));
    }

    #[test]
    fn parses_federation_refs_simple() {
        let sql = "SELECT u.email, COUNT(e._id) FROM prod_pg.public.users u JOIN analytics_mongo.analytics.events e ON e.user_id = u.id";
        let refs = parse_federation_refs(sql, &aliases()).unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].connection_alias, "prod_pg");
        assert_eq!(refs[0].table, "users");
        assert_eq!(refs[1].connection_alias, "analytics_mongo");
        assert_eq!(refs[1].table, "events");
    }

    #[test]
    fn rewrite_replaces_table_names() {
        let sql = "SELECT * FROM prod_pg.public.users u JOIN analytics_mongo.analytics.events e ON e.user_id = u.id";
        let refs = parse_federation_refs(sql, &aliases()).unwrap();

        let mut mappings = HashMap::new();
        for r in &refs {
            let dotted = build_dotted_name(&[
                r.connection_alias.clone(),
                r.namespace.database.clone(),
                r.table.clone(),
            ]);
            mappings.insert(dotted, r.local_alias.clone());
        }

        let rewritten = rewrite_query(sql, &mappings).unwrap();
        assert!(rewritten.contains("__fed_users_0"));
        assert!(rewritten.contains("__fed_events_1"));
        assert!(!rewritten.contains("prod_pg"));
        assert!(!rewritten.contains("analytics_mongo"));
    }

    #[test]
    fn rejects_mutation() {
        let sql = "INSERT INTO prod_pg.public.users VALUES (1, 'test')";
        let result = parse_federation_refs(sql, &aliases());
        assert!(result.is_err());
    }

    #[test]
    fn rejects_multi_statement() {
        let sql = "SELECT * FROM prod_pg.public.users; SELECT 1";
        let result = parse_federation_refs(sql, &aliases());
        assert!(result.is_err());
    }

    #[test]
    fn rewrite_supports_four_part_compound_identifier() {
        let sql = "SELECT prod_pg.testdb.public.users.email FROM prod_pg.testdb.public.users";
        let refs = parse_federation_refs(sql, &aliases()).unwrap();

        let mut mappings = HashMap::new();
        for r in &refs {
            let dotted = if let Some(schema) = &r.namespace.schema {
                build_dotted_name(&[
                    r.connection_alias.clone(),
                    r.namespace.database.clone(),
                    schema.clone(),
                    r.table.clone(),
                ])
            } else {
                build_dotted_name(&[
                    r.connection_alias.clone(),
                    r.namespace.database.clone(),
                    r.table.clone(),
                ])
            };
            mappings.insert(dotted, r.local_alias.clone());
        }

        let rewritten = rewrite_query(sql, &mappings).unwrap();
        assert!(rewritten.contains("__fed_users_0.email"));
        assert!(!rewritten.contains("prod_pg.testdb.public.users.email"));
    }
}

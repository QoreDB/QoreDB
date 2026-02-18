// SPDX-License-Identifier: BUSL-1.1

//! Federation query parser.
//!
//! Detects cross-database table references in SQL queries (3-part identifiers
//! like `connection.schema.table`) and rewrites them to DuckDB temp table names.

use std::collections::{HashMap, HashSet};

use sqlparser::ast::{
    Expr, FunctionArguments, ObjectNamePart, OrderByKind, Query, Select, SelectItem, SetExpr,
    Statement, TableFactor, TableWithJoins,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::types::Namespace;

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

    // Federation queries must be SELECT-like
    if !matches!(statement, Statement::Query(_)) {
        return Err(EngineError::validation(
            "Federation queries must be SELECT statements",
        ));
    }

    let refs = extract_table_refs(statement);
    let mut federated_refs = Vec::new();
    let mut counter = 0u32;

    for (parts, _alias) in refs {
        if parts.len() >= 3 {
            let alias_candidate = parts[0].to_lowercase();
            if known_aliases.contains(&alias_candidate) {
                let (namespace, table) = if parts.len() == 3 {
                    // connection.schema_or_db.table
                    (
                        Namespace {
                            database: parts[1].clone(),
                            schema: None,
                        },
                        parts[2].clone(),
                    )
                } else {
                    // connection.database.schema.table (4-part, e.g. PostgreSQL)
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

// --- AST Walking ---

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

fn collect_table_factor_refs(
    factor: &TableFactor,
    refs: &mut Vec<(Vec<String>, Option<String>)>,
) {
    match factor {
        TableFactor::Table { name, .. } => {
            let parts = name_parts(name);
            if parts.len() >= 3 {
                refs.push((parts, None));
            }
        }
        TableFactor::Derived { subquery, .. } => {
            collect_query_refs(subquery, refs);
        }
        TableFactor::NestedJoin { table_with_joins, .. } => {
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

    // Rewrite CTEs
    if let Some(ref mut with) = query.with {
        for cte in &mut with.cte_tables {
            rewrite_query_ast(&mut cte.query, mappings);
        }
    }

    // Rewrite ORDER BY expressions
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
    // Rewrite FROM tables
    for table_with_joins in &mut select.from {
        rewrite_table_with_joins(table_with_joins, mappings);
    }

    // Rewrite SELECT items
    for item in &mut select.projection {
        if let SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } = item {
            rewrite_expr(expr, mappings);
        }
    }

    // Rewrite WHERE
    if let Some(ref mut selection) = select.selection {
        rewrite_expr(selection, mappings);
    }

    // Rewrite GROUP BY
    if let sqlparser::ast::GroupByExpr::Expressions(ref mut exprs, _) = select.group_by {
        for expr in exprs {
            rewrite_expr(expr, mappings);
        }
    }

    // Rewrite HAVING
    if let Some(ref mut having) = select.having {
        rewrite_expr(having, mappings);
    }
}

fn rewrite_table_with_joins(twj: &mut TableWithJoins, mappings: &HashMap<String, String>) {
    rewrite_table_factor(&mut twj.relation, mappings);
    for join in &mut twj.joins {
        rewrite_table_factor(&mut join.relation, mappings);
        // Rewrite JOIN conditions
        match &mut join.join_operator {
            sqlparser::ast::JoinOperator::Inner(constraint)
            | sqlparser::ast::JoinOperator::LeftOuter(constraint)
            | sqlparser::ast::JoinOperator::RightOuter(constraint)
            | sqlparser::ast::JoinOperator::FullOuter(constraint) => {
                if let sqlparser::ast::JoinConstraint::On(ref mut expr) = constraint {
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
                    // Replace the multi-part name with the local DuckDB alias
                    name.0 = vec![ObjectNamePart::Identifier(
                        sqlparser::ast::Ident::new(local_alias.clone()),
                    )];
                }
            }
        }
        TableFactor::Derived { subquery, .. } => {
            rewrite_query_ast(subquery, mappings);
        }
        TableFactor::NestedJoin { table_with_joins, .. } => {
            rewrite_table_with_joins(table_with_joins, mappings);
        }
        _ => {}
    }
}

fn rewrite_expr(expr: &mut Expr, mappings: &HashMap<String, String>) {
    match expr {
        Expr::CompoundIdentifier(idents) => {
            // Check if this is a compound identifier referencing a federated table
            // e.g., prod_pg.public.users.email -> __fed_users_0.email
            if idents.len() >= 4 {
                let first_three: Vec<String> =
                    idents[..3].iter().map(|i| i.value.to_lowercase()).collect();
                let dotted = first_three.join(".");
                if let Some(local_alias) = mappings.get(&dotted) {
                    let mut new_idents = vec![sqlparser::ast::Ident::new(local_alias.clone())];
                    new_idents.extend(idents[3..].iter().cloned());
                    *idents = new_idents;
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
                        sqlparser::ast::FunctionArgExpr::Expr(ref mut e),
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
        Expr::InList { expr: inner, list, .. } => {
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
        Expr::Like { expr: inner, pattern, .. } | Expr::ILike { expr: inner, pattern, .. } => {
            rewrite_expr(inner, mappings);
            rewrite_expr(pattern, mappings);
        }
        _ => {}
    }
}

/// Sanitizes a string for use as a DuckDB identifier part.
fn sanitize_identifier(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect::<String>()
        .to_lowercase()
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
}

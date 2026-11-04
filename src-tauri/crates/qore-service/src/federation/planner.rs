// SPDX-License-Identifier: BUSL-1.1

//! Federation query planner.
//!
//! Resolves connection aliases to session IDs, analyzes WHERE clause for pushdown
//! opportunities, and generates the final `FederationPlan`.

use std::collections::HashMap;

use qore_core::error::{EngineError, EngineResult};
use qore_core::masking::ConnectionMasking;
use qore_sql::generator::SqlDialect;

use super::parser::{
    CandidatePredicate, PredicateKind, SourceOptimizationAnalysis, analyze_source_optimizations,
    build_dotted_name, parse_federation_refs, rewrite_query,
};
use super::types::{
    ConnectionAliasMap, DEFAULT_ROW_LIMIT, FederatedTableRef, FederationPlan, SourceFetchPlan,
};

/// Builds a `FederationPlan` from a user query: extracts federated refs,
/// resolves aliases to sessions, derives source fetch plans, and rewrites
/// the query for DuckDB.
pub fn build_plan(
    sql: &str,
    alias_map: &ConnectionAliasMap,
    row_limit: Option<u64>,
    streaming: bool,
) -> EngineResult<FederationPlan> {
    let known_aliases = alias_map.keys().cloned().collect();
    let federated_refs = parse_federation_refs(sql, &known_aliases)?;
    let optimizations = analyze_source_optimizations(sql, &federated_refs);

    let sources = resolve_sources(&federated_refs, &optimizations, alias_map, row_limit)?;
    let mappings = build_rewrite_mappings(&federated_refs);
    let duckdb_query = rewrite_query(sql, &mappings)?;

    Ok(FederationPlan {
        sources,
        duckdb_query,
        original_query: sql.to_string(),
        streaming,
    })
}

/// Resolves each federated table reference to a `SourceFetchPlan`.
fn resolve_sources(
    refs: &[FederatedTableRef],
    optimizations: &[SourceOptimizationAnalysis],
    alias_map: &ConnectionAliasMap,
    row_limit: Option<u64>,
) -> EngineResult<Vec<SourceFetchPlan>> {
    let effective_limit = row_limit.unwrap_or(DEFAULT_ROW_LIMIT);
    let mut sources = Vec::with_capacity(refs.len());

    for (idx, table_ref) in refs.iter().enumerate() {
        let entry = alias_map.get(&table_ref.connection_alias).ok_or_else(|| {
            let available: Vec<&String> = alias_map.keys().collect();
            EngineError::validation(format!(
                "Unknown connection alias '{}'. Available connections: {}",
                table_ref.connection_alias,
                available
                    .iter()
                    .map(|a| format!("'{a}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;

        let is_sql_driver = !matches!(entry.driver_id.as_str(), "mongodb" | "documentdb")
            && SqlDialect::from_driver_id(&entry.driver_id).is_some();

        let (columns, pushdown_predicates) = if is_sql_driver {
            let dialect =
                SqlDialect::from_driver_id(&entry.driver_id).unwrap_or(SqlDialect::Postgres);
            if let Some(opt) = optimizations.get(idx) {
                let preds = opt
                    .predicates
                    .iter()
                    .map(|p| render_candidate_predicate(p, dialect))
                    .collect();
                (opt.columns.clone(), preds)
            } else {
                (None, Vec::new())
            }
        } else {
            (None, Vec::new())
        };

        sources.push(SourceFetchPlan {
            table_ref: table_ref.clone(),
            session_id: entry.session_id,
            driver_id: entry.driver_id.clone(),
            columns,
            pushdown_predicates,
            row_limit: effective_limit,
        });
    }

    Ok(sources)
}

fn render_candidate_predicate(pred: &CandidatePredicate, dialect: SqlDialect) -> String {
    let quoted_col = dialect.quote_ident(&pred.column);
    match &pred.kind {
        PredicateKind::Comparison { op, literal_sql } => {
            format!("{quoted_col} {op} {literal_sql}")
        }
        PredicateKind::IsNull => format!("{quoted_col} IS NULL"),
        PredicateKind::IsNotNull => format!("{quoted_col} IS NOT NULL"),
        PredicateKind::InList {
            negated,
            literals_sql,
        } => {
            let list_str = literals_sql.join(", ");
            if *negated {
                format!("{quoted_col} NOT IN ({list_str})")
            } else {
                format!("{quoted_col} IN ({list_str})")
            }
        }
    }
}

/// Removes any pushdown predicates that reference a masked column on `source`.
///
/// Masked columns are replaced with masked text after fetching from the source
/// database and before loading into DuckDB. Pushing a raw-value filter to the
/// source database on a masked column would leak information via filter
/// selectivity, so those predicates must only be evaluated by DuckDB on the
/// post-masked values.
pub fn strip_masked_predicates(source: &mut SourceFetchPlan, masking: &ConnectionMasking) {
    if masking.is_empty() || source.pushdown_predicates.is_empty() {
        return;
    }
    let table_name = source.table_ref.table.clone();
    source.pushdown_predicates.retain(|pred| {
        if let Some(col) = extract_leading_quoted_ident(pred) {
            masking.mode_for(Some(&table_name), &col).is_none()
        } else {
            false
        }
    });
}

fn extract_leading_quoted_ident(predicate: &str) -> Option<String> {
    let trimmed = predicate.trim_start();
    let quote_char = trimmed.chars().next()?;
    if quote_char != '"' && quote_char != '`' {
        return None;
    }
    let mut chars = trimmed[quote_char.len_utf8()..].chars().peekable();
    let mut ident = String::new();
    while let Some(ch) = chars.next() {
        if ch == quote_char {
            if chars.peek() == Some(&quote_char) {
                ident.push(quote_char);
                let _ = chars.next();
            } else {
                return Some(ident);
            }
        } else {
            ident.push(ch);
        }
    }
    None
}

/// Builds the rewrite mapping from original dotted names to local DuckDB aliases.
fn build_rewrite_mappings(refs: &[FederatedTableRef]) -> HashMap<String, String> {
    let mut mappings = HashMap::new();
    for r in refs {
        // Dotted name as written in SQL: alias.database[.schema].table.
        let dotted = if let Some(ref schema) = r.namespace.schema {
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
    mappings
}

/// Builds the source query to fetch data from a single source table.
///
/// Generates: `SELECT {columns} FROM {table} WHERE {predicates} LIMIT {limit}`
pub fn build_source_query(source: &SourceFetchPlan) -> String {
    if matches!(source.driver_id.as_str(), "mongodb" | "documentdb") {
        return build_mongo_source_query(source);
    }

    let dialect = SqlDialect::from_driver_id(&source.driver_id).unwrap_or(SqlDialect::Postgres);

    let columns_clause = match &source.columns {
        Some(cols) if !cols.is_empty() => cols
            .iter()
            .map(|c| dialect.quote_ident(c))
            .collect::<Vec<_>>()
            .join(", "),
        _ => "*".to_string(),
    };

    let table_name = &source.table_ref.table;

    let mut sql = format!(
        "SELECT {columns_clause} FROM {}",
        dialect.quote_ident(table_name)
    );

    if !source.pushdown_predicates.is_empty() {
        let where_clause = source.pushdown_predicates.join(" AND ");
        sql.push_str(&format!(" WHERE {where_clause}"));
    }

    sql.push_str(&format!(" LIMIT {}", source.row_limit));
    sql
}

/// Builds a MongoDB-style query for source fetching.
/// Uses the JSON format expected by the MongoDB driver's `parse_query`.
fn build_mongo_source_query(source: &SourceFetchPlan) -> String {
    let database = &source.table_ref.namespace.database;
    let collection = &source.table_ref.table;

    // Format expected by the MongoDB driver's parse_query().
    format!(
        r#"{{"database":"{}","collection":"{}","query":{{}}}}"#,
        database.replace('"', "\\\""),
        collection.replace('"', "\\\""),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::federation::types::AliasEntry;
    use qore_core::types::SessionId;

    fn test_alias_map() -> ConnectionAliasMap {
        let mut map = ConnectionAliasMap::new();
        map.insert(
            "prod_pg".to_string(),
            AliasEntry {
                session_id: SessionId::new(),
                driver_id: "postgres".to_string(),
                display_name: "Production PostgreSQL".to_string(),
            },
        );
        map.insert(
            "analytics_mongo".to_string(),
            AliasEntry {
                session_id: SessionId::new(),
                driver_id: "mongodb".to_string(),
                display_name: "Analytics MongoDB".to_string(),
            },
        );
        map.insert(
            "app_mysql".to_string(),
            AliasEntry {
                session_id: SessionId::new(),
                driver_id: "mysql".to_string(),
                display_name: "App MySQL".to_string(),
            },
        );
        map
    }

    #[test]
    fn builds_plan_from_simple_join() {
        let sql = "SELECT u.email, e.type FROM prod_pg.public.users u JOIN analytics_mongo.analytics.events e ON e.user_id = u.id";
        let alias_map = test_alias_map();
        let plan = build_plan(sql, &alias_map, None, false).unwrap();

        assert_eq!(plan.sources.len(), 2);
        assert_eq!(plan.sources[0].table_ref.table, "users");
        assert_eq!(plan.sources[0].driver_id, "postgres");
        assert_eq!(plan.sources[1].table_ref.table, "events");
        assert_eq!(plan.sources[1].driver_id, "mongodb");
        assert!(!plan.duckdb_query.contains("prod_pg"));
        assert!(!plan.duckdb_query.contains("analytics_mongo"));
    }

    #[test]
    fn unknown_alias_errors() {
        let sql = "SELECT * FROM unknown_db.public.users";
        let alias_map = test_alias_map();
        let result = build_plan(sql, &alias_map, None, false);
        assert!(result.is_err());
    }

    #[test]
    fn source_query_has_limit() {
        let sql = "SELECT * FROM prod_pg.public.users";
        let alias_map = test_alias_map();
        let plan = build_plan(sql, &alias_map, Some(50000), false).unwrap();

        let source_sql = build_source_query(&plan.sources[0]);
        assert!(source_sql.contains("LIMIT 50000"));
    }

    #[test]
    fn mysql_source_query_uses_backticks() {
        let sql = "SELECT * FROM app_mysql.mydb.profile";
        let alias_map = test_alias_map();
        let plan = build_plan(sql, &alias_map, None, false).unwrap();

        let source_sql = build_source_query(&plan.sources[0]);
        assert_eq!(plan.sources[0].driver_id, "mysql");
        assert!(
            source_sql.contains("`profile`"),
            "MySQL source query should use backticks, got: {source_sql}"
        );
        assert!(
            !source_sql.contains("\"profile\""),
            "MySQL source query should NOT use double quotes, got: {source_sql}"
        );
    }

    #[test]
    fn postgres_source_query_uses_double_quotes() {
        let sql = "SELECT * FROM prod_pg.public.users";
        let alias_map = test_alias_map();
        let plan = build_plan(sql, &alias_map, None, false).unwrap();

        let source_sql = build_source_query(&plan.sources[0]);
        assert_eq!(plan.sources[0].driver_id, "postgres");
        assert!(
            source_sql.contains("\"users\""),
            "Postgres source query should use double quotes, got: {source_sql}"
        );
    }

    #[test]
    fn mongo_source_query_format() {
        let sql = "SELECT * FROM analytics_mongo.analytics.events";
        let alias_map = test_alias_map();
        let plan = build_plan(sql, &alias_map, None, false).unwrap();

        let source_sql = build_source_query(&plan.sources[0]);
        assert!(
            source_sql.contains(r#""database":"analytics""#),
            "MongoDB source query should use JSON format with correct database, got: {source_sql}"
        );
        assert!(
            source_sql.contains(r#""collection":"events""#),
            "MongoDB source query should use JSON format with correct collection, got: {source_sql}"
        );
    }

    #[test]
    fn projects_columns_and_pushes_simple_predicates_per_sql_source() {
        let sql = "SELECT u.email, COUNT(p.id) AS total \
                   FROM prod_pg.public.users u \
                   JOIN app_mysql.mydb.profile p ON p.user_id = u.id \
                   WHERE u.active = true AND u.age >= 18 AND p.status IN ('gold', 'silver') AND p.deleted_at IS NULL \
                   GROUP BY u.email \
                   ORDER BY total DESC";
        let alias_map = test_alias_map();
        let plan = build_plan(sql, &alias_map, Some(1000), false).unwrap();

        assert_eq!(
            plan.sources[0].columns,
            Some(vec![
                "email".to_string(),
                "id".to_string(),
                "active".to_string(),
                "age".to_string(),
            ])
        );
        assert_eq!(
            plan.sources[0].pushdown_predicates,
            vec!["\"active\" = TRUE", "\"age\" >= 18"]
        );
        let pg_sql = build_source_query(&plan.sources[0]);
        assert_eq!(
            pg_sql,
            "SELECT \"email\", \"id\", \"active\", \"age\" FROM \"users\" WHERE \"active\" = TRUE AND \"age\" >= 18 LIMIT 1000"
        );

        assert_eq!(
            plan.sources[1].columns,
            Some(vec![
                "id".to_string(),
                "user_id".to_string(),
                "status".to_string(),
                "deleted_at".to_string(),
            ])
        );
        assert_eq!(
            plan.sources[1].pushdown_predicates,
            vec![
                "`status` IN ('gold', 'silver')",
                "`deleted_at` IS NULL"
            ]
        );
        let mysql_sql = build_source_query(&plan.sources[1]);
        assert_eq!(
            mysql_sql,
            "SELECT `id`, `user_id`, `status`, `deleted_at` FROM `profile` WHERE `status` IN ('gold', 'silver') AND `deleted_at` IS NULL LIMIT 1000"
        );

        // Full WHERE clause remains in duckdb_query as a safety net.
        assert!(plan.duckdb_query.contains("WHERE"));
        assert!(plan.duckdb_query.contains("u.active = true"));
    }

    #[test]
    fn wildcard_falls_back_to_select_star_while_preserving_pushdown() {
        let sql = "SELECT u.*, p.bio FROM prod_pg.public.users u JOIN app_mysql.mydb.profile p ON p.user_id = u.id WHERE u.country = 'FR'";
        let alias_map = test_alias_map();
        let plan = build_plan(sql, &alias_map, None, false).unwrap();

        assert_eq!(plan.sources[0].columns, None);
        assert_eq!(plan.sources[0].pushdown_predicates, vec!["\"country\" = 'FR'"]);
        assert_eq!(
            plan.sources[1].columns,
            Some(vec!["bio".to_string(), "user_id".to_string()])
        );
    }

    #[test]
    fn does_not_push_cross_source_or_outer_join_nullable_predicates() {
        let sql = "SELECT u.id FROM prod_pg.public.users u \
                   LEFT JOIN app_mysql.mydb.profile p ON p.user_id = u.id \
                   WHERE u.created_at > p.updated_at AND p.id IS NULL";
        let alias_map = test_alias_map();
        let plan = build_plan(sql, &alias_map, None, false).unwrap();

        assert!(plan.sources[0].pushdown_predicates.is_empty());
        assert!(plan.sources[1].pushdown_predicates.is_empty());
    }

    #[test]
    fn strips_pushdown_predicates_on_masked_columns() {
        use qore_core::masking::{MaskMode, MaskingRule};

        let sql = "SELECT u.email, u.country FROM prod_pg.public.users u WHERE u.email = 'alice@example.com' AND u.country = 'FR'";
        let alias_map = test_alias_map();
        let mut plan = build_plan(sql, &alias_map, None, false).unwrap();

        assert_eq!(plan.sources[0].pushdown_predicates.len(), 2);

        let masking = ConnectionMasking {
            rules: vec![MaskingRule {
                table: "users".to_string(),
                column: "email".to_string(),
                mode: MaskMode::Partial,
            }],
            mask_detected_columns: false,
        };
        strip_masked_predicates(&mut plan.sources[0], &masking);

        assert_eq!(plan.sources[0].pushdown_predicates, vec!["\"country\" = 'FR'"]);
    }
}

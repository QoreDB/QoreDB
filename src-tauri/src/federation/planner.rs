// SPDX-License-Identifier: BUSL-1.1

//! Federation query planner.
//!
//! Resolves connection aliases to session IDs, analyzes WHERE clause for pushdown
//! opportunities, and generates the final `FederationPlan`.

use std::collections::HashMap;

use crate::engine::error::{EngineError, EngineResult};

use super::parser::{build_dotted_name, parse_federation_refs, rewrite_query};
use super::types::{
    ConnectionAliasMap, FederatedTableRef, FederationPlan, SourceFetchPlan, DEFAULT_ROW_LIMIT,
};

/// Builds a `FederationPlan` from a user query.
///
/// 1. Parses the query to extract federated table references
/// 2. Resolves each connection alias to a session ID
/// 3. Generates source fetch plans with pushdown predicates
/// 4. Rewrites the query for DuckDB execution
pub fn build_plan(
    sql: &str,
    alias_map: &ConnectionAliasMap,
    row_limit: Option<u64>,
    streaming: bool,
) -> EngineResult<FederationPlan> {
    let known_aliases = alias_map.keys().cloned().collect();
    let federated_refs = parse_federation_refs(sql, &known_aliases)?;

    // Resolve aliases to sessions
    let sources = resolve_sources(&federated_refs, alias_map, row_limit)?;

    // Build the mapping for query rewriting: dotted_name -> local_alias
    let mappings = build_rewrite_mappings(&federated_refs);

    // Rewrite the query for DuckDB
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
    alias_map: &ConnectionAliasMap,
    row_limit: Option<u64>,
) -> EngineResult<Vec<SourceFetchPlan>> {
    let effective_limit = row_limit.unwrap_or(DEFAULT_ROW_LIMIT);
    let mut sources = Vec::with_capacity(refs.len());

    for table_ref in refs {
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

        sources.push(SourceFetchPlan {
            table_ref: table_ref.clone(),
            session_id: entry.session_id,
            driver_id: entry.driver_id.clone(),
            columns: None, // v1: fetch all columns (SELECT *)
            pushdown_predicates: Vec::new(), // v1: no pushdown (conservative)
            row_limit: effective_limit,
        });
    }

    Ok(sources)
}

/// Builds the rewrite mapping from original dotted names to local DuckDB aliases.
fn build_rewrite_mappings(refs: &[FederatedTableRef]) -> HashMap<String, String> {
    let mut mappings = HashMap::new();
    for r in refs {
        // Build dotted name matching how it appears in the SQL
        let dotted = build_dotted_name(&[
            r.connection_alias.clone(),
            if let Some(ref schema) = r.namespace.schema {
                // 4-part: alias.database.schema.table — map alias.database.schema
                // Actually for the rewrite, we match the full prefix before the table
                format!("{}.{}", r.namespace.database, schema)
            } else {
                r.namespace.database.clone()
            },
            r.table.clone(),
        ]);
        mappings.insert(dotted, r.local_alias.clone());
    }
    mappings
}

/// Builds the source query to fetch data from a single source table.
///
/// Generates: `SELECT {columns} FROM {table} WHERE {predicates} LIMIT {limit}`
pub fn build_source_query(source: &SourceFetchPlan) -> String {
    let columns_clause = match &source.columns {
        Some(cols) if !cols.is_empty() => cols
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", "),
        _ => "*".to_string(),
    };

    let table_name = &source.table_ref.table;

    // For MongoDB, we use a different query format
    if source.driver_id == "mongodb" {
        return build_mongo_source_query(source);
    }

    let mut sql = format!("SELECT {columns_clause} FROM \"{table_name}\"");

    if !source.pushdown_predicates.is_empty() {
        let where_clause = source.pushdown_predicates.join(" AND ");
        sql.push_str(&format!(" WHERE {where_clause}"));
    }

    sql.push_str(&format!(" LIMIT {}", source.row_limit));
    sql
}

/// Builds a MongoDB-style query for source fetching.
/// MongoDB uses JSON-style find queries rather than SQL.
fn build_mongo_source_query(source: &SourceFetchPlan) -> String {
    // MongoDB driver accepts simple find JSON queries
    // For v1 we do a simple find with limit
    let collection = &source.table_ref.table;

    if source.pushdown_predicates.is_empty() {
        format!("db.{collection}.find({{}}).limit({})", source.row_limit)
    } else {
        // Pushdown predicates would need MongoDB JSON translation
        // v1: no pushdown for MongoDB
        format!("db.{collection}.find({{}}).limit({})", source.row_limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::SessionId;
    use crate::federation::types::AliasEntry;

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
    fn mongo_source_query_format() {
        let sql = "SELECT * FROM analytics_mongo.analytics.events";
        let alias_map = test_alias_map();
        let plan = build_plan(sql, &alias_map, None, false).unwrap();

        let source_sql = build_source_query(&plan.sources[0]);
        assert!(source_sql.contains("db.events.find"));
        assert!(source_sql.contains(".limit(100000)"));
    }
}

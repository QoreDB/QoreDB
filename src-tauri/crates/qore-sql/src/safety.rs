// SPDX-License-Identifier: Apache-2.0

//! SQL safety classification for read-only and production enforcement.

use lru::LruCache;
use sqlparser::{
    ast::{Query, Select, SetExpr, Statement},
    dialect::{
        Dialect, DuckDbDialect, GenericDialect, MsSqlDialect, MySqlDialect, PostgreSqlDialect,
    },
    parser::Parser,
};
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqlSafetyAnalysis {
    pub is_mutation: bool,
    pub is_dangerous: bool,
}

type AnalyzeCache = Mutex<LruCache<(String, String), Result<SqlSafetyAnalysis, String>>>;
type ReturnsRowsCache = Mutex<LruCache<(String, String), Result<bool, String>>>;
type SplitCache = Mutex<LruCache<(String, String), Result<Vec<String>, String>>>;

/// Bounded cache of previously-analyzed (driver, trimmed SQL) pairs. sqlparser
/// is the dominant cost in `analyze_sql` (several ms for large queries) and
/// identical queries are re-run constantly during a session. 256 entries caps
/// memory at a few MB worst-case while covering typical editor/reuse patterns.
fn analyze_cache() -> &'static AnalyzeCache {
    static CACHE: OnceLock<AnalyzeCache> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(256).expect("non-zero capacity"),
        ))
    })
}

/// Cache for [`returns_rows`]. `query.rs` consults this on every streaming
/// command to decide whether to dispatch via the row-stream or the affected-
/// rows path; identical queries hit it repeatedly. Keyed identically to
/// `analyze_cache` so a query in the editor pays the parse cost only once
/// regardless of which entry-point the caller hits first.
fn returns_rows_cache() -> &'static ReturnsRowsCache {
    static CACHE: OnceLock<ReturnsRowsCache> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(256).expect("non-zero capacity"),
        ))
    })
}

/// Cache for [`split_sql_statements`]. Used when an editor pastes a multi-
/// statement script — the split result depends only on the dialect + SQL
/// string. Splits up to a few KB are common and re-runs (F5) frequent.
fn split_cache() -> &'static SplitCache {
    static CACHE: OnceLock<SplitCache> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(128).expect("non-zero capacity"),
        ))
    })
}

pub fn analyze_sql(driver_id: &str, sql: &str) -> Result<SqlSafetyAnalysis, String> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err("Empty SQL".to_string());
    }

    let cache_key = (driver_id.to_string(), trimmed.to_string());
    if let Ok(mut cache) = analyze_cache().lock() {
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
    }

    let result = analyze_sql_uncached(driver_id, trimmed);

    if let Ok(mut cache) = analyze_cache().lock() {
        cache.put(cache_key, result.clone());
    }

    result
}

fn analyze_sql_uncached(driver_id: &str, trimmed: &str) -> Result<SqlSafetyAnalysis, String> {
    let dialect = dialect_for_driver(driver_id);
    let statements = Parser::parse_sql(&*dialect, trimmed).map_err(|err| err.to_string())?;

    let mut analysis = SqlSafetyAnalysis {
        is_mutation: false,
        is_dangerous: false,
    };

    for statement in statements {
        if is_mutation_statement(&statement) {
            analysis.is_mutation = true;
        }
        if is_dangerous_statement(&statement) {
            analysis.is_dangerous = true;
        }
    }

    Ok(analysis)
}

pub fn returns_rows(driver_id: &str, sql: &str) -> Result<bool, String> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err("Empty SQL".to_string());
    }

    let cache_key = (driver_id.to_string(), trimmed.to_string());
    if let Ok(mut cache) = returns_rows_cache().lock() {
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
    }

    let result = returns_rows_uncached(driver_id, trimmed);

    if let Ok(mut cache) = returns_rows_cache().lock() {
        cache.put(cache_key, result.clone());
    }

    result
}

fn returns_rows_uncached(driver_id: &str, trimmed: &str) -> Result<bool, String> {
    let dialect = dialect_for_driver(driver_id);
    let statements = Parser::parse_sql(&*dialect, trimmed).map_err(|err| err.to_string())?;

    let first = statements.first().ok_or_else(|| "Empty SQL".to_string())?;
    Ok(statement_returns_rows(first))
}

pub fn split_sql_statements(driver_id: &str, sql: &str) -> Result<Vec<String>, String> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err("Empty SQL".to_string());
    }

    let cache_key = (driver_id.to_string(), trimmed.to_string());
    if let Ok(mut cache) = split_cache().lock() {
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
    }

    let result = split_sql_statements_uncached(driver_id, trimmed);

    if let Ok(mut cache) = split_cache().lock() {
        cache.put(cache_key, result.clone());
    }

    result
}

fn split_sql_statements_uncached(driver_id: &str, trimmed: &str) -> Result<Vec<String>, String> {
    let dialect = dialect_for_driver(driver_id);
    let statements = Parser::parse_sql(&*dialect, trimmed).map_err(|err| err.to_string())?;

    let mut rendered = Vec::with_capacity(statements.len());
    for statement in statements {
        let statement_sql = statement.to_string();
        if !statement_sql.trim().is_empty() {
            rendered.push(statement_sql);
        }
    }

    Ok(rendered)
}

pub fn is_select_prefix(sql: &str) -> bool {
    let trimmed = sql.trim_start().to_ascii_uppercase();
    trimmed.starts_with("SELECT")
        || trimmed.starts_with("WITH")
        || trimmed.starts_with("SHOW")
        || trimmed.starts_with("EXPLAIN")
        || trimmed.starts_with("DESCRIBE")
}

fn dialect_for_driver(driver_id: &str) -> Box<dyn Dialect> {
    if driver_id.eq_ignore_ascii_case("postgres") || driver_id.eq_ignore_ascii_case("cockroachdb") {
        Box::new(PostgreSqlDialect {})
    } else if driver_id.eq_ignore_ascii_case("mysql") {
        Box::new(MySqlDialect {})
    } else if driver_id.eq_ignore_ascii_case("duckdb") {
        Box::new(DuckDbDialect {})
    } else if driver_id.eq_ignore_ascii_case("sqlserver") || driver_id.eq_ignore_ascii_case("mssql")
    {
        Box::new(MsSqlDialect {})
    } else {
        Box::new(GenericDialect {})
    }
}

fn is_mutation_statement(statement: &Statement) -> bool {
    match statement {
        Statement::Query(query) => query_is_mutation(query),
        Statement::Explain {
            analyze, statement, ..
        } => {
            if *analyze {
                is_mutation_statement(statement)
            } else {
                false
            }
        }
        Statement::ExplainTable { .. }
        | Statement::ShowFunctions { .. }
        | Statement::ShowVariable { .. }
        | Statement::ShowStatus { .. }
        | Statement::ShowVariables { .. }
        | Statement::ShowCreate { .. }
        | Statement::ShowColumns { .. }
        | Statement::ShowDatabases { .. }
        | Statement::ShowSchemas { .. }
        | Statement::ShowCharset(_)
        | Statement::ShowObjects(_)
        | Statement::ShowTables { .. }
        | Statement::ShowViews { .. }
        | Statement::ShowCollation { .. }
        | Statement::Set(_)
        | Statement::Use(_)
        | Statement::StartTransaction { .. }
        | Statement::Commit { .. }
        | Statement::Rollback { .. }
        | Statement::Savepoint { .. }
        | Statement::ReleaseSavepoint { .. } => false,
        _ => true,
    }
}

fn statement_returns_rows(statement: &Statement) -> bool {
    matches!(
        statement,
        Statement::Query(_)
            | Statement::Explain { .. }
            | Statement::ExplainTable { .. }
            | Statement::ShowFunctions { .. }
            | Statement::ShowVariable { .. }
            | Statement::ShowStatus { .. }
            | Statement::ShowVariables { .. }
            | Statement::ShowCreate { .. }
            | Statement::ShowColumns { .. }
            | Statement::ShowDatabases { .. }
            | Statement::ShowSchemas { .. }
            | Statement::ShowCharset(_)
            | Statement::ShowObjects(_)
            | Statement::ShowTables { .. }
            | Statement::ShowViews { .. }
            | Statement::ShowCollation { .. }
    )
}

fn is_dangerous_statement(statement: &Statement) -> bool {
    match statement {
        Statement::Drop { .. }
        | Statement::DropFunction(_)
        | Statement::DropDomain(_)
        | Statement::DropProcedure { .. }
        | Statement::Truncate(_)
        | Statement::AlterTable(_)
        | Statement::AlterSchema(_)
        | Statement::AlterIndex { .. }
        | Statement::AlterView { .. }
        | Statement::AlterType(_)
        | Statement::AlterRole { .. }
        | Statement::AlterPolicy { .. }
        | Statement::AlterConnector { .. }
        | Statement::AlterSession { .. }
        | Statement::AlterUser(_) => true,
        Statement::Update(update) => update.selection.is_none(),
        Statement::Delete(delete) => delete.selection.is_none(),
        Statement::Explain {
            analyze, statement, ..
        } if *analyze => is_dangerous_statement(statement),
        _ => false,
    }
}

fn query_is_mutation(query: &Query) -> bool {
    set_expr_is_mutation(&query.body)
}

fn set_expr_is_mutation(expr: &SetExpr) -> bool {
    match expr {
        SetExpr::Select(select) => select_has_into(select),
        SetExpr::Query(query) => query_is_mutation(query),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_is_mutation(left) || set_expr_is_mutation(right)
        }
        SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Delete(_) | SetExpr::Merge(_) => true,
        SetExpr::Values(_) | SetExpr::Table(_) => false,
    }
}

fn select_has_into(select: &Select) -> bool {
    select.into.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_cte_select_is_read_only() {
        let analysis = analyze_sql(
            "postgres",
            "WITH cte AS (SELECT * FROM users) SELECT * FROM cte",
        )
        .expect("should parse");

        assert!(!analysis.is_mutation);
        assert!(!analysis.is_dangerous);
    }

    #[test]
    fn postgres_multi_statement_flags_mutation() {
        let analysis = analyze_sql(
            "postgres",
            "SELECT 1; UPDATE users SET name = 'x' WHERE id = 1;",
        )
        .expect("should parse");

        assert!(analysis.is_mutation);
        assert!(!analysis.is_dangerous);
    }

    #[test]
    fn postgres_update_without_where_is_dangerous() {
        let analysis =
            analyze_sql("postgres", "UPDATE users SET name = 'x'").expect("should parse");

        assert!(analysis.is_mutation);
        assert!(analysis.is_dangerous);
    }

    #[test]
    fn mysql_delete_without_where_is_dangerous() {
        let analysis = analyze_sql("mysql", "DELETE FROM users").expect("should parse");

        assert!(analysis.is_mutation);
        assert!(analysis.is_dangerous);
    }

    #[test]
    fn select_into_is_mutation() {
        let analysis = analyze_sql("postgres", "SELECT * INTO new_table FROM old_table")
            .expect("should parse");

        assert!(analysis.is_mutation);
        assert!(!analysis.is_dangerous);
    }

    #[test]
    fn alter_table_is_dangerous() {
        let analysis =
            analyze_sql("postgres", "ALTER TABLE users ADD COLUMN age INT").expect("should parse");

        assert!(analysis.is_mutation);
        assert!(analysis.is_dangerous);
    }

    #[test]
    fn mysql_show_tables_is_read_only() {
        let analysis = analyze_sql("mysql", "SHOW TABLES").expect("should parse");

        assert!(!analysis.is_mutation);
        assert!(!analysis.is_dangerous);
    }

    #[test]
    fn splits_postgres_multi_statement() {
        let statements = split_sql_statements(
            "postgres",
            "CREATE TABLE a (id INT); CREATE TABLE b (id INT);",
        )
        .expect("should parse");

        assert_eq!(statements.len(), 2);
        assert!(statements[0]
            .to_ascii_uppercase()
            .starts_with("CREATE TABLE"));
        assert!(statements[1]
            .to_ascii_uppercase()
            .starts_with("CREATE TABLE"));
    }
}

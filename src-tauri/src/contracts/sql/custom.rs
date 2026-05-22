// SPDX-License-Identifier: BUSL-1.1

//! Custom-SQL rule.
//!
//! The user-supplied SQL is treated as an assertion: it must return zero
//! rows for the rule to pass. We wrap it as a subquery so the metric
//! query always has a single COUNT row.
//!
//! Before wrapping, the user SQL is parsed and forced to be **exactly one
//! `SELECT`/CTE statement**. Without this gate, `custom_sql: "DROP TABLE
//! users; SELECT 1"` would slip through as soon as the driver allows the
//! Simple Query protocol (PostgreSQL does, by design) — cf. audit B7-C4.

use sqlparser::ast::Statement;
use sqlparser::dialect::{
    ClickHouseDialect, Dialect as SqlparserDialect, DuckDbDialect, GenericDialect, MsSqlDialect,
    MySqlDialect, PostgreSqlDialect, SQLiteDialect,
};
use sqlparser::parser::Parser;

use super::{dialect::Dialect, RuleSql, RuleSqlKind, SqlBuildError};

pub fn build_custom_sql(
    dialect: Dialect,
    user_sql: &str,
    sample_limit: u32,
) -> Result<RuleSql, SqlBuildError> {
    let trimmed = strip_trailing_semicolon(user_sql.trim());
    if trimmed.is_empty() {
        return Err(SqlBuildError::Invalid(
            "custom_sql must be non-empty".into(),
        ));
    }

    // Parse-and-validate: must be exactly one statement, and that statement
    // must be a Query (SELECT / WITH ... SELECT). Anything else — DROP,
    // UPDATE, multi-statement scripts — is rejected.
    let parser_dialect = parser_dialect_for(dialect);
    let parsed = Parser::parse_sql(parser_dialect.as_ref(), trimmed)
        .map_err(|e| SqlBuildError::Invalid(format!("custom_sql failed to parse: {e}")))?;
    match parsed.as_slice() {
        [Statement::Query(_)] => {}
        [_only_one] => {
            return Err(SqlBuildError::Invalid(
                "custom_sql must be a single SELECT (DDL/DML statements are not allowed)".into(),
            ));
        }
        _ => {
            return Err(SqlBuildError::Invalid(
                "custom_sql must contain a single statement".into(),
            ));
        }
    }

    let metric_query = format!("SELECT count(*) AS violations FROM ({trimmed}) AS contract_sub");
    let samples_query = Some(match dialect {
        Dialect::SqlServer => {
            format!("SELECT TOP {sample_limit} * FROM ({trimmed}) AS contract_sub")
        }
        _ => format!("SELECT * FROM ({trimmed}) AS contract_sub LIMIT {sample_limit}"),
    });
    Ok(RuleSql {
        kind: RuleSqlKind::CustomViolations,
        metric_query,
        samples_query,
    })
}

fn parser_dialect_for(dialect: Dialect) -> Box<dyn SqlparserDialect> {
    match dialect {
        Dialect::Postgres => Box::new(PostgreSqlDialect {}),
        Dialect::MySql => Box::new(MySqlDialect {}),
        Dialect::Sqlite => Box::new(SQLiteDialect {}),
        Dialect::DuckDb => Box::new(DuckDbDialect {}),
        Dialect::SqlServer => Box::new(MsSqlDialect {}),
        Dialect::ClickHouse => Box::new(ClickHouseDialect {}),
    }
}

#[allow(dead_code)]
fn parser_dialect_generic() -> Box<dyn SqlparserDialect> {
    Box::new(GenericDialect {})
}

fn strip_trailing_semicolon(s: &str) -> &str {
    s.trim_end().trim_end_matches(';').trim_end()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_user_sql() {
        let r = build_custom_sql(
            Dialect::Postgres,
            "SELECT id FROM orders WHERE amount < 0",
            10,
        )
        .unwrap();
        assert!(r
            .metric_query
            .contains("SELECT count(*) AS violations FROM ("));
        assert!(r
            .metric_query
            .contains("SELECT id FROM orders WHERE amount < 0"));
        assert!(r.samples_query.unwrap().contains("LIMIT 10"));
    }

    #[test]
    fn strips_semicolon() {
        let r = build_custom_sql(Dialect::Postgres, "SELECT 1 ;  ", 5).unwrap();
        assert!(!r.metric_query.contains("; )"));
    }

    #[test]
    fn mssql_uses_top() {
        let r = build_custom_sql(Dialect::SqlServer, "SELECT 1", 5).unwrap();
        assert!(r.samples_query.unwrap().contains("TOP 5"));
    }

    #[test]
    fn rejects_empty() {
        assert!(build_custom_sql(Dialect::Postgres, "  ;  ", 5).is_err());
    }

    #[test]
    fn rejects_drop_statement() {
        let err = build_custom_sql(Dialect::Postgres, "DROP TABLE users", 5).unwrap_err();
        assert!(matches!(err, SqlBuildError::Invalid(_)));
    }

    #[test]
    fn rejects_multi_statement_with_destructive_first() {
        let err = build_custom_sql(Dialect::Postgres, "DROP TABLE users; SELECT 1", 5).unwrap_err();
        assert!(matches!(err, SqlBuildError::Invalid(_)));
    }

    #[test]
    fn rejects_multi_statement_select_then_select() {
        let err = build_custom_sql(Dialect::Postgres, "SELECT 1; SELECT 2", 5).unwrap_err();
        assert!(matches!(err, SqlBuildError::Invalid(_)));
    }

    #[test]
    fn accepts_cte() {
        build_custom_sql(
            Dialect::Postgres,
            "WITH bad AS (SELECT id FROM orders WHERE amount < 0) SELECT * FROM bad",
            5,
        )
        .unwrap();
    }

    #[test]
    fn rejects_update_statement() {
        let err =
            build_custom_sql(Dialect::Postgres, "UPDATE users SET name = 'x'", 5).unwrap_err();
        assert!(matches!(err, SqlBuildError::Invalid(_)));
    }
}

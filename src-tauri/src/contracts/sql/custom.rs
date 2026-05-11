// SPDX-License-Identifier: BUSL-1.1

//! Custom-SQL rule.
//!
//! The user-supplied SQL is treated as an assertion: it must return zero
//! rows for the rule to pass. We wrap it as a subquery so the metric
//! query always has a single COUNT row.

use super::{dialect::Dialect, RuleSql, RuleSqlKind, SqlBuildError};

pub fn build_custom_sql(
    dialect: Dialect,
    user_sql: &str,
    sample_limit: u32,
) -> Result<RuleSql, SqlBuildError> {
    let trimmed = strip_trailing_semicolon(user_sql.trim());
    if trimmed.is_empty() {
        return Err(SqlBuildError::Invalid("custom_sql must be non-empty".into()));
    }
    let metric_query = format!("SELECT count(*) AS violations FROM ({trimmed}) AS contract_sub");
    let samples_query = Some(match dialect {
        Dialect::SqlServer => format!(
            "SELECT TOP {sample_limit} * FROM ({trimmed}) AS contract_sub"
        ),
        _ => format!(
            "SELECT * FROM ({trimmed}) AS contract_sub LIMIT {sample_limit}"
        ),
    });
    Ok(RuleSql {
        kind: RuleSqlKind::CustomViolations,
        metric_query,
        samples_query,
    })
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
        assert!(r.metric_query.contains("SELECT count(*) AS violations FROM ("));
        assert!(r.metric_query.contains("SELECT id FROM orders WHERE amount < 0"));
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
}

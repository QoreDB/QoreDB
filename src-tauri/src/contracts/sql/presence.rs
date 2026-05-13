// SPDX-License-Identifier: BUSL-1.1

//! Presence rules: `not_null_pct`, `not_empty`.

use super::{dialect::Dialect, RuleSql, RuleSqlKind, SqlBuildError};

/// `metric_value` is the percentage of non-NULL rows.
pub fn build_not_null_pct(
    _dialect: Dialect,
    table_sql: &str,
    column_sql: &str,
    sample_limit: u32,
) -> Result<RuleSql, SqlBuildError> {
    let metric_query = format!(
        "SELECT \
         CASE WHEN count(*) = 0 THEN 100.0 \
         ELSE count({column_sql}) * 100.0 / count(*) END AS metric_value \
         FROM {table_sql}"
    );
    let samples_query = Some(format!(
        "SELECT * FROM {table_sql} WHERE {column_sql} IS NULL LIMIT {sample_limit}"
    ));
    Ok(RuleSql {
        kind: RuleSqlKind::SingleMetric,
        metric_query,
        samples_query,
    })
}

/// `not_empty`: violation = NULL OR (text and = '').
pub fn build_not_empty(
    dialect: Dialect,
    table_sql: &str,
    column_sql: &str,
    sample_limit: u32,
) -> Result<RuleSql, SqlBuildError> {
    let _ = dialect;
    let predicate = format!("({column_sql} IS NULL OR {column_sql} = '')");
    let metric_query = format!(
        "SELECT (SELECT count(*) FROM {table_sql} WHERE {predicate}) AS violations, \
         (SELECT count(*) FROM {table_sql}) AS total"
    );
    let samples_query = Some(format!(
        "SELECT * FROM {table_sql} WHERE {predicate} LIMIT {sample_limit}"
    ));
    Ok(RuleSql {
        kind: RuleSqlKind::ViolationsCount,
        metric_query,
        samples_query,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_null_pct_postgres() {
        let r = build_not_null_pct(Dialect::Postgres, "\"orders\"", "\"email\"", 10).unwrap();
        assert!(r.metric_query.contains("count(\"email\") * 100.0 / count(*)"));
        assert!(r.metric_query.contains("FROM \"orders\""));
        assert!(r.samples_query.unwrap().contains("WHERE \"email\" IS NULL LIMIT 10"));
    }

    #[test]
    fn not_empty_emits_or_predicate() {
        let r = build_not_empty(Dialect::Postgres, "\"orders\"", "\"name\"", 5).unwrap();
        assert!(r.metric_query.contains("\"name\" IS NULL OR \"name\" = ''"));
        assert!(matches!(r.kind, RuleSqlKind::ViolationsCount));
    }
}

// SPDX-License-Identifier: BUSL-1.1

//! Uniqueness rules: `unique`, `distinct_count`.

use super::{dialect::Dialect, RuleSql, RuleSqlKind, SqlBuildError};

pub fn build_unique(
    dialect: Dialect,
    table_sql: &str,
    column_sqls: &[String],
    sample_limit: u32,
) -> Result<RuleSql, SqlBuildError> {
    if column_sqls.is_empty() {
        return Err(SqlBuildError::Invalid("unique requires at least one column".into()));
    }
    let group_by = column_sqls.join(", ");
    let metric_query = format!(
        "SELECT count(*) AS violations FROM (\
         SELECT {group_by} FROM {table_sql} \
         WHERE {} \
         GROUP BY {group_by} \
         HAVING count(*) > 1\
         ) sub",
        not_null_predicate(column_sqls)
    );
    let samples_query = Some(format!(
        "SELECT {group_by}, count(*) AS dup_count FROM {table_sql} \
         WHERE {pred} \
         GROUP BY {group_by} \
         HAVING count(*) > 1 \
         {limit}",
        pred = not_null_predicate(column_sqls),
        limit = limit_clause(dialect, sample_limit)
    ));
    Ok(RuleSql {
        kind: RuleSqlKind::ViolationsCount,
        metric_query,
        samples_query,
    })
}

pub fn build_distinct_count(
    _dialect: Dialect,
    table_sql: &str,
    column_sql: &str,
) -> Result<RuleSql, SqlBuildError> {
    let metric_query = format!(
        "SELECT count(DISTINCT {column_sql}) AS metric_value FROM {table_sql}"
    );
    Ok(RuleSql {
        kind: RuleSqlKind::SingleMetric,
        metric_query,
        samples_query: None,
    })
}

fn not_null_predicate(cols: &[String]) -> String {
    cols.iter()
        .map(|c| format!("{c} IS NOT NULL"))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn limit_clause(dialect: Dialect, n: u32) -> String {
    match dialect {
        Dialect::SqlServer => String::new(), // TOP must be in SELECT, handled by caller
        _ => format!("LIMIT {n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_single_column() {
        let r = build_unique(Dialect::Postgres, "\"t\"", &["\"id\"".into()], 10).unwrap();
        assert!(r.metric_query.contains("GROUP BY \"id\""));
        assert!(r.metric_query.contains("HAVING count(*) > 1"));
        assert!(r.metric_query.contains("\"id\" IS NOT NULL"));
    }

    #[test]
    fn unique_composite() {
        let r = build_unique(
            Dialect::Postgres,
            "\"t\"",
            &["\"a\"".into(), "\"b\"".into()],
            10,
        )
        .unwrap();
        assert!(r.metric_query.contains("GROUP BY \"a\", \"b\""));
        assert!(r.metric_query.contains("\"a\" IS NOT NULL AND \"b\" IS NOT NULL"));
    }

    #[test]
    fn distinct_count_returns_metric() {
        let r = build_distinct_count(Dialect::Postgres, "\"t\"", "\"name\"").unwrap();
        assert!(r.metric_query.contains("count(DISTINCT \"name\")"));
        assert!(matches!(r.kind, RuleSqlKind::SingleMetric));
        assert!(r.samples_query.is_none());
    }
}

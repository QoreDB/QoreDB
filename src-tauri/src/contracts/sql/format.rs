// SPDX-License-Identifier: BUSL-1.1

//! Format rules: `regex_match`, `length_range`.

use super::{dialect::Dialect, RuleSql, RuleSqlKind, SqlBuildError};

pub fn build_regex_match(
    dialect: Dialect,
    table_sql: &str,
    column_sql: &str,
    pattern: &str,
    sample_limit: u32,
) -> Result<RuleSql, SqlBuildError> {
    let violation_pred = dialect
        .regex_predicate(column_sql, pattern, true)
        .ok_or_else(|| SqlBuildError::UnsupportedOnDialect("regex_match", driver_label(dialect)))?;
    let predicate = format!("({column_sql} IS NOT NULL AND {violation_pred})");
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

pub fn build_length_range(
    dialect: Dialect,
    table_sql: &str,
    column_sql: &str,
    min: Option<i64>,
    max: Option<i64>,
    sample_limit: u32,
) -> Result<RuleSql, SqlBuildError> {
    let len_fn = dialect.length_fn();
    let len_expr = format!("{len_fn}({column_sql})");
    let mut parts = Vec::new();
    if let Some(mn) = min {
        parts.push(format!("{len_expr} < {mn}"));
    }
    if let Some(mx) = max {
        parts.push(format!("{len_expr} > {mx}"));
    }
    if parts.is_empty() {
        return Err(SqlBuildError::Invalid(
            "length_range requires at least one of min/max".into(),
        ));
    }
    let violation = parts.join(" OR ");
    let predicate = format!("({column_sql} IS NOT NULL AND ({violation}))");
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

fn driver_label(d: Dialect) -> &'static str {
    match d {
        Dialect::Postgres => "Postgres family",
        Dialect::MySql => "MySQL/MariaDB",
        Dialect::Sqlite => "SQLite",
        Dialect::DuckDb => "DuckDB",
        Dialect::SqlServer => "SQL Server",
        Dialect::ClickHouse => "ClickHouse",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regex_postgres() {
        let r =
            build_regex_match(Dialect::Postgres, "\"orders\"", "\"email\"", "^[a-z]", 10).unwrap();
        assert!(r.metric_query.contains("\"email\" !~ '^[a-z]'"));
    }

    #[test]
    fn regex_sqlite_unsupported() {
        let err = build_regex_match(Dialect::Sqlite, "\"t\"", "\"c\"", "x", 5).unwrap_err();
        assert!(matches!(err, SqlBuildError::UnsupportedOnDialect("regex_match", _)));
    }

    #[test]
    fn length_range_both_bounds() {
        let r = build_length_range(Dialect::Postgres, "\"t\"", "\"c\"", Some(3), Some(50), 10)
            .unwrap();
        assert!(r.metric_query.contains("length(\"c\") < 3"));
        assert!(r.metric_query.contains("length(\"c\") > 50"));
    }

    #[test]
    fn length_range_mysql_uses_char_length() {
        let r = build_length_range(Dialect::MySql, "`t`", "`c`", Some(1), None, 5).unwrap();
        assert!(r.metric_query.contains("CHAR_LENGTH(`c`) < 1"));
    }

    #[test]
    fn length_range_mssql_uses_len() {
        let r = build_length_range(Dialect::SqlServer, "[t]", "[c]", None, Some(10), 5).unwrap();
        assert!(r.metric_query.contains("LEN([c]) > 10"));
    }
}

// SPDX-License-Identifier: BUSL-1.1

//! Domain rules: `numeric_range`, `date_range`, `allowed_values`.

use crate::contracts::AllowedValue;

use super::{
    dialect::Dialect,
    literal::{format_allowed_value, format_number},
    RuleSql, RuleSqlKind, SqlBuildError,
};

pub fn build_numeric_range(
    dialect: Dialect,
    table_sql: &str,
    column_sql: &str,
    min: Option<f64>,
    max: Option<f64>,
    inclusive_min: Option<bool>,
    inclusive_max: Option<bool>,
    sample_limit: u32,
) -> Result<RuleSql, SqlBuildError> {
    let mut parts = Vec::new();
    if let Some(mn) = min {
        let lit =
            format_number(mn).ok_or_else(|| SqlBuildError::Invalid("min not finite".into()))?;
        let op = if inclusive_min.unwrap_or(true) {
            "<"
        } else {
            "<="
        };
        parts.push(format!("{column_sql} {op} {lit}"));
    }
    if let Some(mx) = max {
        let lit =
            format_number(mx).ok_or_else(|| SqlBuildError::Invalid("max not finite".into()))?;
        let op = if inclusive_max.unwrap_or(true) {
            ">"
        } else {
            ">="
        };
        parts.push(format!("{column_sql} {op} {lit}"));
    }
    if parts.is_empty() {
        return Err(SqlBuildError::Invalid(
            "numeric_range requires at least one bound".into(),
        ));
    }
    let predicate = format!("({column_sql} IS NOT NULL AND ({}))", parts.join(" OR "));
    Ok(violation_query(
        dialect,
        table_sql,
        &predicate,
        sample_limit,
    ))
}

pub fn build_date_range(
    dialect: Dialect,
    table_sql: &str,
    column_sql: &str,
    min: Option<&str>,
    max: Option<&str>,
    max_age: Option<&str>,
    sample_limit: u32,
) -> Result<RuleSql, SqlBuildError> {
    let mut parts = Vec::new();
    if let Some(s) = min {
        parts.push(format!("{column_sql} < {}", dialect.quote_string(s)));
    }
    if let Some(s) = max {
        parts.push(format!("{column_sql} > {}", dialect.quote_string(s)));
    }
    if let Some(age) = max_age {
        let (amount, unit) = split_duration(age)?;
        let threshold = dialect.now_minus_duration(amount, unit).ok_or_else(|| {
            SqlBuildError::Invalid(format!("max_age unsupported on this dialect: {age}"))
        })?;
        parts.push(format!("{column_sql} < {threshold}"));
    }
    if parts.is_empty() {
        return Err(SqlBuildError::Invalid(
            "date_range requires at least one of min/max/max_age".into(),
        ));
    }
    let predicate = format!("({column_sql} IS NOT NULL AND ({}))", parts.join(" OR "));
    Ok(violation_query(
        dialect,
        table_sql,
        &predicate,
        sample_limit,
    ))
}

pub fn build_allowed_values(
    dialect: Dialect,
    table_sql: &str,
    column_sql: &str,
    values: &[AllowedValue],
    sample_limit: u32,
) -> Result<RuleSql, SqlBuildError> {
    if values.is_empty() {
        return Err(SqlBuildError::Invalid(
            "allowed_values cannot be empty".into(),
        ));
    }
    let allow_null = values.iter().any(|v| matches!(v, AllowedValue::Null));
    let non_null: Vec<String> = values
        .iter()
        .filter(|v| !matches!(v, AllowedValue::Null))
        .map(|v| {
            format_allowed_value(dialect, v)
                .ok_or_else(|| SqlBuildError::Invalid("invalid allowed value".into()))
        })
        .collect::<Result<_, _>>()?;

    let predicate = match (non_null.is_empty(), allow_null) {
        (true, true) => format!("({column_sql} IS NOT NULL)"),
        (true, false) => unreachable!("we checked non-empty above"),
        (false, true) => format!(
            "({column_sql} IS NOT NULL AND {column_sql} NOT IN ({}))",
            non_null.join(", ")
        ),
        (false, false) => format!(
            "({column_sql} IS NULL OR {column_sql} NOT IN ({}))",
            non_null.join(", ")
        ),
    };
    Ok(violation_query(
        dialect,
        table_sql,
        &predicate,
        sample_limit,
    ))
}

fn violation_query(
    _dialect: Dialect,
    table_sql: &str,
    predicate: &str,
    sample_limit: u32,
) -> RuleSql {
    let metric_query = format!(
        "SELECT (SELECT count(*) FROM {table_sql} WHERE {predicate}) AS violations, \
         (SELECT count(*) FROM {table_sql}) AS total"
    );
    let samples_query = Some(format!(
        "SELECT * FROM {table_sql} WHERE {predicate} LIMIT {sample_limit}"
    ));
    RuleSql {
        kind: RuleSqlKind::ViolationsCount,
        metric_query,
        samples_query,
    }
}

fn split_duration(s: &str) -> Result<(u64, &str), SqlBuildError> {
    let cut = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num, unit) = s.split_at(cut);
    let n: u64 = num
        .parse()
        .map_err(|_| SqlBuildError::Invalid(format!("bad duration {s}")))?;
    if !matches!(unit, "ms" | "s" | "m" | "h" | "d") {
        return Err(SqlBuildError::Invalid(format!("bad duration unit {unit}")));
    }
    Ok((n, unit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_range_inclusive_default() {
        let r = build_numeric_range(
            Dialect::Postgres,
            "\"t\"",
            "\"x\"",
            Some(0.0),
            Some(100.0),
            None,
            None,
            10,
        )
        .unwrap();
        assert!(r.metric_query.contains("\"x\" < 0"));
        assert!(r.metric_query.contains("\"x\" > 100"));
    }

    #[test]
    fn numeric_range_exclusive_min() {
        let r = build_numeric_range(
            Dialect::Postgres,
            "\"t\"",
            "\"x\"",
            Some(0.0),
            None,
            Some(false),
            None,
            10,
        )
        .unwrap();
        assert!(r.metric_query.contains("\"x\" <= 0"));
    }

    #[test]
    fn date_range_with_max_age() {
        let r = build_date_range(
            Dialect::Postgres,
            "\"t\"",
            "\"updated_at\"",
            None,
            None,
            Some("7d"),
            10,
        )
        .unwrap();
        assert!(r.metric_query.contains("INTERVAL '7 day'"));
    }

    #[test]
    fn allowed_values_basic() {
        let r = build_allowed_values(
            Dialect::Postgres,
            "\"t\"",
            "\"status\"",
            &[
                AllowedValue::Text("pending".into()),
                AllowedValue::Text("paid".into()),
            ],
            10,
        )
        .unwrap();
        assert!(r
            .metric_query
            .contains("\"status\" NOT IN ('pending', 'paid')"));
        assert!(r.metric_query.contains("\"status\" IS NULL OR"));
    }

    #[test]
    fn allowed_values_with_null() {
        let r = build_allowed_values(
            Dialect::Postgres,
            "\"t\"",
            "\"status\"",
            &[AllowedValue::Text("ok".into()), AllowedValue::Null],
            10,
        )
        .unwrap();
        assert!(r
            .metric_query
            .contains("\"status\" IS NOT NULL AND \"status\" NOT IN ('ok')"));
    }

    #[test]
    fn split_duration_parses() {
        assert_eq!(split_duration("7d").unwrap(), (7, "d"));
        assert_eq!(split_duration("30m").unwrap(), (30, "m"));
        assert!(split_duration("7days").is_err());
        assert!(split_duration("xd").is_err());
    }
}

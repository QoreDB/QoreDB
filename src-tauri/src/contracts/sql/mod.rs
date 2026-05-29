// SPDX-License-Identifier: BUSL-1.1

//! SQL generation for contract rules.
//!
//! Each rule type lives in its own submodule and exposes a builder that
//! takes a `Dialect`, an already-quoted table reference, and the rule's
//! arguments. `build_rule_sql` is the single entry point used by the
//! runner and the `preview_rule_sql` Tauri command.

pub mod cardinality;
pub mod custom;
pub mod dialect;
pub mod domain;
pub mod format;
pub mod literal;
pub mod presence;
pub mod referential;
pub mod uniqueness;

use thiserror::Error;

use super::{ContractTarget, Rule};
use dialect::Dialect;

pub const DEFAULT_SAMPLE_LIMIT: u32 = 10;

#[derive(Debug, Error)]
pub enum SqlBuildError {
    #[error("unknown driver id: {0}")]
    UnknownDialect(String),
    #[error("rule {0} is not supported on {1}")]
    UnsupportedOnDialect(&'static str, &'static str),
    #[error("invalid rule arguments: {0}")]
    Invalid(String),
}

/// How the runner should interpret `metric_query` columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSqlKind {
    /// Returns `(violations BIGINT, total BIGINT)`. Pass if violations == 0.
    ViolationsCount,
    /// Returns a single `metric_value` column compared to rule bounds.
    SingleMetric,
    /// Returns `(violations BIGINT)`. Used by custom_sql.
    CustomViolations,
}

#[derive(Debug, Clone)]
pub struct RuleSql {
    pub kind: RuleSqlKind,
    pub metric_query: String,
    pub samples_query: Option<String>,
}

pub fn build_rule_sql(
    rule: &Rule,
    target: &ContractTarget,
    dialect: Dialect,
    sample_limit: u32,
) -> Result<RuleSql, SqlBuildError> {
    let table_sql = dialect.qualified_table(target.schema.as_deref(), &target.table);
    match rule {
        Rule::NotNullPct { column, .. } => {
            let col = dialect.quote_ident(column);
            presence::build_not_null_pct(dialect, &table_sql, &col, sample_limit)
        }
        Rule::NotEmpty { column, .. } => {
            let col = dialect.quote_ident(column);
            presence::build_not_empty(dialect, &table_sql, &col, sample_limit)
        }
        Rule::RegexMatch {
            column, pattern, ..
        } => {
            let col = dialect.quote_ident(column);
            format::build_regex_match(dialect, &table_sql, &col, pattern, sample_limit)
        }
        Rule::LengthRange {
            column, min, max, ..
        } => {
            let col = dialect.quote_ident(column);
            format::build_length_range(dialect, &table_sql, &col, *min, *max, sample_limit)
        }
        Rule::NumericRange {
            column,
            min,
            max,
            inclusive_min,
            inclusive_max,
            ..
        } => {
            let col = dialect.quote_ident(column);
            domain::build_numeric_range(
                dialect,
                &table_sql,
                &col,
                *min,
                *max,
                *inclusive_min,
                *inclusive_max,
                sample_limit,
            )
        }
        Rule::DateRange {
            column,
            min,
            max,
            max_age,
            ..
        } => {
            let col = dialect.quote_ident(column);
            domain::build_date_range(
                dialect,
                &table_sql,
                &col,
                min.as_deref(),
                max.as_deref(),
                max_age.as_deref(),
                sample_limit,
            )
        }
        Rule::AllowedValues { column, values, .. } => {
            let col = dialect.quote_ident(column);
            domain::build_allowed_values(dialect, &table_sql, &col, values, sample_limit)
        }
        Rule::Unique { columns, .. } => {
            let cols: Vec<String> = columns.iter().map(|c| dialect.quote_ident(c)).collect();
            uniqueness::build_unique(dialect, &table_sql, &cols, sample_limit)
        }
        Rule::DistinctCount { column, .. } => {
            let col = dialect.quote_ident(column);
            uniqueness::build_distinct_count(dialect, &table_sql, &col)
        }
        Rule::ForeignKeyIntegrity {
            column, references, ..
        } => {
            let src_col = dialect.quote_ident(column);
            let ref_table =
                dialect.qualified_table(references.schema.as_deref(), &references.table);
            let ref_col = dialect.quote_ident(&references.column);
            referential::build_foreign_key_integrity(
                dialect,
                &table_sql,
                &src_col,
                &ref_table,
                &ref_col,
                sample_limit,
            )
        }
        Rule::RowCount { .. } => cardinality::build_row_count(dialect, &table_sql),
        Rule::CustomSql { sql, .. } => custom::build_custom_sql(dialect, sql, sample_limit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{AllowedValue, ContractTarget, ForeignKeyReference};

    fn target() -> ContractTarget {
        ContractTarget {
            connection: "c".into(),
            schema: Some("public".into()),
            table: "orders".into(),
        }
    }

    #[test]
    fn dispatch_not_null_pct() {
        let rule = Rule::NotNullPct {
            id: "r".into(),
            description: None,
            severity: None,
            enabled: None,
            column: "email".into(),
            threshold_min_pct: 99.0,
        };
        let sql = build_rule_sql(&rule, &target(), Dialect::Postgres, 10).unwrap();
        assert!(sql.metric_query.contains("FROM \"public\".\"orders\""));
        assert!(sql.metric_query.contains("count(\"email\")"));
    }

    #[test]
    fn dispatch_unique() {
        let rule = Rule::Unique {
            id: "r".into(),
            description: None,
            severity: None,
            enabled: None,
            columns: vec!["a".into(), "b".into()],
        };
        let sql = build_rule_sql(&rule, &target(), Dialect::MySql, 10).unwrap();
        assert!(sql.metric_query.contains("GROUP BY `a`, `b`"));
        assert!(sql.metric_query.contains("FROM `public`.`orders`"));
    }

    #[test]
    fn dispatch_foreign_key() {
        let rule = Rule::ForeignKeyIntegrity {
            id: "r".into(),
            description: None,
            severity: None,
            enabled: None,
            column: "customer_id".into(),
            references: ForeignKeyReference {
                table: "customers".into(),
                column: "id".into(),
                schema: Some("public".into()),
            },
        };
        let sql = build_rule_sql(&rule, &target(), Dialect::Postgres, 5).unwrap();
        assert!(sql.metric_query.contains("\"public\".\"customers\""));
        assert!(sql
            .metric_query
            .contains("ref.\"id\" = src.\"customer_id\""));
    }

    #[test]
    fn dispatch_allowed_values_with_int() {
        let rule = Rule::AllowedValues {
            id: "r".into(),
            description: None,
            severity: None,
            enabled: None,
            column: "code".into(),
            values: vec![AllowedValue::Int(1), AllowedValue::Int(2)],
        };
        let sql = build_rule_sql(&rule, &target(), Dialect::Postgres, 10).unwrap();
        assert!(sql.metric_query.contains("\"code\" NOT IN (1, 2)"));
    }

    #[test]
    fn dispatch_custom_sql() {
        let rule = Rule::CustomSql {
            id: "r".into(),
            description: None,
            severity: None,
            enabled: None,
            sql: "SELECT id FROM \"public\".\"orders\" WHERE amount < 0".into(),
        };
        let sql = build_rule_sql(&rule, &target(), Dialect::Postgres, 10).unwrap();
        assert!(matches!(sql.kind, RuleSqlKind::CustomViolations));
        assert!(sql.metric_query.contains("amount < 0"));
    }

    #[test]
    fn dispatch_regex_unsupported_on_sqlite() {
        let rule = Rule::RegexMatch {
            id: "r".into(),
            description: None,
            severity: None,
            enabled: None,
            column: "email".into(),
            pattern: "^.+@.+$".into(),
        };
        let err = build_rule_sql(&rule, &target(), Dialect::Sqlite, 10).unwrap_err();
        assert!(matches!(
            err,
            SqlBuildError::UnsupportedOnDialect("regex_match", _)
        ));
    }

    #[test]
    fn dispatch_clickhouse_uses_backticks() {
        let rule = Rule::NotEmpty {
            id: "r".into(),
            description: None,
            severity: None,
            enabled: None,
            column: "status".into(),
        };
        let sql = build_rule_sql(&rule, &target(), Dialect::ClickHouse, 10).unwrap();
        assert!(sql.metric_query.contains("`public`.`orders`"));
        assert!(sql.metric_query.contains("`status`"));
    }
}

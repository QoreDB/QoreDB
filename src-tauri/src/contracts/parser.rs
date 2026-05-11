// SPDX-License-Identifier: BUSL-1.1

//! Server-side relecture of contract YAML/JSON.
//!
//! Defense-in-depth: even if the frontend validates, contracts on disk can be
//! edited externally. Same invariants as `src/lib/contracts/parser.ts`.

use std::collections::HashSet;

use regex::Regex;
use serde::Deserialize;
use thiserror::Error;

use super::{AllowedValue, Contract, Rule};

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("invalid contract: {0}")]
    Invalid(String),
    #[error("invalid YAML: {0}")]
    Yaml(#[from] serde_yml::Error),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Format {
    Yaml,
    Json,
    Auto,
}

pub fn parse_contract(source: &str, format: Format) -> Result<Contract, ContractError> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Err(ContractError::Invalid("source is empty".into()));
    }

    let contract: Contract = match format {
        Format::Json => serde_json::from_str(source)?,
        Format::Yaml => serde_yml::from_str(source)?,
        Format::Auto => {
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                match serde_json::from_str(source) {
                    Ok(c) => c,
                    Err(_) => serde_yml::from_str(source)?,
                }
            } else {
                serde_yml::from_str(source)?
            }
        }
    };

    validate(&contract)?;
    Ok(contract)
}

fn validate(contract: &Contract) -> Result<(), ContractError> {
    let ident = ident_regex();
    let duration = duration_regex();

    if !ident.is_match(&contract.name) {
        return Err(ContractError::Invalid(format!(
            "$.name: must match [A-Za-z_][A-Za-z0-9_]* (got {:?})",
            contract.name
        )));
    }
    if contract.version != 1 {
        return Err(ContractError::Invalid(format!(
            "$.version: unsupported version {} (expected 1)",
            contract.version
        )));
    }
    if contract.target.connection.is_empty() {
        return Err(ContractError::Invalid("$.target.connection: required".into()));
    }
    if contract.target.table.is_empty() {
        return Err(ContractError::Invalid("$.target.table: required".into()));
    }
    if contract.rules.is_empty() {
        return Err(ContractError::Invalid("$.rules: must be non-empty".into()));
    }

    let mut ids: HashSet<&str> = HashSet::new();
    for (i, rule) in contract.rules.iter().enumerate() {
        let path = format!("$.rules[{}]", i);
        let id = rule.id();
        if !ident.is_match(id) {
            return Err(ContractError::Invalid(format!(
                "{}.id: must match [A-Za-z_][A-Za-z0-9_]* (got {:?})",
                path, id
            )));
        }
        if !ids.insert(id) {
            return Err(ContractError::Invalid(format!(
                "{}.id: duplicate id {:?}",
                path, id
            )));
        }
        validate_rule(rule, &path, &duration)?;
    }

    Ok(())
}

fn validate_rule(rule: &Rule, path: &str, duration: &Regex) -> Result<(), ContractError> {
    match rule {
        Rule::NotNullPct {
            threshold_min_pct, ..
        } => {
            if !threshold_min_pct.is_finite()
                || *threshold_min_pct < 0.0
                || *threshold_min_pct > 100.0
            {
                return Err(ContractError::Invalid(format!(
                    "{}.threshold_min_pct: must be in [0, 100]",
                    path
                )));
            }
        }
        Rule::RegexMatch { pattern, .. } => {
            Regex::new(pattern).map_err(|e| {
                ContractError::Invalid(format!("{}.pattern: invalid regex: {}", path, e))
            })?;
        }
        Rule::LengthRange { min, max, .. } => check_int_range(*min, *max, path)?,
        Rule::NumericRange { min, max, .. } => check_float_range(*min, *max, path)?,
        Rule::DateRange {
            min,
            max,
            max_age,
            ..
        } => {
            if min.is_none() && max.is_none() && max_age.is_none() {
                return Err(ContractError::Invalid(format!(
                    "{}: provide at least one of min, max, max_age",
                    path
                )));
            }
            if let Some(age) = max_age {
                if !duration.is_match(age) {
                    return Err(ContractError::Invalid(format!(
                        "{}.max_age: must be <number><ms|s|m|h|d>",
                        path
                    )));
                }
            }
        }
        Rule::AllowedValues { values, .. } => {
            if values.is_empty() {
                return Err(ContractError::Invalid(format!(
                    "{}.values: must be non-empty",
                    path
                )));
            }
            for (i, v) in values.iter().enumerate() {
                if let AllowedValue::Float(f) = v {
                    if !f.is_finite() {
                        return Err(ContractError::Invalid(format!(
                            "{}.values[{}]: must be finite",
                            path, i
                        )));
                    }
                }
            }
        }
        Rule::Unique { columns, .. } => {
            if columns.is_empty() {
                return Err(ContractError::Invalid(format!(
                    "{}.columns: must be non-empty",
                    path
                )));
            }
        }
        Rule::DistinctCount { min, max, .. } => check_int_range(*min, *max, path)?,
        Rule::ForeignKeyIntegrity { references, .. } => {
            if references.table.is_empty() {
                return Err(ContractError::Invalid(format!(
                    "{}.references.table: required",
                    path
                )));
            }
            if references.column.is_empty() {
                return Err(ContractError::Invalid(format!(
                    "{}.references.column: required",
                    path
                )));
            }
        }
        Rule::RowCount { min, max, .. } => check_int_range(*min, *max, path)?,
        Rule::CustomSql { sql, .. } => {
            if sql.trim().is_empty() {
                return Err(ContractError::Invalid(format!("{}.sql: required", path)));
            }
        }
        Rule::NotEmpty { .. } => {}
    }
    Ok(())
}

fn check_int_range(min: Option<i64>, max: Option<i64>, path: &str) -> Result<(), ContractError> {
    if min.is_none() && max.is_none() {
        return Err(ContractError::Invalid(format!(
            "{}: provide at least one of min, max",
            path
        )));
    }
    if let (Some(a), Some(b)) = (min, max) {
        if a > b {
            return Err(ContractError::Invalid(format!(
                "{}: min ({}) must be <= max ({})",
                path, a, b
            )));
        }
    }
    Ok(())
}

fn check_float_range(
    min: Option<f64>,
    max: Option<f64>,
    path: &str,
) -> Result<(), ContractError> {
    if min.is_none() && max.is_none() {
        return Err(ContractError::Invalid(format!(
            "{}: provide at least one of min, max",
            path
        )));
    }
    if let Some(v) = min {
        if !v.is_finite() {
            return Err(ContractError::Invalid(format!("{}.min: must be finite", path)));
        }
    }
    if let Some(v) = max {
        if !v.is_finite() {
            return Err(ContractError::Invalid(format!("{}.max: must be finite", path)));
        }
    }
    if let (Some(a), Some(b)) = (min, max) {
        if a > b {
            return Err(ContractError::Invalid(format!(
                "{}: min ({}) must be <= max ({})",
                path, a, b
            )));
        }
    }
    Ok(())
}

fn ident_regex() -> Regex {
    Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").expect("static regex")
}

fn duration_regex() -> Regex {
    Regex::new(r"^\d+(ms|s|m|h|d)$").expect("static regex")
}

/// Loose envelope used to extract the `name` from a contract file without
/// parsing the rest. Useful for indexing the contracts directory.
#[derive(Deserialize)]
struct NameOnly {
    name: String,
}

pub fn extract_name(source: &str) -> Option<String> {
    serde_yml::from_str::<NameOnly>(source)
        .ok()
        .map(|n| n.name)
        .filter(|n| ident_regex().is_match(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_YAML: &str = r#"
name: orders_quality
version: 1
target:
  connection: prod-pg
  schema: public
  table: orders
rules:
  - id: id_unique
    type: unique
    columns: [id]
  - id: status_enum
    type: allowed_values
    column: status
    values: [pending, paid, shipped, refunded]
  - id: amount_positive
    type: numeric_range
    column: amount_cents
    min: 0
    inclusive_min: true
  - id: customer_fk
    type: foreign_key_integrity
    column: customer_id
    references: { table: customers, column: id }
"#;

    #[test]
    fn parses_canonical_yaml() {
        let c = parse_contract(VALID_YAML, Format::Auto).expect("valid");
        assert_eq!(c.name, "orders_quality");
        assert_eq!(c.version, 1);
        assert_eq!(c.target.connection, "prod-pg");
        assert_eq!(c.rules.len(), 4);
    }

    #[test]
    fn parses_json_form() {
        let json = serde_json::json!({
            "name": "x",
            "version": 1,
            "target": { "connection": "c", "table": "t" },
            "rules": [{ "id": "r1", "type": "not_empty", "column": "c1" }],
        })
        .to_string();
        let c = parse_contract(&json, Format::Auto).expect("valid");
        assert_eq!(c.name, "x");
    }

    #[test]
    fn rejects_empty_source() {
        let err = parse_contract("   ", Format::Auto).unwrap_err();
        assert!(matches!(err, ContractError::Invalid(_)));
    }

    #[test]
    fn rejects_bad_name() {
        let src = r#"
name: "1bad-name"
version: 1
target: { connection: c, table: t }
rules: [{ id: r1, type: not_empty, column: c1 }]
"#;
        let err = parse_contract(src, Format::Auto).unwrap_err();
        assert!(err.to_string().contains("$.name"));
    }

    #[test]
    fn rejects_unsupported_version() {
        let src = r#"
name: ok
version: 2
target: { connection: c, table: t }
rules: [{ id: r1, type: not_empty, column: c1 }]
"#;
        let err = parse_contract(src, Format::Auto).unwrap_err();
        assert!(err.to_string().contains("version"));
    }

    #[test]
    fn rejects_duplicate_rule_id() {
        let src = r#"
name: ok
version: 1
target: { connection: c, table: t }
rules:
  - { id: dup, type: not_empty, column: a }
  - { id: dup, type: not_empty, column: b }
"#;
        let err = parse_contract(src, Format::Auto).unwrap_err();
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn rejects_threshold_out_of_range() {
        let src = r#"
name: ok
version: 1
target: { connection: c, table: t }
rules:
  - { id: r1, type: not_null_pct, column: c1, threshold_min_pct: 150 }
"#;
        let err = parse_contract(src, Format::Auto).unwrap_err();
        assert!(err.to_string().contains("threshold_min_pct"));
    }

    #[test]
    fn rejects_invalid_regex() {
        let src = r#"
name: ok
version: 1
target: { connection: c, table: t }
rules:
  - { id: r1, type: regex_match, column: c1, pattern: "[" }
"#;
        let err = parse_contract(src, Format::Auto).unwrap_err();
        assert!(err.to_string().contains("regex"));
    }

    #[test]
    fn rejects_inverted_range() {
        let src = r#"
name: ok
version: 1
target: { connection: c, table: t }
rules:
  - { id: r1, type: numeric_range, column: c1, min: 100, max: 0 }
"#;
        let err = parse_contract(src, Format::Auto).unwrap_err();
        assert!(err.to_string().contains("min"));
    }

    #[test]
    fn rejects_range_without_bounds() {
        let src = r#"
name: ok
version: 1
target: { connection: c, table: t }
rules:
  - { id: r1, type: row_count }
"#;
        let err = parse_contract(src, Format::Auto).unwrap_err();
        assert!(err.to_string().contains("at least one"));
    }

    #[test]
    fn rejects_bad_max_age() {
        let src = r#"
name: ok
version: 1
target: { connection: c, table: t }
rules:
  - { id: r1, type: date_range, column: c1, max_age: "7days" }
"#;
        let err = parse_contract(src, Format::Auto).unwrap_err();
        assert!(err.to_string().contains("max_age"));
    }

    #[test]
    fn rejects_empty_unique_columns() {
        let src = r#"
name: ok
version: 1
target: { connection: c, table: t }
rules:
  - { id: r1, type: unique, columns: [] }
"#;
        let err = parse_contract(src, Format::Auto).unwrap_err();
        assert!(err.to_string().contains("columns"));
    }

    #[test]
    fn rejects_empty_allowed_values() {
        let src = r#"
name: ok
version: 1
target: { connection: c, table: t }
rules:
  - { id: r1, type: allowed_values, column: c1, values: [] }
"#;
        let err = parse_contract(src, Format::Auto).unwrap_err();
        assert!(err.to_string().contains("values"));
    }

    #[test]
    fn round_trip_json() {
        let c = parse_contract(VALID_YAML, Format::Auto).expect("valid");
        let json = serde_json::to_string(&c).expect("ser");
        let back = parse_contract(&json, Format::Json).expect("re-parse");
        assert_eq!(c, back);
    }

    #[test]
    fn extract_name_works() {
        assert_eq!(
            extract_name(VALID_YAML).as_deref(),
            Some("orders_quality")
        );
        assert!(extract_name("garbage").is_none());
    }

    #[test]
    fn rule_id_and_enabled_accessors() {
        let c = parse_contract(VALID_YAML, Format::Auto).expect("valid");
        assert_eq!(c.rules[0].id(), "id_unique");
        assert!(c.rules[0].enabled());
    }
}

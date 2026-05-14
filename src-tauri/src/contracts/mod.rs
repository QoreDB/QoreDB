// SPDX-License-Identifier: BUSL-1.1

//! Data Contracts (Pro) — declarative data-quality assertions.
//!
//! Mirror of `src/lib/contracts/types.ts`. Field names use snake_case to
//! match the canonical YAML/JSON serialization on disk
//! (`.qoredb/contracts/<name>.yml`).

#![cfg(feature = "pro")]

pub mod alert;
pub mod events;
pub mod parser;
pub mod runner;
pub mod sql;
pub mod storage;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractTarget {
    pub connection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub table: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForeignKeyReference {
    pub table: String,
    pub column: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AllowedValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Rule {
    NotNullPct {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        column: String,
        threshold_min_pct: f64,
    },
    NotEmpty {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        column: String,
    },
    RegexMatch {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        column: String,
        pattern: String,
    },
    LengthRange {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        column: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    NumericRange {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        column: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inclusive_min: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inclusive_max: Option<bool>,
    },
    DateRange {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        column: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_age: Option<String>,
    },
    AllowedValues {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        column: String,
        values: Vec<AllowedValue>,
    },
    Unique {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        columns: Vec<String>,
    },
    DistinctCount {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        column: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    ForeignKeyIntegrity {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        column: String,
        references: ForeignKeyReference,
    },
    RowCount {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    CustomSql {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        severity: Option<Severity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        sql: String,
    },
}

impl Rule {
    pub fn id(&self) -> &str {
        match self {
            Rule::NotNullPct { id, .. }
            | Rule::NotEmpty { id, .. }
            | Rule::RegexMatch { id, .. }
            | Rule::LengthRange { id, .. }
            | Rule::NumericRange { id, .. }
            | Rule::DateRange { id, .. }
            | Rule::AllowedValues { id, .. }
            | Rule::Unique { id, .. }
            | Rule::DistinctCount { id, .. }
            | Rule::ForeignKeyIntegrity { id, .. }
            | Rule::RowCount { id, .. }
            | Rule::CustomSql { id, .. } => id,
        }
    }

    pub fn enabled(&self) -> bool {
        let enabled = match self {
            Rule::NotNullPct { enabled, .. }
            | Rule::NotEmpty { enabled, .. }
            | Rule::RegexMatch { enabled, .. }
            | Rule::LengthRange { enabled, .. }
            | Rule::NumericRange { enabled, .. }
            | Rule::DateRange { enabled, .. }
            | Rule::AllowedValues { enabled, .. }
            | Rule::Unique { enabled, .. }
            | Rule::DistinctCount { enabled, .. }
            | Rule::ForeignKeyIntegrity { enabled, .. }
            | Rule::RowCount { enabled, .. }
            | Rule::CustomSql { enabled, .. } => enabled,
        };
        enabled.unwrap_or(true)
    }

    /// Discriminant string matching the YAML/JSON `type:` field and the
    /// TypeScript `RuleType` union (snake_case).
    pub fn rule_type(&self) -> &'static str {
        match self {
            Rule::NotNullPct { .. } => "not_null_pct",
            Rule::NotEmpty { .. } => "not_empty",
            Rule::RegexMatch { .. } => "regex_match",
            Rule::LengthRange { .. } => "length_range",
            Rule::NumericRange { .. } => "numeric_range",
            Rule::DateRange { .. } => "date_range",
            Rule::AllowedValues { .. } => "allowed_values",
            Rule::Unique { .. } => "unique",
            Rule::DistinctCount { .. } => "distinct_count",
            Rule::ForeignKeyIntegrity { .. } => "foreign_key_integrity",
            Rule::RowCount { .. } => "row_count",
            Rule::CustomSql { .. } => "custom_sql",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Contract {
    pub name: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub target: ContractTarget,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuleStatus {
    Pass,
    Fail,
    Skipped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleResult {
    pub id: String,
    pub rule_type: String,
    pub status: RuleStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub violations_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub samples: Option<Vec<serde_json::Value>>,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Index entry returned by `list_contracts`. `path` is the absolute path to
/// the YAML file; `id` is the canonical contract name (which is also the
/// filename without extension).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMeta {
    pub id: String,
    pub name: String,
    pub path: String,
    pub rules_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<ContractRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractRun {
    pub contract_id: String,
    pub contract_name: String,
    pub connection_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
    pub pass_count: u32,
    pub fail_count: u32,
    pub error_count: u32,
    pub results: Vec<RuleResult>,
}

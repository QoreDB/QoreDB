// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{CapabilityKind, PluginError};

pub const MAX_CELL_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_CELL_OUTPUT_BYTES: usize = 256 * 1024;
pub const MAX_TABLE_COLUMNS: usize = 64;
pub const MAX_TABLE_ROWS: usize = 1000;
pub const MAX_BADGE_LABEL_CHARS: usize = 64;

const CELL_HOOK_CAPABILITIES: [CapabilityKind; 2] = [CapabilityKind::Log, CapabilityKind::Storage];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CellHookKind {
    Action,
    Render,
}

impl CellHookKind {
    pub const fn export_name(self) -> &'static str {
        match self {
            CellHookKind::Action => "cell_action",
            CellHookKind::Render => "render_cell",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellColumn {
    pub name: String,
    #[serde(default)]
    pub data_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellInput {
    pub id: String,
    pub value: serde_json::Value,
    pub column: CellColumn,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BadgeTone {
    #[default]
    Neutral,
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CellOutput {
    Text {
        text: String,
    },
    Badge {
        label: String,
        #[serde(default)]
        tone: BadgeTone,
    },
    Json {
        value: serde_json::Value,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<serde_json::Value>>,
    },
}

pub fn encode_input(input: &CellInput) -> Result<Vec<u8>, PluginError> {
    let bytes = serde_json::to_vec(input).map_err(|e| PluginError::Abi(e.to_string()))?;
    if bytes.len() > MAX_CELL_INPUT_BYTES {
        return Err(PluginError::Abi(format!(
            "cell value exceeds {MAX_CELL_INPUT_BYTES} bytes"
        )));
    }
    Ok(bytes)
}

pub fn parse_output(bytes: &[u8]) -> Result<CellOutput, PluginError> {
    if bytes.len() > MAX_CELL_OUTPUT_BYTES {
        return Err(PluginError::Abi(format!(
            "cell output exceeds {MAX_CELL_OUTPUT_BYTES} bytes"
        )));
    }
    let output: CellOutput = serde_json::from_slice(bytes)
        .map_err(|e| PluginError::Abi(format!("invalid cell output: {e}")))?;
    validate_output(&output)?;
    Ok(output)
}

fn validate_output(output: &CellOutput) -> Result<(), PluginError> {
    match output {
        CellOutput::Badge { label, .. } => {
            let chars = label.trim().chars().count();
            if chars == 0 || chars > MAX_BADGE_LABEL_CHARS {
                return Err(PluginError::Abi(format!(
                    "badge label must be 1..={MAX_BADGE_LABEL_CHARS} characters"
                )));
            }
        }
        CellOutput::Table { columns, rows } => {
            if columns.is_empty() || columns.len() > MAX_TABLE_COLUMNS {
                return Err(PluginError::Abi(format!(
                    "table must have 1..={MAX_TABLE_COLUMNS} columns"
                )));
            }
            if rows.len() > MAX_TABLE_ROWS {
                return Err(PluginError::Abi(format!(
                    "table exceeds {MAX_TABLE_ROWS} rows"
                )));
            }
            if rows.iter().any(|row| row.len() != columns.len()) {
                return Err(PluginError::Abi(
                    "every table row must have one value per column".into(),
                ));
            }
        }
        CellOutput::Text { .. } | CellOutput::Json { .. } => {}
    }
    Ok(())
}

/// Cell hooks run on data the user is looking at, so they never get network,
/// filesystem, secrets or query-result access, whatever the plugin was granted.
pub fn restrict_consent(consent: &BTreeSet<CapabilityKind>) -> BTreeSet<CapabilityKind> {
    consent
        .iter()
        .copied()
        .filter(|kind| CELL_HOOK_CAPABILITIES.contains(kind))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_output_kind() {
        assert_eq!(
            parse_output(br#"{"kind":"text","text":"hi"}"#).unwrap(),
            CellOutput::Text { text: "hi".into() }
        );
        assert_eq!(
            parse_output(br#"{"kind":"badge","label":"valid"}"#).unwrap(),
            CellOutput::Badge {
                label: "valid".into(),
                tone: BadgeTone::Neutral
            }
        );
        assert!(matches!(
            parse_output(br#"{"kind":"json","value":{"a":1}}"#).unwrap(),
            CellOutput::Json { .. }
        ));
        assert!(matches!(
            parse_output(br#"{"kind":"table","columns":["k","v"],"rows":[["a",1]]}"#).unwrap(),
            CellOutput::Table { .. }
        ));
    }

    #[test]
    fn rejects_unknown_kinds_such_as_html() {
        let err = parse_output(br#"{"kind":"html","html":"<script>x</script>"}"#).unwrap_err();
        assert!(err.to_string().contains("invalid cell output"));
    }

    #[test]
    fn rejects_ragged_or_oversized_tables() {
        assert!(parse_output(br#"{"kind":"table","columns":["a","b"],"rows":[["x"]]}"#).is_err());
        assert!(parse_output(br#"{"kind":"table","columns":[],"rows":[]}"#).is_err());
        let rows = vec![vec![serde_json::json!(1)]; MAX_TABLE_ROWS + 1];
        let raw = serde_json::to_vec(&serde_json::json!({
            "kind": "table", "columns": ["a"], "rows": rows
        }))
        .unwrap();
        assert!(parse_output(&raw).is_err());
    }

    #[test]
    fn rejects_empty_or_long_badges() {
        assert!(parse_output(br#"{"kind":"badge","label":"  "}"#).is_err());
        let long = "x".repeat(MAX_BADGE_LABEL_CHARS + 1);
        let raw = serde_json::to_vec(&serde_json::json!({"kind":"badge","label":long})).unwrap();
        assert!(parse_output(&raw).is_err());
    }

    #[test]
    fn rejects_oversized_output_and_input() {
        let text = "x".repeat(MAX_CELL_OUTPUT_BYTES);
        let raw = serde_json::to_vec(&serde_json::json!({"kind":"text","text":text})).unwrap();
        assert!(parse_output(&raw).is_err());

        let input = CellInput {
            id: "a".into(),
            value: serde_json::Value::String("x".repeat(MAX_CELL_INPUT_BYTES)),
            column: CellColumn {
                name: "c".into(),
                data_type: None,
            },
        };
        assert!(encode_input(&input).is_err());
    }

    #[test]
    fn restricted_consent_drops_network_fs_secrets_and_query_read() {
        let all: BTreeSet<_> = CapabilityKind::ALL.into_iter().collect();
        let restricted = restrict_consent(&all);
        assert_eq!(
            restricted,
            BTreeSet::from([CapabilityKind::Log, CapabilityKind::Storage])
        );
    }
}

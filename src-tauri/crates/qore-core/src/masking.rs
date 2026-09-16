// SPDX-License-Identifier: BUSL-1.1

//! Column value masking, configured per saved connection and applied to every
//! result a session hands out. A rule names a table (empty for any table) and
//! a column. Results that do not come from a single known table (free queries)
//! match rules by column name alone: that over-masks rather than leaks.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::sensitive::is_sensitive_column;
use crate::traits::{StreamEvent, StreamSender};
use crate::types::{ColumnInfo, QueryResult, Row, Value};

pub const HIDDEN_VALUE: &str = "••••••";
const HASH_HEX_LEN: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaskMode {
    Hidden,
    Partial,
    Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskingRule {
    #[serde(default)]
    pub table: String,
    pub column: String,
    pub mode: MaskMode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionMasking {
    #[serde(default)]
    pub rules: Vec<MaskingRule>,
    /// Applies `partial` to columns whose name looks sensitive.
    #[serde(default)]
    pub mask_detected_columns: bool,
}

impl ConnectionMasking {
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && !self.mask_detected_columns
    }

    /// `table` is the table the result comes from, when there is exactly one.
    pub fn mode_for(&self, table: Option<&str>, column: &str) -> Option<MaskMode> {
        self.rules
            .iter()
            .find(|rule| {
                rule.column.eq_ignore_ascii_case(column)
                    && (rule.table.is_empty()
                        || table.is_none_or(|table| same_table(&rule.table, table)))
            })
            .map(|rule| rule.mode)
            .or_else(|| {
                (self.mask_detected_columns && is_sensitive_column(column))
                    .then_some(MaskMode::Partial)
            })
    }

    /// True when `self` masks nothing that `other` leaves visible: every rule
    /// is kept as is and detection is not switched on.
    pub fn is_subset_of(&self, other: &ConnectionMasking) -> bool {
        (!self.mask_detected_columns || other.mask_detected_columns)
            && self.rules.iter().all(|rule| other.rules.contains(rule))
    }
}

/// Schema qualifiers are ignored: a rule on `public.users` matches `users`.
fn same_table(rule_table: &str, table: &str) -> bool {
    let last = |name: &str| name.rsplit('.').next().unwrap_or(name).trim().to_string();
    last(rule_table).eq_ignore_ascii_case(&last(table))
}

/// Masking bound to a live session. The connection id salts `hash` so equal
/// values stay joinable within a connection without being comparable to a
/// plain SHA-256 dictionary.
#[derive(Debug)]
pub struct SessionMasking {
    pub connection_id: String,
    pub config: ConnectionMasking,
}

impl SessionMasking {
    pub fn for_connection(connection_id: &str, config: &ConnectionMasking) -> Option<Arc<Self>> {
        (!config.is_empty()).then(|| {
            Arc::new(Self {
                connection_id: connection_id.to_string(),
                config: config.clone(),
            })
        })
    }

    /// Flags the masked columns and returns the plan for their rows.
    pub fn plan(self: &Arc<Self>, table: Option<&str>, columns: &mut [ColumnInfo]) -> MaskPlan {
        let modes = columns
            .iter_mut()
            .map(|column| {
                let mode = self.config.mode_for(table, &column.name);
                column.masked = mode.is_some();
                mode
            })
            .collect();
        MaskPlan {
            session: Arc::clone(self),
            table: table.map(str::to_string),
            modes,
        }
    }

    /// For rows already turned into JSON objects keyed by column name.
    pub fn apply_json(self: &Arc<Self>, table: Option<&str>, value: &mut serde_json::Value) {
        self.plan(table, &mut []).mask_document(value);
    }

    pub fn apply(self: &Arc<Self>, table: Option<&str>, result: &mut QueryResult) {
        let plan = self.plan(table, &mut result.columns);
        for row in &mut result.rows {
            plan.apply_row(row);
        }
    }
}

pub struct MaskPlan {
    session: Arc<SessionMasking>,
    table: Option<String>,
    modes: Vec<Option<MaskMode>>,
}

impl MaskPlan {
    pub fn apply_row(&self, row: &mut Row) {
        for (index, value) in row.values.iter_mut().enumerate() {
            match self.modes.get(index).copied().flatten() {
                Some(mode) => *value = self.mask_value(mode, value),
                None => {
                    if let Value::Json(document) = value {
                        self.mask_document(document);
                    }
                }
            }
        }
    }

    pub fn apply_rows(&self, rows: &mut [Row]) {
        for row in rows {
            self.apply_row(row);
        }
    }

    fn mask_value(&self, mode: MaskMode, value: &Value) -> Value {
        match value {
            Value::Null => Value::Null,
            Value::Text(text) => Value::Text(self.mask_text(mode, text)),
            other => Value::Text(self.mask_text(mode, &json_text(&other.to_json()))),
        }
    }

    /// Document stores return rows as one JSON column: keys are matched like
    /// columns, at any depth.
    fn mask_document(&self, document: &mut serde_json::Value) {
        match document {
            serde_json::Value::Object(map) => {
                for (key, value) in map.iter_mut() {
                    match self.session.config.mode_for(self.table.as_deref(), key) {
                        Some(_) if value.is_null() => {}
                        Some(mode) => {
                            *value =
                                serde_json::Value::String(self.mask_text(mode, &json_text(value)))
                        }
                        None => self.mask_document(value),
                    }
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    self.mask_document(item);
                }
            }
            _ => {}
        }
    }

    fn mask_text(&self, mode: MaskMode, text: &str) -> String {
        match mode {
            MaskMode::Hidden => HIDDEN_VALUE.to_string(),
            // Never more than half the value, so short secrets (CVV, PIN)
            // are not shown in full.
            MaskMode::Partial => {
                let shown = (text.chars().count() / 2).min(3);
                format!("{}***", text.chars().take(shown).collect::<String>())
            }
            MaskMode::Hash => {
                let mut hasher = Sha256::new();
                hasher.update(self.session.connection_id.as_bytes());
                hasher.update([0]);
                hasher.update(text.as_bytes());
                hasher
                    .finalize()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()[..HASH_HEX_LEN]
                    .to_string()
            }
        }
    }
}

/// Returns a sender whose events reach `downstream` in order, masked. Rows
/// streamed before any column event only get their JSON documents masked,
/// since there is nothing to match their positions against.
pub fn mask_stream(
    masking: Arc<SessionMasking>,
    table: Option<String>,
    downstream: StreamSender,
) -> StreamSender {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(downstream.max_capacity());
    tokio::spawn(async move {
        let mut plan: Option<MaskPlan> = None;
        while let Some(event) = receiver.recv().await {
            let event = match event {
                StreamEvent::Columns(mut columns) => {
                    plan = Some(masking.plan(table.as_deref(), &mut columns));
                    StreamEvent::Columns(columns)
                }
                StreamEvent::Row(mut row) => {
                    plan.get_or_insert_with(|| masking.plan(table.as_deref(), &mut []))
                        .apply_row(&mut row);
                    StreamEvent::Row(row)
                }
                StreamEvent::RowBatch(mut rows) => {
                    plan.get_or_insert_with(|| masking.plan(table.as_deref(), &mut []))
                        .apply_rows(&mut rows);
                    StreamEvent::RowBatch(rows)
                }
                other => other,
            };
            if downstream.send(event).await.is_err() {
                break;
            }
        }
    });
    sender
}

fn json_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(table: &str, column: &str, mode: MaskMode) -> MaskingRule {
        MaskingRule {
            table: table.to_string(),
            column: column.to_string(),
            mode,
        }
    }

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            masked: false,
        }
    }

    fn session(rules: Vec<MaskingRule>, detected: bool) -> Arc<SessionMasking> {
        SessionMasking::for_connection(
            "conn-1",
            &ConnectionMasking {
                rules,
                mask_detected_columns: detected,
            },
        )
        .unwrap()
    }

    fn result(columns: &[&str], values: Vec<Value>) -> QueryResult {
        QueryResult {
            columns: columns.iter().map(|name| column(name)).collect(),
            rows: vec![Row { values }],
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }

    #[test]
    fn applies_the_three_modes_and_flags_columns() {
        let masking = session(
            vec![
                rule("users", "email", MaskMode::Partial),
                rule("users", "password", MaskMode::Hidden),
                rule("", "ssn", MaskMode::Hash),
            ],
            false,
        );
        let mut result = result(
            &["id", "email", "password", "ssn"],
            vec![
                Value::Int(1),
                Value::Text("alice@example.com".into()),
                Value::Text("hunter2".into()),
                Value::Int(123456789),
            ],
        );
        masking.apply(Some("public.users"), &mut result);

        let flags: Vec<bool> = result.columns.iter().map(|c| c.masked).collect();
        assert_eq!(flags, vec![false, true, true, true]);
        let values = &result.rows[0].values;
        assert!(matches!(values[0], Value::Int(1)));
        assert!(matches!(&values[1], Value::Text(t) if t == "ali***"));
        assert!(matches!(&values[2], Value::Text(t) if t == HIDDEN_VALUE));
        let Value::Text(hash) = &values[3] else {
            panic!("hash is text")
        };
        assert_eq!(hash.len(), HASH_HEX_LEN);
    }

    #[test]
    fn hash_is_stable_per_connection_and_salted() {
        let a = session(vec![rule("", "ssn", MaskMode::Hash)], false);
        let other = SessionMasking::for_connection("conn-2", &a.config).unwrap();
        let hash = |masking: &Arc<SessionMasking>| {
            let mut r = result(&["ssn"], vec![Value::Text("123".into())]);
            masking.apply(None, &mut r);
            r.rows.remove(0).values.remove(0)
        };
        let (first, second, salted) = (hash(&a), hash(&a), hash(&other));
        assert!(matches!((&first, &second), (Value::Text(x), Value::Text(y)) if x == y));
        assert!(matches!((&first, &salted), (Value::Text(x), Value::Text(y)) if x != y));
    }

    #[test]
    fn table_rules_only_bind_their_table_when_it_is_known() {
        let masking = session(vec![rule("users", "email", MaskMode::Hidden)], false);
        assert_eq!(
            masking.config.mode_for(Some("orders"), "email"),
            None,
            "another table keeps its column"
        );
        assert_eq!(
            masking.config.mode_for(None, "EMAIL"),
            Some(MaskMode::Hidden),
            "free queries match by name"
        );
    }

    #[test]
    fn detection_masks_sensitive_names_and_partial_never_shows_short_values_in_full() {
        let masking = session(Vec::new(), true);
        let mut result = result(
            &["cvv", "name", "phone"],
            vec![
                Value::Text("123".into()),
                Value::Text("Alice".into()),
                Value::Null,
            ],
        );
        masking.apply(None, &mut result);
        let values = &result.rows[0].values;
        assert!(matches!(&values[0], Value::Text(t) if t == "1***"));
        assert!(matches!(&values[1], Value::Text(t) if t == "Alice"));
        assert!(matches!(values[2], Value::Null));
    }

    #[test]
    fn documents_are_masked_by_key_at_any_depth() {
        let masking = session(vec![rule("", "email", MaskMode::Hidden)], false);
        let mut result = result(
            &["document"],
            vec![Value::Json(serde_json::json!({
                "_id": 1,
                "email": "a@b.c",
                "contacts": [{ "email": "c@d.e", "kind": "work" }]
            }))],
        );
        masking.apply(Some("users"), &mut result);
        let Value::Json(document) = &result.rows[0].values[0] else {
            panic!("document stays json")
        };
        assert_eq!(document["email"], HIDDEN_VALUE);
        assert_eq!(document["contacts"][0]["email"], HIDDEN_VALUE);
        assert_eq!(document["contacts"][0]["kind"], "work");
        assert!(!result.columns[0].masked);
    }

    #[test]
    fn subset_tells_removals_from_additions() {
        let full = ConnectionMasking {
            rules: vec![
                rule("users", "email", MaskMode::Hidden),
                rule("", "ssn", MaskMode::Hash),
            ],
            mask_detected_columns: true,
        };
        let fewer = ConnectionMasking {
            rules: vec![rule("", "ssn", MaskMode::Hash)],
            mask_detected_columns: false,
        };
        assert!(fewer.is_subset_of(&full));
        assert!(!full.is_subset_of(&fewer));

        let weaker_mode = ConnectionMasking {
            rules: vec![rule("", "ssn", MaskMode::Partial)],
            mask_detected_columns: false,
        };
        assert!(!weaker_mode.is_subset_of(&full));
        assert!(SessionMasking::for_connection("c", &ConnectionMasking::default()).is_none());
    }
}

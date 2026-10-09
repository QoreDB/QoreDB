// SPDX-License-Identifier: BUSL-1.1

use std::collections::HashMap;

use qore_core::masking::{ConnectionMasking, HIDDEN_VALUE};
use serde_json::Value;

use super::types::ChangelogEntry;

const REDACTED: &str = "[REDACTED]";

/// History cannot restore partial/hash masks, so every protected value is erased.
/// Reuse the connection's rule matching and also apply the changelog's own policy.
pub(super) struct HistoryPrivacy<'a> {
    sensitive: Vec<String>,
    masking: Option<&'a ConnectionMasking>,
}

impl<'a> HistoryPrivacy<'a> {
    pub fn new(sensitive: &[String], masking: Option<&'a ConnectionMasking>) -> Self {
        Self {
            sensitive: sensitive
                .iter()
                .map(|name| normalize(name))
                .filter(|name| !name.is_empty())
                .collect(),
            masking,
        }
    }

    pub fn project(&self, entry: &ChangelogEntry) -> ChangelogEntry {
        let mut protected = entry.clone();
        self.protect(&mut protected);
        protected
    }

    pub fn protect(&self, entry: &mut ChangelogEntry) {
        self.protect_map(&entry.table_name, &mut entry.primary_key);
        for image in [&mut entry.before, &mut entry.after].into_iter().flatten() {
            self.protect_map(&entry.table_name, image);
        }
    }

    pub fn protect_map(&self, table: &str, map: &mut HashMap<String, Value>) {
        for (column, value) in map.iter_mut() {
            self.protect_field(table, column, column, value);
        }
    }

    fn protect_field(&self, table: &str, column: &str, path: &str, value: &mut Value) {
        let normalized = normalize(path);
        let sensitive = self
            .sensitive
            .iter()
            .any(|token| normalized.contains(token));
        let masked = self.masking.is_some_and(|rules| {
            rules.mode_for(Some(table), path).is_some()
                || rules.mode_for(Some(table), column).is_some()
                || column
                    .split('.')
                    .any(|part| rules.mode_for(Some(table), part).is_some())
        });
        if sensitive || masked {
            *value = Value::String(REDACTED.into());
        } else {
            self.protect_nested(table, path, value);
        }
    }

    fn protect_nested(&self, table: &str, path: &str, value: &mut Value) {
        match value {
            Value::Object(fields) => {
                for (column, value) in fields {
                    self.protect_field(table, column, &format!("{path}.{column}"), value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    self.protect_nested(table, path, value);
                }
            }
            _ => {}
        }
    }

    pub fn can_identify(&self, table: &str, key: &HashMap<String, Value>) -> bool {
        let mut protected = key.clone();
        self.protect_map(table, &mut protected);
        key_is_available(&protected)
    }
}

fn normalize(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub(super) fn key_is_available(key: &HashMap<String, Value>) -> bool {
    !key.is_empty() && !key.values().any(value_is_unavailable)
}

/// Markers nested inside a JSON value are just as lossy as a scalar placeholder.
pub(super) fn value_is_unavailable(value: &Value) -> bool {
    match value {
        Value::String(value) => {
            value == REDACTED
                || value == HIDDEN_VALUE
                || (value.starts_with("<binary ") && value.ends_with(" bytes>"))
        }
        Value::Array(values) => values.iter().any(value_is_unavailable),
        Value::Object(fields) => fields.values().any(value_is_unavailable),
        _ => false,
    }
}

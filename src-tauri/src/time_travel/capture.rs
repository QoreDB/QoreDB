// SPDX-License-Identifier: BUSL-1.1

//! Change Capture Helpers
//!
//! Utilities for building ChangelogEntry records from mutation data.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tracing::warn;
use uuid::Uuid;

use crate::engine::traits::DataEngine;
use crate::engine::types::{
    ColumnFilter, FilterOperator, Namespace, RowData, SessionId, TableQueryOptions, Value,
};

use super::types::{ChangeOperation, ChangelogEntry};

/// Fetch the current state of a row by its primary key.
///
/// Best-effort: returns None if the row is not found or if an error occurs.
/// This is used to capture the before-image before UPDATE/DELETE.
pub async fn fetch_row_by_pk(
    driver: &Arc<dyn DataEngine>,
    session_id: SessionId,
    namespace: &Namespace,
    table: &str,
    primary_key: &RowData,
) -> Option<HashMap<String, serde_json::Value>> {
    // Build filters from PK columns
    let filters: Vec<ColumnFilter> = primary_key
        .columns
        .iter()
        .map(|(col, val)| ColumnFilter {
            column: col.clone(),
            operator: FilterOperator::Eq,
            value: val.clone(),
            options: Default::default(),
        })
        .collect();

    if filters.is_empty() {
        return None;
    }

    let options = TableQueryOptions {
        page: Some(1),
        page_size: Some(1),
        sort_column: None,
        sort_direction: None,
        filters: Some(filters),
        search: None,
    };

    // Use tokio timeout (2 seconds) for the before-image fetch
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        driver.query_table(session_id, namespace, table, options),
    )
    .await;

    match result {
        Ok(Ok(paginated)) => {
            if let Some(first_row) = paginated.result.rows.first() {
                let columns = &paginated.result.columns;
                Some(row_to_map(columns, &first_row.values))
            } else {
                None
            }
        }
        Ok(Err(e)) => {
            warn!("Time-travel: failed to fetch before-image for {}.{}: {}", namespace.database, table, e);
            None
        }
        Err(_) => {
            warn!("Time-travel: before-image fetch timed out for {}.{}", namespace.database, table);
            None
        }
    }
}

/// Build a ChangelogEntry from mutation data.
pub fn build_changelog_entry(
    session_id: &str,
    driver_id: &str,
    namespace: &Namespace,
    table: &str,
    operation: ChangeOperation,
    primary_key: &RowData,
    before: Option<HashMap<String, serde_json::Value>>,
    after: Option<HashMap<String, serde_json::Value>>,
    connection_name: Option<&str>,
    environment: &str,
) -> ChangelogEntry {
    let changed_columns = compute_changed_columns(&before, &after);

    ChangelogEntry {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        session_id: session_id.to_string(),
        driver_id: driver_id.to_string(),
        namespace: namespace.clone(),
        table_name: table.to_string(),
        operation,
        primary_key: rowdata_to_json_map(primary_key),
        before,
        after,
        changed_columns,
        connection_name: connection_name.map(String::from),
        environment: environment.to_string(),
    }
}

/// Determine which columns changed between before and after states.
fn compute_changed_columns(
    before: &Option<HashMap<String, serde_json::Value>>,
    after: &Option<HashMap<String, serde_json::Value>>,
) -> Vec<String> {
    match (before, after) {
        (Some(b), Some(a)) => a
            .iter()
            .filter(|(k, v)| b.get(*k) != Some(v))
            .map(|(k, _)| k.clone())
            .collect(),
        _ => vec![],
    }
}

/// Convert a RowData (used by mutation commands) into a JSON map.
pub fn rowdata_to_json_map(data: &RowData) -> HashMap<String, serde_json::Value> {
    data.columns
        .iter()
        .map(|(k, v)| (k.clone(), value_to_json(v)))
        .collect()
}

/// Convert a Value enum to serde_json::Value (public for sandbox integration).
pub fn value_to_json_pub(value: &Value) -> serde_json::Value {
    value_to_json(value)
}

/// Convert a Value enum to serde_json::Value.
fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::json!(b),
        Value::Int(i) => serde_json::json!(i),
        Value::Float(f) => serde_json::json!(f),
        Value::Text(s) => serde_json::json!(s),
        Value::Bytes(b) => serde_json::json!(format!("<binary {} bytes>", b.len())),
        Value::Json(j) => j.clone(),
        Value::Array(arr) => serde_json::json!(arr.iter().map(value_to_json).collect::<Vec<_>>()),
    }
}

/// Convert query result columns + row values into a JSON map.
fn row_to_map(
    columns: &[crate::engine::types::ColumnInfo],
    values: &[Value],
) -> HashMap<String, serde_json::Value> {
    columns
        .iter()
        .zip(values.iter())
        .map(|(col, val)| (col.name.clone(), value_to_json(val)))
        .collect()
}

/// Merge before-image with mutation data to produce after-image.
///
/// For UPDATE: the after-image is the before-image with the changed columns
/// overwritten by the new values from `data`.
pub fn merge_before_with_data(
    before: &HashMap<String, serde_json::Value>,
    data: &RowData,
) -> HashMap<String, serde_json::Value> {
    let mut after = before.clone();
    for (k, v) in &data.columns {
        after.insert(k.clone(), value_to_json(v));
    }
    after
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_changed_columns() {
        let before = Some(HashMap::from([
            ("id".to_string(), serde_json::json!(1)),
            ("name".to_string(), serde_json::json!("Alice")),
            ("age".to_string(), serde_json::json!(30)),
        ]));
        let after = Some(HashMap::from([
            ("id".to_string(), serde_json::json!(1)),
            ("name".to_string(), serde_json::json!("Bob")),
            ("age".to_string(), serde_json::json!(30)),
        ]));

        let changed = compute_changed_columns(&before, &after);
        assert_eq!(changed, vec!["name".to_string()]);
    }

    #[test]
    fn test_compute_changed_columns_insert() {
        let changed = compute_changed_columns(&None, &Some(HashMap::new()));
        assert!(changed.is_empty());
    }

    #[test]
    fn test_merge_before_with_data() {
        let before = HashMap::from([
            ("id".to_string(), serde_json::json!(1)),
            ("name".to_string(), serde_json::json!("Alice")),
            ("age".to_string(), serde_json::json!(30)),
        ]);
        let mut data = RowData {
            columns: HashMap::new(),
        };
        data.columns
            .insert("name".to_string(), Value::Text("Bob".to_string()));

        let after = merge_before_with_data(&before, &data);
        assert_eq!(after.get("name"), Some(&serde_json::json!("Bob")));
        assert_eq!(after.get("age"), Some(&serde_json::json!(30)));
    }

    #[test]
    fn test_rowdata_to_json_map() {
        let mut data = RowData {
            columns: HashMap::new(),
        };
        data.columns
            .insert("id".to_string(), Value::Int(42));
        data.columns
            .insert("name".to_string(), Value::Text("Alice".to_string()));

        let map = rowdata_to_json_map(&data);
        assert_eq!(map.get("id"), Some(&serde_json::json!(42)));
        assert_eq!(map.get("name"), Some(&serde_json::json!("Alice")));
    }
}

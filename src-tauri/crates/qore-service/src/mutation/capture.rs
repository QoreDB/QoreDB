// SPDX-License-Identifier: BUSL-1.1

//! Best-effort database images for Time Travel. Missing evidence stays missing:
//! supplied values are never substituted for an unreadable or generated row.

use std::time::Duration;

use qore_core::{
    ColumnFilter, CountMode, DataEngine, FilterOperator, Namespace, QueryResult, RowData,
    SessionId, TableQueryOptions, Value,
};
use qore_sql::generator::SandboxChangeType;

// Same payload ceiling as a query-cache entry. This bounds retained images,
// not the driver's transient allocation while reading a large row.
pub(crate) const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Default)]
pub struct MutationCapture {
    pub primary_key: RowData,
    pub before: Option<RowData>,
    pub after: Option<RowData>,
}

impl MutationCapture {
    pub(crate) fn retained_bytes(&self, limit: usize) -> Option<usize> {
        bounded_json_size(&(&self.primary_key, &self.before, &self.after), limit)
    }
}

fn bounded_json_size(value: &impl serde::Serialize, limit: usize) -> Option<usize> {
    struct Counter {
        bytes: usize,
        limit: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes) {
                return Err(std::io::Error::other("capture payload limit"));
            }
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { bytes: 0, limit };
    serde_json::to_writer(&mut counter, value).ok()?;
    Some(counter.bytes)
}

/// A key verified against the database schema, with the image read just before
/// this particular write (not the potentially stale values supplied by the UI).
#[derive(Default)]
pub struct PreparedCapture {
    key_columns: Vec<String>,
    primary_key: RowData,
    before: Option<RowData>,
}

impl PreparedCapture {
    pub fn returning_columns(&self) -> &[String] {
        &self.key_columns
    }

    /// Statement values establish identity only. The final image is still read
    /// from the database because AFTER triggers can change the inserted row.
    pub fn use_inserted_values(&mut self, values: Option<RowData>) {
        if let Some(values) = values {
            self.primary_key = key_from_columns(&self.key_columns, &values).unwrap_or_default();
        }
    }
}

fn key_from_columns(columns: &[String], row: &RowData) -> Option<RowData> {
    let columns = columns
        .iter()
        .map(|column| {
            row.columns
                .get(column)
                .filter(|value| matches!(value, Value::Bool(_) | Value::Int(_) | Value::Text(_)))
                .map(|value| (column.clone(), value.clone()))
        })
        .collect::<Option<_>>()?;
    Some(RowData { columns })
}

pub async fn prepare_capture(
    driver: &dyn DataEngine,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    operation: SandboxChangeType,
    selector: &RowData,
) -> PreparedCapture {
    if !driver.supports_safe_row_capture() {
        return PreparedCapture::default();
    }
    let key_columns = tokio::time::timeout(Duration::from_secs(2), async {
        driver
            .describe_table(session, namespace, table)
            .await
            .ok()?
            .primary_key
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default();
    let key = key_from_columns(&key_columns, selector).unwrap_or_default();
    let before = if !matches!(operation, SandboxChangeType::Insert) {
        read_row(driver, session, namespace, table, &key)
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    let primary_key = if let Some(before) = &before {
        key_from_image(&key, before).unwrap_or_default()
    } else {
        key
    };
    PreparedCapture {
        key_columns,
        primary_key,
        before,
    }
}

pub async fn finish_capture(
    driver: &dyn DataEngine,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    operation: SandboxChangeType,
    data: &RowData,
    prepared: PreparedCapture,
    result: &QueryResult,
) -> Option<MutationCapture> {
    if result.affected_rows == Some(0) {
        return None;
    }
    let mut capture = MutationCapture {
        primary_key: prepared.primary_key,
        before: prepared.before,
        after: None,
    };
    // No arbitrary selector, guessed generated ID, multi-row image, or moved
    // identity may become a rollback target. The event remains visible without
    // a usable key; existing diff/rollback guards report incomplete history.
    let key_changed = matches!(operation, SandboxChangeType::Update)
        && capture.primary_key.columns.iter().any(|(column, value)| {
            data.columns
                .get(column)
                .is_some_and(|new_value| !key_value_eq(new_value, value))
        });
    if result.affected_rows != Some(1) || capture.primary_key.columns.is_empty() || key_changed {
        capture.primary_key.columns.clear();
        return Some(capture);
    }
    match read_row(driver, session, namespace, table, &capture.primary_key).await {
        Ok(None) if matches!(operation, SandboxChangeType::Delete) && capture.before.is_some() => {}
        Ok(Some(after)) if !matches!(operation, SandboxChangeType::Delete) => {
            if let Some(key) = key_from_image(&capture.primary_key, &after) {
                if matches!(operation, SandboxChangeType::Insert)
                    || key.columns.iter().all(|(column, value)| {
                        capture
                            .primary_key
                            .columns
                            .get(column)
                            .is_some_and(|expected| key_value_eq(value, expected))
                    })
                {
                    capture.primary_key = key;
                    capture.after = Some(after);
                } else {
                    capture.primary_key.columns.clear();
                }
            } else {
                capture.primary_key.columns.clear();
            }
        }
        _ => capture.primary_key.columns.clear(),
    }
    if capture.retained_bytes(MAX_CAPTURE_BYTES).is_none() {
        return Some(MutationCapture::default());
    }
    Some(capture)
}

fn key_value_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Text(a), Value::Text(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        _ => false,
    }
}

fn key_from_image(key: &RowData, image: &RowData) -> Option<RowData> {
    let columns = key
        .columns
        .keys()
        .map(|column| {
            image
                .columns
                .get(column)
                .filter(|value| matches!(value, Value::Bool(_) | Value::Int(_) | Value::Text(_)))
                .map(|value| (column.clone(), value.clone()))
        })
        .collect::<Option<_>>()?;
    Some(RowData { columns })
}

async fn read_row(
    driver: &dyn DataEngine,
    session: SessionId,
    namespace: &Namespace,
    table: &str,
    key: &RowData,
) -> Result<Option<RowData>, ()> {
    if key.columns.is_empty() {
        return Err(());
    }
    let options = TableQueryOptions {
        page: Some(1),
        page_size: Some(2),
        count_mode: Some(CountMode::None),
        filters: Some(
            key.columns
                .iter()
                .map(|(column, value)| ColumnFilter {
                    column: column.clone(),
                    operator: FilterOperator::Eq,
                    value: value.clone(),
                    options: Default::default(),
                })
                .collect(),
        ),
        ..Default::default()
    };
    let page = tokio::time::timeout(
        Duration::from_secs(2),
        driver.query_table_for_capture(session, namespace, table, options),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())?;
    if page.has_more || page.result.rows.len() > 1 {
        return Err(());
    }
    let Some(row) = page.result.rows.first() else {
        return Ok(None);
    };
    if row.values.len() != page.result.columns.len() {
        return Err(());
    }
    let columns: std::collections::HashMap<_, _> = page
        .result
        .columns
        .iter()
        .zip(&row.values)
        .map(|(column, value)| (column.name.to_string(), value.clone()))
        .collect();
    if columns.len() != row.values.len() {
        return Err(());
    }
    let row = RowData { columns };
    bounded_json_size(&row, MAX_CAPTURE_BYTES).ok_or(())?;
    Ok(Some(row))
}

#[cfg(all(test, feature = "driver-sqlite"))]
#[path = "capture_tests.rs"]
mod tests;

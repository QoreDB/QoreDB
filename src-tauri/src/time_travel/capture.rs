// SPDX-License-Identifier: BUSL-1.1

//! Change Capture Helpers
//!
//! Utilities for building ChangelogEntry records from mutation data.

use std::collections::HashMap;

use chrono::Utc;
use uuid::Uuid;

use crate::engine::types::{Namespace, RowData, Value};
use qore_service::mutation::capture::MutationCapture;

use super::types::{ChangeOperation, ChangelogEntry};

pub fn record_capture(
    store: &super::store::ChangelogStore,
    session_id: &str,
    driver_id: &str,
    workspace_id: Option<&str>,
    connection_id: Option<&str>,
    connection_name: Option<&str>,
    environment: &str,
    namespace: &Namespace,
    table: &str,
    operation: ChangeOperation,
    capture: &MutationCapture,
    masking: Option<&qore_core::masking::ConnectionMasking>,
) {
    if !store.should_capture(table, environment) {
        return;
    }
    store.record_with_masking(
        build_changelog_entry(
            session_id,
            driver_id,
            workspace_id,
            connection_id,
            namespace,
            table,
            operation,
            &capture.primary_key,
            capture.before.as_ref().map(rowdata_to_json_map),
            capture.after.as_ref().map(rowdata_to_json_map),
            connection_name,
            environment,
        ),
        masking,
    );
}

/// Build a ChangelogEntry from mutation data.
pub fn build_changelog_entry(
    session_id: &str,
    driver_id: &str,
    workspace_id: Option<&str>,
    connection_id: Option<&str>,
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
        workspace_id: workspace_id.map(String::from),
        connection_id: connection_id.map(String::from),
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

/// Record only confirmed writes after the service has resolved the transaction.
pub fn capture_confirmed_batch(
    store: &super::store::ChangelogStore,
    session_id: &str,
    driver_id: &str,
    workspace_id: Option<&str>,
    connection_id: Option<&str>,
    connection_name: Option<&str>,
    environment: &str,
    changes: &[qore_sql::generator::SandboxChangeDto],
    result: &qore_service::mutation::batch::ApplyBatchResult,
    masking: Option<&qore_core::masking::ConnectionMasking>,
) {
    use qore_sql::generator::SandboxChangeType;

    for &index in &result.applied_indices {
        let Some(change) = changes.get(index) else {
            continue;
        };
        if !store.should_capture(&change.table_name, environment) {
            continue;
        }
        let operation = match change.change_type {
            SandboxChangeType::Insert => ChangeOperation::Insert,
            SandboxChangeType::Update => ChangeOperation::Update,
            SandboxChangeType::Delete => ChangeOperation::Delete,
        };
        // A confirmed write without a readable image remains an incomplete
        // event; never fall back to the UI's old_values/new_values.
        let missing = MutationCapture::default();
        let capture = result
            .captures
            .iter()
            .find(|(captured_index, _)| *captured_index == index)
            .map(|(_, capture)| capture)
            .unwrap_or(&missing);
        record_capture(
            store,
            session_id,
            driver_id,
            workspace_id,
            connection_id,
            connection_name,
            environment,
            &change.namespace,
            &change.table_name,
            operation,
            capture,
            masking,
        );
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
    fn test_rowdata_to_json_map() {
        let mut data = RowData {
            columns: HashMap::new(),
        };
        data.columns.insert("id".to_string(), Value::Int(42));
        data.columns
            .insert("name".to_string(), Value::Text("Alice".to_string()));

        let map = rowdata_to_json_map(&data);
        assert_eq!(map.get("id"), Some(&serde_json::json!(42)));
        assert_eq!(map.get("name"), Some(&serde_json::json!("Alice")));
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use crate::time_travel::{ChangelogFilter, ChangelogScope, ChangelogStore};
    use qore_service::mutation::batch::ApplyBatchResult;
    use qore_sql::generator::{SandboxChangeDto, SandboxChangeType};

    #[tokio::test]
    async fn verified_capture_masks_disk_and_roundtrips_rollback_on_sqlite() {
        use crate::time_travel::rollback::generate_rollback_statements;
        use qore_core::{ConnectionConfig, DataEngine, QueryId};
        use qore_drivers::drivers::sqlite::SqliteDriver;
        use qore_service::mutation::capture::{finish_capture, prepare_capture};

        let dir = tempfile::tempdir().unwrap();
        let driver = SqliteDriver::new();
        let session = driver
            .connect(&ConnectionConfig {
                driver: "sqlite".into(),
                host: dir.path().join("capture.db").to_string_lossy().into(),
                ..Default::default()
            })
            .await
            .unwrap();
        driver.execute(session,
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, api_key TEXT); INSERT INTO items VALUES (9007199254740993, 'before', 'synthetic-private-token')",
            QueryId::new()).await.unwrap();
        let namespace = Namespace::new("main");
        let key = RowData::new().with_column("id", Value::Int(9_007_199_254_740_993));
        let data = RowData::new().with_column("name", Value::Text("after".into()));
        let prepared = prepare_capture(
            &driver,
            session,
            &namespace,
            "items",
            SandboxChangeType::Update,
            &key,
        )
        .await;
        let result = driver
            .update_row(session, &namespace, "items", &key, &data)
            .await
            .unwrap();
        let capture = finish_capture(
            &driver,
            session,
            &namespace,
            "items",
            SandboxChangeType::Update,
            &data,
            prepared,
            &result,
        )
        .await
        .unwrap();
        let store = ChangelogStore::new(dir.path().join("history"));
        record_capture(
            &store,
            "session",
            "sqlite",
            Some("test-workspace"),
            Some("saved"),
            None,
            "development",
            &namespace,
            "items",
            ChangeOperation::Update,
            &capture,
            None,
        );
        let raw = std::fs::read_to_string(dir.path().join("history/changelog.jsonl")).unwrap();
        assert!(!raw.contains("synthetic-private-token"));
        assert!(raw.contains("[REDACTED]"));
        let persisted: ChangelogEntry = serde_json::from_str(raw.trim()).unwrap();
        assert_eq!(persisted.workspace_id.as_deref(), Some("test-workspace"));
        let scope = ChangelogScope {
            masking: None,
            session_id: "session".into(),
            workspace_id: Some("test-workspace".into()),
            connection_id: Some("saved".into()),
            driver_id: "sqlite".into(),
        };
        let rollback = generate_rollback_statements(
            &store
                .get_entries(&scope, &ChangelogFilter::default())
                .unwrap(),
            "sqlite",
        );
        assert_eq!(rollback.statements_count, 1);
        assert!(!rollback.sql.contains("api_key"));
        driver
            .execute(session, &rollback.sql, QueryId::new())
            .await
            .unwrap();
        let stored = driver
            .execute(
                session,
                "SELECT name, api_key FROM items WHERE id = 9007199254740993",
                QueryId::new(),
            )
            .await
            .unwrap();
        assert!(matches!(&stored.rows[0].values[0], Value::Text(name) if name == "before"));
        assert!(
            matches!(&stored.rows[0].values[1], Value::Text(value) if value == "synthetic-private-token")
        );
        driver.disconnect(session).await.unwrap();
    }

    #[test]
    fn privacy_batch_capture_passes_session_rules_before_persistence() {
        use qore_core::masking::{ConnectionMasking, MaskMode, MaskingRule};
        let dir = tempfile::tempdir().unwrap();
        let store = ChangelogStore::new(dir.path().into());
        let changes = vec![SandboxChangeDto {
            change_type: SandboxChangeType::Delete,
            namespace: Namespace {
                database: "fixture".into(),
                schema: None,
            },
            table_name: "items".into(),
            primary_key: Some(RowData::new().with_column("id", Value::Int(1))),
            old_values: Some(
                RowData::new()
                    .with_column("alias", Value::Text("captured-secret-fixture".into()))
                    .columns,
            ),
            new_values: None,
        }];
        let masking = ConnectionMasking {
            rules: vec![MaskingRule {
                table: "items".into(),
                column: "alias".into(),
                mode: MaskMode::Hash,
            }],
            mask_detected_columns: false,
        };
        let mut result = ApplyBatchResult::default();
        result.success = true;
        result.applied_count = 1;
        result.applied_indices = vec![0];
        result.captures.push((
            0,
            MutationCapture {
                primary_key: changes[0].primary_key.clone().unwrap(),
                before: Some(RowData {
                    columns: changes[0].old_values.clone().unwrap(),
                }),
                after: None,
            },
        ));
        capture_confirmed_batch(
            &store,
            "session",
            "sqlite",
            Some("test-workspace"),
            Some("saved"),
            None,
            "development",
            &changes,
            &result,
            Some(&masking),
        );
        let content = std::fs::read_to_string(dir.path().join("changelog.jsonl")).unwrap();
        assert!(!content.contains("captured-secret-fixture"));
        assert!(content.contains("[REDACTED]"));
        for line in content.lines() {
            let persisted: ChangelogEntry = serde_json::from_str(line).unwrap();
            assert_eq!(persisted.workspace_id.as_deref(), Some("test-workspace"));
        }
    }

    #[test]
    fn batch_capture_excludes_unconfirmed_writes_and_uses_database_images() {
        let dir = tempfile::tempdir().unwrap();
        let store = ChangelogStore::new(dir.path().into());
        let changes = vec![SandboxChangeDto {
            change_type: SandboxChangeType::Update,
            namespace: Namespace {
                database: "fixture".into(),
                schema: None,
            },
            table_name: "items".into(),
            primary_key: Some(RowData::new().with_column("id", Value::Int(1))),
            old_values: Some(
                RowData::new()
                    .with_column("id", Value::Int(1))
                    .with_column("name", Value::Text("before".into()))
                    .columns,
            ),
            new_values: Some(
                RowData::new()
                    .with_column("name", Value::Text("after".into()))
                    .columns,
            ),
        }];
        let scope = ChangelogScope {
            masking: None,
            session_id: "session".into(),
            workspace_id: Some("test-workspace".into()),
            connection_id: Some("saved".into()),
            driver_id: "sqlite".into(),
        };
        let mut result = ApplyBatchResult::default();
        for unknown in [false, true] {
            result.outcome_unknown = unknown;
            capture_confirmed_batch(
                &store,
                "session",
                "sqlite",
                Some("test-workspace"),
                Some("saved"),
                None,
                "development",
                &changes,
                &result,
                None,
            );
            assert!(
                store
                    .get_entries(&scope, &ChangelogFilter::default())
                    .unwrap()
                    .is_empty()
            );
        }
        // An explicit nontransactional batch may have confirmed writes before its failure.
        result.captures.push((
            0,
            MutationCapture {
                primary_key: RowData::new().with_column("id", Value::Int(1)),
                before: Some(
                    RowData::new()
                        .with_column("id", Value::Int(1))
                        .with_column("name", Value::Text("server before".into())),
                ),
                after: Some(
                    RowData::new()
                        .with_column("id", Value::Int(1))
                        .with_column("name", Value::Text("SERVER AFTER".into())),
                ),
            },
        ));
        result.applied_count = 1;
        result.applied_indices = vec![0];
        capture_confirmed_batch(
            &store,
            "session",
            "sqlite",
            Some("test-workspace"),
            Some("saved"),
            None,
            "development",
            &changes,
            &result,
            None,
        );
        let entries = store
            .get_entries(&scope, &ChangelogFilter::default())
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].after.as_ref().unwrap()["id"],
            serde_json::json!(1)
        );
        assert_eq!(
            entries[0].after.as_ref().unwrap()["name"],
            serde_json::json!("SERVER AFTER")
        );
        assert_eq!(entries[0].changed_columns, vec!["name"]);
    }
}

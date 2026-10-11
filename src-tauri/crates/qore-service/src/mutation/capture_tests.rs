// SPDX-License-Identifier: BUSL-1.1

use super::*;
use qore_core::{ConnectionConfig, QueryId};
use qore_drivers::drivers::sqlite::SqliteDriver;

macro_rules! assert_values_eq {
    ($left:expr, $right:expr) => {
        assert_eq!(
            serde_json::to_value(&$left).unwrap(),
            serde_json::to_value(&$right).unwrap()
        );
    };
}

struct Fixture {
    driver: SqliteDriver,
    session: SessionId,
    namespace: Namespace,
    _dir: tempfile::TempDir,
}

impl Fixture {
    async fn new() -> Self {
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
        let fixture = Self {
            driver,
            session,
            namespace: Namespace::new("main"),
            _dir: dir,
        };
        fixture
            .execute(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, label TEXT, rank INTEGER DEFAULT 7)",
            )
            .await;
        fixture
    }

    async fn execute(&self, sql: &str) {
        self.driver
            .execute(self.session, sql, QueryId::new())
            .await
            .unwrap();
    }

    async fn mutate(
        &self,
        operation: SandboxChangeType,
        key: &RowData,
        data: &RowData,
    ) -> Option<MutationCapture> {
        let mut prepared = prepare_capture(
            &self.driver,
            self.session,
            &self.namespace,
            "items",
            operation.clone(),
            if matches!(operation, SandboxChangeType::Insert) {
                data
            } else {
                key
            },
        )
        .await;
        let result = match operation {
            SandboxChangeType::Insert => self
                .driver
                .insert_row_returning(
                    self.session,
                    &self.namespace,
                    "items",
                    data,
                    prepared.returning_columns(),
                )
                .await
                .map(|outcome| {
                    prepared.use_inserted_values(outcome.returned_values);
                    outcome.result
                }),
            SandboxChangeType::Update => {
                self.driver
                    .update_row(self.session, &self.namespace, "items", key, data)
                    .await
            }
            SandboxChangeType::Delete => {
                self.driver
                    .delete_row(self.session, &self.namespace, "items", key)
                    .await
            }
        }
        .unwrap();
        finish_capture(
            &self.driver,
            self.session,
            &self.namespace,
            "items",
            operation,
            data,
            prepared,
            &result,
        )
        .await
    }
}

fn key(id: i64) -> RowData {
    RowData::new().with_column("id", Value::Int(id))
}
fn label(value: &str) -> RowData {
    RowData::new().with_column("label", Value::Text(value.into()))
}

#[tokio::test]
async fn generated_insert_key_and_canonical_image_are_captured() {
    let f = Fixture::new().await;
    f.execute("CREATE TRIGGER normalize AFTER INSERT ON items BEGIN UPDATE items SET label = upper(NEW.label) WHERE id = NEW.id; END").await;
    let capture = f
        .mutate(
            SandboxChangeType::Insert,
            &RowData::new(),
            &label("generated"),
        )
        .await
        .unwrap();
    assert_values_eq!(capture.primary_key.columns.get("id"), Some(Value::Int(1)));
    assert_values_eq!(
        capture.after.unwrap().columns["label"],
        Value::Text("GENERATED".into())
    );
}

#[tokio::test]
async fn capture_rejects_a_non_unique_row_selector() {
    let f = Fixture::new().await;
    f.execute("INSERT INTO items (id, label) VALUES (1, 'same'), (2, 'same')")
        .await;
    let prepared = prepare_capture(
        &f.driver,
        f.session,
        &f.namespace,
        "items",
        SandboxChangeType::Update,
        &label("same"),
    )
    .await;
    assert!(prepared.primary_key.columns.is_empty());
    assert!(
        prepared.before.is_none(),
        "never capture an arbitrary first row"
    );
    assert!(
        read_row(&f.driver, f.session, &f.namespace, "items", &label("same"))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn capture_reads_trigger_values_and_defaults_instead_of_input() {
    let f = Fixture::new().await;
    f.execute("CREATE TRIGGER normalize_insert AFTER INSERT ON items BEGIN UPDATE items SET label = upper(trim(NEW.label)) WHERE id = NEW.id; END").await;
    f.execute("CREATE TRIGGER normalize_update AFTER UPDATE OF label ON items BEGIN UPDATE items SET label = upper(trim(NEW.label)), rank = OLD.rank + 1 WHERE id = NEW.id; END").await;
    let inserted = f
        .mutate(
            SandboxChangeType::Insert,
            &RowData::new(),
            &key(1).with_column("label", Value::Text(" first ".into())),
        )
        .await
        .unwrap();
    assert_values_eq!(inserted.primary_key.columns, key(1).columns);
    assert!(inserted.before.is_none());
    let after = inserted.after.unwrap();
    assert_values_eq!(after.columns["label"], Value::Text("FIRST".into()));
    assert_values_eq!(after.columns["rank"], Value::Int(8));
    let updated = f
        .mutate(SandboxChangeType::Update, &key(1), &label(" second "))
        .await
        .unwrap();
    assert_values_eq!(updated.before.unwrap().columns, after.columns);
    let after = updated.after.unwrap();
    assert_values_eq!(after.columns["label"], Value::Text("SECOND".into()));
    assert_values_eq!(after.columns["rank"], Value::Int(9));
    let deleted = f
        .mutate(SandboxChangeType::Delete, &key(1), &RowData::new())
        .await
        .unwrap();
    assert_values_eq!(deleted.before.unwrap().columns, after.columns);
    assert_values_eq!(deleted.primary_key.columns, key(1).columns);
    assert!(deleted.after.is_none());
}

#[tokio::test]
async fn moved_or_reinserted_rows_are_not_guessed() {
    let f = Fixture::new().await;
    let generated = f
        .mutate(
            SandboxChangeType::Insert,
            &RowData::new(),
            &label("generated"),
        )
        .await
        .unwrap();
    assert_values_eq!(generated.primary_key.columns, key(1).columns);
    assert!(generated.after.is_some());
    let moved = f
        .mutate(SandboxChangeType::Update, &key(1), &key(2))
        .await
        .unwrap();
    assert!(moved.primary_key.columns.is_empty());
    assert!(moved.after.is_none());
    f.execute("CREATE TRIGGER move_key AFTER UPDATE OF label ON items BEGIN UPDATE items SET id = NEW.id + 10 WHERE id = NEW.id; END").await;
    let triggered = f
        .mutate(SandboxChangeType::Update, &key(2), &label("moved"))
        .await
        .unwrap();
    assert!(triggered.primary_key.columns.is_empty());
    assert!(triggered.after.is_none());
    f.execute("CREATE TRIGGER recreate AFTER DELETE ON items BEGIN INSERT INTO items (id, label) VALUES (OLD.id, 'recreated'); END").await;
    let deleted = f
        .mutate(SandboxChangeType::Delete, &key(12), &RowData::new())
        .await
        .unwrap();
    assert!(
        deleted.primary_key.columns.is_empty(),
        "a trigger recreated the deleted identity"
    );
}

#[tokio::test]
async fn no_effect_produces_no_capture_and_missing_readback_has_no_fallback() {
    let f = Fixture::new().await;
    assert!(
        f.mutate(SandboxChangeType::Update, &key(1), &label("absent"))
            .await
            .is_none()
    );
    assert!(
        f.mutate(SandboxChangeType::Delete, &key(1), &RowData::new())
            .await
            .is_none()
    );
    f.execute("INSERT INTO items (id, label) VALUES (1, 'before')")
        .await;
    let prepared = prepare_capture(
        &f.driver,
        f.session,
        &f.namespace,
        "items",
        SandboxChangeType::Update,
        &key(1),
    )
    .await;
    f.execute("DROP TABLE items").await;
    let capture = finish_capture(
        &f.driver,
        f.session,
        &f.namespace,
        "items",
        SandboxChangeType::Update,
        &label("unconfirmed"),
        prepared,
        &QueryResult::with_affected_rows(1, 0.0),
    )
    .await
    .unwrap();
    assert!(capture.primary_key.columns.is_empty());
    assert!(capture.after.is_none());
}

#[tokio::test]
async fn composite_key_keeps_exact_large_integers_and_requires_every_component() {
    let f = Fixture::new().await;
    f.execute("DROP TABLE items; CREATE TABLE items (tenant TEXT NOT NULL, id INTEGER NOT NULL, label TEXT, PRIMARY KEY (tenant, id)); INSERT INTO items VALUES ('a', 9007199254740993, 'before'), ('b', 9007199254740993, 'other')").await;
    let partial = prepare_capture(
        &f.driver,
        f.session,
        &f.namespace,
        "items",
        SandboxChangeType::Update,
        &key(9_007_199_254_740_993),
    )
    .await;
    assert!(partial.before.is_none());
    let key = key(9_007_199_254_740_993).with_column("tenant", Value::Text("a".into()));
    let capture = f
        .mutate(SandboxChangeType::Update, &key, &label("after"))
        .await
        .unwrap();
    assert_values_eq!(capture.primary_key.columns, key.columns);
    assert_values_eq!(
        capture.before.unwrap().columns["label"],
        Value::Text("before".into())
    );
    assert_values_eq!(
        capture.after.unwrap().columns["label"],
        Value::Text("after".into())
    );
}

#[tokio::test]
async fn oversized_row_images_are_not_retained() {
    let f = Fixture::new().await;
    f.execute("INSERT INTO items (id, label) VALUES (1, hex(zeroblob(5000000)))")
        .await;
    let prepared = prepare_capture(
        &f.driver,
        f.session,
        &f.namespace,
        "items",
        SandboxChangeType::Update,
        &key(1),
    )
    .await;
    assert!(prepared.before.is_none());
    let capture = f
        .mutate(SandboxChangeType::Update, &key(1), &label("small"))
        .await
        .unwrap();
    assert!(
        capture.before.is_none(),
        "oversized before-image must remain unavailable"
    );
    assert!(capture.retained_bytes(MAX_CAPTURE_BYTES).is_some());
}

#[tokio::test]
async fn inserted_key_is_taken_from_the_canonical_row() {
    let f = Fixture::new().await;
    let data = RowData::new()
        .with_column("id", Value::Text("42".into()))
        .with_column("label", Value::Text("fixture".into()));
    let capture = f
        .mutate(SandboxChangeType::Insert, &RowData::new(), &data)
        .await
        .unwrap();
    assert_values_eq!(capture.primary_key.columns["id"], Value::Int(42));
}

#[tokio::test]
async fn canonical_float_keys_are_not_accepted_through_integer_input() {
    let f = Fixture::new().await;
    f.execute("DROP TABLE items; CREATE TABLE items (id REAL PRIMARY KEY, label TEXT)")
        .await;
    let capture = f
        .mutate(
            SandboxChangeType::Insert,
            &RowData::new(),
            &key(1).with_column("label", Value::Text("fixture".into())),
        )
        .await
        .unwrap();
    assert!(capture.primary_key.columns.is_empty());
    assert!(capture.after.is_none());
}

#[tokio::test]
async fn generated_composite_key_default_values_and_wire_result() {
    let f = Fixture::new().await;
    f.execute("DROP TABLE items").await;
    f.execute("CREATE TABLE items (\"tenant\"\"key\" TEXT DEFAULT 'generated', id INTEGER DEFAULT 9007199254740993, label TEXT DEFAULT 'default', PRIMARY KEY (\"tenant\"\"key\", id)) WITHOUT ROWID").await;
    let mut prepared = prepare_capture(
        &f.driver,
        f.session,
        &f.namespace,
        "items",
        SandboxChangeType::Insert,
        &RowData::new(),
    )
    .await;
    let outcome = f
        .driver
        .insert_row_returning(
            f.session,
            &f.namespace,
            "items",
            &RowData::new(),
            prepared.returning_columns(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.result.affected_rows, Some(1));
    assert!(outcome.result.rows.is_empty());
    assert!(outcome.result.columns.is_empty());
    prepared.use_inserted_values(outcome.returned_values);
    let capture = finish_capture(
        &f.driver,
        f.session,
        &f.namespace,
        "items",
        SandboxChangeType::Insert,
        &RowData::new(),
        prepared,
        &outcome.result,
    )
    .await
    .unwrap();
    assert_values_eq!(
        capture.primary_key.columns["id"],
        Value::Int(9007199254740993)
    );
    assert_values_eq!(
        capture.primary_key.columns["tenant\"key"],
        Value::Text("generated".into())
    );
    assert_values_eq!(
        capture.after.unwrap().columns["label"],
        Value::Text("default".into())
    );
}

#[tokio::test]
async fn returning_does_not_retry_failed_or_ignored_inserts() {
    let f = Fixture::new().await;
    f.execute("CREATE TRIGGER ignore_insert BEFORE INSERT ON items WHEN NEW.label = 'ignored' BEGIN SELECT RAISE(IGNORE); END").await;
    assert!(
        f.mutate(
            SandboxChangeType::Insert,
            &RowData::new(),
            &label("ignored")
        )
        .await
        .is_none()
    );
    f.execute("CREATE TRIGGER move_insert AFTER INSERT ON items BEGIN UPDATE items SET id = NEW.id + 10 WHERE id = NEW.id; END").await;
    let moved = f
        .mutate(SandboxChangeType::Insert, &RowData::new(), &label("moved"))
        .await
        .unwrap();
    assert!(moved.primary_key.columns.is_empty());
    assert!(moved.after.is_none());
    let error = f
        .driver
        .insert_row_returning(f.session, &f.namespace, "items", &key(11), &["id".into()])
        .await;
    assert!(error.is_err());
    let rows = f
        .driver
        .execute(f.session, "SELECT COUNT(*) FROM items", QueryId::new())
        .await
        .unwrap();
    assert_values_eq!(rows.rows[0].values[0], Value::Int(1));
}

#[tokio::test]
async fn driver_without_returning_keeps_generated_identity_incomplete() {
    let f = Fixture::new().await;
    let prepared = prepare_capture(
        &f.driver,
        f.session,
        &f.namespace,
        "items",
        SandboxChangeType::Insert,
        &label("generated"),
    )
    .await;
    let result = f
        .driver
        .insert_row(f.session, &f.namespace, "items", &label("generated"))
        .await
        .unwrap();
    let capture = finish_capture(
        &f.driver,
        f.session,
        &f.namespace,
        "items",
        SandboxChangeType::Insert,
        &label("generated"),
        prepared,
        &result,
    )
    .await
    .unwrap();
    assert!(capture.primary_key.columns.is_empty());
    assert!(capture.after.is_none());
}

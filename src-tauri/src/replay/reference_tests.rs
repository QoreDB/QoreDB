// SPDX-License-Identifier: BUSL-1.1

use super::*;
use crate::engine::types::{ColumnInfo, Row, Value};
use crate::replay::store::ReplaySetStore;
use crate::replay::types::{ReplayReport, ReplaySet, ReplayVerdict};

struct Fixture {
    _dir: tempfile::TempDir,
    captures: CaptureStore,
    sets: ReplaySetStore,
    set: ReplaySet,
    original: RunMeta,
    observed: ReplayReport,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let captures = CaptureStore::scoped(dir.path(), "default").unwrap();
        let sets = ReplaySetStore::new(&dir.path().join("workspace"));
        let entries: Vec<_> = (0..2)
            .map(|order| {
                serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(), "order": order,
                    "query": "SELECT id", "driver_id": "postgres", "operation_type": "select",
                    "expected": { "execution_time_ms": 1.0, "row_count": 1, "success": true,
                        "result_digest": "old" }
                })
            })
            .collect();
        let set: ReplaySet = serde_json::from_value(serde_json::json!({
            "version": 1, "name": "synthetic", "created_at": "2026-10-09T10:00:00Z",
            "source": { "driver_id": "postgres", "environment": "development" },
            "entries": entries
        }))
        .unwrap();
        let original: RunMeta = serde_json::from_value(serde_json::json!({
            "run_id": uuid::Uuid::new_v4().to_string(), "project_id": "default",
            "set_slug": "synthetic", "set_name": "synthetic", "started_at": "2026-10-09T10:00:00Z",
            "driver_id": "postgres", "environment": "development", "capture_mode": "full",
            "is_baseline": true
        }))
        .unwrap();
        let mut run = original.clone();
        run.run_id = uuid::Uuid::new_v4().to_string();
        run.is_baseline = false;
        run.started_at = "2026-10-09T11:00:00Z".into();
        let results = set
            .entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "entry_id": entry.id, "order": entry.order, "query_preview": entry.query,
                    "verdict": "digest_diff", "success": true, "execution_time_ms": 2.0,
                    "expected_execution_time_ms": 1.0, "row_count": 1, "expected_row_count": 1,
                    "digest": "new", "expected_digest": "old", "captured": true
                })
            })
            .collect::<Vec<_>>();
        let observed: ReplayReport = serde_json::from_value(serde_json::json!({
            "run": run, "baseline_run_id": original.run_id, "results": results,
            "summary": {"total": 2,"matched": 0,"broken": 0,"row_count_diff": 0,"digest_diff": 2,"slower": 0,"skipped": 0}
        })).unwrap();
        assert!(set.baseline_run_id.is_none());
        assert!(!original.reference_generation);
        let fixture = Self {
            _dir: dir,
            captures,
            sets,
            set,
            original,
            observed,
        };
        fixture.sets.save("synthetic", &fixture.set).unwrap();
        fixture.captures.save_run_meta(&fixture.original).unwrap();
        fixture
            .captures
            .save_run_meta(&fixture.observed.run)
            .unwrap();
        fixture.save_report();
        for entry in &fixture.set.entries {
            fixture.save_rows(&fixture.original.run_id, &entry.id, 1);
            fixture.save_rows(&fixture.observed.run.run_id, &entry.id, 2);
        }
        fixture
    }

    fn save_rows(&self, run: &str, entry: &str, value: i64) {
        let result = QueryResult {
            columns: vec![ColumnInfo {
                name: "id".into(),
                data_type: "BIGINT".into(),
                nullable: false,
                masked: false,
            }],
            rows: vec![Row {
                values: vec![Value::Int(value)],
            }],
            affected_rows: None,
            execution_time_ms: 1.0,
        };
        self.captures
            .save_entry(
                run,
                entry,
                "SELECT id",
                "postgres",
                None,
                None,
                &result,
                10,
                100_000,
            )
            .unwrap();
    }

    fn save_report(&self) {
        self.captures
            .save_report(&self.observed.run.run_id, &self.observed)
            .unwrap();
    }

    fn accept(&self, wanted: Option<&[String]>) -> Result<ReplaySet, String> {
        self.captures.accept_run(
            self.sets.load("synthetic").unwrap(),
            "synthetic",
            &self.observed.run.run_id,
            wanted,
            |set| self.sets.save("synthetic", set).map(|_| ()),
        )
    }

    fn value(&self, run: &str, index: usize) -> i64 {
        let snapshot = self
            .captures
            .load_entry(run, &self.set.entries[index].id)
            .unwrap();
        match snapshot.rows[0].values[0] {
            Value::Int(value) => value,
            _ => panic!("expected integer"),
        }
    }
}

#[test]
fn accepting_selected_entries_preserves_history_and_unselected_expectations() {
    let fixture = Fixture::new();
    let accepted = fixture
        .accept(Some(&[fixture.set.entries[0].id.clone()]))
        .unwrap();
    let id = accepted.baseline_run_id.as_ref().unwrap();
    assert_ne!(id, &fixture.original.run_id);
    assert_eq!(fixture.value(id, 0), 2);
    assert_eq!(fixture.value(id, 1), 1);
    assert_eq!(fixture.value(&fixture.original.run_id, 0), 1);
    assert_eq!(
        accepted.entries[0].expected.result_digest.as_deref(),
        Some("new")
    );
    assert_eq!(
        accepted.entries[1].expected.result_digest.as_deref(),
        Some("old")
    );
    let reopened = fixture.sets.load("synthetic").unwrap();
    assert_eq!(
        fixture
            .captures
            .baseline_for_set("synthetic", &reopened)
            .unwrap()
            .unwrap()
            .run_id,
        *id
    );
    assert_eq!(
        fixture
            .captures
            .load_report(&fixture.observed.run.run_id)
            .unwrap()
            .baseline_run_id,
        Some(fixture.original.run_id)
    );
}

#[test]
fn metadata_only_acceptance_does_not_adopt_stale_files() {
    let mut fixture = Fixture::new();
    fixture.observed.results[0].captured = false;
    fixture.save_report();
    // Keep the source file on purpose: report metadata remains authoritative.
    let accepted = fixture.accept(None).unwrap();
    let id = accepted.baseline_run_id.unwrap();
    assert!(!fixture.captures.has_entry(&id, &fixture.set.entries[0].id));
    assert_eq!(fixture.value(&id, 1), 2);
    assert_eq!(fixture.value(&fixture.original.run_id, 0), 1);
}

#[test]
fn skipped_entries_keep_their_previous_reference() {
    let mut fixture = Fixture::new();
    fixture.observed.results[1].verdict = ReplayVerdict::Skipped;
    fixture.save_report();
    let accepted = fixture.accept(None).unwrap();
    assert_eq!(
        fixture.value(accepted.baseline_run_id.as_ref().unwrap(), 1),
        1
    );
    assert_eq!(
        accepted.entries[1].expected.result_digest.as_deref(),
        Some("old")
    );
}

#[test]
fn missing_or_corrupt_capture_rolls_back_all_selected_entries() {
    for corrupt in [false, true] {
        let fixture = Fixture::new();
        let path = fixture
            .captures
            .entry_path(&fixture.observed.run.run_id, &fixture.set.entries[1].id)
            .unwrap();
        if corrupt {
            fs::write(path, "invalid").unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
        let before = fs::read(fixture.sets.path_for("synthetic").unwrap()).unwrap();
        assert!(fixture.accept(None).is_err());
        assert_eq!(
            fs::read(fixture.sets.path_for("synthetic").unwrap()).unwrap(),
            before
        );
        assert_eq!(fixture.value(&fixture.original.run_id, 0), 1);
        assert_eq!(fs::read_dir(fixture.captures.root()).unwrap().count(), 2);
    }
}

#[test]
fn publication_failure_discards_only_the_prepared_generation() {
    let fixture = Fixture::new();
    let before = fs::read(fixture.sets.path_for("synthetic").unwrap()).unwrap();
    let result = fixture.captures.accept_run(
        fixture.set.clone(),
        "synthetic",
        &fixture.observed.run.run_id,
        None,
        |_| Err("synthetic publication failure".into()),
    );
    assert!(result.unwrap_err().contains("publication failure"));
    assert_eq!(
        fs::read(fixture.sets.path_for("synthetic").unwrap()).unwrap(),
        before
    );
    assert_eq!(fixture.value(&fixture.original.run_id, 0), 1);
    assert_eq!(fs::read_dir(fixture.captures.root()).unwrap().count(), 2);
}

#[test]
fn missing_local_reference_does_not_fall_back_to_old_rows() {
    let fixture = Fixture::new();
    let mut imported = fixture.set.clone();
    imported.baseline_run_id = Some(uuid::Uuid::new_v4().to_string());
    assert!(
        fixture
            .captures
            .baseline_for_set("synthetic", &imported)
            .unwrap()
            .is_none()
    );
    let mut foreign = fixture.original.clone();
    foreign.run_id = imported.baseline_run_id.clone().unwrap();
    foreign.set_slug = "other".into();
    fixture.captures.save_run_meta(&foreign).unwrap();
    assert!(
        fixture
            .captures
            .baseline_for_set("synthetic", &imported)
            .is_err()
    );
}

#[test]
fn reference_retention_preserves_active_and_historical_generations_only() {
    let mut fixture = Fixture::new();
    let first = fixture.accept(None).unwrap().baseline_run_id.unwrap();
    fixture.observed.baseline_run_id = Some(first.clone());
    fixture.save_report();
    let orphan = fixture.accept(None).unwrap().baseline_run_id.unwrap();
    let active = fixture.accept(None).unwrap().baseline_run_id.unwrap();
    fixture
        .captures
        .prune("synthetic", 1, Some(&active))
        .unwrap();
    for id in [&first, &active, &fixture.original.run_id] {
        assert!(fixture.captures.load_run_meta(id).is_ok());
    }
    assert!(fixture.captures.load_run_meta(&orphan).is_err());
}

#[test]
fn empty_selection_and_foreign_workspace_do_not_publish() {
    let fixture = Fixture::new();
    assert!(fixture.accept(Some(&[])).is_err());
    let other = CaptureStore::scoped(fixture._dir.path(), "other-workspace").unwrap();
    assert!(
        other
            .accept_run(
                fixture.set.clone(),
                "synthetic",
                &fixture.observed.run.run_id,
                None,
                |_| panic!("must not publish another workspace's report")
            )
            .is_err()
    );
    assert_eq!(fs::read_dir(fixture.captures.root()).unwrap().count(), 2);
}

// SPDX-License-Identifier: BUSL-1.1

//! Captured rows for a replay run, under `data_dir/replays/<run_id>/`.
//!
//! These are the local half of the feature and never reach the repository.
//! Rows are written in the existing `Snapshot` shape so a report row can feed
//! `DataDiffViewer` without a conversion step.

use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::types::{Namespace, QueryResult};
use crate::snapshots::{Snapshot, SnapshotMeta};

use super::types::RunMeta;

const RUN_META_FILE: &str = "run.json";
const REPORT_FILE: &str = "report.json";
const AB_REPORT_FILE: &str = "report-ab.json";

pub struct CaptureStore {
    root: PathBuf,
}

fn validate_id(id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id).map_err(|_| "Invalid replay identifier".to_string())?;
    Ok(())
}

/// Workspace ids are `default` or `ws_<16 hex>` (cf. `WorkspaceManager`).
/// Validated because the id becomes a path segment.
fn validate_project_id(project_id: &str) -> Result<(), String> {
    if project_id.is_empty()
        || project_id.len() > 64
        || !project_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Invalid workspace identifier".to_string());
    }
    Ok(())
}

impl CaptureStore {
    pub fn new(root: PathBuf) -> Self {
        let _ = fs::create_dir_all(&root);
        Self { root }
    }

    /// Captures are scoped to a workspace
    pub fn scoped(data_dir: &std::path::Path, project_id: &str) -> Result<Self, String> {
        validate_project_id(project_id)?;
        Ok(Self::new(data_dir.join(project_id)))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn run_dir(&self, run_id: &str) -> Result<PathBuf, String> {
        validate_id(run_id)?;
        Ok(self.root.join(run_id))
    }

    fn entry_path(&self, run_id: &str, entry_id: &str) -> Result<PathBuf, String> {
        validate_id(entry_id)?;
        Ok(self.run_dir(run_id)?.join(format!("{}.json", entry_id)))
    }

    pub fn save_run_meta(&self, meta: &RunMeta) -> Result<(), String> {
        let dir = self.run_dir(&meta.run_id)?;
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create run directory: {}", e))?;
        let content = serde_json::to_string_pretty(meta)
            .map_err(|e| format!("Failed to serialize run metadata: {}", e))?;
        fs::write(dir.join(RUN_META_FILE), content)
            .map_err(|e| format!("Failed to write run metadata: {}", e))
    }

    pub fn load_run_meta(&self, run_id: &str) -> Result<RunMeta, String> {
        let path = self.run_dir(run_id)?.join(RUN_META_FILE);
        let content =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read run metadata: {}", e))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse run metadata: {}", e))
    }

    /// Keeps the report next to the rows it describes, so closing the tab (or
    /// the app) mid-run does not lose what the run found.
    pub fn save_report(
        &self,
        run_id: &str,
        report: &super::types::ReplayReport,
    ) -> Result<(), String> {
        let dir = self.run_dir(run_id)?;
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create run directory: {}", e))?;
        let content = serde_json::to_string(report)
            .map_err(|e| format!("Failed to serialize report: {}", e))?;
        fs::write(dir.join(REPORT_FILE), content)
            .map_err(|e| format!("Failed to write report: {}", e))
    }

    pub fn load_report(&self, run_id: &str) -> Result<super::types::ReplayReport, String> {
        let path = self.run_dir(run_id)?.join(REPORT_FILE);
        let content =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read report: {}", e))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse report: {}", e))
    }

    /// The A/B comparison is not a run of its own: it is stored next to its
    /// right-hand run, which is where the report reader looks for it.
    pub fn save_ab_report(
        &self,
        run_id: &str,
        report: &super::types::ReplayAbReport,
    ) -> Result<(), String> {
        let dir = self.run_dir(run_id)?;
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create run directory: {}", e))?;
        let content = serde_json::to_string(report)
            .map_err(|e| format!("Failed to serialize report: {}", e))?;
        fs::write(dir.join(AB_REPORT_FILE), content)
            .map_err(|e| format!("Failed to write report: {}", e))
    }

    pub fn load_ab_report(&self, run_id: &str) -> Result<super::types::ReplayAbReport, String> {
        let path = self.run_dir(run_id)?.join(AB_REPORT_FILE);
        let content =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read report: {}", e))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse report: {}", e))
    }

    /// The most recent A/B comparison of a set, if any.
    pub fn latest_ab_report(
        &self,
        set_slug: &str,
    ) -> Result<Option<super::types::ReplayAbReport>, String> {
        for run in self.list_runs(set_slug)? {
            if run.is_baseline {
                continue;
            }
            if let Ok(report) = self.load_ab_report(&run.run_id) {
                return Ok(Some(report));
            }
        }
        Ok(None)
    }

    /// The most recent run of a set that produced a report, if any.
    pub fn latest_report(
        &self,
        set_slug: &str,
    ) -> Result<Option<super::types::ReplayReport>, String> {
        for run in self.list_runs(set_slug)? {
            if run.is_baseline {
                continue;
            }
            if let Ok(report) = self.load_report(&run.run_id) {
                return Ok(Some(report));
            }
        }
        Ok(None)
    }

    /// Persists rows for one entry, returning the bytes written.
    ///
    /// `budget_left` is a hard bound: an entry that does not fit is not written
    /// at all and reports `Ok(None)`. Checking only after writing would let a
    /// single wide result blow past the run's budget entirely.
    #[allow(clippy::too_many_arguments)]
    pub fn save_entry(
        &self,
        run_id: &str,
        entry_id: &str,
        query: &str,
        driver_id: &str,
        connection_label: Option<&str>,
        namespace: Option<Namespace>,
        result: &QueryResult,
        max_rows: usize,
        budget_left: u64,
    ) -> Result<Option<u64>, String> {
        let path = self.entry_path(run_id, entry_id)?;
        let dir = self.run_dir(run_id)?;
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create run directory: {}", e))?;

        // Take before cloning: a large result must not be duplicated whole
        // just to drop most of it.
        let rows: Vec<_> = result.rows.iter().take(max_rows).cloned().collect();

        let snapshot = Snapshot {
            meta: SnapshotMeta {
                id: entry_id.to_string(),
                name: super::types::query_preview(query),
                description: None,
                source: query.to_string(),
                source_type: "query".to_string(),
                connection_name: connection_label.map(|s| s.to_string()),
                driver: Some(driver_id.to_string()),
                namespace,
                columns: result.columns.clone(),
                row_count: rows.len(),
                created_at: chrono::Utc::now().to_rfc3339(),
                file_size: 0,
            },
            rows,
        };

        let content = serde_json::to_string(&snapshot)
            .map_err(|e| format!("Failed to serialize capture: {}", e))?;
        let size = content.len() as u64;
        if size > budget_left {
            return Ok(None);
        }
        fs::write(&path, &content).map_err(|e| format!("Failed to write capture: {}", e))?;
        Ok(Some(size))
    }

    pub fn load_entry(&self, run_id: &str, entry_id: &str) -> Result<Snapshot, String> {
        let path = self.entry_path(run_id, entry_id)?;
        let content =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read capture: {}", e))?;
        let mut snapshot: Snapshot = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse capture: {}", e))?;
        snapshot.meta.file_size = content.len() as u64;
        Ok(snapshot)
    }

    pub fn delete_entry(&self, run_id: &str, entry_id: &str) -> Result<(), String> {
        let path = self.entry_path(run_id, entry_id)?;
        if !path.exists() {
            return Ok(());
        }
        fs::remove_file(&path).map_err(|e| format!("Failed to delete capture: {}", e))
    }

    pub fn has_entry(&self, run_id: &str, entry_id: &str) -> bool {
        self.entry_path(run_id, entry_id)
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    /// Runs for one set, newest first.
    pub fn list_runs(&self, set_slug: &str) -> Result<Vec<RunMeta>, String> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("Failed to read replays directory: {}", e)),
        };

        let mut runs = Vec::new();
        for entry in entries.flatten() {
            let Some(run_id) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            let Ok(meta) = self.load_run_meta(&run_id) else {
                continue;
            };
            if meta.set_slug == set_slug {
                runs.push(meta);
            }
        }

        runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(runs)
    }

    pub fn delete_run(&self, run_id: &str) -> Result<(), String> {
        let dir = self.run_dir(run_id)?;
        if !dir.exists() {
            return Ok(());
        }
        fs::remove_dir_all(&dir).map_err(|e| format!("Failed to delete run: {}", e))
    }

    /// Keeps the `retention` newest runs of a set, plus its baseline: dropping
    /// the baseline would leave later runs with nothing to compare against.
    pub fn prune(&self, set_slug: &str, retention: usize) -> Result<usize, String> {
        let runs = self.list_runs(set_slug)?;
        let mut deleted = 0;
        for (index, run) in runs.iter().enumerate() {
            if index < retention || run.is_baseline {
                continue;
            }
            self.delete_run(&run.run_id)?;
            deleted += 1;
        }
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::{ColumnInfo, Row, Value};
    use crate::replay::types::CaptureMode;

    fn store() -> (CaptureStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("qoredb_capture_{}", uuid::Uuid::new_v4()));
        (CaptureStore::new(dir.clone()), dir)
    }

    fn meta(run_id: &str, slug: &str, baseline: bool, started_at: &str) -> RunMeta {
        RunMeta {
            run_id: run_id.to_string(),
            project_id: "default".to_string(),
            set_slug: slug.to_string(),
            set_name: slug.to_string(),
            started_at: started_at.to_string(),
            finished_at: None,
            connection_label: None,
            driver_id: "postgres".to_string(),
            environment: "staging".to_string(),
            capture_mode: CaptureMode::Full,
            capture_stopped_reason: None,
            is_baseline: baseline,
            captured_bytes: 0,
            entry_count: 0,
        }
    }

    fn sample_result() -> QueryResult {
        QueryResult {
            columns: vec![ColumnInfo {
                name: "id".into(),
                data_type: "int".into(),
                nullable: false,
            }],
            rows: (0..5)
                .map(|i| Row {
                    values: vec![Value::Int(i)],
                })
                .collect(),
            affected_rows: None,
            execution_time_ms: 0.0,
        }
    }

    #[test]
    fn rejects_path_traversal_ids() {
        let (store, dir) = store();
        assert!(store.load_run_meta("../../etc").is_err());
        assert!(
            store
                .load_entry(&uuid::Uuid::new_v4().to_string(), "../x")
                .is_err()
        );
        assert!(store.delete_run("not-a-uuid").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn round_trips_a_capture() {
        let (store, dir) = store();
        let run_id = uuid::Uuid::new_v4().to_string();
        let entry_id = uuid::Uuid::new_v4().to_string();
        store
            .save_run_meta(&meta(&run_id, "checkout", true, "2026-08-21T10:00:00Z"))
            .unwrap();

        let written = store
            .save_entry(
                &run_id,
                &entry_id,
                "SELECT id FROM orders",
                "postgres",
                Some("staging"),
                None,
                &sample_result(),
                1000,
                u64::MAX,
            )
            .unwrap()
            .expect("the entry fits");
        assert!(written > 0);
        assert!(store.has_entry(&run_id, &entry_id));

        let snapshot = store.load_entry(&run_id, &entry_id).unwrap();
        assert_eq!(snapshot.rows.len(), 5);
        assert_eq!(snapshot.to_query_result().columns.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn capture_honours_the_row_bound() {
        let (store, dir) = store();
        let run_id = uuid::Uuid::new_v4().to_string();
        let entry_id = uuid::Uuid::new_v4().to_string();
        store
            .save_entry(
                &run_id,
                &entry_id,
                "SELECT id FROM orders",
                "postgres",
                None,
                None,
                &sample_result(),
                2,
                u64::MAX,
            )
            .unwrap();
        assert_eq!(store.load_entry(&run_id, &entry_id).unwrap().rows.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    /// The budget is a bound, not a tally: an entry that does not fit is not
    /// written at all, so a single wide result cannot blow past it.
    #[test]
    fn an_entry_that_does_not_fit_the_budget_is_not_written() {
        let (store, dir) = store();
        let run_id = uuid::Uuid::new_v4().to_string();
        let entry_id = uuid::Uuid::new_v4().to_string();

        let written = store
            .save_entry(
                &run_id,
                &entry_id,
                "SELECT id FROM orders",
                "postgres",
                None,
                None,
                &sample_result(),
                1000,
                8, // far below the serialized size
            )
            .unwrap();

        assert!(written.is_none(), "the caller is told nothing was stored");
        assert!(
            !store.has_entry(&run_id, &entry_id),
            "and nothing reached the disk"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_keeps_the_newest_runs_and_the_baseline() {
        let (store, dir) = store();
        let baseline = uuid::Uuid::new_v4().to_string();
        store
            .save_run_meta(&meta(&baseline, "checkout", true, "2026-08-01T10:00:00Z"))
            .unwrap();
        let mut later = Vec::new();
        for day in 10..14 {
            let id = uuid::Uuid::new_v4().to_string();
            store
                .save_run_meta(&meta(
                    &id,
                    "checkout",
                    false,
                    &format!("2026-08-{}T10:00:00Z", day),
                ))
                .unwrap();
            later.push(id);
        }

        assert_eq!(store.prune("checkout", 2).unwrap(), 2);
        let remaining = store.list_runs("checkout").unwrap();
        assert_eq!(remaining.len(), 3);
        assert!(remaining.iter().any(|r| r.run_id == baseline));

        let _ = fs::remove_dir_all(&dir);
    }

    /// Two workspaces may hold a set under the same slug. Neither may see —
    /// or delete — the other's runs.
    #[test]
    fn two_workspaces_sharing_a_slug_stay_isolated() {
        let root = std::env::temp_dir().join(format!("qoredb_ws_{}", uuid::Uuid::new_v4()));
        let a = CaptureStore::scoped(&root, "ws_aaaaaaaaaaaaaaaa").unwrap();
        let b = CaptureStore::scoped(&root, "ws_bbbbbbbbbbbbbbbb").unwrap();

        let run_a = uuid::Uuid::new_v4().to_string();
        let run_b = uuid::Uuid::new_v4().to_string();
        a.save_run_meta(&meta(&run_a, "checkout", true, "2026-08-21T10:00:00Z"))
            .unwrap();
        b.save_run_meta(&meta(&run_b, "checkout", true, "2026-08-21T11:00:00Z"))
            .unwrap();

        assert_eq!(a.list_runs("checkout").unwrap().len(), 1);
        assert_eq!(b.list_runs("checkout").unwrap().len(), 1);
        assert_eq!(a.list_runs("checkout").unwrap()[0].run_id, run_a);

        // Deleting the whole set in B leaves A untouched.
        for run in b.list_runs("checkout").unwrap() {
            b.delete_run(&run.run_id).unwrap();
        }
        assert!(b.list_runs("checkout").unwrap().is_empty());
        assert_eq!(
            a.list_runs("checkout").unwrap().len(),
            1,
            "workspace A lost its runs when B deleted its own set"
        );

        // And one cannot read the other's captures by run id.
        assert!(b.load_run_meta(&run_a).is_err());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_an_unsafe_workspace_id() {
        let root = std::env::temp_dir().join("qoredb_ws_validation");
        assert!(CaptureStore::scoped(&root, "../../etc").is_err());
        assert!(CaptureStore::scoped(&root, "").is_err());
        assert!(CaptureStore::scoped(&root, "ws/evil").is_err());
        assert!(CaptureStore::scoped(&root, "default").is_ok());
        assert!(CaptureStore::scoped(&root, "ws_49f7a110a4ef9f9b").is_ok());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn list_runs_only_returns_the_requested_set() {
        let (store, dir) = store();
        let a = uuid::Uuid::new_v4().to_string();
        let b = uuid::Uuid::new_v4().to_string();
        store
            .save_run_meta(&meta(&a, "checkout", false, "2026-08-21T10:00:00Z"))
            .unwrap();
        store
            .save_run_meta(&meta(&b, "billing", false, "2026-08-21T11:00:00Z"))
            .unwrap();

        let runs = store.list_runs("checkout").unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, a);

        let _ = fs::remove_dir_all(&dir);
    }
}

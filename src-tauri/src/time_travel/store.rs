// SPDX-License-Identifier: BUSL-1.1

//! Changelog Store
//!
//! Persistent, append-only store for row-level change records.
//! Follows the same JSONL + in-memory cache pattern as AuditStore.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use tracing::{debug, error, info, warn};

use super::types::{
    ChangeOperation, ChangelogEntry, ChangelogFilter, DiffRowStatus, TemporalDiff,
    TemporalDiffRow, TemporalDiffStats, TimelineEvent, TimeTravelConfig,
};
use crate::engine::types::Namespace;

/// Maximum entries kept in the in-memory cache.
const MAX_CACHE_ENTRIES: usize = 5_000;

/// Persistent changelog store with JSONL file backend.
pub struct ChangelogStore {
    /// In-memory cache of recent entries
    entries: RwLock<VecDeque<ChangelogEntry>>,
    /// Path to the changelog JSONL file
    log_path: PathBuf,
    /// Path to the configuration file
    config_path: PathBuf,
    /// Current configuration
    config: RwLock<TimeTravelConfig>,
    /// Tracked line count for the file (avoids O(n) recount)
    file_line_count: AtomicUsize,
}

impl ChangelogStore {
    pub fn new(data_dir: PathBuf) -> Self {
        let log_path = data_dir.join("changelog.jsonl");
        let config_path = data_dir.join("time-travel.json");

        if let Err(e) = fs::create_dir_all(&data_dir) {
            error!("Failed to create time-travel directory: {}", e);
        }

        let store = Self {
            entries: RwLock::new(VecDeque::with_capacity(MAX_CACHE_ENTRIES)),
            log_path,
            config_path,
            config: RwLock::new(TimeTravelConfig::default()),
            file_line_count: AtomicUsize::new(0),
        };

        store.load_config_from_disk();
        store.load_recent_entries();

        store
    }

    // ─── Configuration ─────────────────────────────────────────────────

    pub fn get_config(&self) -> TimeTravelConfig {
        self.config.read().clone()
    }

    pub fn update_config(&self, config: TimeTravelConfig) {
        *self.config.write() = config;
        self.save_config_to_disk();
    }

    pub fn is_enabled(&self) -> bool {
        self.config.read().enabled
    }

    /// Check if a table is excluded from capture.
    pub fn is_table_excluded(&self, table_name: &str) -> bool {
        let config = self.config.read();
        config
            .excluded_tables
            .iter()
            .any(|t| t.eq_ignore_ascii_case(table_name))
    }

    /// Check if capture should happen for the given environment.
    pub fn should_capture(&self, table_name: &str, environment: &str) -> bool {
        let config = self.config.read();
        if !config.enabled {
            return false;
        }
        if self.is_table_excluded(table_name) {
            return false;
        }
        if config.production_only && environment != "production" {
            return false;
        }
        true
    }

    fn load_config_from_disk(&self) {
        if !self.config_path.exists() {
            return;
        }
        match fs::read_to_string(&self.config_path) {
            Ok(content) => match serde_json::from_str::<TimeTravelConfig>(&content) {
                Ok(config) => {
                    *self.config.write() = config;
                    debug!("Loaded time-travel config");
                }
                Err(e) => warn!("Failed to parse time-travel config: {}", e),
            },
            Err(e) => warn!("Failed to read time-travel config: {}", e),
        }
    }

    fn save_config_to_disk(&self) {
        let config = self.config.read().clone();
        match serde_json::to_string_pretty(&config) {
            Ok(json) => {
                if let Err(e) = fs::write(&self.config_path, json) {
                    error!("Failed to write time-travel config: {}", e);
                }
            }
            Err(e) => error!("Failed to serialize time-travel config: {}", e),
        }
    }

    // ─── Recording ─────────────────────────────────────────────────────

    /// Record a changelog entry. Best-effort: never blocks the caller on failure.
    pub fn record(&self, entry: ChangelogEntry) {
        if !self.is_enabled() {
            return;
        }

        // Add to in-memory cache
        {
            let mut entries = self.entries.write();
            if entries.len() >= MAX_CACHE_ENTRIES {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }

        // Append to file
        if let Err(e) = self.append_to_file(&entry) {
            error!("Failed to write changelog entry: {}", e);
        }

        self.maybe_rotate();
    }

    fn append_to_file(&self, entry: &ChangelogEntry) -> std::io::Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;

        let mut writer = BufWriter::new(file);
        let json = serde_json::to_string(entry)?;
        writeln!(writer, "{}", json)?;
        writer.flush()?;

        self.file_line_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn load_recent_entries(&self) {
        if !self.log_path.exists() {
            return;
        }

        match File::open(&self.log_path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                let mut entries = self.entries.write();
                let mut line_count: usize = 0;

                for line in reader.lines() {
                    if let Ok(line) = line {
                        line_count += 1;
                        if let Ok(entry) = serde_json::from_str::<ChangelogEntry>(&line) {
                            if entries.len() >= MAX_CACHE_ENTRIES {
                                entries.pop_front();
                            }
                            entries.push_back(entry);
                        }
                    }
                }

                self.file_line_count.store(line_count, Ordering::Relaxed);
                debug!("Loaded {} changelog entries from file", entries.len());
            }
            Err(e) => warn!("Failed to load changelog file: {}", e),
        }
    }

    // ─── Rotation ──────────────────────────────────────────────────────

    fn maybe_rotate(&self) {
        let line_count = self.file_line_count.load(Ordering::Relaxed);
        let max_entries = self.config.read().max_entries;
        if line_count <= max_entries {
            return;
        }

        let entries_to_keep = max_entries * 3 / 4;
        match self.rotate_file(entries_to_keep) {
            Ok(removed) => {
                self.file_line_count.fetch_sub(removed, Ordering::Relaxed);
                info!("Rotated changelog, removed {} old entries", removed);
            }
            Err(e) => error!("Failed to rotate changelog: {}", e),
        }
    }

    fn rotate_file(&self, keep_count: usize) -> std::io::Result<usize> {
        let file = File::open(&self.log_path)?;
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();

        let total = lines.len();
        if total <= keep_count {
            return Ok(0);
        }

        let skip = total - keep_count;
        let to_keep: Vec<&String> = lines.iter().skip(skip).collect();

        let temp_path = self.log_path.with_extension("jsonl.tmp");
        {
            let file = File::create(&temp_path)?;
            let mut writer = BufWriter::new(file);
            for line in to_keep {
                writeln!(writer, "{}", line)?;
            }
            writer.flush()?;
        }

        fs::rename(&temp_path, &self.log_path)?;
        Ok(skip)
    }

    // ─── Querying ──────────────────────────────────────────────────────

    /// Get timeline events for a table, ordered by timestamp DESC.
    pub fn get_timeline(
        &self,
        namespace: &Namespace,
        table_name: &str,
        filter: &ChangelogFilter,
    ) -> Vec<TimelineEvent> {
        let entries = self.entries.read();
        let limit = filter.limit.unwrap_or(100);
        let offset = filter.offset.unwrap_or(0);

        entries
            .iter()
            .rev()
            .filter(|e| self.matches_table(e, namespace, table_name))
            .filter(|e| self.matches_filter(e, filter))
            .skip(offset)
            .take(limit)
            .map(|e| TimelineEvent {
                timestamp: e.timestamp,
                operation: e.operation,
                row_count: 1,
                session_id: e.session_id.clone(),
                connection_name: e.connection_name.clone(),
                primary_key: Some(e.primary_key.clone()),
                entry_id: e.id,
            })
            .collect()
    }

    /// Get the total count of events for a table (for pagination).
    pub fn get_timeline_count(&self, namespace: &Namespace, table_name: &str) -> usize {
        let entries = self.entries.read();
        entries
            .iter()
            .filter(|e| self.matches_table(e, namespace, table_name))
            .count()
    }

    /// Get filtered changelog entries.
    pub fn get_entries(&self, filter: &ChangelogFilter) -> Vec<ChangelogEntry> {
        let entries = self.entries.read();
        let limit = filter.limit.unwrap_or(100);
        let offset = filter.offset.unwrap_or(0);

        entries
            .iter()
            .rev()
            .filter(|e| self.matches_filter(e, filter))
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get the full history of a specific row, ordered by timestamp DESC.
    pub fn get_row_history(
        &self,
        namespace: &Namespace,
        table_name: &str,
        primary_key: &std::collections::HashMap<String, serde_json::Value>,
        limit: Option<usize>,
    ) -> Vec<ChangelogEntry> {
        let entries = self.entries.read();
        let limit = limit.unwrap_or(50);

        entries
            .iter()
            .rev()
            .filter(|e| self.matches_table(e, namespace, table_name))
            .filter(|e| pk_matches(&e.primary_key, primary_key))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get a single entry by ID.
    pub fn get_entry(&self, entry_id: &uuid::Uuid) -> Option<ChangelogEntry> {
        let entries = self.entries.read();
        entries.iter().find(|e| e.id == *entry_id).cloned()
    }

    /// Compute a temporal diff between two timestamps for a table.
    ///
    /// Replays all changes between t1 and t2 to determine what was added,
    /// modified, or removed.
    pub fn compute_temporal_diff(
        &self,
        namespace: &Namespace,
        table_name: &str,
        t1: DateTime<Utc>,
        t2: DateTime<Utc>,
        limit: Option<usize>,
    ) -> TemporalDiff {
        let entries = self.entries.read();
        let limit = limit.unwrap_or(10_000);

        // Collect all entries between t1 and t2 for this table, ordered by timestamp ASC
        let mut relevant: Vec<&ChangelogEntry> = entries
            .iter()
            .filter(|e| self.matches_table(e, namespace, table_name))
            .filter(|e| e.timestamp > t1 && e.timestamp <= t2)
            .collect();
        relevant.sort_by_key(|e| e.timestamp);

        // Replay changes to build the diff
        // Key: serialized PK → accumulated state
        let mut diff_rows: std::collections::HashMap<String, TemporalDiffRow> =
            std::collections::HashMap::new();
        let mut all_columns: std::collections::HashSet<String> = std::collections::HashSet::new();

        for entry in &relevant {
            let pk_key = serialize_pk(&entry.primary_key);

            // Collect column names
            if let Some(before) = &entry.before {
                all_columns.extend(before.keys().cloned());
            }
            if let Some(after) = &entry.after {
                all_columns.extend(after.keys().cloned());
            }

            let existing = diff_rows.get(&pk_key);

            match entry.operation {
                ChangeOperation::Insert => {
                    diff_rows.insert(
                        pk_key,
                        TemporalDiffRow {
                            primary_key: entry.primary_key.clone(),
                            state_at_t1: existing.and_then(|e| e.state_at_t1.clone()),
                            state_at_t2: entry.after.clone(),
                            changed_columns: vec![],
                            status: if existing.is_some() {
                                DiffRowStatus::Modified
                            } else {
                                DiffRowStatus::Added
                            },
                        },
                    );
                }
                ChangeOperation::Update => {
                    let t1_state = existing
                        .and_then(|e| e.state_at_t1.clone())
                        .or_else(|| entry.before.clone());
                    diff_rows.insert(
                        pk_key,
                        TemporalDiffRow {
                            primary_key: entry.primary_key.clone(),
                            state_at_t1: t1_state,
                            state_at_t2: entry.after.clone(),
                            changed_columns: entry.changed_columns.clone(),
                            status: DiffRowStatus::Modified,
                        },
                    );
                }
                ChangeOperation::Delete => {
                    let t1_state = existing
                        .and_then(|e| e.state_at_t1.clone())
                        .or_else(|| entry.before.clone());

                    if existing.map_or(false, |e| e.status == DiffRowStatus::Added) {
                        // Was added then deleted — net effect is nothing
                        diff_rows.remove(&pk_key);
                    } else {
                        diff_rows.insert(
                            pk_key,
                            TemporalDiffRow {
                                primary_key: entry.primary_key.clone(),
                                state_at_t1: t1_state,
                                state_at_t2: None,
                                changed_columns: vec![],
                                status: DiffRowStatus::Removed,
                            },
                        );
                    }
                }
            }
        }

        // Recompute changed_columns for Modified rows
        for row in diff_rows.values_mut() {
            if row.status == DiffRowStatus::Modified {
                if let (Some(t1), Some(t2)) = (&row.state_at_t1, &row.state_at_t2) {
                    row.changed_columns = t2
                        .iter()
                        .filter(|(k, v)| t1.get(*k) != Some(v))
                        .map(|(k, _)| k.clone())
                        .collect();
                }
            }
        }

        let mut rows: Vec<TemporalDiffRow> = diff_rows.into_values().take(limit).collect();
        rows.sort_by(|a, b| {
            serialize_pk(&a.primary_key).cmp(&serialize_pk(&b.primary_key))
        });

        let stats = TemporalDiffStats {
            added: rows.iter().filter(|r| r.status == DiffRowStatus::Added).count(),
            modified: rows
                .iter()
                .filter(|r| r.status == DiffRowStatus::Modified)
                .count(),
            removed: rows
                .iter()
                .filter(|r| r.status == DiffRowStatus::Removed)
                .count(),
            total_changes: rows.len(),
        };

        let mut columns: Vec<String> = all_columns.into_iter().collect();
        columns.sort();

        TemporalDiff {
            columns,
            rows,
            stats,
        }
    }

    /// Reconstruct the state of a row at a given timestamp.
    ///
    /// Finds the last changelog entry for this PK at or before the timestamp
    /// and returns the resulting state.
    pub fn get_row_state_at(
        &self,
        namespace: &Namespace,
        table_name: &str,
        primary_key: &std::collections::HashMap<String, serde_json::Value>,
        timestamp: DateTime<Utc>,
    ) -> Option<std::collections::HashMap<String, serde_json::Value>> {
        let entries = self.entries.read();

        // Find the last entry for this PK at or before the timestamp
        let last_entry = entries
            .iter()
            .filter(|e| self.matches_table(e, namespace, table_name))
            .filter(|e| pk_matches(&e.primary_key, primary_key))
            .filter(|e| e.timestamp <= timestamp)
            .last();

        match last_entry {
            Some(entry) => match entry.operation {
                ChangeOperation::Insert | ChangeOperation::Update => entry.after.clone(),
                ChangeOperation::Delete => None,
            },
            None => None,
        }
    }

    // ─── Maintenance ───────────────────────────────────────────────────

    /// Clear all changelog entries for a specific table.
    pub fn clear_table(&self, namespace: &Namespace, table_name: &str) {
        {
            let mut entries = self.entries.write();
            entries.retain(|e| !self.matches_table(e, namespace, table_name));
        }
        self.rewrite_file_from_cache();
        info!("Cleared changelog for {}.{}", namespace.database, table_name);
    }

    /// Clear all changelog entries.
    pub fn clear_all(&self) {
        {
            let mut entries = self.entries.write();
            entries.clear();
        }
        if let Err(e) = fs::write(&self.log_path, "") {
            error!("Failed to clear changelog file: {}", e);
        }
        self.file_line_count.store(0, Ordering::Relaxed);
        info!("Cleared all changelog entries");
    }

    /// Purge entries older than retention_days.
    pub fn purge_expired(&self) {
        let retention_days = self.config.read().retention_days;
        if retention_days == 0 {
            return; // Unlimited retention
        }

        let cutoff = Utc::now() - Duration::days(retention_days as i64);

        let removed = {
            let mut entries = self.entries.write();
            let before = entries.len();
            entries.retain(|e| e.timestamp >= cutoff);
            before - entries.len()
        };

        if removed > 0 {
            self.rewrite_file_from_cache();
            info!(
                "Purged {} expired changelog entries (retention: {} days)",
                removed, retention_days
            );
        }
    }

    /// Export filtered changelog entries as JSON.
    pub fn export(&self, filter: &ChangelogFilter) -> String {
        let entries = self.get_entries(filter);
        serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string())
    }

    // ─── Helpers ───────────────────────────────────────────────────────

    fn matches_table(&self, entry: &ChangelogEntry, namespace: &Namespace, table_name: &str) -> bool {
        entry.namespace.database == namespace.database
            && entry.namespace.schema == namespace.schema
            && entry.table_name == table_name
    }

    fn matches_filter(&self, entry: &ChangelogEntry, filter: &ChangelogFilter) -> bool {
        if let Some(ref table) = filter.table_name {
            if entry.table_name != *table {
                return false;
            }
        }
        if let Some(ref ns) = filter.namespace {
            if entry.namespace.database != ns.database || entry.namespace.schema != ns.schema {
                return false;
            }
        }
        if let Some(op) = filter.operation {
            if entry.operation != op {
                return false;
            }
        }
        if let Some(ref sid) = filter.session_id {
            if entry.session_id != *sid {
                return false;
            }
        }
        if let Some(ref cn) = filter.connection_name {
            if entry.connection_name.as_deref() != Some(cn.as_str()) {
                return false;
            }
        }
        if let Some(ref env) = filter.environment {
            if entry.environment != *env {
                return false;
            }
        }
        if let Some(from) = filter.from_timestamp {
            if entry.timestamp < from {
                return false;
            }
        }
        if let Some(to) = filter.to_timestamp {
            if entry.timestamp > to {
                return false;
            }
        }
        if let Some(ref pk_search) = filter.primary_key_search {
            let pk_str = serialize_pk(&entry.primary_key);
            if !pk_str.to_lowercase().contains(&pk_search.to_lowercase()) {
                return false;
            }
        }
        true
    }

    /// Rewrite the JSONL file from the in-memory cache.
    fn rewrite_file_from_cache(&self) {
        let entries = self.entries.read();
        match File::create(&self.log_path) {
            Ok(file) => {
                let mut writer = BufWriter::new(file);
                let mut count = 0;
                for entry in entries.iter() {
                    if let Ok(json) = serde_json::to_string(entry) {
                        let _ = writeln!(writer, "{}", json);
                        count += 1;
                    }
                }
                let _ = writer.flush();
                self.file_line_count.store(count, Ordering::Relaxed);
            }
            Err(e) => error!("Failed to rewrite changelog file: {}", e),
        }
    }
}

// ─── Free functions ────────────────────────────────────────────────────────

/// Check if two primary key maps match.
fn pk_matches(
    a: &std::collections::HashMap<String, serde_json::Value>,
    b: &std::collections::HashMap<String, serde_json::Value>,
) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().all(|(k, v)| b.get(k) == Some(v))
}

/// Serialize a PK map into a deterministic string for hashing.
fn serialize_pk(pk: &std::collections::HashMap<String, serde_json::Value>) -> String {
    let mut pairs: Vec<_> = pk.iter().collect();
    pairs.sort_by_key(|(k, _)| *k);
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn make_entry(
        table: &str,
        op: ChangeOperation,
        pk: HashMap<String, serde_json::Value>,
        before: Option<HashMap<String, serde_json::Value>>,
        after: Option<HashMap<String, serde_json::Value>>,
    ) -> ChangelogEntry {
        ChangelogEntry {
            id: uuid::Uuid::new_v4(),
            timestamp: Utc::now(),
            session_id: "test-session".to_string(),
            driver_id: "postgres".to_string(),
            namespace: Namespace {
                database: "testdb".to_string(),
                schema: Some("public".to_string()),
            },
            table_name: table.to_string(),
            operation: op,
            primary_key: pk,
            before,
            after,
            changed_columns: vec![],
            connection_name: Some("TestConn".to_string()),
            environment: "development".to_string(),
        }
    }

    fn pk(id: i64) -> HashMap<String, serde_json::Value> {
        let mut m = HashMap::new();
        m.insert("id".to_string(), serde_json::json!(id));
        m
    }

    fn row(id: i64, name: &str) -> HashMap<String, serde_json::Value> {
        let mut m = HashMap::new();
        m.insert("id".to_string(), serde_json::json!(id));
        m.insert("name".to_string(), serde_json::json!(name));
        m
    }

    #[test]
    fn test_record_and_retrieve() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());

        let entry = make_entry(
            "users",
            ChangeOperation::Insert,
            pk(1),
            None,
            Some(row(1, "Alice")),
        );
        store.record(entry);

        let ns = Namespace {
            database: "testdb".to_string(),
            schema: Some("public".to_string()),
        };
        let events = store.get_timeline(&ns, "users", &ChangelogFilter::default());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].operation, ChangeOperation::Insert);
    }

    #[test]
    fn test_filter_by_operation() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());

        store.record(make_entry(
            "users",
            ChangeOperation::Insert,
            pk(1),
            None,
            Some(row(1, "Alice")),
        ));
        store.record(make_entry(
            "users",
            ChangeOperation::Update,
            pk(1),
            Some(row(1, "Alice")),
            Some(row(1, "Bob")),
        ));

        let filter = ChangelogFilter {
            operation: Some(ChangeOperation::Update),
            ..Default::default()
        };
        let entries = store.get_entries(&filter);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].operation, ChangeOperation::Update);
    }

    #[test]
    fn test_row_history() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());

        let ns = Namespace {
            database: "testdb".to_string(),
            schema: Some("public".to_string()),
        };

        store.record(make_entry(
            "users",
            ChangeOperation::Insert,
            pk(1),
            None,
            Some(row(1, "Alice")),
        ));
        store.record(make_entry(
            "users",
            ChangeOperation::Update,
            pk(1),
            Some(row(1, "Alice")),
            Some(row(1, "Bob")),
        ));
        // Different row — should not appear
        store.record(make_entry(
            "users",
            ChangeOperation::Insert,
            pk(2),
            None,
            Some(row(2, "Eve")),
        ));

        let history = store.get_row_history(&ns, "users", &pk(1), None);
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_clear_table() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());

        let ns = Namespace {
            database: "testdb".to_string(),
            schema: Some("public".to_string()),
        };

        store.record(make_entry(
            "users",
            ChangeOperation::Insert,
            pk(1),
            None,
            Some(row(1, "Alice")),
        ));
        store.record(make_entry(
            "orders",
            ChangeOperation::Insert,
            pk(100),
            None,
            Some(row(100, "Order1")),
        ));

        store.clear_table(&ns, "users");

        let users = store.get_timeline(&ns, "users", &ChangelogFilter::default());
        assert_eq!(users.len(), 0);

        let orders = store.get_timeline(&ns, "orders", &ChangelogFilter::default());
        assert_eq!(orders.len(), 1);
    }

    #[test]
    fn test_config_persistence() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());

        let mut config = store.get_config();
        config.retention_days = 7;
        config.enabled = false;
        store.update_config(config);

        // Reload from disk
        let store2 = ChangelogStore::new(tmp.path().to_path_buf());
        let config2 = store2.get_config();
        assert_eq!(config2.retention_days, 7);
        assert!(!config2.enabled);
    }

    #[test]
    fn test_should_capture() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());

        assert!(store.should_capture("users", "development"));

        // Disable
        let mut config = store.get_config();
        config.enabled = false;
        store.update_config(config);
        assert!(!store.should_capture("users", "development"));

        // Re-enable, production only
        let mut config = store.get_config();
        config.enabled = true;
        config.production_only = true;
        store.update_config(config);
        assert!(!store.should_capture("users", "development"));
        assert!(store.should_capture("users", "production"));

        // Excluded table
        let mut config = store.get_config();
        config.production_only = false;
        config.excluded_tables = vec!["migrations".to_string()];
        store.update_config(config);
        assert!(store.should_capture("users", "development"));
        assert!(!store.should_capture("migrations", "development"));
    }

    #[test]
    fn test_temporal_diff() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());

        let ns = Namespace {
            database: "testdb".to_string(),
            schema: Some("public".to_string()),
        };

        let t0 = Utc::now() - Duration::seconds(10);

        store.record(make_entry(
            "users",
            ChangeOperation::Insert,
            pk(1),
            None,
            Some(row(1, "Alice")),
        ));
        store.record(make_entry(
            "users",
            ChangeOperation::Update,
            pk(1),
            Some(row(1, "Alice")),
            Some(row(1, "Bob")),
        ));
        store.record(make_entry(
            "users",
            ChangeOperation::Insert,
            pk(2),
            None,
            Some(row(2, "Eve")),
        ));

        let t1 = Utc::now() + Duration::seconds(1);

        let diff = store.compute_temporal_diff(&ns, "users", t0, t1, None);

        assert_eq!(diff.stats.total_changes, 2); // id=1 modified (insert+update), id=2 added
        assert_eq!(diff.stats.added, 1);
        // id=1 was inserted then updated in the window — it's "Added" because state_at_t1 is None
        // Actually the first insert sets it as Added, then the update sets it as Modified
        // But state_at_t1 was None (from the insert being the first entry), so it stays Added
        // Let me verify: the insert creates Added, then the update sees existing with state_at_t1=None
        // so t1_state = existing.state_at_t1 (None) or entry.before. The update's before is Some(row(1, Alice))
        // So t1_state = None (from existing). Status = Modified.
        // So: id=1 = Modified, id=2 = Added
    }

    #[test]
    fn test_pk_matches() {
        let a = pk(1);
        let b = pk(1);
        let c = pk(2);
        assert!(pk_matches(&a, &b));
        assert!(!pk_matches(&a, &c));
    }
}

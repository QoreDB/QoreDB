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
use parking_lot::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

use super::privacy::{HistoryPrivacy, key_is_available, value_is_unavailable};
use super::types::{
    ChangeOperation, ChangelogEntry, ChangelogFilter, ChangelogScope, DiffRowStatus, TemporalDiff,
    TemporalDiffRow, TemporalDiffStats, TimeTravelConfig, TimelineEvent,
};
use crate::engine::types::Namespace;

/// Maximum entries kept in the in-memory cache.
const MAX_CACHE_ENTRIES: usize = 5_000;

/// Persistent changelog store with JSONL file backend.
pub struct ChangelogStore {
    /// Serializes append/rotation/clear so a rewrite cannot lose a new capture.
    file_lock: Mutex<()>,
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
            file_lock: Mutex::new(()),
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
                if let Err(e) =
                    crate::atomic_write::write_atomic(&self.config_path, json.as_bytes())
                {
                    error!("Failed to write time-travel config: {}", e);
                }
            }
            Err(e) => error!("Failed to serialize time-travel config: {}", e),
        }
    }

    /// Record a changelog entry. Best-effort: never blocks the caller on failure.
    pub fn record(&self, entry: ChangelogEntry) {
        self.record_with_masking(entry, None);
    }

    pub fn record_with_masking(
        &self,
        mut entry: ChangelogEntry,
        masking: Option<&qore_core::masking::ConnectionMasking>,
    ) {
        let _file_guard = self.file_lock.lock();
        if !self.is_enabled() {
            return;
        }
        HistoryPrivacy::new(&self.config.read().sensitive_columns, masking).protect(&mut entry);

        {
            let mut entries = self.entries.write();
            if entries.len() >= MAX_CACHE_ENTRIES {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }

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

                for line in reader.lines().map_while(Result::ok) {
                    line_count += 1;
                    if let Ok(entry) = serde_json::from_str::<ChangelogEntry>(&line) {
                        if entries.len() >= MAX_CACHE_ENTRIES {
                            entries.pop_front();
                        }
                        entries.push_back(entry);
                    }
                }

                self.file_line_count.store(line_count, Ordering::Relaxed);
                debug!("Loaded {} changelog entries from file", entries.len());
            }
            Err(e) => warn!("Failed to load changelog file: {}", e),
        }
    }

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
        let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

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

    pub fn can_identify_row(
        &self,
        scope: &ChangelogScope,
        table: &str,
        key: &std::collections::HashMap<String, serde_json::Value>,
    ) -> bool {
        HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        )
        .can_identify(table, key)
    }

    /// Get timeline events for a table, ordered by timestamp DESC.
    pub fn get_timeline(
        &self,
        scope: &ChangelogScope,
        namespace: &Namespace,
        table_name: &str,
        filter: &ChangelogFilter,
    ) -> Vec<TimelineEvent> {
        let entries = self.entries.read();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        let limit = filter.limit.unwrap_or(100);
        let offset = filter.offset.unwrap_or(0);

        entries
            .iter()
            .rev()
            .filter(|e| scope.contains(e) && self.matches_table(e, namespace, table_name))
            .filter(|e| self.matches_filter(e, filter, &privacy))
            .skip(offset)
            .take(limit)
            .map(|e| {
                let mut primary_key = e.primary_key.clone();
                privacy.protect_map(&e.table_name, &mut primary_key);
                TimelineEvent {
                    timestamp: e.timestamp,
                    operation: e.operation,
                    row_count: 1,
                    session_id: e.session_id.clone(),
                    connection_name: e.connection_name.clone(),
                    primary_key: key_is_available(&primary_key).then_some(primary_key),
                    entry_id: e.id,
                }
            })
            .collect()
    }

    /// Get the total count of events for a table (for pagination).
    pub fn get_timeline_count(
        &self,
        scope: &ChangelogScope,
        namespace: &Namespace,
        table_name: &str,
        filter: &ChangelogFilter,
    ) -> usize {
        let entries = self.entries.read();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        entries
            .iter()
            .filter(|e| scope.contains(e) && self.matches_table(e, namespace, table_name))
            .filter(|e| self.matches_filter(e, filter, &privacy))
            .count()
    }

    /// Get filtered changelog entries.
    pub fn get_entries(
        &self,
        scope: &ChangelogScope,
        filter: &ChangelogFilter,
    ) -> Vec<ChangelogEntry> {
        let entries = self.entries.read();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        let limit = filter.limit.unwrap_or(100);
        let offset = filter.offset.unwrap_or(0);

        entries
            .iter()
            .rev()
            .filter(|e| scope.contains(e))
            .filter(|e| self.matches_filter(e, filter, &privacy))
            .skip(offset)
            .take(limit)
            .map(|e| privacy.project(e))
            .collect()
    }

    /// Get the full history of a specific row, ordered by timestamp DESC.
    pub fn get_row_history(
        &self,
        scope: &ChangelogScope,
        namespace: &Namespace,
        table_name: &str,
        primary_key: &std::collections::HashMap<String, serde_json::Value>,
        limit: Option<usize>,
    ) -> Vec<ChangelogEntry> {
        let entries = self.entries.read();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        if !privacy.can_identify(table_name, primary_key) {
            return Vec::new();
        }
        let limit = limit.unwrap_or(50);

        entries
            .iter()
            .rev()
            .filter(|e| scope.contains(e) && self.matches_table(e, namespace, table_name))
            .filter(|e| key_is_available(&e.primary_key) && pk_matches(&e.primary_key, primary_key))
            .take(limit)
            .map(|e| privacy.project(e))
            .collect()
    }

    pub fn get_entry(
        &self,
        scope: &ChangelogScope,
        entry_id: &uuid::Uuid,
    ) -> Option<ChangelogEntry> {
        let entries = self.entries.read();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        entries
            .iter()
            .find(|e| scope.contains(e) && e.id == *entry_id)
            .map(|e| privacy.project(e))
    }

    /// Compute a temporal diff between two timestamps for a table.
    ///
    /// Replays all changes between t1 and t2 to determine what was added,
    /// modified, or removed.
    pub fn compute_temporal_diff(
        &self,
        scope: &ChangelogScope,
        namespace: &Namespace,
        table_name: &str,
        t1: DateTime<Utc>,
        t2: DateTime<Utc>,
        limit: Option<usize>,
    ) -> TemporalDiff {
        let entries = self.entries.read();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        let limit = limit.unwrap_or(10_000);

        let mut relevant: Vec<&ChangelogEntry> = entries
            .iter()
            .filter(|e| scope.contains(e) && self.matches_table(e, namespace, table_name))
            .filter(|e| e.timestamp > t1 && e.timestamp <= t2)
            .collect();
        relevant.sort_by_key(|e| e.timestamp);

        // Replay changes keyed by serialized PK.
        let mut diff_rows: std::collections::HashMap<String, TemporalDiffRow> =
            std::collections::HashMap::new();
        let mut all_columns: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut incomplete_keys = std::collections::HashSet::new();

        let mut protected_values = false;
        // Project one entry at a time; large row images must not be copied into a second cache.
        for raw_entry in relevant {
            let entry = privacy.project(raw_entry);
            if !key_is_available(&entry.primary_key) {
                protected_values = true;
                continue;
            }
            protected_values |= [&entry.before, &entry.after]
                .into_iter()
                .flatten()
                .any(|image| image.values().any(value_is_unavailable));
            let pk_key = serialize_pk(&entry.primary_key);
            if (entry.operation != ChangeOperation::Insert && entry.before.is_none())
                || (entry.operation != ChangeOperation::Delete && entry.after.is_none())
            {
                incomplete_keys.insert(pk_key.clone());
            }

            if let Some(before) = &entry.before {
                all_columns.extend(before.keys().cloned());
            }
            if let Some(after) = &entry.after {
                all_columns.extend(after.keys().cloned());
            }

            // Preserve the first before-image, including None for a row that
            // did not exist at t1. Later mutations only advance the final state.
            diff_rows
                .entry(pk_key)
                .and_modify(|row| row.state_at_t2 = entry.after.clone())
                .or_insert_with(|| TemporalDiffRow {
                    primary_key: entry.primary_key.clone(),
                    state_at_t1: entry.before.clone(),
                    state_at_t2: entry.after.clone(),
                    changed_columns: vec![],
                    status: DiffRowStatus::Modified,
                });
        }

        let incomplete = protected_values || !incomplete_keys.is_empty();
        let mut rows: Vec<TemporalDiffRow> = diff_rows
            .into_iter()
            .filter(|(key, _)| !incomplete_keys.contains(key))
            .map(|(_, row)| row)
            .filter_map(|mut row| {
                row.status = match (&row.state_at_t1, &row.state_at_t2) {
                    (None, None) => return None,
                    (None, Some(_)) => DiffRowStatus::Added,
                    (Some(_), None) => DiffRowStatus::Removed,
                    (Some(before), Some(after)) => {
                        if before == after {
                            return None;
                        }
                        let columns: std::collections::BTreeSet<_> =
                            before.keys().chain(after.keys()).collect();
                        row.changed_columns = columns
                            .into_iter()
                            .filter(|column| before.get(*column) != after.get(*column))
                            .cloned()
                            .collect();
                        DiffRowStatus::Modified
                    }
                };
                Some(row)
            })
            .collect();
        rows.sort_by(|a, b| serialize_pk(&a.primary_key).cmp(&serialize_pk(&b.primary_key)));

        let stats = TemporalDiffStats {
            added: rows
                .iter()
                .filter(|r| r.status == DiffRowStatus::Added)
                .count(),
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

        let truncated = rows.len() > limit;
        rows.truncate(limit);
        let mut columns: Vec<String> = all_columns.into_iter().collect();
        columns.sort();

        TemporalDiff {
            columns,
            rows,
            stats,
            truncated,
            incomplete,
        }
    }

    /// Reconstruct the state of a row at a given timestamp.
    ///
    /// Finds the last changelog entry for this PK at or before the timestamp
    /// and returns the resulting state.
    pub fn get_row_state_at(
        &self,
        scope: &ChangelogScope,
        namespace: &Namespace,
        table_name: &str,
        primary_key: &std::collections::HashMap<String, serde_json::Value>,
        timestamp: DateTime<Utc>,
    ) -> Option<std::collections::HashMap<String, serde_json::Value>> {
        let entries = self.entries.read();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );

        if !privacy.can_identify(table_name, primary_key) {
            return None;
        }
        let last_entry = entries
            .iter()
            .filter(|e| scope.contains(e) && self.matches_table(e, namespace, table_name))
            .filter(|e| key_is_available(&e.primary_key) && pk_matches(&e.primary_key, primary_key))
            .filter(|e| e.timestamp <= timestamp)
            .last()
            .map(|e| privacy.project(e));

        match last_entry {
            Some(entry) => match entry.operation {
                ChangeOperation::Insert | ChangeOperation::Update => entry.after.clone(),
                ChangeOperation::Delete => None,
            },
            None => None,
        }
    }

    /// Clear all changelog entries for a specific table.
    pub fn clear_table(
        &self,
        scope: &ChangelogScope,
        namespace: &Namespace,
        table_name: &str,
    ) -> Result<(), String> {
        let _file_guard = self.file_lock.lock();
        // The file may contain older entries absent from the bounded cache.
        // Preserve those records, including legacy/unparseable lines.
        let mut retained = Vec::new();
        let mut count = 0;
        match File::open(&self.log_path) {
            Ok(file) => {
                for line in BufReader::new(file).lines() {
                    let line = line.map_err(|e| format!("Failed to read changelog: {e}"))?;
                    let remove = serde_json::from_str::<ChangelogEntry>(&line).is_ok_and(|e| {
                        scope.contains(&e) && self.matches_table(&e, namespace, table_name)
                    });
                    if !remove {
                        retained.extend_from_slice(line.as_bytes());
                        retained.push(b'\n');
                        count += 1;
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Failed to read changelog: {e}")),
        }
        crate::atomic_write::write_atomic(&self.log_path, &retained)
            .map_err(|e| format!("Failed to clear changelog: {e}"))?;
        self.entries
            .write()
            .retain(|e| !(scope.contains(e) && self.matches_table(e, namespace, table_name)));
        self.file_line_count.store(count, Ordering::Relaxed);
        info!(
            "Cleared changelog for {}.{}",
            namespace.database, table_name
        );
        Ok(())
    }

    pub fn clear_all(&self) {
        let _file_guard = self.file_lock.lock();
        {
            let mut entries = self.entries.write();
            entries.clear();
        }
        if let Err(e) = crate::atomic_write::write_atomic(&self.log_path, b"") {
            error!("Failed to clear changelog file: {}", e);
        }
        self.file_line_count.store(0, Ordering::Relaxed);
        info!("Cleared all changelog entries");
    }

    /// Purge entries older than retention_days.
    pub fn purge_expired(&self) {
        let _file_guard = self.file_lock.lock();
        let retention_days = self.config.read().retention_days;
        if retention_days == 0 {
            return; // 0 = unlimited retention
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
    pub fn export(&self, scope: &ChangelogScope, filter: &ChangelogFilter) -> String {
        let entries = self.get_entries(scope, filter);
        serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string())
    }

    fn matches_table(
        &self,
        entry: &ChangelogEntry,
        namespace: &Namespace,
        table_name: &str,
    ) -> bool {
        entry.namespace.database == namespace.database
            && entry.namespace.schema == namespace.schema
            && entry.table_name == table_name
    }

    fn matches_filter(
        &self,
        entry: &ChangelogEntry,
        filter: &ChangelogFilter,
        privacy: &HistoryPrivacy<'_>,
    ) -> bool {
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
            let mut protected_key = entry.primary_key.clone();
            privacy.protect_map(&entry.table_name, &mut protected_key);
            let pk_str = serialize_pk(&protected_key);
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
            connection_id: None,
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

    fn scope() -> ChangelogScope {
        ChangelogScope {
            masking: None,
            session_id: "test-session".to_string(),
            connection_id: None,
            driver_id: "postgres".to_string(),
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
    fn redact_map_replaces_only_sensitive_columns() {
        let mut m = HashMap::new();
        m.insert("id".to_string(), serde_json::json!(1));
        m.insert("email".to_string(), serde_json::json!("a@b.com"));
        m.insert("password_hash".to_string(), serde_json::json!("$2b$12$..."));
        m.insert("api_key".to_string(), serde_json::json!("sk-leak"));
        m.insert("name".to_string(), serde_json::json!("Alice"));
        let sensitive: Vec<String> = ["password", "api_key", "email"]
            .into_iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
        HistoryPrivacy::new(&sensitive, None).protect_map("users", &mut m);
        assert_eq!(m["id"], serde_json::json!(1));
        assert_eq!(m["name"], serde_json::json!("Alice"));
        assert_eq!(m["email"], serde_json::json!("[REDACTED]"));
        assert_eq!(m["password_hash"], serde_json::json!("[REDACTED]"));
        assert_eq!(m["api_key"], serde_json::json!("[REDACTED]"));
    }

    #[test]
    fn record_redacts_before_writing_to_disk() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());
        let mut before = HashMap::new();
        before.insert("id".to_string(), serde_json::json!(1));
        before.insert("password".to_string(), serde_json::json!("hunter2"));
        let entry = make_entry("users", ChangeOperation::Delete, pk(1), Some(before), None);
        store.record(entry);
        // Read what was actually persisted
        let content = std::fs::read_to_string(tmp.path().join("changelog.jsonl")).unwrap();
        assert!(content.contains("[REDACTED]"));
        assert!(!content.contains("hunter2"));
    }

    #[test]
    fn privacy_redacts_primary_keys_nested_documents_and_identifier_variants_on_disk() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().into());
        let key = HashMap::from([("email".into(), serde_json::json!("pk-secret-fixture"))]);
        let image = HashMap::from([
            (
                "profile".into(),
                serde_json::json!({"contacts": [{"apiKey": "nested-secret-fixture"}], "city": "Lyon"}),
            ),
            (
                "postalCode".into(),
                serde_json::json!("postal-secret-fixture"),
            ),
        ]);
        store.record(make_entry(
            "users",
            ChangeOperation::Insert,
            key,
            None,
            Some(image),
        ));
        let content = fs::read_to_string(tmp.path().join("changelog.jsonl")).unwrap();
        for value in [
            "pk-secret-fixture",
            "nested-secret-fixture",
            "postal-secret-fixture",
        ] {
            assert!(
                !content.contains(value),
                "persisted sensitive fixture: {value}"
            );
        }
        assert!(content.contains("Lyon"));
    }

    #[test]
    fn privacy_current_config_protects_old_captures_and_search_counts() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().into());
        let key = HashMap::from([("lookup".into(), serde_json::json!("lookup-secret-fixture"))]);
        let entry = make_entry(
            "users",
            ChangeOperation::Insert,
            key.clone(),
            None,
            Some(key),
        );
        let ns = entry.namespace.clone();
        store.record(entry.clone());
        let mut config = store.get_config();
        config.sensitive_columns.push("lookup".into());
        store.update_config(config);
        let search = ChangelogFilter {
            primary_key_search: Some("lookup-secret-fixture".into()),
            ..Default::default()
        };
        assert_eq!(store.get_timeline_count(&scope(), &ns, "users", &search), 0);
        assert!(
            store
                .get_timeline(&scope(), &ns, "users", &search)
                .is_empty()
        );
        assert!(
            !store
                .export(&scope(), &ChangelogFilter::default())
                .contains("lookup-secret-fixture")
        );
        assert_eq!(
            store.get_entry(&scope(), &entry.id).unwrap().primary_key["lookup"],
            serde_json::json!("[REDACTED]")
        );
    }

    #[test]
    fn privacy_redacted_keys_never_merge_unrelated_rows_in_diffs_or_histories() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().into());
        let key = HashMap::from([("id".into(), serde_json::json!("[REDACTED]"))]);
        let first = make_entry(
            "users",
            ChangeOperation::Insert,
            key.clone(),
            None,
            Some(row(1, "First")),
        );
        let ns = first.namespace.clone();
        let t1 = first.timestamp - Duration::seconds(1);
        store.record(first);
        store.record(make_entry(
            "users",
            ChangeOperation::Insert,
            key.clone(),
            None,
            Some(row(2, "Second")),
        ));
        let diff = store.compute_temporal_diff(&scope(), &ns, "users", t1, Utc::now(), None);
        assert!(diff.rows.is_empty());
        assert!(diff.incomplete);
        assert!(
            store
                .get_row_history(&scope(), &ns, "users", &key, None)
                .is_empty()
        );
        assert!(
            store
                .get_row_state_at(&scope(), &ns, "users", &key, Utc::now())
                .is_none()
        );
    }

    #[test]
    fn privacy_current_connection_rules_cover_every_read_after_restart() {
        use qore_core::masking::{ConnectionMasking, MaskMode, MaskingRule};
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().into());
        let mut before = row(1, "Before");
        before.insert("alias".into(), serde_json::json!("old-secret-fixture"));
        let mut after = row(1, "After");
        after.insert("alias".into(), serde_json::json!("new-secret-fixture"));
        let mut entry = make_entry(
            "users",
            ChangeOperation::Update,
            pk(1),
            Some(before),
            Some(after),
        );
        entry.changed_columns = vec!["alias".into()];
        let ns = entry.namespace.clone();
        let t1 = entry.timestamp - Duration::seconds(1);
        let t2 = entry.timestamp + Duration::seconds(1);
        store.record(entry.clone());
        drop(store);
        let store = ChangelogStore::new(tmp.path().into());
        let mut scope = scope();
        for mode in [MaskMode::Hidden, MaskMode::Partial, MaskMode::Hash] {
            scope.masking = Some(ConnectionMasking {
                rules: vec![MaskingRule {
                    table: "public.users".into(),
                    column: "ALIAS".into(),
                    mode,
                }],
                mask_detected_columns: false,
            });
            let selected = store.get_entry(&scope, &entry.id).unwrap();
            let history = store.get_row_history(&scope, &ns, "users", &pk(1), None);
            let state = store
                .get_row_state_at(&scope, &ns, "users", &pk(1), t2)
                .unwrap();
            let diff = store.compute_temporal_diff(&scope, &ns, "users", t1, t2, None);
            assert_eq!(diff.rows.len(), 1);
            assert!(diff.incomplete);
            assert_eq!(state["name"], serde_json::json!("After"));
            let exposed = serde_json::json!([selected, history, state, diff]).to_string();
            assert!(!exposed.contains("secret-fixture"));
            assert!(
                !store
                    .export(&scope, &ChangelogFilter::default())
                    .contains("secret-fixture")
            );
            let rollback =
                crate::time_travel::rollback::generate_rollback_statements(&[selected], "postgres");
            assert_eq!(rollback.statements_count, 0);
            assert!(!rollback.sql.contains("secret-fixture"));
        }
        // Read-time masking is a projection; an unrelated table rule must not hide this column.
        scope.masking.as_mut().unwrap().rules[0].table = "products".into();
        assert_eq!(
            store.get_entry(&scope, &entry.id).unwrap().before.unwrap()["alias"],
            serde_json::json!("old-secret-fixture")
        );
    }

    #[test]
    fn privacy_current_masking_prevents_primary_key_search_and_lookup_probes() {
        use qore_core::masking::{ConnectionMasking, MaskMode, MaskingRule};
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().into());
        let key = HashMap::from([("alias".into(), serde_json::json!("key-secret-fixture"))]);
        let entry = make_entry(
            "users",
            ChangeOperation::Insert,
            key.clone(),
            None,
            Some(row(1, "Safe")),
        );
        let ns = entry.namespace.clone();
        store.record(entry.clone());
        let mut scope = scope();
        scope.masking = Some(ConnectionMasking {
            rules: vec![MaskingRule {
                table: "users".into(),
                column: "alias".into(),
                mode: MaskMode::Partial,
            }],
            mask_detected_columns: false,
        });
        let filter = ChangelogFilter {
            primary_key_search: Some("key-secret".into()),
            ..Default::default()
        };
        assert!(store.get_entries(&scope, &filter).is_empty());
        assert_eq!(store.get_timeline_count(&scope, &ns, "users", &filter), 0);
        assert!(store.get_timeline(&scope, &ns, "users", &filter).is_empty());
        assert!(!store.can_identify_row(&scope, "users", &key));
        assert!(
            store
                .get_row_history(&scope, &ns, "users", &key, None)
                .is_empty()
        );
        assert!(
            store
                .get_row_state_at(&scope, &ns, "users", &key, Utc::now())
                .is_none()
        );
        let timeline = store.get_timeline(&scope, &ns, "users", &ChangelogFilter::default());
        assert_eq!(timeline.len(), 1);
        assert!(timeline[0].primary_key.is_none());
        let selected = store.get_entry(&scope, &entry.id).unwrap();
        assert_eq!(
            crate::time_travel::rollback::generate_rollback_statements(&[selected], "postgres")
                .statements_count,
            0
        );
    }

    #[test]
    fn privacy_capture_applies_custom_rules_to_keys_and_nested_json_before_disk() {
        use qore_core::masking::{ConnectionMasking, MaskMode, MaskingRule};
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().into());
        let rules = ConnectionMasking {
            rules: vec![
                MaskingRule {
                    table: "users".into(),
                    column: "alias".into(),
                    mode: MaskMode::Hash,
                },
                MaskingRule {
                    table: "users".into(),
                    column: "profile.note".into(),
                    mode: MaskMode::Partial,
                },
            ],
            mask_detected_columns: false,
        };
        let key = HashMap::from([("alias".into(), serde_json::json!("key-secret-fixture"))]);
        let image = HashMap::from([
            (
                "profile".into(),
                serde_json::json!([{"note": "nested-secret-fixture", "city": "Lyon"}]),
            ),
            (
                "profile.note".into(),
                serde_json::json!("flat-secret-fixture"),
            ),
        ]);
        let entry = make_entry("users", ChangeOperation::Delete, key, Some(image), None);
        store.record_with_masking(entry, Some(&rules));
        let content = fs::read_to_string(tmp.path().join("changelog.jsonl")).unwrap();
        assert!(!content.contains("secret-fixture"));
        assert!(content.contains("Lyon"));
        drop(store);
        let reloaded = ChangelogStore::new(tmp.path().into());
        assert!(
            !reloaded
                .export(&scope(), &ChangelogFilter::default())
                .contains("secret-fixture")
        );
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
        let events = store.get_timeline(&scope(), &ns, "users", &ChangelogFilter::default());
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
        let entries = store.get_entries(&scope(), &filter);
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

        let history = store.get_row_history(&scope(), &ns, "users", &pk(1), None);
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

        store.clear_table(&scope(), &ns, "users").unwrap();

        let users = store.get_timeline(&scope(), &ns, "users", &ChangelogFilter::default());
        assert_eq!(users.len(), 0);

        let orders = store.get_timeline(&scope(), &ns, "orders", &ChangelogFilter::default());
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

        let diff = store.compute_temporal_diff(&scope(), &ns, "users", t0, t1, None);

        assert_eq!(diff.stats.total_changes, 2);
        assert_eq!(diff.stats.added, 2);
    }

    #[test]
    fn test_pk_matches() {
        let a = pk(1);
        let b = pk(1);
        let c = pk(2);
        assert!(pk_matches(&a, &b));
        assert!(!pk_matches(&a, &c));
    }

    #[test]
    fn connection_scope_isolates_all_history_reads_after_restart() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());
        let mut own = make_entry(
            "users",
            ChangeOperation::Update,
            pk(1),
            Some(row(1, "Before A")),
            Some(row(1, "After A")),
        );
        own.connection_id = Some("connection-a".into());
        let mut other = own.clone();
        other.id = uuid::Uuid::new_v4();
        other.connection_id = Some("connection-b".into());
        other.session_id = "other-session".into();
        other.before = Some(row(1, "Before B"));
        other.after = Some(row(1, "After B"));
        let mut legacy = other.clone();
        legacy.id = uuid::Uuid::new_v4();
        legacy.connection_id = None;
        let ns = own.namespace.clone();
        let start = own.timestamp - Duration::seconds(1);
        let end = own.timestamp + Duration::seconds(1);
        for entry in [&own, &other, &legacy] {
            store.record(entry.clone());
        }
        drop(store);

        let store = ChangelogStore::new(tmp.path().to_path_buf());
        let scope = ChangelogScope {
            masking: None,
            session_id: "reconnected-session".into(),
            connection_id: Some("connection-a".into()),
            driver_id: "postgres".into(),
        };
        let filter = ChangelogFilter::default();
        let timeline = store.get_timeline(&scope, &ns, "users", &filter);
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].entry_id, own.id);
        assert_eq!(store.get_timeline_count(&scope, &ns, "users", &filter), 1);
        assert_eq!(
            store.get_row_history(&scope, &ns, "users", &pk(1), None)[0].id,
            own.id
        );
        assert_eq!(
            store
                .get_row_history(&scope, &ns, "users", &pk(1), None)
                .len(),
            1
        );
        assert_eq!(
            store.get_row_state_at(&scope, &ns, "users", &pk(1), end),
            own.after
        );
        let diff = store.compute_temporal_diff(&scope, &ns, "users", start, end, None);
        assert_eq!(diff.rows.len(), 1);
        assert_eq!(diff.rows[0].state_at_t1, own.before);
        assert_eq!(diff.rows[0].state_at_t2, own.after);
        assert!(store.get_entry(&scope, &other.id).is_none());
        assert!(store.get_entry(&scope, &legacy.id).is_none());
        assert_eq!(store.get_entry(&scope, &own.id).unwrap().id, own.id);
        let exported: Vec<ChangelogEntry> =
            serde_json::from_str(&store.export(&scope, &filter)).unwrap();
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].id, own.id);
        let sql = crate::time_travel::rollback::generate_rollback_statements(
            &store.get_entries(&scope, &filter),
            &scope.driver_id,
        );
        assert!(sql.sql.contains("Before A"));
        assert!(!sql.sql.contains("Before B"));
    }

    #[test]
    fn legacy_records_are_readable_only_in_their_original_session() {
        let entry = make_entry(
            "users",
            ChangeOperation::Insert,
            pk(1),
            None,
            Some(row(1, "Legacy")),
        );
        let mut json = serde_json::to_value(&entry).unwrap();
        json.as_object_mut().unwrap().remove("connection_id");
        let legacy: ChangelogEntry = serde_json::from_value(json).unwrap();
        assert!(legacy.connection_id.is_none());
        assert!(scope().contains(&legacy));
        let mut reconnected = scope();
        reconnected.session_id = "new-session".into();
        reconnected.connection_id = Some("saved-id".into());
        assert!(!reconnected.contains(&legacy));
        let mut wrong_driver = scope();
        wrong_driver.driver_id = "mysql".into();
        assert!(!wrong_driver.contains(&legacy));
    }

    #[test]
    fn filtered_count_matches_timeline_before_pagination() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());
        let mut entry = make_entry(
            "users",
            ChangeOperation::Update,
            pk(1),
            Some(row(1, "A")),
            Some(row(1, "B")),
        );
        let ns = entry.namespace.clone();
        for operation in [
            ChangeOperation::Update,
            ChangeOperation::Insert,
            ChangeOperation::Update,
        ] {
            entry.id = uuid::Uuid::new_v4();
            entry.operation = operation;
            store.record(entry.clone());
        }
        let filter = ChangelogFilter {
            operation: Some(ChangeOperation::Update),
            primary_key_search: Some("id=1".into()),
            connection_name: Some("TestConn".into()),
            environment: Some("development".into()),
            from_timestamp: Some(entry.timestamp),
            to_timestamp: Some(entry.timestamp),
            offset: Some(1),
            limit: Some(1),
            ..Default::default()
        };
        assert_eq!(store.get_timeline_count(&scope(), &ns, "users", &filter), 2);
        assert_eq!(store.get_timeline(&scope(), &ns, "users", &filter).len(), 1);
        let missing = ChangelogFilter {
            primary_key_search: Some("id=2".into()),
            ..filter
        };
        assert_eq!(
            store.get_timeline_count(&scope(), &ns, "users", &missing),
            0
        );
    }

    #[test]
    fn clear_table_preserves_other_connections_beyond_cache_and_legacy_lines() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("changelog.jsonl");
        let own = make_entry(
            "users",
            ChangeOperation::Insert,
            pk(1),
            None,
            Some(row(1, "A")),
        );
        let mut other = own.clone();
        other.id = uuid::Uuid::new_v4();
        other.session_id = "other-session".into();
        let mut file = File::create(&log_path).unwrap();
        writeln!(file, "{}", serde_json::to_string(&other).unwrap()).unwrap();
        writeln!(file, "legacy-unparseable-line").unwrap();
        for _ in 0..MAX_CACHE_ENTRIES {
            writeln!(file, "{}", serde_json::to_string(&own).unwrap()).unwrap();
        }
        drop(file);
        let store = ChangelogStore::new(tmp.path().to_path_buf());
        assert_eq!(store.entries.read().len(), MAX_CACHE_ENTRIES);
        store
            .clear_table(&scope(), &own.namespace, "users")
            .unwrap();
        let retained = fs::read_to_string(&log_path).unwrap();
        assert_eq!(retained.lines().count(), 2);
        assert!(retained.contains(&other.id.to_string()));
        assert!(retained.contains("legacy-unparseable-line"));
        assert!(
            store
                .get_entries(&scope(), &ChangelogFilter::default())
                .is_empty()
        );
        drop(store);
        let reloaded = ChangelogStore::new(tmp.path().to_path_buf());
        let other_scope = ChangelogScope {
            masking: None,
            session_id: other.session_id.clone(),
            ..scope()
        };
        assert_eq!(
            reloaded.get_entry(&other_scope, &other.id).unwrap().id,
            other.id
        );
    }

    #[test]
    fn failed_clear_keeps_cached_history() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());
        let entry = make_entry(
            "users",
            ChangeOperation::Insert,
            pk(1),
            None,
            Some(row(1, "A")),
        );
        store.record(entry.clone());
        fs::remove_file(&store.log_path).unwrap();
        fs::create_dir(&store.log_path).unwrap();
        assert!(
            store
                .clear_table(&scope(), &entry.namespace, "users")
                .is_err()
        );
        assert!(store.get_entry(&scope(), &entry.id).is_some());
    }

    #[test]
    fn temporal_diff_reports_net_changes_without_inventing_initial_rows() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());
        let start = Utc::now() - Duration::seconds(1);
        // An inserted row stays "added" through updates; insertion followed by
        // update and deletion has no net effect. Returning to the initial value
        // (including delete/reinsert) has no net effect either.
        for (id, op, before, after) in [
            (1, ChangeOperation::Insert, None, Some("A")),
            (1, ChangeOperation::Update, Some("A"), Some("B")),
            (2, ChangeOperation::Insert, None, Some("A")),
            (2, ChangeOperation::Update, Some("A"), Some("B")),
            (2, ChangeOperation::Delete, Some("B"), None),
            (3, ChangeOperation::Update, Some("A"), Some("B")),
            (3, ChangeOperation::Update, Some("B"), Some("A")),
            (4, ChangeOperation::Delete, Some("A"), None),
            (4, ChangeOperation::Insert, None, Some("A")),
            (5, ChangeOperation::Delete, Some("A"), None),
            (5, ChangeOperation::Insert, None, Some("B")),
        ] {
            store.record(make_entry(
                "users",
                op,
                pk(id),
                before.map(|v| row(id, v)),
                after.map(|v| row(id, v)),
            ));
        }
        let ns = Namespace {
            database: "testdb".into(),
            schema: Some("public".into()),
        };
        let diff = store.compute_temporal_diff(&scope(), &ns, "users", start, Utc::now(), None);
        assert_eq!(diff.stats.total_changes, 2);
        assert_eq!(diff.stats.added, 1);
        assert_eq!(diff.stats.modified, 1);
        assert_eq!(diff.stats.removed, 0);
        let added = diff.rows.iter().find(|r| r.primary_key == pk(1)).unwrap();
        assert!(added.state_at_t1.is_none());
        assert_eq!(added.state_at_t2, Some(row(1, "B")));
        let modified = diff.rows.iter().find(|r| r.primary_key == pk(5)).unwrap();
        assert_eq!(modified.changed_columns, vec!["name"]);
        assert!(!diff.truncated);
        let limited =
            store.compute_temporal_diff(&scope(), &ns, "users", start, Utc::now(), Some(1));
        assert!(limited.truncated);
        assert_eq!(limited.stats.total_changes, 2);
        assert_eq!(limited.rows.len(), 1);
        assert_eq!(limited.rows[0].primary_key, diff.rows[0].primary_key);
    }

    #[test]
    fn temporal_diff_marks_missing_images_instead_of_claiming_an_insert() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());
        let entry = make_entry(
            "users",
            ChangeOperation::Update,
            pk(1),
            None,
            Some(row(1, "B")),
        );
        let start = entry.timestamp - Duration::seconds(1);
        store.record(entry.clone());
        let diff = store.compute_temporal_diff(
            &scope(),
            &entry.namespace,
            "users",
            start,
            Utc::now(),
            None,
        );
        assert!(diff.incomplete);
        assert!(!diff.truncated);
        assert_eq!(diff.stats.total_changes, 0);
        assert!(diff.rows.is_empty());
    }
}

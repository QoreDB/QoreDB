// SPDX-License-Identifier: BUSL-1.1

//! Changelog Store
//!
//! Persistent, append-only store for row-level change records.
//! Follows the same JSONL + in-memory cache pattern as AuditStore.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use chrono::{DateTime, Utc};
use parking_lot::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

use super::privacy::{HistoryPrivacy, key_is_available, value_is_unavailable};
use super::types::{
    ChangeOperation, ChangelogEntry, ChangelogFilter, ChangelogScope, DiffRowStatus, TemporalDiff,
    TemporalDiffRow, TemporalDiffStats, TimeTravelConfig, TimelineEvent,
};
use crate::engine::types::Namespace;

#[path = "retention.rs"]
mod retention;
pub use retention::run_retention_maintenance;

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
    /// True only when the cache covers the whole successfully loaded journal.
    cache_complete: AtomicBool,
    /// An unreadable policy must not be replaced by default capture/privacy rules.
    config_valid: AtomicBool,
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
            cache_complete: AtomicBool::new(false),
            config_valid: AtomicBool::new(true),
        };

        store.load_config_from_disk();
        store.load_recent_entries();

        store
    }

    pub fn get_config(&self) -> Result<TimeTravelConfig, String> {
        let _file_guard = self.file_lock.lock();
        if !self.config_valid.load(Ordering::Relaxed) {
            // Settings can retry after the original file has been restored.
            self.load_config_from_disk();
        }
        self.require_valid_config()?;
        Ok(self.config.read().clone())
    }

    fn require_valid_config(&self) -> Result<(), String> {
        if self.config_valid.load(Ordering::Relaxed) {
            Ok(())
        } else {
            Err("Time-travel configuration is unreadable; restore it and retry in settings".into())
        }
    }

    pub fn update_config(&self, config: TimeTravelConfig) -> Result<(), String> {
        let _file_guard = self.file_lock.lock();
        let json = serde_json::to_vec_pretty(&config)
            .map_err(|error| format!("Failed to serialize time-travel config: {error}"))?;
        crate::atomic_write::write_atomic(&self.config_path, &json)
            .map_err(|error| format!("Failed to save time-travel config: {error}"))?;
        *self.config.write() = config.clone();
        self.config_valid.store(true, Ordering::Relaxed);
        // Configuration and journal are separate files. A failed cleanup is
        // reported, while the saved policy remains available for the next retry.
        self.enforce_retention_locked(&config, Utc::now())
            .map_err(|error| format!("Time-travel settings saved, but retention failed: {error}"))
    }

    pub fn is_enabled(&self) -> bool {
        self.config_valid.load(Ordering::Relaxed) && self.config.read().enabled
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
        if !self.config_valid.load(Ordering::Relaxed) {
            return false;
        }
        let config = self.config.read();
        if !config.enabled {
            return false;
        }
        if config
            .excluded_tables
            .iter()
            .any(|table| table.eq_ignore_ascii_case(table_name))
        {
            return false;
        }
        if config.production_only && environment != "production" {
            return false;
        }
        true
    }

    fn load_config_from_disk(&self) {
        let result = match fs::read_to_string(&self.config_path) {
            Ok(content) => serde_json::from_str::<TimeTravelConfig>(&content).map_err(|error| {
                format!(
                    "Failed to parse time-travel config at line {}, column {}",
                    error.line(),
                    error.column()
                )
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => Err(format!("Failed to read time-travel config: {error}")),
        };
        match result {
            Ok(config) => {
                *self.config.write() = config;
                self.config_valid.store(true, Ordering::Relaxed);
                debug!("Loaded time-travel config");
            }
            Err(error) => {
                self.config_valid.store(false, Ordering::Relaxed);
                warn!("{error}; capture, history access and automatic retention are suspended");
            }
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
        if !self.should_capture(&entry.table_name, &entry.environment) {
            return;
        }
        HistoryPrivacy::new(&self.config.read().sensitive_columns, masking).protect(&mut entry);

        if let Err(e) = self.append_to_file(&entry) {
            self.cache_complete.store(false, Ordering::Relaxed);
            error!("Failed to write changelog entry: {}", e);
            return;
        }
        {
            let mut entries = self.entries.write();
            if entries.len() >= MAX_CACHE_ENTRIES {
                entries.pop_front();
                self.cache_complete.store(false, Ordering::Relaxed);
            }
            entries.push_back(entry);
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
        let mut recent = VecDeque::new();
        let mut line_count = 0;
        let result = self.visit_file(|line| {
            line_count += 1;
            if let Ok(entry) = serde_json::from_str::<ChangelogEntry>(line) {
                if recent.len() == MAX_CACHE_ENTRIES {
                    recent.pop_front();
                }
                recent.push_back(entry);
            }
        });
        match result {
            Ok(()) => {
                self.cache_complete
                    .store(line_count <= MAX_CACHE_ENTRIES, Ordering::Relaxed);
                self.file_line_count.store(line_count, Ordering::Relaxed);
                debug!("Loaded {} changelog entries from file", recent.len());
                *self.entries.write() = recent;
            }
            Err(e) => warn!("Failed to load changelog file: {}", e),
        }
    }

    // Callers hold file_lock across a complete read, including both pagination passes.
    fn visit_file(&self, mut visit: impl FnMut(&str)) -> Result<(), String> {
        let file = match File::open(&self.log_path) {
            Ok(file) => file,
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && self.file_line_count.load(Ordering::Relaxed) == 0 =>
            {
                return Ok(());
            }
            Err(e) => return Err(format!("Failed to read changelog: {e}")),
        };
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|e| format!("Failed to read changelog: {e}"))?;
            visit(&line);
        }
        Ok(())
    }

    fn visit_entries(&self, mut visit: impl FnMut(&ChangelogEntry)) -> Result<(), String> {
        self.require_valid_config()?;
        if self.cache_complete.load(Ordering::Relaxed) {
            for entry in self.entries.read().iter() {
                visit(entry);
            }
            return Ok(());
        }
        self.visit_file(|line| {
            if let Ok(entry) = serde_json::from_str::<ChangelogEntry>(line) {
                visit(&entry);
            }
        })
    }

    fn maybe_rotate(&self) {
        let config = self.config.read().clone();
        let exceeds_count = self.file_line_count.load(Ordering::Relaxed) > config.max_entries;
        let exceeds_size = config.max_file_size_mb != 0
            && fs::metadata(&self.log_path).is_ok_and(|metadata| {
                metadata.len() > config.max_file_size_mb.saturating_mul(1_048_576)
            });
        if (exceeds_count || exceeds_size)
            && let Err(error) = self.enforce_retention_locked(&config, Utc::now())
        {
            error!("Failed to rotate changelog: {error}");
        }
    }

    /// Rewrite from the complete journal, retaining unknown legacy lines unless
    /// the caller explicitly prunes by position. Publish the new cache only after rename.
    fn retain_file(
        &self,
        mut retain: impl FnMut(usize, Option<&ChangelogEntry>) -> bool,
    ) -> Result<usize, String> {
        let temp_path = self.log_path.with_extension("jsonl.tmp");
        let mut recent = VecDeque::new();
        let mut retained_count = 0;
        let mut total = 0;
        let result = (|| {
            let mut writer = BufWriter::new(File::create(&temp_path)?);
            let mut write_error = None;
            self.visit_file(|line| {
                let entry = serde_json::from_str::<ChangelogEntry>(line).ok();
                let keep = retain(total, entry.as_ref());
                total += 1;
                if !keep || write_error.is_some() {
                    return;
                }
                if let Err(error) = writeln!(writer, "{line}") {
                    write_error = Some(error);
                    return;
                }
                retained_count += 1;
                if let Some(entry) = entry {
                    if recent.len() == MAX_CACHE_ENTRIES {
                        recent.pop_front();
                    }
                    recent.push_back(entry);
                }
            })
            .map_err(std::io::Error::other)?;
            if let Some(error) = write_error {
                return Err(error);
            }
            writer.flush()?;
            writer.get_ref().sync_all()?;
            drop(writer);
            fs::rename(&temp_path, &self.log_path)
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temp_path);
            return Err(format!("Failed to rewrite changelog: {error}"));
        }
        *self.entries.write() = recent;
        self.file_line_count
            .store(retained_count, Ordering::Relaxed);
        self.cache_complete
            .store(retained_count <= MAX_CACHE_ENTRIES, Ordering::Relaxed);
        Ok(total - retained_count)
    }

    fn matching_positions(
        &self,
        matches: impl Fn(&ChangelogEntry) -> bool,
    ) -> Result<Vec<(DateTime<Utc>, usize)>, String> {
        let mut positions = Vec::new();
        let mut index = 0;
        self.visit_entries(|entry| {
            if matches(entry) {
                positions.push((entry.timestamp, index));
            }
            index += 1;
        })?;
        positions.sort_unstable_by(|a, b| b.cmp(a));
        Ok(positions)
    }

    fn read_positions(
        &self,
        privacy: &HistoryPrivacy<'_>,
        positions: impl Iterator<Item = (DateTime<Utc>, usize)>,
    ) -> Result<Vec<ChangelogEntry>, String> {
        let selected: std::collections::HashSet<_> = positions.map(|(_, index)| index).collect();
        if selected.is_empty() {
            return Ok(Vec::new());
        }
        let mut result = Vec::new();
        let mut index = 0;
        self.visit_entries(|entry| {
            if selected.contains(&index) {
                result.push((index, privacy.project(entry)));
            }
            index += 1;
        })?;
        result.sort_unstable_by(|(a_index, a), (b_index, b)| {
            (b.timestamp, b_index).cmp(&(a.timestamp, a_index))
        });
        Ok(result.into_iter().map(|(_, entry)| entry).collect())
    }

    /// Keep only timestamps and ordinals while choosing a page: a large offset
    /// must not retain skipped row images. Callers hold file_lock before reading
    /// the privacy policy, through both pagination passes and the count.
    fn select_entries(
        &self,
        privacy: &HistoryPrivacy<'_>,
        matches: impl Fn(&ChangelogEntry) -> bool,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<ChangelogEntry>, usize), String> {
        let positions = self.matching_positions(matches)?;
        let count = positions.len();
        let entries =
            self.read_positions(privacy, positions.into_iter().skip(offset).take(limit))?;
        Ok((entries, count))
    }

    /// A whole-table rollback must never silently omit changes beyond its budget.
    pub fn get_rollback_entries(
        &self,
        scope: &ChangelogScope,
        namespace: &Namespace,
        table_name: &str,
        target: DateTime<Utc>,
    ) -> Result<Vec<ChangelogEntry>, String> {
        let _file_guard = self.file_lock.lock();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        let positions = self.matching_positions(|entry| {
            scope.contains(entry)
                && self.matches_table(entry, namespace, table_name)
                && entry.timestamp > target
        })?;
        if positions.len() > 10_000 {
            return Err(
                "Rollback exceeds 10000 retained changes; choose a more recent target".into(),
            );
        }
        self.read_positions(&privacy, positions.into_iter())
    }

    pub fn can_identify_row(
        &self,
        scope: &ChangelogScope,
        table: &str,
        key: &std::collections::HashMap<String, serde_json::Value>,
    ) -> bool {
        if !self.config_valid.load(Ordering::Relaxed) {
            return false;
        }
        HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        )
        .can_identify(table, key)
    }

    pub fn get_timeline(
        &self,
        scope: &ChangelogScope,
        namespace: &Namespace,
        table_name: &str,
        filter: &ChangelogFilter,
    ) -> Result<Vec<TimelineEvent>, String> {
        self.get_timeline_page(scope, namespace, table_name, filter)
            .map(|(events, _)| events)
    }

    /// Get timeline events for a table, ordered by timestamp DESC.
    pub fn get_timeline_page(
        &self,
        scope: &ChangelogScope,
        namespace: &Namespace,
        table_name: &str,
        filter: &ChangelogFilter,
    ) -> Result<(Vec<TimelineEvent>, usize), String> {
        let _file_guard = self.file_lock.lock();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        let (entries, count) = self.select_entries(
            &privacy,
            |entry| {
                scope.contains(entry)
                    && self.matches_table(entry, namespace, table_name)
                    && self.matches_filter(entry, filter, &privacy)
            },
            filter.limit.unwrap_or(100),
            filter.offset.unwrap_or(0),
        )?;
        let events = entries
            .into_iter()
            .map(|entry| TimelineEvent {
                timestamp: entry.timestamp,
                operation: entry.operation,
                row_count: 1,
                session_id: entry.session_id,
                connection_name: entry.connection_name,
                primary_key: key_is_available(&entry.primary_key).then_some(entry.primary_key),
                entry_id: entry.id,
            })
            .collect();
        Ok((events, count))
    }

    /// Get the total matching count before pagination, including retained disk history.
    pub fn get_timeline_count(
        &self,
        scope: &ChangelogScope,
        namespace: &Namespace,
        table_name: &str,
        filter: &ChangelogFilter,
    ) -> Result<usize, String> {
        let _file_guard = self.file_lock.lock();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        let mut count = 0;
        self.visit_entries(|entry| {
            if scope.contains(entry)
                && self.matches_table(entry, namespace, table_name)
                && self.matches_filter(entry, filter, &privacy)
            {
                count += 1;
            }
        })?;
        Ok(count)
    }

    pub fn get_entries(
        &self,
        scope: &ChangelogScope,
        filter: &ChangelogFilter,
    ) -> Result<Vec<ChangelogEntry>, String> {
        let _file_guard = self.file_lock.lock();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        self.select_entries(
            &privacy,
            |entry| scope.contains(entry) && self.matches_filter(entry, filter, &privacy),
            filter.limit.unwrap_or(100),
            filter.offset.unwrap_or(0),
        )
        .map(|(entries, _)| entries)
    }

    /// Get a row's history in descending timestamp/append order.
    pub fn get_row_history(
        &self,
        scope: &ChangelogScope,
        namespace: &Namespace,
        table_name: &str,
        primary_key: &std::collections::HashMap<String, serde_json::Value>,
        limit: Option<usize>,
    ) -> Result<Vec<ChangelogEntry>, String> {
        let _file_guard = self.file_lock.lock();
        self.require_valid_config()?;
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        if !privacy.can_identify(table_name, primary_key) {
            return Ok(Vec::new());
        }
        self.select_entries(
            &privacy,
            |entry| {
                scope.contains(entry)
                    && self.matches_table(entry, namespace, table_name)
                    && key_is_available(&entry.primary_key)
                    && pk_matches(&entry.primary_key, primary_key)
            },
            limit.unwrap_or(50),
            0,
        )
        .map(|(entries, _)| entries)
    }

    pub fn get_entry(
        &self,
        scope: &ChangelogScope,
        entry_id: &uuid::Uuid,
    ) -> Result<Option<ChangelogEntry>, String> {
        let _file_guard = self.file_lock.lock();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        Ok(self
            .select_entries(
                &privacy,
                |entry| scope.contains(entry) && entry.id == *entry_id,
                1,
                0,
            )?
            .0
            .pop())
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
    ) -> Result<TemporalDiff, String> {
        let _file_guard = self.file_lock.lock();
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        let limit = limit.unwrap_or(10_000);

        // Replay changes keyed by serialized PK.
        let mut diff_rows: std::collections::HashMap<
            String,
            (DateTime<Utc>, DateTime<Utc>, TemporalDiffRow),
        > = std::collections::HashMap::new();
        let mut all_columns: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut incomplete_keys = std::collections::HashSet::new();

        let mut protected_values = false;
        // Project one entry at a time; large row images must not be copied into a second cache.
        self.visit_entries(|raw_entry| {
            if !scope.contains(raw_entry)
                || !self.matches_table(raw_entry, namespace, table_name)
                || raw_entry.timestamp <= t1
                || raw_entry.timestamp > t2
            {
                return;
            }
            let entry = privacy.project(raw_entry);
            if !key_is_available(&entry.primary_key) {
                protected_values = true;
                return;
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
                .and_modify(|(first, last, row)| {
                    if entry.timestamp < *first {
                        *first = entry.timestamp;
                        row.state_at_t1 = entry.before.clone();
                    }
                    if entry.timestamp >= *last {
                        *last = entry.timestamp;
                        row.state_at_t2 = entry.after.clone();
                    }
                })
                .or_insert_with(|| {
                    (
                        entry.timestamp,
                        entry.timestamp,
                        TemporalDiffRow {
                            primary_key: entry.primary_key.clone(),
                            state_at_t1: entry.before.clone(),
                            state_at_t2: entry.after.clone(),
                            changed_columns: vec![],
                            status: DiffRowStatus::Modified,
                        },
                    )
                });
        })?;

        let incomplete = protected_values || !incomplete_keys.is_empty();
        let mut rows: Vec<TemporalDiffRow> = diff_rows
            .into_iter()
            .filter(|(key, _)| !incomplete_keys.contains(key))
            .map(|(_, (_, _, row))| row)
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

        Ok(TemporalDiff {
            columns,
            rows,
            stats,
            truncated,
            incomplete,
        })
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
    ) -> Result<Option<std::collections::HashMap<String, serde_json::Value>>, String> {
        let _file_guard = self.file_lock.lock();
        self.require_valid_config()?;
        let privacy = HistoryPrivacy::new(
            &self.config.read().sensitive_columns,
            scope.masking.as_ref(),
        );
        if !privacy.can_identify(table_name, primary_key) {
            return Ok(None);
        }
        let mut last_entry: Option<ChangelogEntry> = None;
        self.visit_entries(|entry| {
            // An unidentified write may have changed this row and invalidates an older image.
            if scope.contains(entry)
                && self.matches_table(entry, namespace, table_name)
                && (!key_is_available(&entry.primary_key)
                    || pk_matches(&entry.primary_key, primary_key))
                && entry.timestamp <= timestamp
                && last_entry
                    .as_ref()
                    .is_none_or(|last| last.timestamp <= entry.timestamp)
            {
                last_entry = Some(privacy.project(entry));
            }
        })?;
        Ok(last_entry
            .filter(|entry| key_is_available(&entry.primary_key))
            .and_then(|entry| match entry.operation {
                ChangeOperation::Insert | ChangeOperation::Update => entry.after,
                ChangeOperation::Delete => None,
            }))
    }

    pub fn clear_table(
        &self,
        scope: &ChangelogScope,
        namespace: &Namespace,
        table_name: &str,
    ) -> Result<(), String> {
        let _file_guard = self.file_lock.lock();
        self.retain_file(|_, entry| {
            !entry.is_some_and(|entry| {
                scope.contains(entry) && self.matches_table(entry, namespace, table_name)
            })
        })?;
        info!(
            "Cleared changelog for {}.{}",
            namespace.database, table_name
        );
        Ok(())
    }

    pub fn clear_workspace(&self, workspace_id: &str) -> Result<(), String> {
        let _file_guard = self.file_lock.lock();
        self.retain_file(|_, entry| {
            !entry.is_some_and(|entry| entry.workspace_id.as_deref() == Some(workspace_id))
        })?;
        Ok(())
    }

    pub fn clear_all(&self) -> Result<(), String> {
        let _file_guard = self.file_lock.lock();
        crate::atomic_write::write_atomic(&self.log_path, b"")
            .map_err(|error| format!("Failed to clear changelog: {error}"))?;
        self.entries.write().clear();
        self.file_line_count.store(0, Ordering::Relaxed);
        self.cache_complete.store(true, Ordering::Relaxed);
        info!("Cleared all changelog entries");
        Ok(())
    }

    pub fn export(
        &self,
        scope: &ChangelogScope,
        filter: &ChangelogFilter,
    ) -> Result<String, String> {
        let entries = self.get_entries(scope, filter)?;
        serde_json::to_string_pretty(&entries)
            .map_err(|error| format!("Failed to export changelog: {error}"))
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
    use chrono::Duration;
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
            workspace_id: Some("test-workspace".into()),
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
            workspace_id: Some("test-workspace".into()),
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
    fn unidentified_capture_invalidates_older_row_state_until_a_verified_image() {
        let dir = TempDir::new().unwrap();
        let store = ChangelogStore::new(dir.path().into());
        let start = Utc::now() - Duration::seconds(3);
        let mut before = make_entry(
            "users",
            ChangeOperation::Insert,
            pk(1),
            None,
            Some(row(1, "before")),
        );
        before.timestamp = start;
        let namespace = before.namespace.clone();
        store.record(before);
        let mut unknown = make_entry("users", ChangeOperation::Update, HashMap::new(), None, None);
        unknown.timestamp = start + Duration::seconds(1);
        store.record(unknown);
        assert!(
            store
                .get_row_state_at(&scope(), &namespace, "users", &pk(1), start)
                .unwrap()
                .is_some()
        );
        assert!(
            store
                .get_row_state_at(&scope(), &namespace, "users", &pk(1), Utc::now())
                .unwrap()
                .is_none()
        );
        let mut refreshed = make_entry(
            "users",
            ChangeOperation::Update,
            pk(1),
            None,
            Some(row(1, "verified")),
        );
        refreshed.timestamp = start + Duration::seconds(2);
        store.record(refreshed);
        assert_eq!(
            store
                .get_row_state_at(&scope(), &namespace, "users", &pk(1), Utc::now())
                .unwrap()
                .unwrap()["name"],
            "verified"
        );
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
        let mut config = store.get_config().unwrap();
        config.sensitive_columns.push("lookup".into());
        store.update_config(config).unwrap();
        let search = ChangelogFilter {
            primary_key_search: Some("lookup-secret-fixture".into()),
            ..Default::default()
        };
        assert_eq!(
            store
                .get_timeline_count(&scope(), &ns, "users", &search)
                .unwrap(),
            0
        );
        assert!(
            store
                .get_timeline(&scope(), &ns, "users", &search)
                .unwrap()
                .is_empty()
        );
        assert!(
            !store
                .export(&scope(), &ChangelogFilter::default())
                .unwrap()
                .contains("lookup-secret-fixture")
        );
        assert_eq!(
            store
                .get_entry(&scope(), &entry.id)
                .unwrap()
                .unwrap()
                .primary_key["lookup"],
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
        let diff = store
            .compute_temporal_diff(&scope(), &ns, "users", t1, Utc::now(), None)
            .unwrap();
        assert!(diff.rows.is_empty());
        assert!(diff.incomplete);
        assert!(
            store
                .get_row_history(&scope(), &ns, "users", &key, None)
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .get_row_state_at(&scope(), &ns, "users", &key, Utc::now())
                .unwrap()
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
            let selected = store.get_entry(&scope, &entry.id).unwrap().unwrap();
            let history = store
                .get_row_history(&scope, &ns, "users", &pk(1), None)
                .unwrap();
            let state = store
                .get_row_state_at(&scope, &ns, "users", &pk(1), t2)
                .unwrap()
                .unwrap();
            let diff = store
                .compute_temporal_diff(&scope, &ns, "users", t1, t2, None)
                .unwrap();
            assert_eq!(diff.rows.len(), 1);
            assert!(diff.incomplete);
            assert_eq!(state["name"], serde_json::json!("After"));
            let exposed = serde_json::json!([selected, history, state, diff]).to_string();
            assert!(!exposed.contains("secret-fixture"));
            assert!(
                !store
                    .export(&scope, &ChangelogFilter::default())
                    .unwrap()
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
            store
                .get_entry(&scope, &entry.id)
                .unwrap()
                .unwrap()
                .before
                .unwrap()["alias"],
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
        assert!(store.get_entries(&scope, &filter).unwrap().is_empty());
        assert_eq!(
            store
                .get_timeline_count(&scope, &ns, "users", &filter)
                .unwrap(),
            0
        );
        assert!(
            store
                .get_timeline(&scope, &ns, "users", &filter)
                .unwrap()
                .is_empty()
        );
        assert!(!store.can_identify_row(&scope, "users", &key));
        assert!(
            store
                .get_row_history(&scope, &ns, "users", &key, None)
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .get_row_state_at(&scope, &ns, "users", &key, Utc::now())
                .unwrap()
                .is_none()
        );
        let timeline = store
            .get_timeline(&scope, &ns, "users", &ChangelogFilter::default())
            .unwrap();
        assert_eq!(timeline.len(), 1);
        assert!(timeline[0].primary_key.is_none());
        let selected = store.get_entry(&scope, &entry.id).unwrap().unwrap();
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
                .unwrap()
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
        let events = store
            .get_timeline(&scope(), &ns, "users", &ChangelogFilter::default())
            .unwrap();
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
        let entries = store.get_entries(&scope(), &filter).unwrap();
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

        let history = store
            .get_row_history(&scope(), &ns, "users", &pk(1), None)
            .unwrap();
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

        let users = store
            .get_timeline(&scope(), &ns, "users", &ChangelogFilter::default())
            .unwrap();
        assert_eq!(users.len(), 0);

        let orders = store
            .get_timeline(&scope(), &ns, "orders", &ChangelogFilter::default())
            .unwrap();
        assert_eq!(orders.len(), 1);
    }

    #[test]
    fn test_config_persistence() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());

        let mut config = store.get_config().unwrap();
        config.retention_days = 7;
        config.enabled = false;
        store.update_config(config).unwrap();

        // Reload from disk
        let store2 = ChangelogStore::new(tmp.path().to_path_buf());
        let config2 = store2.get_config().unwrap();
        assert_eq!(config2.retention_days, 7);
        assert!(!config2.enabled);
    }

    #[test]
    fn test_should_capture() {
        let tmp = TempDir::new().unwrap();
        let store = ChangelogStore::new(tmp.path().to_path_buf());

        assert!(store.should_capture("users", "development"));

        // Disable
        let mut config = store.get_config().unwrap();
        config.enabled = false;
        store.update_config(config).unwrap();
        assert!(!store.should_capture("users", "development"));

        // Re-enable, production only
        let mut config = store.get_config().unwrap();
        config.enabled = true;
        config.production_only = true;
        store.update_config(config).unwrap();
        assert!(!store.should_capture("users", "development"));
        assert!(store.should_capture("users", "production"));

        // Excluded table
        let mut config = store.get_config().unwrap();
        config.production_only = false;
        config.excluded_tables = vec!["migrations".to_string()];
        store.update_config(config).unwrap();
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

        let diff = store
            .compute_temporal_diff(&scope(), &ns, "users", t0, t1, None)
            .unwrap();

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
            workspace_id: Some("test-workspace".into()),
            connection_id: Some("connection-a".into()),
            driver_id: "postgres".into(),
        };
        let filter = ChangelogFilter::default();
        let timeline = store.get_timeline(&scope, &ns, "users", &filter).unwrap();
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].entry_id, own.id);
        assert_eq!(
            store
                .get_timeline_count(&scope, &ns, "users", &filter)
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .get_row_history(&scope, &ns, "users", &pk(1), None)
                .unwrap()[0]
                .id,
            own.id
        );
        assert_eq!(
            store
                .get_row_history(&scope, &ns, "users", &pk(1), None)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .get_row_state_at(&scope, &ns, "users", &pk(1), end)
                .unwrap(),
            own.after
        );
        let diff = store
            .compute_temporal_diff(&scope, &ns, "users", start, end, None)
            .unwrap();
        assert_eq!(diff.rows.len(), 1);
        assert_eq!(diff.rows[0].state_at_t1, own.before);
        assert_eq!(diff.rows[0].state_at_t2, own.after);
        assert!(store.get_entry(&scope, &other.id).unwrap().is_none());
        assert!(store.get_entry(&scope, &legacy.id).unwrap().is_none());
        assert_eq!(
            store.get_entry(&scope, &own.id).unwrap().unwrap().id,
            own.id
        );
        let exported: Vec<ChangelogEntry> =
            serde_json::from_str(&store.export(&scope, &filter).unwrap()).unwrap();
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].id, own.id);
        let sql = crate::time_travel::rollback::generate_rollback_statements(
            &store.get_entries(&scope, &filter).unwrap(),
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
        assert_eq!(
            store
                .get_timeline_count(&scope(), &ns, "users", &filter)
                .unwrap(),
            2
        );
        assert_eq!(
            store
                .get_timeline(&scope(), &ns, "users", &filter)
                .unwrap()
                .len(),
            1
        );
        let missing = ChangelogFilter {
            primary_key_search: Some("id=2".into()),
            ..filter
        };
        assert_eq!(
            store
                .get_timeline_count(&scope(), &ns, "users", &missing)
                .unwrap(),
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
                .unwrap()
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
            reloaded
                .get_entry(&other_scope, &other.id)
                .unwrap()
                .unwrap()
                .id,
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
        assert!(store.get_entry(&scope(), &entry.id).unwrap().is_some());
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
        let diff = store
            .compute_temporal_diff(&scope(), &ns, "users", start, Utc::now(), None)
            .unwrap();
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
        let limited = store
            .compute_temporal_diff(&scope(), &ns, "users", start, Utc::now(), Some(1))
            .unwrap();
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
        let diff = store
            .compute_temporal_diff(&scope(), &entry.namespace, "users", start, Utc::now(), None)
            .unwrap();
        assert!(diff.incomplete);
        assert!(!diff.truncated);
        assert_eq!(diff.stats.total_changes, 0);
        assert!(diff.rows.is_empty());
    }

    fn write_history(dir: &TempDir, entries: impl IntoIterator<Item = ChangelogEntry>) {
        let mut file = BufWriter::new(File::create(dir.path().join("changelog.jsonl")).unwrap());
        for entry in entries {
            serde_json::to_writer(&mut file, &entry).unwrap();
            writeln!(file).unwrap();
        }
        file.flush().unwrap();
    }

    fn history_fixture(count: usize) -> Vec<ChangelogEntry> {
        let start = Utc::now() - Duration::hours(1);
        (0..count)
            .map(|index| {
                let mut entry = make_entry(
                    "users",
                    ChangeOperation::Insert,
                    pk(index as i64),
                    None,
                    Some(row(index as i64, "retained")),
                );
                entry.timestamp = start + Duration::milliseconds(index as i64);
                entry
            })
            .collect()
    }

    #[test]
    fn retained_history_reads_beyond_cache() {
        let dir = TempDir::new().unwrap();
        let entries = history_fixture(MAX_CACHE_ENTRIES + 5);
        let oldest = entries[0].clone();
        let ns = oldest.namespace.clone();
        write_history(&dir, entries);
        let store = ChangelogStore::new(dir.path().into());
        assert_eq!(store.entries.read().len(), MAX_CACHE_ENTRIES);
        let filter = ChangelogFilter {
            offset: Some(MAX_CACHE_ENTRIES + 3),
            limit: Some(2),
            ..Default::default()
        };
        assert_eq!(
            store
                .get_timeline_count(&scope(), &ns, "users", &filter)
                .unwrap(),
            MAX_CACHE_ENTRIES + 5
        );
        let page = store.get_timeline(&scope(), &ns, "users", &filter).unwrap();
        assert_eq!(page.len(), 2);
        assert_eq!(page[1].entry_id, oldest.id);
        assert_eq!(
            store.get_entries(&scope(), &filter).unwrap()[1].id,
            oldest.id
        );
        assert_eq!(
            store
                .get_entry(&scope(), &oldest.id)
                .unwrap()
                .unwrap()
                .after,
            oldest.after
        );
        assert_eq!(
            store
                .get_row_history(&scope(), &ns, "users", &pk(0), None)
                .unwrap()[0]
                .id,
            oldest.id
        );
        assert_eq!(
            store
                .get_row_state_at(&scope(), &ns, "users", &pk(0), Utc::now())
                .unwrap(),
            oldest.after
        );
        let diff = store
            .compute_temporal_diff(
                &scope(),
                &ns,
                "users",
                oldest.timestamp - Duration::seconds(1),
                Utc::now(),
                Some(1),
            )
            .unwrap();
        assert_eq!(diff.stats.added, MAX_CACHE_ENTRIES + 5);
        assert!(diff.truncated);
        let exported: Vec<ChangelogEntry> =
            serde_json::from_str(&store.export(&scope(), &filter).unwrap()).unwrap();
        assert_eq!(exported[1].id, oldest.id);
        let rollback = crate::time_travel::rollback::generate_rollback_statements(
            &[store.get_entry(&scope(), &oldest.id).unwrap().unwrap()],
            "postgres",
        );
        assert_eq!(rollback.statements_count, 1);
    }

    #[test]
    fn retained_history_purge_preserves_valid_entries_outside_cache() {
        let dir = TempDir::new().unwrap();
        let mut entries = history_fixture(MAX_CACHE_ENTRIES + 5);
        let oldest = entries[0].clone();
        entries.last_mut().unwrap().timestamp = Utc::now() - Duration::days(40);
        write_history(&dir, entries);
        let store = ChangelogStore::new(dir.path().into());
        store.enforce_retention().unwrap();
        let contents = fs::read_to_string(&store.log_path).unwrap();
        assert_eq!(contents.lines().count(), MAX_CACHE_ENTRIES + 4);
        assert!(contents.contains(&oldest.id.to_string()));
    }

    #[test]
    fn retained_history_purge_removes_expired_entries_outside_cache() {
        let dir = TempDir::new().unwrap();
        let mut entries = history_fixture(MAX_CACHE_ENTRIES + 5);
        entries[0].timestamp = Utc::now() - Duration::days(40);
        let expired_id = entries[0].id;
        write_history(&dir, entries);
        let store = ChangelogStore::new(dir.path().into());
        store.enforce_retention().unwrap();
        let contents = fs::read_to_string(&store.log_path).unwrap();
        assert_eq!(contents.lines().count(), MAX_CACHE_ENTRIES + 4);
        assert!(!contents.contains(&expired_id.to_string()));
    }

    #[test]
    fn retained_history_rotation_removes_pruned_cache_entries() {
        let dir = TempDir::new().unwrap();
        let store = ChangelogStore::new(dir.path().into());
        store
            .update_config(TimeTravelConfig {
                max_entries: 4,
                ..Default::default()
            })
            .unwrap();
        let entries = history_fixture(5);
        let first = entries[0].clone();
        for entry in entries {
            store.record(entry);
        }
        assert!(store.get_entry(&scope(), &first.id).unwrap().is_none());
        assert_eq!(
            store
                .get_timeline_count(
                    &scope(),
                    &first.namespace,
                    "users",
                    &ChangelogFilter::default()
                )
                .unwrap(),
            3
        );
    }

    #[test]
    fn retained_history_uses_timestamp_order_with_append_order_for_ties() {
        let dir = TempDir::new().unwrap();
        let store = ChangelogStore::new(dir.path().into());
        let mut entries = history_fixture(3);
        for entry in &mut entries {
            entry.primary_key = pk(1);
        }
        let oldest = entries[0].clone();
        let latest = entries[2].clone();
        for index in [1, 2, 0] {
            store.record(entries[index].clone());
        }
        let history = store
            .get_row_history(&scope(), &oldest.namespace, "users", &pk(1), None)
            .unwrap();
        assert_eq!(history[0].id, latest.id);
        assert_eq!(
            store
                .get_row_state_at(&scope(), &oldest.namespace, "users", &pk(1), Utc::now())
                .unwrap(),
            latest.after
        );
        let mut tied = latest.clone();
        tied.id = uuid::Uuid::new_v4();
        tied.after = Some(row(1, "last append"));
        store.record(tied.clone());
        let filter = ChangelogFilter {
            limit: Some(1),
            ..Default::default()
        };
        assert_eq!(
            store
                .get_timeline(&scope(), &oldest.namespace, "users", &filter)
                .unwrap()[0]
                .entry_id,
            tied.id
        );
        assert_eq!(
            store
                .get_row_state_at(&scope(), &oldest.namespace, "users", &pk(1), Utc::now())
                .unwrap(),
            tied.after
        );
        let diff = store
            .compute_temporal_diff(
                &scope(),
                &oldest.namespace,
                "users",
                oldest.timestamp - Duration::seconds(1),
                Utc::now(),
                None,
            )
            .unwrap();
        assert_eq!(diff.rows[0].state_at_t2, tied.after);
    }

    #[test]
    fn retained_history_privacy_and_scope_apply_to_evicted_entries() {
        use qore_core::masking::{ConnectionMasking, MaskMode, MaskingRule};
        let dir = TempDir::new().unwrap();
        let mut entries = history_fixture(MAX_CACHE_ENTRIES + 5);
        entries[0].connection_id = Some("connection-a".into());
        entries[0]
            .after
            .as_mut()
            .unwrap()
            .insert("alias".into(), serde_json::json!("old-secret-fixture"));
        let own = entries[0].clone();
        for entry in &mut entries[1..] {
            entry.connection_id = Some("connection-b".into());
        }
        let other = entries[1].clone();
        write_history(&dir, entries);
        let store = ChangelogStore::new(dir.path().into());
        let mut scoped = ChangelogScope {
            workspace_id: Some("test-workspace".into()),
            connection_id: Some("connection-a".into()),
            session_id: "reconnected".into(),
            ..scope()
        };
        scoped.masking = Some(ConnectionMasking {
            rules: vec![MaskingRule {
                table: "users".into(),
                column: "alias".into(),
                mode: MaskMode::Hidden,
            }],
            mask_detected_columns: false,
        });
        let ns = &own.namespace;
        let filter = ChangelogFilter::default();
        let (timeline, count) = store
            .get_timeline_page(&scoped, ns, "users", &filter)
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(timeline[0].entry_id, own.id);
        assert!(store.get_entry(&scoped, &other.id).unwrap().is_none());
        let selected = store.get_entry(&scoped, &own.id).unwrap().unwrap();
        let history = store
            .get_row_history(&scoped, ns, "users", &pk(0), None)
            .unwrap();
        let state = store
            .get_row_state_at(&scoped, ns, "users", &pk(0), Utc::now())
            .unwrap();
        let diff = store
            .compute_temporal_diff(
                &scoped,
                ns,
                "users",
                own.timestamp - Duration::seconds(1),
                Utc::now(),
                None,
            )
            .unwrap();
        assert_eq!(diff.stats.added, 1);
        assert!(diff.incomplete);
        let exposed = serde_json::json!([selected, history, state, diff]).to_string();
        assert!(!exposed.contains("old-secret-fixture"));
        assert!(
            !store
                .export(&scoped, &filter)
                .unwrap()
                .contains("old-secret-fixture")
        );
        scoped.masking.as_mut().unwrap().rules[0].column = "id".into();
        let probe = ChangelogFilter {
            primary_key_search: Some("id=0".into()),
            ..Default::default()
        };
        assert_eq!(
            store
                .get_timeline_count(&scoped, ns, "users", &probe)
                .unwrap(),
            0
        );
        assert!(
            store
                .get_row_history(&scoped, ns, "users", &pk(0), None)
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .get_row_state_at(&scoped, ns, "users", &pk(0), Utc::now())
                .unwrap()
                .is_none()
        );
        assert!(
            store.get_timeline(&scoped, ns, "users", &filter).unwrap()[0]
                .primary_key
                .is_none()
        );
    }

    #[test]
    fn retained_history_diff_replays_both_sides_of_cache_boundary() {
        let dir = TempDir::new().unwrap();
        let mut entries = history_fixture(MAX_CACHE_ENTRIES + 5);
        let initial = entries[0].clone();
        let mut updated = make_entry(
            "users",
            ChangeOperation::Update,
            pk(0),
            initial.after.clone(),
            Some(row(0, "updated")),
        );
        updated.timestamp = Utc::now();
        let mut deleted = make_entry(
            "users",
            ChangeOperation::Delete,
            pk(1),
            entries[1].after.clone(),
            None,
        );
        deleted.timestamp = updated.timestamp;
        entries.extend([updated, deleted]);
        write_history(&dir, entries);
        let store = ChangelogStore::new(dir.path().into());
        let diff = store
            .compute_temporal_diff(
                &scope(),
                &initial.namespace,
                "users",
                initial.timestamp - Duration::seconds(1),
                Utc::now(),
                None,
            )
            .unwrap();
        assert_eq!(diff.stats.added, MAX_CACHE_ENTRIES + 4);
        assert_eq!(diff.stats.modified, 0);
        assert_eq!(diff.stats.removed, 0);
        assert_eq!(
            diff.rows
                .iter()
                .find(|row| row.primary_key == pk(0))
                .unwrap()
                .state_at_t2,
            Some(row(0, "updated"))
        );
        assert!(!diff.rows.iter().any(|row| row.primary_key == pk(1)));
    }

    #[test]
    fn retained_history_read_failure_is_never_successful_partial_history() {
        let dir = TempDir::new().unwrap();
        let entries = history_fixture(MAX_CACHE_ENTRIES + 1);
        let oldest = entries[0].clone();
        write_history(&dir, entries);
        let store = ChangelogStore::new(dir.path().into());
        // Readable first record followed by an I/O decoding error must not produce partial results.
        let mut contents = serde_json::to_vec(&oldest).unwrap();
        contents.extend_from_slice(b"\n\xff\n");
        fs::write(&store.log_path, contents).unwrap();
        let ns = &oldest.namespace;
        let filter = ChangelogFilter::default();
        assert!(store.get_entries(&scope(), &filter).is_err());
        assert!(
            store
                .get_timeline_page(&scope(), ns, "users", &filter)
                .is_err()
        );
        assert!(
            store
                .get_timeline_count(&scope(), ns, "users", &filter)
                .is_err()
        );
        assert!(store.get_entry(&scope(), &oldest.id).is_err());
        assert!(
            store
                .get_row_history(&scope(), ns, "users", &pk(0), None)
                .is_err()
        );
        assert!(
            store
                .get_row_state_at(&scope(), ns, "users", &pk(0), Utc::now())
                .is_err()
        );
        assert!(
            store
                .compute_temporal_diff(
                    &scope(),
                    ns,
                    "users",
                    oldest.timestamp - Duration::seconds(1),
                    Utc::now(),
                    None
                )
                .is_err()
        );
        assert!(
            store
                .get_rollback_entries(
                    &scope(),
                    ns,
                    "users",
                    oldest.timestamp - Duration::seconds(1)
                )
                .is_err()
        );
        assert!(store.export(&scope(), &filter).is_err());
        // Failed startup loads must not turn an unreadable journal into an empty complete cache.
        let reloaded = ChangelogStore::new(dir.path().into());
        assert!(reloaded.get_entries(&scope(), &filter).is_err());
    }

    #[test]
    fn retained_history_failed_purge_and_clear_preserve_cache_and_disk() {
        let dir = TempDir::new().unwrap();
        let store = ChangelogStore::new(dir.path().into());
        let mut entry = history_fixture(1).pop().unwrap();
        entry.timestamp = Utc::now() - Duration::days(40);
        store.record(entry.clone());
        let contents = fs::read(&store.log_path).unwrap();
        let temp_path = store.log_path.with_extension("jsonl.tmp");
        fs::create_dir(&temp_path).unwrap();
        assert!(store.enforce_retention().is_err());
        assert!(store.clear_all().is_err());
        assert!(
            store
                .clear_table(&scope(), &entry.namespace, "users")
                .is_err()
        );
        assert_eq!(fs::read(&store.log_path).unwrap(), contents);
        assert!(store.get_entry(&scope(), &entry.id).unwrap().is_some());
        fs::remove_dir(temp_path).unwrap();
        store.clear_all().unwrap();
        assert!(store.get_entry(&scope(), &entry.id).unwrap().is_none());
        assert!(fs::read(&store.log_path).unwrap().is_empty());
    }

    #[test]
    fn retained_history_purge_keeps_legacy_lines_and_unlimited_retention() {
        let dir = TempDir::new().unwrap();
        let mut entries = history_fixture(2);
        entries[0].timestamp = Utc::now() - Duration::days(40);
        let valid = entries[1].clone();
        write_history(&dir, entries);
        writeln!(
            OpenOptions::new()
                .append(true)
                .open(dir.path().join("changelog.jsonl"))
                .unwrap(),
            "legacy-unparseable-line"
        )
        .unwrap();
        let store = ChangelogStore::new(dir.path().into());
        let config = store.get_config().unwrap();
        store
            .update_config(TimeTravelConfig {
                retention_days: 0,
                ..config.clone()
            })
            .unwrap();
        store.enforce_retention().unwrap();
        assert_eq!(
            fs::read_to_string(&store.log_path).unwrap().lines().count(),
            3
        );
        store.update_config(config).unwrap();
        store.enforce_retention().unwrap();
        let contents = fs::read_to_string(&store.log_path).unwrap();
        assert_eq!(contents.lines().count(), 2);
        assert!(contents.contains("legacy-unparseable-line"));
        assert_eq!(
            store
                .get_entries(&scope(), &ChangelogFilter::default())
                .unwrap()[0]
                .id,
            valid.id
        );
    }

    #[test]
    fn retained_history_rollback_refuses_truncation_and_excludes_target_instant() {
        let dir = TempDir::new().unwrap();
        let entries = history_fixture(10_002);
        let first = entries[0].clone();
        let second_timestamp = entries[1].timestamp;
        write_history(&dir, entries);
        let store = ChangelogStore::new(dir.path().into());
        assert!(
            store
                .get_rollback_entries(&scope(), &first.namespace, "users", first.timestamp)
                .is_err()
        );
        let accepted = store
            .get_rollback_entries(&scope(), &first.namespace, "users", second_timestamp)
            .unwrap();
        assert_eq!(accepted.len(), 10_000);
        assert!(
            accepted
                .iter()
                .all(|entry| entry.timestamp > second_timestamp)
        );
        assert!(
            accepted
                .windows(2)
                .all(|pair| pair[0].timestamp >= pair[1].timestamp)
        );
    }

    #[test]
    fn retained_history_rewrite_serializes_with_new_captures() {
        use std::sync::Arc;
        let dir = TempDir::new().unwrap();
        write_history(&dir, history_fixture(MAX_CACHE_ENTRIES + 1));
        let store = Arc::new(ChangelogStore::new(dir.path().into()));
        let writer = Arc::clone(&store);
        let join = std::thread::spawn(move || {
            for mut entry in history_fixture(100) {
                entry.table_name = "retained".into();
                writer.record(entry);
            }
        });
        let ns = Namespace {
            database: "testdb".into(),
            schema: Some("public".into()),
        };
        store.clear_table(&scope(), &ns, "users").unwrap();
        join.join().unwrap();
        let restarted = ChangelogStore::new(dir.path().into());
        for store in [&*store, &restarted] {
            assert_eq!(
                store
                    .get_timeline_count(&scope(), &ns, "retained", &ChangelogFilter::default())
                    .unwrap(),
                100
            );
            assert_eq!(
                store
                    .get_timeline_count(&scope(), &ns, "users", &ChangelogFilter::default())
                    .unwrap(),
                0
            );
        }
    }

    #[test]
    fn retained_history_default_capacity_keeps_cache_bounded() {
        let dir = TempDir::new().unwrap();
        let first = history_fixture(1).pop().unwrap();
        let start = first.timestamp;
        write_history(
            &dir,
            (0..50_000).map(|index| {
                let mut entry = first.clone();
                entry.id = uuid::Uuid::new_v4();
                entry.timestamp = start + Duration::milliseconds(index);
                entry.primary_key = pk(index);
                entry.after = Some(row(index, &"x".repeat(128)));
                entry
            }),
        );
        let started = std::time::Instant::now();
        let store = ChangelogStore::new(dir.path().into());
        let load_ms = started.elapsed().as_millis();
        let started = std::time::Instant::now();
        let filter = ChangelogFilter {
            limit: Some(50),
            offset: Some(49_950),
            ..Default::default()
        };
        let (page, count) = store
            .get_timeline_page(&scope(), &first.namespace, "users", &filter)
            .unwrap();
        let page_ms = started.elapsed().as_millis();
        assert_eq!(count, 50_000);
        assert_eq!(page.len(), 50);
        assert_eq!(page.last().unwrap().primary_key, Some(pk(0)));
        let started = std::time::Instant::now();
        let diff = store
            .compute_temporal_diff(
                &scope(),
                &first.namespace,
                "users",
                start - Duration::seconds(1),
                Utc::now(),
                Some(50),
            )
            .unwrap();
        let diff_ms = started.elapsed().as_millis();
        assert_eq!(diff.stats.added, 50_000);
        assert_eq!(diff.rows.len(), 50);
        assert!(diff.truncated);
        assert!(!diff.incomplete);
        assert_eq!(store.entries.read().len(), MAX_CACHE_ENTRIES);
        eprintln!(
            "history fixture: entries=50000 bytes={} load_ms={load_ms} deep_page_ms={page_ms} diff_ms={diff_ms}",
            fs::metadata(&store.log_path).unwrap().len()
        );
    }

    #[test]
    fn workspace_isolation_rejects_copied_connection_id_on_every_history_read() {
        let dir = TempDir::new().unwrap();
        let store = ChangelogStore::new(dir.path().into());
        let mut own = history_fixture(1).pop().unwrap();
        own.workspace_id = Some("workspace-a".into());
        own.connection_id = Some("copied-connection".into());
        let mut other = own.clone();
        other.id = uuid::Uuid::new_v4();
        other.workspace_id = Some("workspace-b".into());
        other.after = Some(row(0, "other workspace"));
        store.record(own.clone());
        store.record(other.clone());
        drop(store);
        let store = ChangelogStore::new(dir.path().into());
        let scoped = ChangelogScope {
            workspace_id: own.workspace_id.clone(),
            connection_id: own.connection_id.clone(),
            session_id: "reconnected".into(),
            ..scope()
        };
        let ns = &own.namespace;
        let filter = ChangelogFilter::default();
        let (page, count) = store
            .get_timeline_page(&scoped, ns, "users", &filter)
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(page[0].entry_id, own.id);
        assert!(store.get_entry(&scoped, &other.id).unwrap().is_none());
        assert_eq!(
            store
                .get_row_history(&scoped, ns, "users", &pk(0), None)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .get_row_state_at(&scoped, ns, "users", &pk(0), Utc::now())
                .unwrap(),
            own.after
        );
        let start = own.timestamp - Duration::seconds(1);
        let diff = store
            .compute_temporal_diff(&scoped, ns, "users", start, Utc::now(), None)
            .unwrap();
        assert_eq!(diff.rows[0].state_at_t2, own.after);
        assert_eq!(
            store
                .get_rollback_entries(&scoped, ns, "users", start)
                .unwrap()
                .len(),
            1
        );
        assert!(
            !store
                .export(&scoped, &filter)
                .unwrap()
                .contains("other workspace")
        );
        store.clear_table(&scoped, ns, "users").unwrap();
        let other_scope = ChangelogScope {
            workspace_id: other.workspace_id,
            ..scoped
        };
        assert!(store.get_entry(&other_scope, &other.id).unwrap().is_some());
    }

    #[test]
    fn workspace_isolation_never_assigns_legacy_history_to_a_new_session() {
        let mut legacy = history_fixture(1).pop().unwrap();
        legacy.connection_id = Some("copied-connection".into());
        let mut json = serde_json::to_value(&legacy).unwrap();
        json.as_object_mut().unwrap().remove("workspace_id");
        let legacy: ChangelogEntry = serde_json::from_value(json).unwrap();
        let scoped = ChangelogScope {
            connection_id: legacy.connection_id.clone(),
            session_id: "reconnected".into(),
            ..scope()
        };
        assert!(!scoped.contains(&legacy));
        let original_session = ChangelogScope {
            session_id: legacy.session_id.clone(),
            ..scoped
        };
        assert!(original_session.contains(&legacy));
    }

    #[test]
    fn workspace_isolation_clear_preserves_other_workspaces_and_unknown_origins_beyond_cache() {
        let dir = TempDir::new().unwrap();
        let mut entries = history_fixture(MAX_CACHE_ENTRIES + 5);
        let mut legacy = entries[0].clone();
        legacy.workspace_id = None;
        let mut other = entries[1].clone();
        other.workspace_id = Some("workspace-b".into());
        entries[0] = legacy.clone();
        entries[1] = other.clone();
        write_history(&dir, entries);
        writeln!(
            OpenOptions::new()
                .append(true)
                .open(dir.path().join("changelog.jsonl"))
                .unwrap(),
            "unrecognized-legacy-line"
        )
        .unwrap();
        let store = ChangelogStore::new(dir.path().into());
        store.clear_workspace("test-workspace").unwrap();
        let remaining = fs::read_to_string(&store.log_path).unwrap();
        assert_eq!(remaining.lines().count(), 3);
        assert!(remaining.contains(&legacy.id.to_string()));
        assert!(remaining.contains(&other.id.to_string()));
        assert!(remaining.contains("unrecognized-legacy-line"));
        let other_scope = ChangelogScope {
            workspace_id: other.workspace_id,
            ..scope()
        };
        assert!(store.get_entry(&other_scope, &other.id).unwrap().is_some());
        assert!(store.get_entry(&scope(), &other.id).unwrap().is_none());
        assert_eq!(store.file_line_count.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn workspace_isolation_applies_to_direct_connections_and_disk_only_history() {
        let dir = TempDir::new().unwrap();
        let mut entries = history_fixture(MAX_CACHE_ENTRIES + 1);
        let own = entries[0].clone();
        for entry in &mut entries[1..] {
            entry.workspace_id = Some("workspace-b".into());
        }
        write_history(&dir, entries);
        let store = ChangelogStore::new(dir.path().into());
        let (page, count) = store
            .get_timeline_page(
                &scope(),
                &own.namespace,
                "users",
                &ChangelogFilter::default(),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(page[0].entry_id, own.id);
        let unknown = ChangelogScope {
            workspace_id: None,
            ..scope()
        };
        assert!(!unknown.contains(&own));
    }

    #[test]
    fn retention_policy_size_is_enforced_during_capture() {
        let dir = TempDir::new().unwrap();
        let store = ChangelogStore::new(dir.path().into());
        store
            .update_config(TimeTravelConfig {
                max_file_size_mb: 1,
                ..Default::default()
            })
            .unwrap();
        let entries = history_fixture(3);
        let latest_id = entries[2].id;
        for mut entry in entries {
            entry.after = Some(row(1, &"x".repeat(400_000)));
            store.record(entry);
        }
        assert!(fs::metadata(&store.log_path).unwrap().len() <= 1_048_576);
        assert!(store.get_entry(&scope(), &latest_id).unwrap().is_some());
    }

    #[test]
    fn retention_policy_new_duration_applies_to_existing_history() {
        let dir = TempDir::new().unwrap();
        let store = ChangelogStore::new(dir.path().into());
        let mut entry = history_fixture(1).pop().unwrap();
        entry.timestamp = Utc::now() - Duration::days(40);
        store.record(entry.clone());
        store
            .update_config(TimeTravelConfig {
                retention_days: 7,
                ..Default::default()
            })
            .unwrap();
        assert!(store.get_entry(&scope(), &entry.id).unwrap().is_none());
        assert!(fs::read(&store.log_path).unwrap().is_empty());
    }

    #[test]
    fn retention_policy_failed_config_save_keeps_previous_settings() {
        let dir = TempDir::new().unwrap();
        let store = ChangelogStore::new(dir.path().into());
        fs::create_dir(dir.path().join("time-travel.json.tmp")).unwrap();
        assert!(
            store
                .update_config(TimeTravelConfig {
                    enabled: false,
                    ..Default::default()
                })
                .is_err()
        );
        assert!(store.get_config().unwrap().enabled);
    }

    #[test]
    fn retention_policy_lower_entry_limit_applies_without_a_new_capture() {
        let dir = TempDir::new().unwrap();
        write_history(&dir, history_fixture(20));
        let store = ChangelogStore::new(dir.path().into());
        store
            .update_config(TimeTravelConfig {
                max_entries: 4,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            fs::read_to_string(&store.log_path).unwrap().lines().count(),
            3
        );
    }

    #[test]
    fn retention_policy_oversized_latest_event_never_exposes_a_stale_state() {
        let dir = TempDir::new().unwrap();
        let store = ChangelogStore::new(dir.path().into());
        store
            .update_config(TimeTravelConfig {
                max_file_size_mb: 1,
                ..Default::default()
            })
            .unwrap();
        let mut entry = history_fixture(1).pop().unwrap();
        store.record(entry.clone());
        entry.id = uuid::Uuid::new_v4();
        entry.after = Some(row(1, &"é".repeat(600_000)));
        store.record(entry);
        assert!(fs::read(&store.log_path).unwrap().is_empty());
        assert!(
            store
                .get_entries(&scope(), &ChangelogFilter::default())
                .unwrap()
                .is_empty()
        );
        let next = history_fixture(1).pop().unwrap();
        store.record(next.clone());
        assert!(store.get_entry(&scope(), &next.id).unwrap().is_some());
    }

    #[test]
    fn retention_policy_combines_age_count_and_bytes_without_dropping_newer_events() {
        let dir = TempDir::new().unwrap();
        let mut entries = history_fixture(9);
        for (index, entry) in entries.iter_mut().enumerate() {
            entry.after = Some(row(index as i64, &"é".repeat(150_000)));
            if index % 2 == 0 {
                entry.timestamp = Utc::now() - Duration::days(40);
            }
        }
        let expected: Vec<_> = [5, 7].map(|index| entries[index].id).into();
        write_history(&dir, entries);
        let store = ChangelogStore::new(dir.path().into());
        store
            .update_config(TimeTravelConfig {
                max_entries: 3,
                max_file_size_mb: 1,
                retention_days: 7,
                ..Default::default()
            })
            .unwrap();
        let actual: Vec<_> = store.entries.read().iter().map(|entry| entry.id).collect();
        assert_eq!(actual, expected);
        assert!(fs::metadata(&store.log_path).unwrap().len() <= 1_048_576);
        assert_eq!(store.file_line_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn profile_late_capture_respects_new_exclusions_and_environment_policy() {
        for production_only in [false, true] {
            let dir = TempDir::new().unwrap();
            let store = ChangelogStore::new(dir.path().into());
            let entry = history_fixture(1).pop().unwrap();
            assert!(store.should_capture(&entry.table_name, &entry.environment));
            store
                .update_config(TimeTravelConfig {
                    production_only,
                    excluded_tables: if production_only {
                        vec![]
                    } else {
                        vec![entry.table_name.clone()]
                    },
                    ..Default::default()
                })
                .unwrap();
            // The mutation was prepared before the settings changed.
            store.record(entry);
            assert!(!store.log_path.exists());
            assert!(store.entries.read().is_empty());
        }
    }

    #[test]
    fn profile_v0139_history_and_custom_policy_survive_reopening() {
        let dir = TempDir::new().unwrap();
        // Literal pre-upgrade schema: no connection_id or workspace_id in v0.1.39.
        let legacy = serde_json::json!({
            "id": "c15477e3-87ef-41f9-9b1e-2ba24d73e381",
            "timestamp": "2025-01-01T00:00:00Z",
            "session_id": "test-session",
            "driver_id": "postgres",
            "namespace": {"database": "testdb", "schema": "public"},
            "table_name": "users",
            "operation": "insert",
            "primary_key": {"id": 9007199254740993_i64},
            "before": null,
            "after": {"id": 9007199254740993_i64, "private_note": "synthetic-legacy-secret"},
            "changed_columns": [],
            "connection_name": "Legacy fixture",
            "environment": "production"
        });
        let config = br#"{"enabled":false,"max_entries":12345,"retention_days":0,"max_file_size_mb":0,"excluded_tables":["sessions"],"production_only":true,"sensitive_columns":["private_note"]}"#;
        let journal = format!("{legacy}\n");
        fs::write(dir.path().join("time-travel.json"), config).unwrap();
        fs::write(dir.path().join("changelog.jsonl"), &journal).unwrap();
        for _ in 0..2 {
            let store = ChangelogStore::new(dir.path().into());
            let restored = store.get_config().unwrap();
            assert_eq!(
                serde_json::to_value(&restored).unwrap(),
                serde_json::from_slice::<serde_json::Value>(config).unwrap()
            );
            assert!(!store.should_capture("users", "production"));
            store.enforce_retention().unwrap();
            let entries = store
                .get_entries(&scope(), &ChangelogFilter::default())
                .unwrap();
            assert_eq!(entries.len(), 1);
            assert!(entries[0].connection_id.is_none());
            assert!(entries[0].workspace_id.is_none());
            assert_eq!(entries[0].primary_key["id"], 9007199254740993_i64);
            assert_eq!(
                entries[0].after.as_ref().unwrap()["private_note"],
                "[REDACTED]"
            );
            let reconnected = ChangelogScope {
                session_id: "new-session-after-upgrade".into(),
                ..scope()
            };
            assert!(
                store
                    .get_entries(&reconnected, &ChangelogFilter::default())
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(fs::read(&store.config_path).unwrap(), config);
            assert_eq!(fs::read_to_string(&store.log_path).unwrap(), journal);
        }
        let store = ChangelogStore::new(dir.path().into());
        let mut restored = store.get_config().unwrap();
        restored.enabled = true;
        store.update_config(restored).unwrap();
        let store = ChangelogStore::new(dir.path().into());
        assert!(store.should_capture("users", "production"));
        assert!(!store.should_capture("users", "development"));
        assert!(!store.should_capture("sessions", "production"));
        assert_eq!(
            store.get_config().unwrap().sensitive_columns,
            ["private_note"]
        );
        assert_eq!(fs::read_to_string(&store.log_path).unwrap(), journal);
    }

    #[test]
    fn profile_pending_reads_use_the_restored_privacy_policy() {
        use std::sync::{Arc, Barrier, mpsc};
        use std::time::Duration as StdDuration;

        let dir = TempDir::new().unwrap();
        let mut entry = history_fixture(1).pop().unwrap();
        entry.primary_key = HashMap::from([("lookup".into(), serde_json::json!("profile-secret"))]);
        write_history(&dir, [entry.clone()]);
        fs::write(dir.path().join("time-travel.json"), b"{broken").unwrap();
        let store = Arc::new(ChangelogStore::new(dir.path().into()));
        let ready = Arc::new(Barrier::new(4));
        let (completed, results) = mpsc::channel();
        // Hold the same lock as settings recovery while history requests queue.
        let journal = store.file_lock.lock();
        let readers: Vec<_> = (0..3)
            .map(|kind| {
                let store = Arc::clone(&store);
                let ready = Arc::clone(&ready);
                let completed = completed.clone();
                let entry = entry.clone();
                std::thread::spawn(move || {
                    ready.wait();
                    let result = match kind {
                        0 => store
                            .get_entries(&scope(), &ChangelogFilter::default())
                            .map(|value| serde_json::to_string(&value).unwrap()),
                        1 => store
                            .get_timeline_page(
                                &scope(),
                                &entry.namespace,
                                "users",
                                &ChangelogFilter::default(),
                            )
                            .map(|value| serde_json::to_string(&value).unwrap()),
                        _ => store
                            .get_entry(&scope(), &entry.id)
                            .map(|value| serde_json::to_string(&value).unwrap()),
                    };
                    completed.send(result).unwrap();
                })
            })
            .collect();
        ready.wait();
        assert!(results.recv_timeout(StdDuration::from_millis(100)).is_err());
        fs::write(
            &store.config_path,
            serde_json::to_vec(&TimeTravelConfig {
                sensitive_columns: vec!["lookup".into(), "name".into()],
                ..Default::default()
            })
            .unwrap(),
        )
        .unwrap();
        store.load_config_from_disk();
        drop(journal);
        for reader in readers {
            reader.join().unwrap();
        }
        for _ in 0..3 {
            let result = results
                .recv_timeout(StdDuration::from_secs(1))
                .unwrap()
                .unwrap();
            assert!(
                !result.contains("profile-secret"),
                "A queued read used the default policy after recovery"
            );
            assert!(
                !result.contains("retained"),
                "A queued read ignored the restored sensitive columns"
            );
        }
    }

    #[test]
    fn invalid_profile_config_recovers_only_after_a_readable_policy_is_restored() {
        let dir = TempDir::new().unwrap();
        write_history(&dir, history_fixture(1));
        let config_path = dir.path().join("time-travel.json");
        fs::write(&config_path, b"{broken").unwrap();
        let store = ChangelogStore::new(dir.path().into());
        let original = fs::read(&store.log_path).unwrap();
        assert!(store.get_config().is_err());
        assert_eq!(fs::read(&config_path).unwrap(), b"{broken");
        fs::remove_file(&config_path).unwrap();
        assert!(store.get_config().is_err());
        assert!(!store.is_enabled());
        let config = TimeTravelConfig {
            enabled: false,
            retention_days: 0,
            sensitive_columns: vec!["name".into()],
            ..Default::default()
        };
        fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        assert!(!store.get_config().unwrap().enabled);
        assert!(!store.is_enabled());
        let entries = store
            .get_entries(&scope(), &ChangelogFilter::default())
            .unwrap();
        assert_eq!(entries[0].after.as_ref().unwrap()["name"], "[REDACTED]");
        assert_eq!(fs::read(&store.log_path).unwrap(), original);
    }

    #[test]
    fn invalid_profile_config_cannot_resume_capture_with_default_privacy() {
        for unreadable in [false, true] {
            let dir = TempDir::new().unwrap();
            write_history(&dir, history_fixture(1));
            let config_path = dir.path().join("time-travel.json");
            if unreadable {
                fs::create_dir(&config_path).unwrap();
            } else {
                fs::write(&config_path, b"{broken").unwrap();
            }
            let store = ChangelogStore::new(dir.path().into());
            let original = fs::read(&store.log_path).unwrap();
            let mut entry = history_fixture(1).pop().unwrap();
            entry.after = Some(HashMap::from([(
                "private_note".into(),
                serde_json::json!("synthetic-confidential-value"),
            )]));
            store.record(entry);
            assert_eq!(fs::read(&store.log_path).unwrap(), original);
            assert!(!store.is_enabled());
            assert!(!store.should_capture("users", "production"));
            assert!(store.enforce_retention().is_err());
        }
    }

    #[test]
    fn invalid_profile_config_cannot_expose_history_using_default_privacy() {
        let dir = TempDir::new().unwrap();
        let entry = history_fixture(1).pop().unwrap();
        write_history(&dir, [entry.clone()]);
        fs::write(dir.path().join("time-travel.json"), b"{broken").unwrap();
        let store = ChangelogStore::new(dir.path().into());
        let ns = &entry.namespace;
        let filter = ChangelogFilter::default();
        let outcomes = [
            ("entry", store.get_entry(&scope(), &entry.id).is_err()),
            ("entries", store.get_entries(&scope(), &filter).is_err()),
            ("export", store.export(&scope(), &filter).is_err()),
            (
                "timeline",
                store
                    .get_timeline_page(&scope(), ns, "users", &filter)
                    .is_err(),
            ),
            (
                "count",
                store
                    .get_timeline_count(&scope(), ns, "users", &filter)
                    .is_err(),
            ),
            (
                "row history",
                store
                    .get_row_history(&scope(), ns, "users", &entry.primary_key, None)
                    .is_err(),
            ),
            (
                "row state",
                store
                    .get_row_state_at(&scope(), ns, "users", &entry.primary_key, Utc::now())
                    .is_err(),
            ),
            (
                "rollback",
                store
                    .get_rollback_entries(
                        &scope(),
                        ns,
                        "users",
                        entry.timestamp - Duration::seconds(1),
                    )
                    .is_err(),
            ),
            (
                "diff",
                store
                    .compute_temporal_diff(
                        &scope(),
                        ns,
                        "users",
                        entry.timestamp - Duration::seconds(1),
                        Utc::now(),
                        None,
                    )
                    .is_err(),
            ),
        ];
        let exposed: Vec<_> = outcomes.into_iter().filter(|(_, denied)| !denied).collect();
        assert!(
            exposed.is_empty(),
            "Unreadable privacy policy accepted by {exposed:?}"
        );
        assert!(!store.can_identify_row(&scope(), "users", &entry.primary_key));
    }

    #[test]
    fn retention_policy_invalid_config_suspends_cleanup_until_a_valid_save() {
        let dir = TempDir::new().unwrap();
        let mut entry = history_fixture(1).pop().unwrap();
        entry.timestamp = Utc::now() - Duration::days(40);
        write_history(&dir, [entry.clone()]);
        fs::write(dir.path().join("time-travel.json"), b"{broken").unwrap();
        let store = ChangelogStore::new(dir.path().into());
        let original = fs::read(&store.log_path).unwrap();
        assert!(store.enforce_retention().is_err());
        assert_eq!(fs::read(&store.log_path).unwrap(), original);
        assert!(store.get_entry(&scope(), &entry.id).is_err());
        store.update_config(TimeTravelConfig::default()).unwrap();
        assert!(fs::read(&store.log_path).unwrap().is_empty());
    }

    #[test]
    fn retention_policy_cleanup_failure_reports_saved_policy_and_preserves_history() {
        let dir = TempDir::new().unwrap();
        write_history(&dir, history_fixture(4));
        let store = ChangelogStore::new(dir.path().into());
        let original = fs::read(&store.log_path).unwrap();
        let blocked = store.log_path.with_extension("jsonl.tmp");
        fs::create_dir(&blocked).unwrap();
        let error = store
            .update_config(TimeTravelConfig {
                max_entries: 1,
                ..Default::default()
            })
            .unwrap_err();
        assert!(error.contains("settings saved, but retention failed"));
        assert_eq!(fs::read(&store.log_path).unwrap(), original);
        assert_eq!(store.entries.read().len(), 4);
        let saved: TimeTravelConfig =
            serde_json::from_slice(&fs::read(&store.config_path).unwrap()).unwrap();
        assert_eq!(saved.max_entries, 1);
        assert_eq!(store.get_config().unwrap().max_entries, 1);
        fs::remove_dir(blocked).unwrap();
        store.enforce_retention().unwrap();
        assert!(fs::read(&store.log_path).unwrap().is_empty());
    }

    #[test]
    fn retention_policy_unlimited_and_extreme_values_leave_history_untouched() {
        let dir = TempDir::new().unwrap();
        let mut entry = history_fixture(1).pop().unwrap();
        entry.timestamp = Utc::now() - Duration::days(400);
        entry.after = Some(row(1, &"x".repeat(1_100_000)));
        write_history(&dir, [entry]);
        let store = ChangelogStore::new(dir.path().into());
        let original = fs::read(&store.log_path).unwrap();
        // A no-op pass must not need a writable replacement file.
        fs::create_dir(store.log_path.with_extension("jsonl.tmp")).unwrap();
        for (days, size) in [(0, 0), (u32::MAX, u64::MAX)] {
            store
                .update_config(TimeTravelConfig {
                    retention_days: days,
                    max_file_size_mb: size,
                    max_entries: usize::MAX,
                    ..Default::default()
                })
                .unwrap();
            store.enforce_retention().unwrap();
            assert_eq!(fs::read(&store.log_path).unwrap(), original);
        }
    }

    #[tokio::test]
    async fn retention_policy_worker_cleans_on_start_and_retries_while_capture_is_disabled() {
        use std::sync::Arc;
        use std::time::Duration as StdDuration;

        for blocked_first_pass in [false, true] {
            let dir = TempDir::new().unwrap();
            let mut entry = history_fixture(1).pop().unwrap();
            entry.timestamp = Utc::now() - Duration::days(40);
            write_history(&dir, [entry]);
            fs::write(
                dir.path().join("time-travel.json"),
                serde_json::to_vec(&TimeTravelConfig {
                    enabled: false,
                    ..Default::default()
                })
                .unwrap(),
            )
            .unwrap();
            let store = Arc::new(ChangelogStore::new(dir.path().into()));
            let path = store.log_path.clone();
            let blocked = path.with_extension("jsonl.tmp");
            if blocked_first_pass {
                fs::create_dir(&blocked).unwrap();
            }
            let worker = tokio::spawn(retention::maintenance_loop(
                Arc::downgrade(&store),
                StdDuration::from_millis(20),
            ));
            if blocked_first_pass {
                tokio::time::sleep(StdDuration::from_millis(80)).await;
                assert!(!fs::read(&path).unwrap().is_empty());
                assert!(!worker.is_finished());
                fs::remove_dir(&blocked).unwrap();
            }
            tokio::time::timeout(StdDuration::from_secs(2), async {
                while !fs::read(&path).unwrap().is_empty() {
                    tokio::time::sleep(StdDuration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            assert!(store.entries.read().is_empty());
            assert!(!store.is_enabled());
            drop(store);
            tokio::time::timeout(StdDuration::from_secs(2), worker)
                .await
                .unwrap()
                .unwrap();
        }
    }
}

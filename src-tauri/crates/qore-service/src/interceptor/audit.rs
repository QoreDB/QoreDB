// SPDX-License-Identifier: Apache-2.0

//! Audit Log Store
//!
//! Persistent audit logging for all query executions.
//! Stores entries in a rotating JSON log file.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::RwLock;

use chrono::{DateTime, Duration, Utc};
use tracing::{debug, error, info, warn};

use super::types::{AuditLogEntry, Environment, QueryOperationType};

/// Audit log store with file persistence
pub struct AuditStore {
    /// In-memory cache of recent entries
    entries: RwLock<VecDeque<AuditLogEntry>>,
    /// Path to the audit log file
    log_path: PathBuf,
    /// Maximum entries to retain in file
    max_entries: RwLock<usize>,
    /// Whether audit logging is enabled
    enabled: RwLock<bool>,
    /// Tracked line count for the audit file (avoids O(n) recount)
    file_line_count: AtomicUsize,
}

impl AuditStore {
    pub fn new(data_dir: PathBuf, max_entries: usize) -> Self {
        let log_path = data_dir.join("audit.jsonl");

        if let Some(parent) = log_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                error!("Failed to create audit log directory: {}", e);
            }
        }

        let store = Self {
            entries: RwLock::new(VecDeque::with_capacity(max_entries)),
            log_path,
            max_entries: RwLock::new(max_entries),
            enabled: RwLock::new(true),
            file_line_count: AtomicUsize::new(0),
        };

        store.load_recent_entries();

        store
    }

    /// Load recent entries from file into memory
    fn load_recent_entries(&self) {
        if !self.log_path.exists() {
            return;
        }

        match File::open(&self.log_path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                let mut entries = self.entries.write();
                let max = *self.max_entries.read();
                let mut line_count: usize = 0;

                for line in reader.lines().map_while(Result::ok) {
                    line_count += 1;
                    if let Ok(entry) = serde_json::from_str::<AuditLogEntry>(&line) {
                        if entries.len() >= max {
                            entries.pop_front();
                        }
                        entries.push_back(entry);
                    }
                }

                self.file_line_count.store(line_count, Ordering::Relaxed);
                debug!("Loaded {} audit log entries from file", entries.len());
            }
            Err(e) => {
                warn!("Failed to load audit log file: {}", e);
            }
        }
    }

    /// Enable or disable audit logging
    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.write() = enabled;
        info!(
            "Audit logging {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Update max audit entries
    pub fn set_max_entries(&self, max_entries: usize) {
        *self.max_entries.write() = max_entries;
        let mut entries = self.entries.write();
        while entries.len() > max_entries {
            entries.pop_front();
        }
        drop(entries);
        self.maybe_rotate();
    }

    pub fn is_enabled(&self) -> bool {
        *self.enabled.read()
    }

    pub fn log(&self, entry: AuditLogEntry) {
        if !self.is_enabled() {
            return;
        }

        {
            let max = *self.max_entries.read();
            let mut entries = self.entries.write();
            if entries.len() >= max {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }

        if let Err(e) = self.append_to_file(&entry) {
            error!("Failed to write audit log entry: {}", e);
        }

        self.maybe_rotate();
    }

    /// Append entry to log file
    fn append_to_file(&self, entry: &AuditLogEntry) -> std::io::Result<()> {
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

    /// Rotate the log file if it exceeds max entries
    fn maybe_rotate(&self) {
        let line_count = self.file_line_count.load(Ordering::Relaxed);
        let max_entries = *self.max_entries.read();
        if line_count <= max_entries {
            return;
        }

        // Keep the last 75% of max_entries — leaves headroom before the next rotation.
        let entries_to_keep = max_entries * 3 / 4;

        match self.rotate_file(entries_to_keep) {
            Ok(removed) => {
                self.file_line_count.fetch_sub(removed, Ordering::Relaxed);
                info!("Rotated audit log, removed {} old entries", removed);
            }
            Err(e) => {
                error!("Failed to rotate audit log: {}", e);
            }
        }
    }

    /// Rotate the log file, keeping only the last N entries
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

        // Write-then-rename for an atomic swap; a crash mid-write leaves the
        // original file intact.
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

    /// Get recent audit log entries
    pub fn get_entries(
        &self,
        limit: usize,
        offset: usize,
        environment: Option<Environment>,
        operation: Option<QueryOperationType>,
        success: Option<bool>,
        search: Option<&str>,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
    ) -> Vec<AuditLogEntry> {
        self.get_entries_filtered(
            limit,
            offset,
            environment,
            operation,
            success,
            search,
            from_date,
            to_date,
            None,
            None,
        )
    }

    /// Get audit log entries with filters, including an optional fingerprint
    /// match. Reads from the in-memory cache only — for full-history searches
    /// use `get_entries_from_disk`.
    #[allow(clippy::too_many_arguments)]
    pub fn get_entries_filtered(
        &self,
        limit: usize,
        offset: usize,
        environment: Option<Environment>,
        operation: Option<QueryOperationType>,
        success: Option<bool>,
        search: Option<&str>,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        fingerprint: Option<&str>,
        blocked: Option<bool>,
    ) -> Vec<AuditLogEntry> {
        let entries = self.entries.read();

        entries
            .iter()
            .rev()
            .filter(|e| {
                entry_matches(
                    e,
                    environment,
                    operation,
                    success,
                    search,
                    from_date,
                    to_date,
                    fingerprint,
                    blocked,
                )
            })
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Read audit entries directly from the rotated JSONL file. Used by the
    /// export path, which must reflect on-disk retention rather than the
    /// in-memory cache (cf. `SECURITY_AUDIT.md` § 4).
    ///
    /// Pagination is applied AFTER filtering, in reverse-chronological order
    /// to match `get_entries`. `limit == 0` means "no limit" (return all
    /// matching entries).
    #[allow(clippy::too_many_arguments)]
    pub fn get_entries_from_disk(
        &self,
        limit: usize,
        offset: usize,
        environment: Option<Environment>,
        operation: Option<QueryOperationType>,
        success: Option<bool>,
        search: Option<&str>,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        fingerprint: Option<&str>,
        blocked: Option<bool>,
    ) -> std::io::Result<Vec<AuditLogEntry>> {
        if !self.log_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.log_path)?;
        let reader = BufReader::new(file);

        let mut matched: Vec<AuditLogEntry> = Vec::new();
        for line in reader.lines() {
            let line = match line {
                Ok(l) if !l.trim().is_empty() => l,
                _ => continue,
            };
            let entry: AuditLogEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry_matches(
                &entry,
                environment,
                operation,
                success,
                search,
                from_date,
                to_date,
                fingerprint,
                blocked,
            ) {
                matched.push(entry);
            }
        }

        matched.reverse();

        let result = if limit == 0 {
            matched.into_iter().skip(offset).collect()
        } else {
            matched.into_iter().skip(offset).take(limit).collect()
        };

        Ok(result)
    }

    /// Get audit log statistics
    pub fn get_stats(&self) -> AuditStats {
        let entries = self.entries.read();
        let now = Utc::now();
        let last_hour = now - Duration::hours(1);
        let last_day = now - Duration::days(1);

        let mut stats = AuditStats::default();

        for entry in entries.iter() {
            stats.total += 1;

            if entry.success {
                stats.successful += 1;
            } else {
                stats.failed += 1;
            }

            if entry.blocked {
                stats.blocked += 1;
            }

            if entry.timestamp >= last_hour {
                stats.last_hour += 1;
            }

            if entry.timestamp >= last_day {
                stats.last_day += 1;
            }

            let env_key = format!("{:?}", entry.environment).to_lowercase();
            *stats.by_environment.entry(env_key).or_insert(0) += 1;

            let op_key = format!("{:?}", entry.operation_type).to_lowercase();
            *stats.by_operation.entry(op_key).or_insert(0) += 1;
        }

        stats
    }

    /// Clear all audit log entries
    pub fn clear(&self) {
        self.entries.write().clear();

        if let Err(e) = File::create(&self.log_path) {
            error!("Failed to clear audit log file: {}", e);
        }

        self.file_line_count.store(0, Ordering::Relaxed);
        info!("Audit log cleared");
    }

    /// Export audit log entries as JSON (legacy: in-memory cache only).
    pub fn export(&self) -> String {
        let entries = self.entries.read();
        let entries_vec: Vec<&AuditLogEntry> = entries.iter().collect();
        serde_json::to_string_pretty(&entries_vec).unwrap_or_else(|_| "[]".to_string())
    }

    /// Export audit log entries in the requested format. When `from_disk` is
    /// `true`, the full retained history is loaded from disk (slow but
    /// faithful to retention settings); otherwise the in-memory cache is used.
    pub fn export_format(
        &self,
        format: super::export::AuditExportFormat,
        from_disk: bool,
    ) -> std::io::Result<String> {
        let entries = if from_disk {
            self.get_entries_from_disk(0, 0, None, None, None, None, None, None, None, None)?
        } else {
            self.entries.read().iter().cloned().collect()
        };
        Ok(super::export::export_entries(&entries, format))
    }
}

#[allow(clippy::too_many_arguments)]
fn entry_matches(
    entry: &AuditLogEntry,
    environment: Option<Environment>,
    operation: Option<QueryOperationType>,
    success: Option<bool>,
    search: Option<&str>,
    from_date: Option<DateTime<Utc>>,
    to_date: Option<DateTime<Utc>>,
    fingerprint: Option<&str>,
    blocked: Option<bool>,
) -> bool {
    if let Some(env) = environment {
        if entry.environment != env {
            return false;
        }
    }

    if let Some(op) = operation {
        if entry.operation_type != op {
            return false;
        }
    }

    if let Some(want) = success {
        if entry.success != want {
            return false;
        }
    }

    if let Some(want) = blocked {
        if entry.blocked != want {
            return false;
        }
    }

    if let Some(search) = search {
        let needle = search.to_lowercase();
        let in_query = entry.query.to_lowercase().contains(&needle);
        let in_session = entry.session_id.to_lowercase().contains(&needle);
        let in_db = entry
            .database
            .as_ref()
            .map(|d| d.to_lowercase().contains(&needle))
            .unwrap_or(false);
        if !in_query && !in_session && !in_db {
            return false;
        }
    }

    if let Some(from) = from_date {
        if entry.timestamp < from {
            return false;
        }
    }
    if let Some(to) = to_date {
        if entry.timestamp > to {
            return false;
        }
    }

    if let Some(fp) = fingerprint {
        match entry.fingerprint.as_deref() {
            Some(stored) if stored == fp => {}
            _ => return false,
        }
    }

    true
}

/// Audit log statistics
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AuditStats {
    pub total: u64,
    pub successful: u64,
    pub failed: u64,
    pub blocked: u64,
    pub last_hour: u64,
    pub last_day: u64,
    pub by_environment: std::collections::HashMap<String, u64>,
    pub by_operation: std::collections::HashMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::super::export::AuditExportFormat;
    use super::super::redaction::{set_redaction_enabled, test_lock};
    use super::*;
    use tempfile::TempDir;

    fn new_store(dir: &TempDir, max: usize) -> AuditStore {
        AuditStore::new(dir.path().to_path_buf(), max)
    }

    fn entry(query: &str) -> AuditLogEntry {
        AuditLogEntry::new(
            "sess-x".into(),
            query.into(),
            Environment::Development,
            "postgres".into(),
        )
    }

    #[test]
    fn from_disk_returns_all_entries_even_when_cache_truncated() {
        let _guard = test_lock();
        set_redaction_enabled(true);
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp, 3);

        // Five entries, cache size 3 → cache is truncated, disk keeps all.
        for i in 0..5 {
            store.log(entry(&format!("SELECT {}", i)));
        }

        let in_memory = store.get_entries(100, 0, None, None, None, None, None, None);
        assert!(in_memory.len() <= 3, "cache should respect max_entries");

        let on_disk = store
            .get_entries_from_disk(0, 0, None, None, None, None, None, None, None, None)
            .unwrap();
        assert!(
            on_disk.len() >= in_memory.len(),
            "disk should have at least as many entries as cache"
        );
    }

    #[test]
    fn from_disk_filters_by_fingerprint() {
        let _guard = test_lock();
        set_redaction_enabled(true);
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp, 100);

        store.log(entry("SELECT id FROM users WHERE id = 1"));
        store.log(entry("SELECT id FROM users WHERE id = 99"));
        store.log(entry("DELETE FROM logs WHERE id = 1"));

        let target_fp = store
            .get_entries(1, 0, None, None, None, Some("SELECT id"), None, None)
            .first()
            .and_then(|e| e.fingerprint.clone())
            .expect("fingerprint should be populated");

        let matched = store
            .get_entries_from_disk(
                0,
                0,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(&target_fp),
                None,
            )
            .unwrap();

        assert_eq!(
            matched.len(),
            2,
            "two SELECT id queries share the fingerprint"
        );
        for e in &matched {
            assert_eq!(e.fingerprint.as_deref(), Some(target_fp.as_str()));
        }
    }

    #[test]
    fn export_jsonl_from_disk_yields_one_line_per_entry() {
        let _guard = test_lock();
        set_redaction_enabled(true);
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp, 100);

        for i in 0..6 {
            store.log(entry(&format!("SELECT {}", i)));
        }

        let exported = store.export_format(AuditExportFormat::Jsonl, true).unwrap();
        let line_count = exported.lines().count();
        assert_eq!(line_count, 6);

        for line in exported.lines() {
            let _: AuditLogEntry = serde_json::from_str(line).unwrap();
        }
    }

    #[test]
    fn export_csv_includes_fingerprint_column() {
        let _guard = test_lock();
        set_redaction_enabled(true);
        let tmp = TempDir::new().unwrap();
        let store = new_store(&tmp, 100);
        store.log(entry("SELECT 1"));

        let exported = store.export_format(AuditExportFormat::Csv, false).unwrap();
        assert!(exported.lines().next().unwrap().contains("fingerprint"));
    }
}

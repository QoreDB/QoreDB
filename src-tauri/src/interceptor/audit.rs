// SPDX-License-Identifier: Apache-2.0

//! Audit Log Store
//!
//! Persistent audit logging for all query executions.
//! Stores entries in a rotating JSON log file.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::RwLock;

use chrono::{DateTime, Duration, Utc};
use tracing::{debug, error, info, warn};

use super::types::{AuditLogEntry, Environment, QueryOperationType};

/// Maximum entries to keep in memory for fast access
const MEMORY_CACHE_SIZE: usize = 1000;

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
            entries: RwLock::new(VecDeque::with_capacity(MEMORY_CACHE_SIZE)),
            log_path,
            max_entries: RwLock::new(max_entries),
            enabled: RwLock::new(true),
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
                let mut entries = self.entries.write().unwrap();

                for line in reader.lines() {
                    if let Ok(line) = line {
                        if let Ok(entry) = serde_json::from_str::<AuditLogEntry>(&line) {
                            if entries.len() >= MEMORY_CACHE_SIZE {
                                entries.pop_front();
                            }
                            entries.push_back(entry);
                        }
                    }
                }

                debug!("Loaded {} audit log entries from file", entries.len());
            }
            Err(e) => {
                warn!("Failed to load audit log file: {}", e);
            }
        }
    }

    /// Enable or disable audit logging
    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.write().unwrap() = enabled;
        info!("Audit logging {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Update max audit entries
    pub fn set_max_entries(&self, max_entries: usize) {
        *self.max_entries.write().unwrap() = max_entries;
        self.maybe_rotate();
    }

    pub fn is_enabled(&self) -> bool {
        *self.enabled.read().unwrap()
    }

    pub fn log(&self, entry: AuditLogEntry) {
        if !self.is_enabled() {
            return;
        }

        {
            let mut entries = self.entries.write().unwrap();
            if entries.len() >= MEMORY_CACHE_SIZE {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }

        if let Err(e) = self.append_to_file(&entry) {
            error!("Failed to write audit log entry: {}", e);
        }

        // Rotate if needed
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

        Ok(())
    }

    /// Rotate the log file if it exceeds max entries
    fn maybe_rotate(&self) {
        // Count lines in file
        let line_count = match File::open(&self.log_path) {
            Ok(file) => BufReader::new(file).lines().count(),
            Err(_) => return,
        };

        let max_entries = *self.max_entries.read().unwrap();
        if line_count <= max_entries {
            return;
        }

        // Keep only the last max_entries
        let entries_to_keep = max_entries * 3 / 4;

        match self.rotate_file(entries_to_keep) {
            Ok(removed) => {
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
        let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();

        let total = lines.len();
        if total <= keep_count {
            return Ok(0);
        }

        let skip = total - keep_count;
        let to_keep: Vec<&String> = lines.iter().skip(skip).collect();

        // Write to temp file then rename
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
        let entries = self.entries.read().unwrap();

        entries
            .iter()
            .rev() // Most recent first
            .filter(|e| {
                // Filter by environment
                if let Some(env) = environment {
                    if e.environment != env {
                        return false;
                    }
                }

                // Filter by operation
                if let Some(op) = operation {
                    if e.operation_type != op {
                        return false;
                    }
                }

                // Filter by success
                if let Some(success) = success {
                    if e.success != success {
                        return false;
                    }
                }

                // Filter by search term
                if let Some(search) = search {
                    let search_lower = search.to_lowercase();
                    if !e.query.to_lowercase().contains(&search_lower)
                        && !e.session_id.to_lowercase().contains(&search_lower)
                        && !e.database.as_ref().map(|d| d.to_lowercase().contains(&search_lower)).unwrap_or(false)
                    {
                        return false;
                    }
                }

                // Filter by date range
                if let Some(from) = from_date {
                    if e.timestamp < from {
                        return false;
                    }
                }

                if let Some(to) = to_date {
                    if e.timestamp > to {
                        return false;
                    }
                }

                true
            })
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get audit log statistics
    pub fn get_stats(&self) -> AuditStats {
        let entries = self.entries.read().unwrap();
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

            // Count by environment
            let env_key = format!("{:?}", entry.environment).to_lowercase();
            *stats.by_environment.entry(env_key).or_insert(0) += 1;

            // Count by operation
            let op_key = format!("{:?}", entry.operation_type).to_lowercase();
            *stats.by_operation.entry(op_key).or_insert(0) += 1;
        }

        stats
    }

    /// Clear all audit log entries
    pub fn clear(&self) {
        // Clear memory cache
        self.entries.write().unwrap().clear();

        // Clear file
        if let Err(e) = File::create(&self.log_path) {
            error!("Failed to clear audit log file: {}", e);
        }

        info!("Audit log cleared");
    }

    /// Export audit log entries as JSON
    pub fn export(&self) -> String {
        let entries = self.entries.read().unwrap();
        let entries_vec: Vec<&AuditLogEntry> = entries.iter().collect();
        serde_json::to_string_pretty(&entries_vec).unwrap_or_else(|_| "[]".to_string())
    }
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

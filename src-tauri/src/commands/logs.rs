// SPDX-License-Identifier: Apache-2.0

//! Log export and local data storage management commands.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{observability, paths};

#[derive(Debug, Serialize)]
pub struct LogsExportResponse {
    pub success: bool,
    pub filename: Option<String>,
    pub content: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn export_logs() -> Result<LogsExportResponse, String> {
    match observability::collect_logs() {
        Ok(export) => Ok(LogsExportResponse {
            success: true,
            filename: Some(export.filename),
            content: Some(export.content),
            error: None,
        }),
        Err(err) => Ok(LogsExportResponse {
            success: false,
            filename: None,
            content: None,
            error: Some(err),
        }),
    }
}

#[tauri::command]
pub fn get_logs_directory() -> String {
    observability::log_directory_string()
}

#[tauri::command]
pub fn get_pending_crash_reports() -> Result<Vec<observability::CrashReport>, String> {
    observability::pending_crash_reports()
}

#[tauri::command]
pub fn acknowledge_crash_reports() -> Result<usize, String> {
    observability::acknowledge_crash_reports()
}

#[derive(Debug, serde::Deserialize)]
pub struct FrontendLogEntry {
    pub level: String,
    pub message: String,
    pub stack: Option<String>,
    pub timestamp: String,
}

/// Maximum number of frontend log entries accepted per `WINDOW`. A buggy or
/// malicious renderer can otherwise spam 10k logs/s, blowing up disk usage
/// and CPU on the tracing subscriber (cf. audit B6-H12).
const FRONTEND_LOG_RATE_LIMIT: u32 = 50;
const FRONTEND_LOG_WINDOW: Duration = Duration::from_secs(1);
/// Cap on the inline message payload. Anything longer is truncated so a
/// runaway frontend can't write multi-MB rows into the logs.
const FRONTEND_LOG_MAX_LEN: usize = 8 * 1024;

fn frontend_log_state() -> &'static Mutex<(Instant, u32, u32)> {
    use std::sync::OnceLock;
    static STATE: OnceLock<Mutex<(Instant, u32, u32)>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new((Instant::now(), 0, 0)))
}

fn admit_frontend_log() -> bool {
    let mut guard = match frontend_log_state().lock() {
        Ok(g) => g,
        Err(_) => return true, // poisoned mutex: prefer logging over dropping
    };
    let (start, count, dropped) = &mut *guard;
    if start.elapsed() >= FRONTEND_LOG_WINDOW {
        if *dropped > 0 {
            tracing::warn!(
                dropped = dropped,
                "frontend log rate-limit reached; dropped events"
            );
        }
        *start = Instant::now();
        *count = 0;
        *dropped = 0;
    }
    if *count >= FRONTEND_LOG_RATE_LIMIT {
        *dropped = dropped.saturating_add(1);
        false
    } else {
        *count = count.saturating_add(1);
        true
    }
}

#[tauri::command]
pub async fn log_frontend_message(entry: FrontendLogEntry) -> Result<(), String> {
    if !admit_frontend_log() {
        return Ok(());
    }
    // Truncate ridiculously long messages — the frontend should not be
    // piping multi-MB payloads through the log channel.
    let message = if entry.message.len() > FRONTEND_LOG_MAX_LEN {
        format!("{}…[truncated]", &entry.message[..FRONTEND_LOG_MAX_LEN])
    } else {
        entry.message
    };
    match entry.level.as_str() {
        "error" => tracing::error!(target: "frontend", stack = ?entry.stack, "{}", message),
        "warn" => tracing::warn!(target: "frontend", stack = ?entry.stack, "{}", message),
        "info" => tracing::info!(target: "frontend", "{}", message),
        "debug" => tracing::debug!(target: "frontend", "{}", message),
        _ => tracing::info!(target: "frontend", level = %entry.level, "{}", message),
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalDataCategory {
    TimeTravel,
    Replay,
    Snapshots,
    AuditLogs,
    AiModels,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalDataCategoryUsage {
    pub category: LocalDataCategory,
    pub path: String,
    pub bytes: u64,
    pub file_count: u64,
    pub encrypted_at_rest: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalDataUsageResponse {
    pub app_data_dir: String,
    pub total_bytes: u64,
    pub categories: Vec<LocalDataCategoryUsage>,
}

fn compute_dir_usage(path: &Path) -> (u64, u64) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return (0, 0);
    };
    let mut bytes = 0u64;
    let mut files = 0u64;
    for entry in entries.flatten() {
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            let (sub_b, sub_f) = compute_dir_usage(&entry.path());
            bytes = bytes.saturating_add(sub_b);
            files = files.saturating_add(sub_f);
        } else if meta.is_file() {
            bytes = bytes.saturating_add(meta.len());
            files = files.saturating_add(1);
        }
    }
    (bytes, files)
}

fn compute_audit_and_logs_usage(data_dir: &Path) -> (u64, u64) {
    let logs_dir = data_dir.join("logs");
    let (mut bytes, mut files) = compute_dir_usage(&logs_dir);

    let interceptor_dir = data_dir.join("interceptor");
    if let Ok(entries) = std::fs::read_dir(&interceptor_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            // Count audit log files, not interceptor policy config.json
            if name.starts_with("audit.jsonl")
                && let Ok(meta) = std::fs::symlink_metadata(&path)
                && meta.is_file()
            {
                bytes = bytes.saturating_add(meta.len());
                files = files.saturating_add(1);
            }
        }
    }
    (bytes, files)
}

pub fn collect_local_data_usage_at(data_dir: &Path) -> LocalDataUsageResponse {
    let time_travel_dir = data_dir.join("time-travel");
    let (tt_bytes, tt_files) = compute_dir_usage(&time_travel_dir);

    let replay_dir = data_dir.join("replays");
    let (replay_bytes, replay_files) = compute_dir_usage(&replay_dir);

    let snapshots_dir = data_dir.join("snapshots");
    let (snap_bytes, snap_files) = compute_dir_usage(&snapshots_dir);

    let logs_dir = data_dir.join("logs");
    let (logs_bytes, logs_files) = compute_audit_and_logs_usage(data_dir);

    let ai_dir = data_dir.join("ai-local");
    let (ai_bytes, ai_files) = compute_dir_usage(&ai_dir);

    let categories = vec![
        LocalDataCategoryUsage {
            category: LocalDataCategory::TimeTravel,
            path: time_travel_dir.to_string_lossy().to_string(),
            bytes: tt_bytes,
            file_count: tt_files,
            encrypted_at_rest: false,
        },
        LocalDataCategoryUsage {
            category: LocalDataCategory::Replay,
            path: replay_dir.to_string_lossy().to_string(),
            bytes: replay_bytes,
            file_count: replay_files,
            encrypted_at_rest: false,
        },
        LocalDataCategoryUsage {
            category: LocalDataCategory::Snapshots,
            path: snapshots_dir.to_string_lossy().to_string(),
            bytes: snap_bytes,
            file_count: snap_files,
            encrypted_at_rest: false,
        },
        LocalDataCategoryUsage {
            category: LocalDataCategory::AuditLogs,
            path: logs_dir.to_string_lossy().to_string(),
            bytes: logs_bytes,
            file_count: logs_files,
            encrypted_at_rest: false,
        },
        LocalDataCategoryUsage {
            category: LocalDataCategory::AiModels,
            path: ai_dir.to_string_lossy().to_string(),
            bytes: ai_bytes,
            file_count: ai_files,
            encrypted_at_rest: false,
        },
    ];

    let total_bytes = categories
        .iter()
        .fold(0u64, |acc, c| acc.saturating_add(c.bytes));

    LocalDataUsageResponse {
        app_data_dir: data_dir.to_string_lossy().to_string(),
        total_bytes,
        categories,
    }
}

fn clear_directory_contents(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path)
            .map_err(|e| format!("Failed to inspect {}: {e}", path.display()))?;
        if meta.is_dir() && !meta.file_type().is_symlink() {
            std::fs::remove_dir_all(&path)
                .map_err(|e| format!("Failed to remove directory {}: {e}", path.display()))?;
        } else {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to remove file {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

pub fn purge_category_files_at(data_dir: &Path, category: LocalDataCategory) -> Result<(), String> {
    match category {
        LocalDataCategory::TimeTravel => clear_directory_contents(&data_dir.join("time-travel")),
        LocalDataCategory::Replay => clear_directory_contents(&data_dir.join("replays")),
        LocalDataCategory::Snapshots => clear_directory_contents(&data_dir.join("snapshots")),
        LocalDataCategory::AuditLogs => {
            clear_directory_contents(&data_dir.join("logs"))?;
            let interceptor_dir = data_dir.join("interceptor");
            if let Ok(entries) = std::fs::read_dir(&interceptor_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default();
                    if name.starts_with("audit.jsonl") {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
            Ok(())
        }
        LocalDataCategory::AiModels => clear_directory_contents(&data_dir.join("ai-local")),
    }
}

#[tauri::command]
pub async fn get_local_data_usage() -> Result<LocalDataUsageResponse, String> {
    let data_dir = paths::app_data_dir();
    Ok(collect_local_data_usage_at(&data_dir))
}

#[tauri::command]
pub async fn purge_local_data_category(
    state: State<'_, crate::SharedState>,
    category: LocalDataCategory,
) -> Result<LocalDataUsageResponse, String> {
    let data_dir = paths::app_data_dir();

    match category {
        LocalDataCategory::TimeTravel => {
            let changelog_store = {
                let guard = state.lock().await;
                std::sync::Arc::clone(&guard.changelog_store)
            };
            changelog_store.clear_all()?;
            purge_category_files_at(&data_dir, category)?;
        }
        LocalDataCategory::Replay => {
            #[cfg(feature = "pro")]
            {
                let replay = {
                    let guard = state.lock().await;
                    std::sync::Arc::clone(&guard.replay)
                };
                if replay.is_busy() {
                    return Err(
                        "Cannot purge replay captures while a recording or replay run is active"
                            .to_string(),
                    );
                }
            }
            purge_category_files_at(&data_dir, category)?;
        }
        LocalDataCategory::Snapshots => {
            purge_category_files_at(&data_dir, category)?;
        }
        LocalDataCategory::AuditLogs => {
            let interceptor = {
                let guard = state.lock().await;
                std::sync::Arc::clone(&guard.interceptor)
            };
            interceptor.clear_audit();
            interceptor.clear_slow_queries();
            purge_category_files_at(&data_dir, category)?;
        }
        LocalDataCategory::AiModels => {
            #[cfg(feature = "pro")]
            {
                let local_runtime = {
                    let guard = state.lock().await;
                    guard.ai_manager.local_runtime()
                };
                local_runtime.cancel_installation();
                local_runtime.stop();
            }
            purge_category_files_at(&data_dir, category)?;
        }
    }

    Ok(collect_local_data_usage_at(&data_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_and_purges_local_data_categories_safely() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        std::fs::create_dir_all(root.join("time-travel")).unwrap();
        std::fs::write(root.join("time-travel/changelog.jsonl"), b"1234567890").unwrap();

        std::fs::create_dir_all(root.join("replays/default")).unwrap();
        std::fs::write(root.join("replays/default/run.json"), b"12345").unwrap();

        std::fs::create_dir_all(root.join("snapshots")).unwrap();
        std::fs::write(root.join("snapshots/snap.json"), b"123456").unwrap();

        std::fs::create_dir_all(root.join("logs")).unwrap();
        std::fs::write(root.join("logs/qoredb.log"), b"logdata").unwrap();
        std::fs::create_dir_all(root.join("interceptor")).unwrap();
        std::fs::write(root.join("interceptor/audit.jsonl"), b"audit").unwrap();
        std::fs::write(root.join("interceptor/config.json"), b"{\"keep\":true}").unwrap();

        std::fs::create_dir_all(root.join("ai-local/models")).unwrap();
        std::fs::write(root.join("ai-local/models/model.gguf"), b"gguf").unwrap();

        let usage = collect_local_data_usage_at(root);
        assert_eq!(usage.categories.len(), 5);
        assert_eq!(usage.total_bytes, 10 + 5 + 6 + (7 + 5) + 4);

        // Purging AuditLogs removes logs and audit.jsonl while preserving interceptor/config.json
        purge_category_files_at(root, LocalDataCategory::AuditLogs).unwrap();
        assert!(root.join("interceptor/config.json").exists());
        let after_logs = collect_local_data_usage_at(root);
        let audit_cat = after_logs
            .categories
            .iter()
            .find(|c| c.category == LocalDataCategory::AuditLogs)
            .unwrap();
        assert_eq!(audit_cat.bytes, 0);
        assert_eq!(audit_cat.file_count, 0);

        // Purging Snapshots leaves other categories untouched
        purge_category_files_at(root, LocalDataCategory::Snapshots).unwrap();
        let after_snap = collect_local_data_usage_at(root);
        assert_eq!(after_snap.total_bytes, 10 + 5 + 4);
    }
}

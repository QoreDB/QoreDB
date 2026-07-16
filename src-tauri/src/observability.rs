// SPDX-License-Identifier: Apache-2.0

//! Logging and observability helpers.

pub use qore_service::sensitive::{self, Sensitive};

use std::backtrace::Backtrace;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::Local;
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::EnvFilter;

const LOG_FILE_PREFIX: &str = "qoredb.log";
const CRASH_FILE_PREFIX: &str = "qoredb-crash";
const RUN_MARKER_FILE: &str = "qoredb.running";
const LOG_RETENTION_DAYS: u64 = 7;

fn is_managed_log_name(name: &str) -> bool {
    name.starts_with(LOG_FILE_PREFIX) || name.starts_with(CRASH_FILE_PREFIX)
}

pub fn init_tracing() {
    let log_dir = log_directory();
    let _ = fs::create_dir_all(&log_dir);

    if let Err(e) = cleanup_old_logs(&log_dir, LOG_RETENTION_DAYS) {
        eprintln!("Failed to clean up old logs: {}", e);
    }

    let file_appender: RollingFileAppender =
        tracing_appender::rolling::daily(&log_dir, LOG_FILE_PREFIX);
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("qoredb=info,tauri=warn"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(file_appender)
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_ansi(false)
        .try_init();

    // Chain into the previous panic hook so we keep its behaviour (e.g. abort).
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let payload = panic_info.payload();
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());

        let msg = if let Some(s) = payload.downcast_ref::<&str>() {
            format!("PANIC: {}", s)
        } else if let Some(s) = payload.downcast_ref::<String>() {
            format!("PANIC: {}", s)
        } else {
            "PANIC: unknown cause".to_string()
        };

        tracing::error!(target: "panic", location = %location, message = %msg, "Application panicked");
        write_crash_report(
            "Native panic",
            &format!(
                "Location: {location}\nMessage: {msg}\nBacktrace:\n{}",
                Backtrace::force_capture()
            ),
        );

        previous_hook(panic_info);
    }));

    record_run_started();
    tracing::info!("Tracing initialized. Logs directory: {:?}", log_dir);
}

fn record_run_started() {
    let log_dir = log_directory();
    let marker_path = log_dir.join(RUN_MARKER_FILE);

    if marker_path.exists() {
        let previous_run = fs::read_to_string(&marker_path)
            .unwrap_or_else(|_| "Previous run metadata was unreadable.".to_string());
        write_crash_report(
            "Unclean shutdown detected",
            &format!(
                "QoreDB did not emit a clean exit event. This can indicate a forced termination, \
                 renderer/GPU process failure, out-of-memory termination, native crash, or power loss.\n\n\
                 Previous run:\n{previous_run}"
            ),
        );
    }

    let marker = format!(
        "started_at={}\nversion={}\nos={}\narch={}\npid={}\n",
        Local::now().to_rfc3339(),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::process::id()
    );
    if let Err(error) = fs::write(&marker_path, marker) {
        tracing::warn!(%error, "failed to write run marker");
    }
}

pub fn mark_clean_shutdown() {
    let marker_path = log_directory().join(RUN_MARKER_FILE);
    if let Err(error) = fs::remove_file(&marker_path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(%error, "failed to remove run marker");
        }
    }
}

fn write_crash_report(kind: &str, details: &str) {
    let log_dir = log_directory();
    let _ = fs::create_dir_all(&log_dir);
    let path = log_dir.join(format!(
        "{}-{}.log",
        CRASH_FILE_PREFIX,
        Local::now().format("%Y%m%d-%H%M%S%.3f")
    ));
    let report = format!(
        "QoreDB crash report\nrecorded_at={}\nversion={}\nos={}\narch={}\npid={}\nkind={}\n\n{}\n",
        Local::now().to_rfc3339(),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::process::id(),
        kind,
        details
    );

    if let Ok(mut file) = OpenOptions::new().create_new(true).write(true).open(path) {
        let _ = file.write_all(report.as_bytes());
        let _ = file.sync_all();
    }
}

pub struct LogExport {
    pub filename: String,
    pub content: String,
}

pub fn collect_logs() -> Result<LogExport, String> {
    let log_dir = log_directory();
    let entries = fs::read_dir(&log_dir)
        .map_err(|e| format!("Failed to read log directory {}: {}", log_dir.display(), e))?;

    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(is_managed_log_name)
                .unwrap_or(false)
        })
        .collect();

    if files.is_empty() {
        return Err("No log files found".to_string());
    }

    files.sort_by_key(|path| path.file_name().map(|name| name.to_os_string()));

    let mut content = String::new();
    for path in files {
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let data = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read log file {}: {}", path.display(), e))?;

        if !content.is_empty() {
            content.push_str("\n\n");
        }
        content.push_str(&format!("===== {} =====\n", filename));
        content.push_str(&data);
    }

    let filename = format!("qoredb-logs-{}.log", Local::now().format("%Y%m%d-%H%M%S"));

    Ok(LogExport { filename, content })
}

pub fn log_directory() -> PathBuf {
    // Delegates to the shared `paths` module so policy / logs / interceptor
    // all live under the same root (cf. audit B1-H4).
    crate::paths::app_log_dir()
}

pub fn log_directory_string() -> String {
    log_directory().to_string_lossy().into_owned()
}

fn cleanup_old_logs(log_dir: &Path, retention_days: u64) -> std::io::Result<()> {
    let entries = fs::read_dir(log_dir)?;
    let now = SystemTime::now();
    let retention_duration = Duration::from_secs(retention_days * 24 * 60 * 60);

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        let is_managed_log = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(is_managed_log_name)
            .unwrap_or(false);
        if !is_managed_log {
            continue;
        }

        if let Ok(metadata) = fs::metadata(&path) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = now.duration_since(modified) {
                    if age > retention_duration {
                        if let Err(e) = fs::remove_file(&path) {
                            eprintln!("Failed to remove old log file {:?}: {}", path, e);
                        } else {
                            println!("Removed old log file: {:?}", path);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_managed_log_name;

    #[test]
    fn recognizes_runtime_and_crash_logs_only() {
        assert!(is_managed_log_name("qoredb.log.2026-07-14"));
        assert!(is_managed_log_name("qoredb-crash-20260714-172500.log"));
        assert!(!is_managed_log_name("qoredb.running"));
        assert!(!is_managed_log_name("unrelated.log"));
    }
}

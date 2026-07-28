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

/// Acknowledged reports keep the crash prefix so retention and log export
/// still pick them up; only the pending-report listing filters them out.
const CRASH_ACK_SUFFIX: &str = ".seen";
/// A runaway panic loop can leave hundreds of reports behind. The UI only
/// ever shows the most recent ones.
const MAX_PENDING_CRASH_REPORTS: usize = 10;
/// Backtraces can be enormous; cap what we hand to the renderer.
const MAX_CRASH_REPORT_BYTES: usize = 64 * 1024;

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

/// Bundles the retained logs into a single shareable document. An export exists
/// to be sent to someone — otherwise the user would just open the log folder —
/// so it goes through the same scrubbing as crash reports. Nothing removed here
/// (credentials, tokens, home path) helps diagnose a bug.
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

    Ok(LogExport {
        filename,
        content: scrub_secrets(&content),
    })
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReport {
    pub filename: String,
    pub recorded_at: String,
    pub kind: String,
    pub content: String,
}

fn crash_report_paths() -> Result<Vec<PathBuf>, String> {
    let log_dir = log_directory();
    let entries = match fs::read_dir(&log_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "Failed to read log directory {}: {}",
                log_dir.display(),
                error
            ));
        }
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(is_pending_crash_name)
                .unwrap_or(false)
        })
        .collect();

    // Filenames embed a sortable timestamp, so lexical order is chronological.
    paths.sort_by_key(|path| path.file_name().map(|name| name.to_os_string()));
    paths.reverse();
    Ok(paths)
}

fn is_pending_crash_name(name: &str) -> bool {
    name.starts_with(CRASH_FILE_PREFIX) && !name.ends_with(CRASH_ACK_SUFFIX)
}

/// Crash reports the user has not dismissed yet, most recent first.
pub fn pending_crash_reports() -> Result<Vec<CrashReport>, String> {
    let paths = crash_report_paths()?;
    let total = paths.len();

    let reports: Vec<CrashReport> = paths
        .into_iter()
        .take(MAX_PENDING_CRASH_REPORTS)
        .filter_map(|path| {
            let filename = path.file_name()?.to_str()?.to_string();
            let raw = fs::read_to_string(&path).ok()?;
            Some(CrashReport {
                recorded_at: header_value(&raw, "recorded_at").unwrap_or_default(),
                kind: header_value(&raw, "kind").unwrap_or_else(|| "Unknown".to_string()),
                content: scrub_secrets(&truncate_chars(&raw, MAX_CRASH_REPORT_BYTES)),
                filename,
            })
        })
        .collect();

    if total > reports.len() {
        tracing::info!(
            total,
            shown = reports.len(),
            "more crash reports on disk than surfaced to the UI"
        );
    }

    Ok(reports)
}

/// Marks every pending report as seen. Returns how many were acknowledged.
pub fn acknowledge_crash_reports() -> Result<usize, String> {
    let mut acknowledged = 0;
    for path in crash_report_paths()? {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let target = path.with_file_name(format!("{name}{CRASH_ACK_SUFFIX}"));
        match fs::rename(&path, &target) {
            Ok(()) => acknowledged += 1,
            Err(error) => tracing::warn!(%error, ?path, "failed to acknowledge crash report"),
        }
    }
    Ok(acknowledged)
}

fn header_value(report: &str, key: &str) -> Option<String> {
    report
        .lines()
        .take_while(|line| !line.is_empty())
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .map(|value| value.trim().to_string())
}

fn secret_rules() -> &'static [(regex::Regex, &'static str)] {
    static RULES: std::sync::OnceLock<Vec<(regex::Regex, &'static str)>> =
        std::sync::OnceLock::new();
    RULES.get_or_init(|| {
        vec![
            (
                regex::Regex::new(r"(?i)\b([a-z][a-z0-9+.\-]*://)[^\s/:@]+:[^\s/@]+@").unwrap(),
                "${1}***:***@",
            ),
            (
                regex::Regex::new(
                    r#"(?i)\b(password|passwd|pwd|token|secret|api[_-]?key|access[_-]?key|private[_-]?key)\b(\s*[:=]\s*)"?[^\s",;]+"#,
                )
                .unwrap(),
                "${1}${2}***",
            ),
            (
                regex::Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._\-]+").unwrap(),
                "Bearer ***",
            ),
        ]
    })
}

/// Strips credentials and the user's home path out of a crash report. Reports
/// are meant to be pasted into a public issue, so this runs before the content
/// ever reaches the renderer — a panic message can carry a connection URL.
pub fn scrub_secrets(text: &str) -> String {
    let mut out = text.to_string();
    for (rule, replacement) in secret_rules() {
        out = rule.replace_all(&out, *replacement).into_owned();
    }
    for var in ["HOME", "USERPROFILE"] {
        if let Ok(home) = std::env::var(var) {
            if home.len() > 3 {
                out = out.replace(&home, "~");
            }
        }
    }
    out
}

fn truncate_chars(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n…[truncated]", &text[..end])
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
    use super::{
        header_value, is_managed_log_name, is_pending_crash_name, scrub_secrets, truncate_chars,
    };

    #[test]
    fn recognizes_runtime_and_crash_logs_only() {
        assert!(is_managed_log_name("qoredb.log.2026-07-14"));
        assert!(is_managed_log_name("qoredb-crash-20260714-172500.log"));
        assert!(!is_managed_log_name("qoredb.running"));
        assert!(!is_managed_log_name("unrelated.log"));
    }

    #[test]
    fn acknowledged_reports_are_not_pending_but_stay_managed() {
        let acked = "qoredb-crash-20260714-172500.log.seen";
        assert!(is_pending_crash_name("qoredb-crash-20260714-172500.log"));
        assert!(!is_pending_crash_name(acked));
        assert!(is_managed_log_name(acked));
    }

    #[test]
    fn scrubs_credentials_from_connection_urls() {
        let scrubbed =
            scrub_secrets("PANIC: failed on postgres://admin:hunter2@db.acme.io:5432/prod");
        assert!(scrubbed.contains("postgres://***:***@db.acme.io:5432/prod"));
        assert!(!scrubbed.contains("hunter2"));
        assert!(!scrubbed.contains("admin"));
    }

    #[test]
    fn scrubs_keyed_secrets_and_bearer_tokens() {
        let scrubbed = scrub_secrets("password=s3cr3t token: abc.def Authorization: Bearer xyz123");
        assert!(!scrubbed.contains("s3cr3t"));
        assert!(!scrubbed.contains("abc.def"));
        assert!(!scrubbed.contains("xyz123"));
        assert!(scrubbed.contains("password=***"));
    }

    #[test]
    fn leaves_ordinary_crash_text_intact() {
        let text = "Location: src/engine/query.rs:42:9\nMessage: PANIC: index out of bounds";
        assert_eq!(scrub_secrets(text), text);
    }

    #[test]
    fn reads_header_fields_from_report_preamble() {
        let report = "QoreDB crash report\nrecorded_at=2026-07-26T09:12:00+02:00\nkind=Native panic\n\nBacktrace:\nkind=not-a-header\n";
        assert_eq!(
            header_value(report, "recorded_at").as_deref(),
            Some("2026-07-26T09:12:00+02:00")
        );
        assert_eq!(
            header_value(report, "kind").as_deref(),
            Some("Native panic")
        );
        assert_eq!(header_value(report, "missing"), None);
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        // 100 two-byte chars; cutting at an odd byte index must not split one.
        let text = "é".repeat(100);
        let truncated = truncate_chars(&text, 51);
        assert!(truncated.ends_with("…[truncated]"));
        assert_eq!(truncated.matches('é').count(), 25);
    }

    #[test]
    fn short_reports_are_returned_verbatim() {
        assert_eq!(truncate_chars("short", 64), "short");
    }
}

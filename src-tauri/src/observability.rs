//! Logging and observability helpers.

use std::fs;
use std::path::PathBuf;

use chrono::Local;
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::EnvFilter;

const LOG_FILE_PREFIX: &str = "qoredb.log";

pub fn init_tracing() {
    let log_dir = log_directory();
    let _ = fs::create_dir_all(&log_dir);

    let file_appender: RollingFileAppender = tracing_appender::rolling::daily(log_dir, LOG_FILE_PREFIX);
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("qoredb=info,tauri=info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(file_appender)
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .with_ansi(false)
        .with_span_events(FmtSpan::CLOSE)
        .try_init();
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
                .map(|name| name.starts_with(LOG_FILE_PREFIX))
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

    let filename = format!(
        "qoredb-logs-{}.log",
        Local::now().format("%Y%m%d-%H%M%S")
    );

    Ok(LogExport { filename, content })
}

fn log_directory() -> PathBuf {
    if cfg!(windows) {
        let appdata = std::env::var_os("APPDATA")
            .unwrap_or_else(|| std::env::var_os("USERPROFILE").unwrap_or_default());
        let mut path = PathBuf::from(appdata);
        path.push("QoreDB");
        path.push("logs");
        path
    } else {
        let home = std::env::var_os("HOME").unwrap_or_default();
        let mut path = PathBuf::from(home);
        path.push(".qoredb");
        path.push("logs");
        path
    }
}

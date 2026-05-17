// SPDX-License-Identifier: Apache-2.0

//! Log export commands.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::observability;

/// Response wrapper for log export
#[derive(Debug, Serialize)]
pub struct LogsExportResponse {
    pub success: bool,
    pub filename: Option<String>,
    pub content: Option<String>,
    pub error: Option<String>,
}

/// Exports backend logs for support.
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

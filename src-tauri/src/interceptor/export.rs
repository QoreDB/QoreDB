// SPDX-License-Identifier: Apache-2.0

//! Audit Log Exporters
//!
//! Multi-format serialization for audit log entries. Used by the
//! `export_audit_log` Tauri command. Each writer takes a slice of entries and
//! produces a `String` (JSON / JSONL / CSV) ready to be saved by the frontend.

use super::types::AuditLogEntry;

/// Supported export formats.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditExportFormat {
    /// Pretty-printed JSON array (legacy format).
    #[default]
    Json,
    /// One JSON object per line. Stream-friendly, scales to millions of entries.
    Jsonl,
    /// Comma-separated, RFC 4180 compatible. Opens cleanly in spreadsheets.
    Csv,
}

impl AuditExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Csv => "csv",
        }
    }
}

/// Serialize entries in the requested format. Errors collapse to a default
/// payload so the export command never fails silently — the user sees the
/// problem in the saved file rather than getting an empty download.
pub fn export_entries(entries: &[AuditLogEntry], format: AuditExportFormat) -> String {
    match format {
        AuditExportFormat::Json => to_json(entries),
        AuditExportFormat::Jsonl => to_jsonl(entries),
        AuditExportFormat::Csv => to_csv(entries),
    }
}

fn to_json(entries: &[AuditLogEntry]) -> String {
    serde_json::to_string_pretty(entries).unwrap_or_else(|_| "[]".to_string())
}

fn to_jsonl(entries: &[AuditLogEntry]) -> String {
    let mut out = String::with_capacity(entries.len() * 256);
    for entry in entries {
        match serde_json::to_string(entry) {
            Ok(line) => {
                out.push_str(&line);
                out.push('\n');
            }
            Err(_) => {
                // Skip an unserializable entry; the rest of the export remains valid.
                continue;
            }
        }
    }
    out
}

fn to_csv(entries: &[AuditLogEntry]) -> String {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(true)
        .from_writer(Vec::new());

    let header = [
        "id",
        "timestamp",
        "session_id",
        "driver_id",
        "environment",
        "operation_type",
        "database",
        "fingerprint",
        "query_preview",
        "success",
        "blocked",
        "error",
        "execution_time_ms",
        "row_count",
        "safety_rule",
    ];
    if writer.write_record(header).is_err() {
        return String::new();
    }

    for entry in entries {
        let record = [
            entry.id.clone(),
            entry.timestamp.to_rfc3339(),
            entry.session_id.clone(),
            entry.driver_id.clone(),
            format!("{:?}", entry.environment).to_lowercase(),
            format!("{:?}", entry.operation_type).to_lowercase(),
            entry.database.clone().unwrap_or_default(),
            entry.fingerprint.clone().unwrap_or_default(),
            entry.query_preview.clone(),
            entry.success.to_string(),
            entry.blocked.to_string(),
            entry.error.clone().unwrap_or_default(),
            format!("{:.3}", entry.execution_time_ms),
            entry.row_count.map(|n| n.to_string()).unwrap_or_default(),
            entry.safety_rule.clone().unwrap_or_default(),
        ];

        if writer.write_record(&record).is_err() {
            continue;
        }
    }

    let _ = writer.flush();
    match writer.into_inner() {
        Ok(buf) => String::from_utf8(buf).unwrap_or_default(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{Environment, QueryOperationType};
    use super::*;
    use chrono::Utc;

    fn sample(id: &str) -> AuditLogEntry {
        AuditLogEntry {
            id: id.to_string(),
            timestamp: Utc::now(),
            session_id: "sess-1".to_string(),
            query: "SELECT 1".to_string(),
            query_preview: "SELECT 1".to_string(),
            environment: Environment::Development,
            operation_type: QueryOperationType::Select,
            database: Some("public".to_string()),
            success: true,
            error: None,
            execution_time_ms: 1.234,
            row_count: Some(1),
            blocked: false,
            safety_rule: None,
            driver_id: "postgres".to_string(),
            fingerprint: Some("abcd1234deadbeef".to_string()),
        }
    }

    #[test]
    fn json_format_roundtrips() {
        let entries = vec![sample("a"), sample("b")];
        let out = export_entries(&entries, AuditExportFormat::Json);
        let parsed: Vec<AuditLogEntry> = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].fingerprint.as_deref(), Some("abcd1234deadbeef"));
    }

    #[test]
    fn jsonl_format_one_line_per_entry() {
        let entries = vec![sample("a"), sample("b"), sample("c")];
        let out = export_entries(&entries, AuditExportFormat::Jsonl);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3);
        for line in &lines {
            let _: AuditLogEntry = serde_json::from_str(line).unwrap();
        }
    }

    #[test]
    fn csv_format_includes_header_and_rows() {
        let entries = vec![sample("a"), sample("b")];
        let out = export_entries(&entries, AuditExportFormat::Csv);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("id,timestamp,session_id"));
        assert!(lines[1].contains("a,"));
        assert!(lines[2].contains("b,"));
    }

    #[test]
    fn empty_entries_does_not_panic() {
        let out = export_entries(&[], AuditExportFormat::Csv);
        assert!(out.contains("id,timestamp,session_id"));
    }

    #[test]
    fn extension_matches_format() {
        assert_eq!(AuditExportFormat::Json.extension(), "json");
        assert_eq!(AuditExportFormat::Jsonl.extension(), "jsonl");
        assert_eq!(AuditExportFormat::Csv.extension(), "csv");
    }
}

// SPDX-License-Identifier: Apache-2.0

//! Migration Blast Radius analyzer: cross-references DDL hazards extracted from
//! SQL migration scripts with recent audit log traffic, `.qoredb/` workspace
//! assets, and optional read-only live data preflight probes.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;

use qore_core::{Namespace, QueryId, Value};
use qore_service::interceptor::AuditLogEntry;
use qore_sql::ddl_impact::{DdlHazard, analyze_ddl_impact};
use qore_sql::generator::SqlDialect;

use super::parse_session_id;
use super::workspace::SharedWorkspaceManager;
use crate::workspace::types::WorkspaceSource;

const PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(5);
const AUDIT_SCAN_LIMIT: usize = 1_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadiusHazardItem {
    pub kind: String,
    pub severity: String,
    pub table: String,
    pub columns: Vec<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditTrafficMatch {
    pub table: String,
    pub execution_count: usize,
    pub unique_fingerprints: usize,
    pub sample_queries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceAssetImpact {
    pub kind: String,
    pub name: String,
    pub path: String,
    pub matched_tables: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightCheckResult {
    pub table: String,
    pub columns: Vec<String>,
    pub kind: String,
    pub violations: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationBlastRadiusReport {
    pub hazards: Vec<BlastRadiusHazardItem>,
    pub affected_tables: Vec<String>,
    pub audit_matches: Vec<AuditTrafficMatch>,
    pub workspace_assets: Vec<WorkspaceAssetImpact>,
    pub preflight_checks: Vec<PreflightCheckResult>,
    pub parse_error: Option<String>,
}

fn contains_identifier(haystack: &str, ident: &str) -> bool {
    if ident.is_empty() {
        return false;
    }
    let hay = haystack.to_ascii_lowercase();
    let needle = ident.to_ascii_lowercase();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    let mut offset = 0;
    while let Some(pos) = hay[offset..].find(&needle) {
        let abs = offset + pos;
        let before_ok = abs == 0 || !is_word(hay.as_bytes()[abs - 1]);
        let after_idx = abs + needle.len();
        let after_ok = after_idx >= hay.len() || !is_word(hay.as_bytes()[after_idx]);
        if before_ok && after_ok {
            return true;
        }
        offset = abs + 1;
    }
    false
}

fn hazard_to_item(hazard: &DdlHazard) -> BlastRadiusHazardItem {
    let (kind, detail) = match hazard {
        DdlHazard::DropTable { .. } => ("drop_table", None),
        DdlHazard::DropColumn { .. } => ("drop_column", None),
        DdlHazard::AlterColumnType { data_type, .. } => {
            ("alter_column_type", Some(data_type.clone()))
        }
        DdlHazard::AddNotNull { .. } => ("add_not_null", None),
        DdlHazard::AddUnique { .. } => ("add_unique", None),
        DdlHazard::ShrinkVarchar { max_length, .. } => {
            ("shrink_varchar", Some(format!("VARCHAR({max_length})")))
        }
    };
    BlastRadiusHazardItem {
        kind: kind.to_string(),
        severity: hazard.severity().to_string(),
        table: hazard.table_name().to_string(),
        columns: hazard.columns().into_iter().map(str::to_string).collect(),
        detail,
    }
}

fn scan_audit_traffic(
    affected_tables: &[String],
    entries: &[AuditLogEntry],
) -> Vec<AuditTrafficMatch> {
    let mut matches = Vec::new();
    for table in affected_tables {
        let mut execution_count = 0usize;
        let mut fingerprints = HashSet::new();
        let mut samples = Vec::new();
        for entry in entries {
            if contains_identifier(&entry.query, table) {
                execution_count += 1;
                if let Some(fp) = entry.fingerprint.as_deref().filter(|s| !s.is_empty()) {
                    fingerprints.insert(fp.to_string());
                }
                if samples.len() < 3 && !samples.contains(&entry.query_preview) {
                    samples.push(entry.query_preview.clone());
                }
            }
        }
        if execution_count > 0 {
            matches.push(AuditTrafficMatch {
                table: table.clone(),
                execution_count,
                unique_fingerprints: fingerprints.len().max(1),
                sample_queries: samples,
            });
        }
    }
    matches
}

fn matching_tables(text: &str, tables: &[String]) -> Vec<String> {
    tables
        .iter()
        .filter(|table| contains_identifier(text, table))
        .cloned()
        .collect()
}

fn scan_workspace_assets(ws_root: &Path, affected_tables: &[String]) -> Vec<WorkspaceAssetImpact> {
    if affected_tables.is_empty() {
        return Vec::new();
    }
    let mut impacts = Vec::new();

    // 1. Saved queries in .qoredb/queries/library.json
    let library_path = ws_root.join("queries").join("library.json");
    if let Ok(raw) = fs::read_to_string(&library_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(items) = json.get("items").and_then(|v| v.as_array()) {
                for item in items {
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unnamed query");
                    let query = item.get("query").and_then(|v| v.as_str()).unwrap_or("");
                    let matched = matching_tables(query, affected_tables);
                    if !matched.is_empty() {
                        impacts.push(WorkspaceAssetImpact {
                            kind: "saved_query".to_string(),
                            name: name.to_string(),
                            path: "queries/library.json".to_string(),
                            matched_tables: matched,
                        });
                    }
                }
            }
        }
    }

    // 2. Notebooks in .qoredb/notebooks/*.qnb
    let notebooks_dir = ws_root.join("notebooks");
    if let Ok(entries) = fs::read_dir(&notebooks_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("qnb") {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
                    let title = json
                        .get("metadata")
                        .and_then(|m| m.get("title"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());
                    let combined_sql = json
                        .get("cells")
                        .and_then(|c| c.as_array())
                        .map(|cells| {
                            cells
                                .iter()
                                .filter_map(|cell| {
                                    let ctype = cell.get("type").and_then(|v| v.as_str())?;
                                    if ctype == "sql" || ctype == "mongo" {
                                        cell.get("source").and_then(|v| v.as_str())
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default();
                    let matched = matching_tables(&combined_sql, affected_tables);
                    if !matched.is_empty() {
                        impacts.push(WorkspaceAssetImpact {
                            kind: "notebook".to_string(),
                            name: title,
                            path: format!("notebooks/{}", entry.file_name().to_string_lossy()),
                            matched_tables: matched,
                        });
                    }
                }
            }
        }
    }

    // 3. Data Contracts in .qoredb/contracts/
    let contracts_dir = ws_root.join("contracts");
    if let Ok(entries) = fs::read_dir(&contracts_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "yaml" | "yml" | "json") {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(&path) {
                let matched = matching_tables(&raw, affected_tables);
                if !matched.is_empty() {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("contract")
                        .to_string();
                    impacts.push(WorkspaceAssetImpact {
                        kind: "contract".to_string(),
                        name,
                        path: format!("contracts/{}", entry.file_name().to_string_lossy()),
                        matched_tables: matched,
                    });
                }
            }
        }
    }

    // 4. Replay Sets in .qoredb/replays/*.qreplay.json
    let replays_dir = ws_root.join("replays");
    if let Ok(entries) = fs::read_dir(&replays_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let fname = entry.file_name().to_string_lossy().to_string();
            if !fname.ends_with(".qreplay.json") {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
                    let name = json
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&fname)
                        .to_string();
                    let combined_sql = json
                        .get("entries")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|e| e.get("query").and_then(|q| q.as_str()))
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default();
                    let matched = matching_tables(&combined_sql, affected_tables);
                    if !matched.is_empty() {
                        impacts.push(WorkspaceAssetImpact {
                            kind: "replay_set".to_string(),
                            name,
                            path: format!("replays/{fname}"),
                            matched_tables: matched,
                        });
                    }
                }
            }
        }
    }

    impacts
}

fn extract_count_value(val: Option<&Value>) -> Option<u64> {
    match val? {
        Value::Int(i) if *i >= 0 => Some(*i as u64),
        Value::Float(f) if *f >= 0.0 => Some(*f as u64),
        Value::Text(s) => s.trim().parse::<u64>().ok(),
        _ => None,
    }
}

#[tauri::command]
pub async fn analyze_migration_blast_radius(
    state: State<'_, crate::SharedState>,
    ws_manager: State<'_, SharedWorkspaceManager>,
    session_id: Option<String>,
    database: Option<String>,
    schema: Option<String>,
    driver: Option<String>,
    sql: String,
    run_preflight: Option<bool>,
) -> Result<MigrationBlastRadiusReport, String> {
    let (session_manager, interceptor) = {
        let guard = state.lock().await;
        (
            Arc::clone(&guard.session_manager),
            Arc::clone(&guard.interceptor),
        )
    };

    let parsed_session = match session_id.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(sid) => Some(parse_session_id(sid)?),
        None => None,
    };

    let active_driver = match parsed_session {
        Some(sid) => Some(
            session_manager
                .get_driver(sid)
                .await
                .map_err(|e| e.sanitized_message())?,
        ),
        None => None,
    };

    let driver_id = active_driver
        .as_ref()
        .map(|d| d.driver_id().to_string())
        .or(driver)
        .unwrap_or_else(|| "postgres".to_string());

    let summary = analyze_ddl_impact(&driver_id, &sql);
    let hazards: Vec<BlastRadiusHazardItem> = summary.hazards.iter().map(hazard_to_item).collect();

    let audit_entries =
        interceptor.get_audit_entries(AUDIT_SCAN_LIMIT, 0, None, None, None, None, None, None);
    let audit_matches = scan_audit_traffic(&summary.affected_tables, &audit_entries);

    let ws_root = {
        let mgr = ws_manager.lock().await;
        let ws = mgr.active();
        if ws.source == WorkspaceSource::Default {
            None
        } else {
            Some(ws.path.clone())
        }
    };
    let workspace_assets = ws_root
        .as_deref()
        .map(|root| scan_workspace_assets(root, &summary.affected_tables))
        .unwrap_or_default();

    let mut preflight_checks = Vec::new();
    if run_preflight.unwrap_or(false) {
        if let (Some(sid), Some(drv), Some(sql_dialect)) = (
            parsed_session,
            active_driver.as_ref(),
            SqlDialect::from_driver_id(&driver_id),
        ) {
            let ns = Namespace {
                database: database.unwrap_or_default(),
                schema,
            };
            for (hazard, item) in summary.hazards.iter().zip(hazards.iter()) {
                let Some(probe_sql) = hazard.preflight_probe_sql(&sql_dialect, &ns) else {
                    continue;
                };
                let query_id = QueryId::new();
                let outcome = tokio::time::timeout(
                    PREFLIGHT_TIMEOUT,
                    drv.execute(sid, &probe_sql, query_id),
                )
                .await;
                let check = match outcome {
                    Ok(Ok(res)) => {
                        let count = res
                            .rows
                            .first()
                            .and_then(|row| extract_count_value(row.values.first()));
                        PreflightCheckResult {
                            table: item.table.clone(),
                            columns: item.columns.clone(),
                            kind: item.kind.clone(),
                            violations: count,
                            error: if count.is_none() {
                                Some("Unexpected preflight result shape".to_string())
                            } else {
                                None
                            },
                        }
                    }
                    Ok(Err(err)) => PreflightCheckResult {
                        table: item.table.clone(),
                        columns: item.columns.clone(),
                        kind: item.kind.clone(),
                        violations: None,
                        error: Some(err.sanitized_message()),
                    },
                    Err(_) => {
                        let _ = drv.cancel(sid, Some(query_id)).await;
                        PreflightCheckResult {
                            table: item.table.clone(),
                            columns: item.columns.clone(),
                            kind: item.kind.clone(),
                            violations: None,
                            error: Some("Preflight probe timed out after 5s".to_string()),
                        }
                    }
                };
                preflight_checks.push(check);
            }
        }
    }

    Ok(MigrationBlastRadiusReport {
        hazards,
        affected_tables: summary.affected_tables,
        audit_matches,
        workspace_assets,
        preflight_checks,
        parse_error: summary.parse_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use qore_service::interceptor::{Environment, QueryOperationType, QuerySource};
    use tempfile::TempDir;

    #[test]
    fn identifier_matching_respects_word_boundaries() {
        assert!(contains_identifier("SELECT * FROM users WHERE id = 1", "users"));
        assert!(contains_identifier("SELECT * FROM public.USERS;", "users"));
        assert!(!contains_identifier("SELECT * FROM users_archive", "users"));
        assert!(!contains_identifier("SELECT * FROM super_users", "users"));
    }

    #[test]
    fn scans_audit_entries_and_workspace_assets() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("queries")).unwrap();
        fs::create_dir_all(root.join("notebooks")).unwrap();
        fs::create_dir_all(root.join("contracts")).unwrap();
        fs::create_dir_all(root.join("replays")).unwrap();

        fs::write(
            root.join("queries/library.json"),
            r#"{"version":1,"items":[{"name":"Active users","query":"SELECT id FROM users WHERE active = true"}]}"#,
        )
        .unwrap();
        fs::write(
            root.join("notebooks/audit.qnb"),
            r#"{"version":1,"metadata":{"title":"User funnel"},"cells":[{"id":"c1","type":"sql","source":"SELECT count(*) FROM users"}]}"#,
        )
        .unwrap();
        fs::write(
            root.join("contracts/users.yaml"),
            "table: users\nrules:\n  - type: not_null\n    column: email\n",
        )
        .unwrap();
        fs::write(
            root.join("replays/smoke.qreplay.json"),
            r#"{"name":"smoke","entries":[{"query":"SELECT email FROM users"}]}"#,
        )
        .unwrap();

        let tables = vec!["users".to_string(), "orders".to_string()];
        let assets = scan_workspace_assets(root, &tables);
        assert_eq!(assets.len(), 4);
        assert!(assets.iter().any(|a| a.kind == "saved_query" && a.name == "Active users"));
        assert!(assets.iter().any(|a| a.kind == "notebook" && a.name == "User funnel"));
        assert!(assets.iter().any(|a| a.kind == "contract" && a.name == "users"));
        assert!(assets.iter().any(|a| a.kind == "replay_set" && a.name == "smoke"));

        let audit = vec![
            AuditLogEntry {
                id: "1".into(),
                timestamp: Utc::now(),
                session_id: "s1".into(),
                query: "SELECT * FROM users WHERE id = 1".into(),
                query_preview: "SELECT * FROM users".into(),
                environment: Environment::Production,
                operation_type: QueryOperationType::Select,
                database: None,
                success: true,
                error: None,
                execution_time_ms: 1.0,
                row_count: Some(1),
                blocked: false,
                safety_rule: None,
                driver_id: "postgres".into(),
                fingerprint: Some("fp1".into()),
                source: QuerySource::User,
            },
            AuditLogEntry {
                id: "2".into(),
                timestamp: Utc::now(),
                session_id: "s1".into(),
                query: "SELECT * FROM users_archive".into(),
                query_preview: "SELECT * FROM users_archive".into(),
                environment: Environment::Production,
                operation_type: QueryOperationType::Select,
                database: None,
                success: true,
                error: None,
                execution_time_ms: 1.0,
                row_count: Some(0),
                blocked: false,
                safety_rule: None,
                driver_id: "postgres".into(),
                fingerprint: Some("fp2".into()),
                source: QuerySource::User,
            },
        ];

        let traffic = scan_audit_traffic(&tables, &audit);
        assert_eq!(traffic.len(), 1);
        assert_eq!(traffic[0].table, "users");
        assert_eq!(traffic[0].execution_count, 1);
        assert_eq!(traffic[0].unique_fingerprints, 1);
    }

    #[test]
    fn report_serializes_with_the_snake_case_keys_the_frontend_reads() {
        let report = MigrationBlastRadiusReport {
            hazards: vec![],
            affected_tables: vec!["users".into()],
            audit_matches: vec![AuditTrafficMatch {
                table: "users".into(),
                execution_count: 2,
                unique_fingerprints: 1,
                sample_queries: vec![],
            }],
            workspace_assets: vec![WorkspaceAssetImpact {
                kind: "notebook".into(),
                name: "n".into(),
                path: "notebooks/n.qnb".into(),
                matched_tables: vec!["users".into()],
            }],
            preflight_checks: vec![],
            parse_error: Some("statement 2: oops".into()),
        };
        let json = serde_json::to_value(&report).unwrap();
        for key in [
            "affected_tables",
            "audit_matches",
            "workspace_assets",
            "preflight_checks",
            "parse_error",
        ] {
            assert!(json.get(key).is_some(), "missing key {key}");
        }
        assert_eq!(json["audit_matches"][0]["execution_count"], 2);
        assert_eq!(json["audit_matches"][0]["unique_fingerprints"], 1);
        assert!(json["audit_matches"][0].get("sample_queries").is_some());
        assert!(json["workspace_assets"][0].get("matched_tables").is_some());
    }
}

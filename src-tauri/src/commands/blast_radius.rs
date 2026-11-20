// SPDX-License-Identifier: Apache-2.0

//! Migration Blast Radius analyzer: cross-references DDL hazards extracted from
//! SQL migration scripts with recent audit log traffic, `.qoredb/` workspace
//! assets, Instant API endpoints, and optional read-only live data probes.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use qore_core::{Namespace, Value};
use qore_service::interceptor::{AuditLogEntry, QuerySource};
use qore_service::workspace::query_library;
use qore_sql::ddl_impact::{DdlHazard, analyze_ddl_impact};
use qore_sql::generator::SqlDialect;

use super::parse_session_id;
use super::workspace::SharedWorkspaceManager;

const PREFLIGHT_TIMEOUT_MS: u64 = 5_000;
const AUDIT_SCAN_LIMIT: usize = 1_000;
const INSTANT_API_STORE: &str = "instant_api/endpoints.json";

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

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn contains_identifier(haystack: &str, ident: &str) -> bool {
    if ident.is_empty() {
        return false;
    }
    let hay = haystack.to_lowercase();
    let needle = ident.to_lowercase();
    hay.match_indices(&needle).any(|(start, matched)| {
        let before_ok = hay[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(c));
        let after_ok = hay[start + matched.len()..]
            .chars()
            .next()
            .is_none_or(|c| !is_word_char(c));
        before_ok && after_ok
    })
}

fn hazard_to_item(hazard: &DdlHazard) -> BlastRadiusHazardItem {
    let (kind, detail) = match hazard {
        DdlHazard::DropTable { .. } => ("drop_table", None),
        DdlHazard::TruncateTable { .. } => ("truncate_table", None),
        DdlHazard::RenameTable { new_name, .. } => ("rename_table", Some(new_name.clone())),
        DdlHazard::DropColumn { .. } => ("drop_column", None),
        DdlHazard::RenameColumn { new_name, .. } => ("rename_column", Some(new_name.clone())),
        DdlHazard::AlterColumnType { data_type, .. } => {
            ("alter_column_type", Some(data_type.clone()))
        }
        DdlHazard::AddNotNull { .. } => ("add_not_null", None),
        DdlHazard::AddNotNullColumn { .. } => ("add_not_null_column", None),
        DdlHazard::AddUnique { .. } => ("add_unique", None),
        DdlHazard::AddForeignKey {
            referenced_table,
            referenced_columns,
            ..
        } => (
            "add_foreign_key",
            Some(format!(
                "{referenced_table}({})",
                referenced_columns.join(", ")
            )),
        ),
        DdlHazard::AddCheck { expression, .. } => ("add_check", Some(expression.clone())),
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

/// Audit entries carry no connection id, so traffic is narrowed to the
/// migration's driver and, when known, its database.
fn audit_entry_in_scope(entry: &AuditLogEntry, driver_id: &str, database: Option<&str>) -> bool {
    if !entry.driver_id.eq_ignore_ascii_case(driver_id) {
        return false;
    }
    match (database, entry.database.as_deref()) {
        (Some(expected), Some(actual)) => expected.eq_ignore_ascii_case(actual),
        _ => true,
    }
}

fn scan_audit_traffic(
    affected_tables: &[String],
    entries: &[AuditLogEntry],
    driver_id: &str,
    database: Option<&str>,
) -> Vec<AuditTrafficMatch> {
    let scoped: Vec<&AuditLogEntry> = entries
        .iter()
        .filter(|entry| audit_entry_in_scope(entry, driver_id, database))
        .collect();
    let mut matches = Vec::new();
    for table in affected_tables {
        let mut execution_count = 0usize;
        let mut fingerprints = HashSet::new();
        let mut samples = Vec::new();
        for entry in &scoped {
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

fn push_impact(
    impacts: &mut Vec<WorkspaceAssetImpact>,
    kind: &str,
    name: String,
    path: String,
    text: &str,
    tables: &[String],
) {
    let matched = matching_tables(text, tables);
    if !matched.is_empty() {
        impacts.push(WorkspaceAssetImpact {
            kind: kind.to_string(),
            name,
            path,
            matched_tables: matched,
        });
    }
}

fn notebook_sql(json: &serde_json::Value) -> String {
    json.get("cells")
        .and_then(|c| c.as_array())
        .map(|cells| {
            cells
                .iter()
                .filter(|cell| {
                    matches!(
                        cell.get("type").and_then(|v| v.as_str()),
                        Some("sql" | "mongo")
                    )
                })
                .filter_map(|cell| cell.get("source").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn scan_workspace_assets(ws_root: &Path, affected_tables: &[String]) -> Vec<WorkspaceAssetImpact> {
    if affected_tables.is_empty() {
        return Vec::new();
    }
    let mut impacts = Vec::new();

    if let Ok(library) = query_library::read(ws_root) {
        for item in &library.items {
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled query");
            let query = item.get("query").and_then(|v| v.as_str()).unwrap_or("");
            push_impact(
                &mut impacts,
                "saved_query",
                title.to_string(),
                "queries/library.json".to_string(),
                query,
                affected_tables,
            );
        }
    }

    if let Ok(entries) = fs::read_dir(ws_root.join("notebooks")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("qnb") {
                continue;
            }
            let Ok(json) = fs::read_to_string(&path)
                .map_err(|_| ())
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).map_err(|_| ()))
            else {
                continue;
            };
            let file_name = entry.file_name().to_string_lossy().to_string();
            let title = json
                .get("metadata")
                .and_then(|m| m.get("title"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| file_name.clone());
            push_impact(
                &mut impacts,
                "notebook",
                title,
                format!("notebooks/{file_name}"),
                &notebook_sql(&json),
                affected_tables,
            );
        }
    }

    if let Ok(entries) = fs::read_dir(ws_root.join("contracts")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("yaml" | "yml" | "json")
            ) {
                continue;
            }
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("contract")
                .to_string();
            push_impact(
                &mut impacts,
                "contract",
                name,
                format!("contracts/{}", entry.file_name().to_string_lossy()),
                &source,
                affected_tables,
            );
        }
    }

    if let Ok(entries) = fs::read_dir(ws_root.join("replays")) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !file_name.ends_with(".qreplay.json") {
                continue;
            }
            let Ok(json) = fs::read_to_string(entry.path())
                .map_err(|_| ())
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).map_err(|_| ()))
            else {
                continue;
            };
            let name = json
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&file_name)
                .to_string();
            let sql = json
                .get("entries")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| e.get("query").and_then(|q| q.as_str()))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            push_impact(
                &mut impacts,
                "replay_set",
                name,
                format!("replays/{file_name}"),
                &sql,
                affected_tables,
            );
        }
    }

    impacts
}

/// Instant API endpoints are global, not per workspace; only the ones bound to
/// the migration's saved connection are relevant when it is known.
fn scan_instant_api_endpoints(
    data_dir: &Path,
    connection_id: Option<&str>,
    affected_tables: &[String],
) -> Vec<WorkspaceAssetImpact> {
    if affected_tables.is_empty() {
        return Vec::new();
    }
    let Ok(raw) = fs::read_to_string(data_dir.join(INSTANT_API_STORE)) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    let mut impacts = Vec::new();
    for endpoint in json
        .get("endpoints")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        let bound_to = endpoint.get("connection_id").and_then(|v| v.as_str());
        if let (Some(expected), Some(actual)) = (connection_id, bound_to)
            && expected != actual
        {
            continue;
        }
        let name = endpoint
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("endpoint");
        let query = endpoint
            .get("query_source")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        push_impact(
            &mut impacts,
            "instant_api",
            name.to_string(),
            format!("/api/{name}"),
            query,
            affected_tables,
        );
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
#[allow(clippy::too_many_arguments)]
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
    let (session_manager, interceptor, query_manager, query_cache, rate_limiter, policy) = {
        let guard = state.lock().await;
        (
            Arc::clone(&guard.session_manager),
            Arc::clone(&guard.interceptor),
            Arc::clone(&guard.query_manager),
            Arc::clone(&guard.query_cache),
            Arc::clone(&guard.query_rate_limiter),
            guard.policy.clone(),
        )
    };

    let session_id = session_id.filter(|s| !s.trim().is_empty());
    let parsed_session = match session_id.as_deref() {
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
    let connection_id = match parsed_session {
        Some(sid) => session_manager
            .get_saved_connection_identity(sid)
            .await
            .map(|(id, _)| id),
        None => None,
    };

    let driver_id = active_driver
        .as_ref()
        .map(|d| d.driver_id().to_string())
        .or(driver)
        .unwrap_or_else(|| "postgres".to_string());
    let database = database.filter(|d| !d.trim().is_empty());

    let summary = analyze_ddl_impact(&driver_id, &sql);
    let hazards: Vec<BlastRadiusHazardItem> = summary.hazards.iter().map(hazard_to_item).collect();

    let audit_entries =
        interceptor.get_audit_entries(AUDIT_SCAN_LIMIT, 0, None, None, None, None, None, None);
    let audit_matches = scan_audit_traffic(
        &summary.affected_tables,
        &audit_entries,
        &driver_id,
        database.as_deref(),
    );

    let ws_root = ws_manager.lock().await.active().path.clone();
    let mut workspace_assets = scan_workspace_assets(&ws_root, &summary.affected_tables);
    workspace_assets.extend(scan_instant_api_endpoints(
        &qore_service::paths::app_data_dir(),
        connection_id.as_deref(),
        &summary.affected_tables,
    ));

    let mut preflight_checks = Vec::new();
    if run_preflight.unwrap_or(false)
        && let (Some(session), Some(sid), Some(sql_dialect)) = (
            parsed_session,
            session_id.as_deref(),
            SqlDialect::from_driver_id(&driver_id),
        )
    {
        let ns = Namespace {
            database: database.clone().unwrap_or_default(),
            schema: schema.filter(|s| !s.trim().is_empty()),
        };
        for (hazard, item) in summary.hazards.iter().zip(hazards.iter()) {
            let Some(probe_sql) = hazard.preflight_probe_sql(&sql_dialect, &ns) else {
                continue;
            };
            let check = |violations: Option<u64>, error: Option<String>| PreflightCheckResult {
                table: item.table.clone(),
                columns: item.columns.clone(),
                kind: item.kind.clone(),
                violations,
                error,
            };
            let preflight = match qore_service::query::preflight_with_source(
                &session_manager,
                &rate_limiter,
                &interceptor,
                &policy,
                session,
                sid,
                &probe_sql,
                Some(&ns),
                false,
                QuerySource::User,
            )
            .await
            {
                Ok(pf) => pf,
                Err(reason) => {
                    preflight_checks.push(check(None, Some(reason)));
                    continue;
                }
            };
            let query_id = query_manager.register(session).await;
            let outcome = qore_service::query::execute(
                &query_manager,
                &query_cache,
                &interceptor,
                &policy,
                preflight.driver,
                &preflight.context,
                session,
                Some(ns.clone()),
                &probe_sql,
                query_id,
                preflight.is_mutation,
                preflight.connection_key.as_deref(),
                preflight.safety_warning.as_deref(),
                Some(PREFLIGHT_TIMEOUT_MS),
                false,
                None,
                None,
                preflight.masking.clone(),
                |_, _| {},
            )
            .await;
            let result = match (outcome.success, outcome.result) {
                (true, Some(result)) => {
                    let count = result
                        .rows
                        .first()
                        .and_then(|row| extract_count_value(row.values.first()));
                    match count {
                        Some(count) => check(Some(count), None),
                        None => check(None, Some("Unexpected preflight result shape".to_string())),
                    }
                }
                (_, _) => check(
                    None,
                    Some(
                        outcome
                            .error
                            .unwrap_or_else(|| "Preflight probe failed".to_string()),
                    ),
                ),
            };
            preflight_checks.push(result);
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
        assert!(contains_identifier(
            "SELECT * FROM users WHERE id = 1",
            "users"
        ));
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
            r#"{"version":1,"folders":[],"items":[{"id":"q1","title":"Active users","query":"SELECT id FROM users WHERE active = true"}]}"#,
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
        assert!(
            assets
                .iter()
                .any(|a| a.kind == "saved_query" && a.name == "Active users")
        );
        assert!(
            assets
                .iter()
                .any(|a| a.kind == "notebook" && a.name == "User funnel")
        );
        assert!(
            assets
                .iter()
                .any(|a| a.kind == "contract" && a.name == "users")
        );
        assert!(
            assets
                .iter()
                .any(|a| a.kind == "replay_set" && a.name == "smoke")
        );

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

        let traffic = scan_audit_traffic(&tables, &audit, "postgres", None);
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

    #[test]
    fn identifier_matching_handles_non_ascii_names_without_panicking() {
        assert!(contains_identifier(
            "SELECT * FROM événements",
            "événements"
        ));
        assert!(!contains_identifier(
            "SELECT * FROM xévénements",
            "événements"
        ));
        assert!(!contains_identifier(
            "SELECT * FROM événements_old",
            "événements"
        ));
        assert!(contains_identifier(
            "SELECT * FROM ÉVÉNEMENTS",
            "événements"
        ));
    }

    fn audit_entry(driver_id: &str, database: Option<&str>, query: &str) -> AuditLogEntry {
        AuditLogEntry {
            id: query.into(),
            timestamp: Utc::now(),
            session_id: "s1".into(),
            query: query.into(),
            query_preview: query.into(),
            environment: Environment::Production,
            operation_type: QueryOperationType::Select,
            database: database.map(str::to_string),
            success: true,
            error: None,
            execution_time_ms: 1.0,
            row_count: Some(1),
            blocked: false,
            safety_rule: None,
            driver_id: driver_id.into(),
            fingerprint: None,
            source: QuerySource::User,
        }
    }

    #[test]
    fn audit_traffic_is_scoped_to_the_driver_and_database() {
        let tables = vec!["users".to_string()];
        let audit = vec![
            audit_entry("postgres", Some("app"), "SELECT * FROM users"),
            audit_entry("postgres", Some("other"), "SELECT * FROM users"),
            audit_entry("mysql", Some("app"), "SELECT * FROM users"),
            audit_entry("postgres", None, "SELECT id FROM users"),
        ];
        let traffic = scan_audit_traffic(&tables, &audit, "postgres", Some("app"));
        assert_eq!(traffic.len(), 1);
        assert_eq!(traffic[0].execution_count, 2);
    }

    #[test]
    fn instant_api_endpoints_are_filtered_by_connection() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("instant_api")).unwrap();
        fs::write(
            tmp.path().join(INSTANT_API_STORE),
            r#"{"endpoints":[
                {"id":"1","name":"users_api","connection_id":"c1","query_source":"SELECT * FROM users"},
                {"id":"2","name":"other_api","connection_id":"c2","query_source":"SELECT * FROM users"}
            ]}"#,
        )
        .unwrap();
        let tables = vec!["users".to_string()];
        let scoped = scan_instant_api_endpoints(tmp.path(), Some("c1"), &tables);
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].kind, "instant_api");
        assert_eq!(scoped[0].path, "/api/users_api");
        assert_eq!(
            scan_instant_api_endpoints(tmp.path(), None, &tables).len(),
            2
        );
    }
}

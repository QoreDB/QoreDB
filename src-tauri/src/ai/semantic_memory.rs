// SPDX-License-Identifier: BUSL-1.1

//! Semantic memory (`.qoredb/context.json` / `.qore-context.json`) for Contextual AI.
//!
//! Stores Git-friendly business annotations per table and column, domain calculation
//! rules, and implicit relations, plus lightweight schema convention detection.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::types::TableSchema;

pub const SEMANTIC_CONTEXT_SCHEMA_VERSION: u32 = 1;
pub const MAX_SEMANTIC_CONTEXT_CHARS: usize = 2_400;

fn default_version() -> u32 {
    SEMANTIC_CONTEXT_SCHEMA_VERSION
}

/// Top-level Git-shareable semantic memory file (`.qoredb/context.json` or `.qore-context.json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticContextFile {
    #[serde(default = "default_version")]
    pub version: u32,
    /// High-level business domain description or conventions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_notes: Option<String>,
    /// Business calculation rules (e.g. "Active subscription: status = 'active' AND deleted_at IS NULL").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub business_rules: Vec<String>,
    /// Per-table semantic annotations keyed by table name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tables: BTreeMap<String, TableSemanticAnnotation>,
}

impl Default for SemanticContextFile {
    fn default() -> Self {
        Self {
            version: SEMANTIC_CONTEXT_SCHEMA_VERSION,
            domain_notes: None,
            business_rules: Vec::new(),
            tables: BTreeMap::new(),
        }
    }
}

impl SemanticContextFile {
    pub fn is_empty(&self) -> bool {
        self.domain_notes
            .as_deref()
            .is_none_or(|s| s.trim().is_empty())
            && self.business_rules.is_empty()
            && self.tables.values().all(TableSemanticAnnotation::is_empty)
    }

    /// Looks up a table annotation case-insensitively (matching either `table` or `schema.table`).
    pub fn get_table(&self, table_name: &str) -> Option<&TableSemanticAnnotation> {
        if let Some(exact) = self.tables.get(table_name) {
            return Some(exact);
        }
        let lower = table_name.to_lowercase();
        self.tables
            .iter()
            .find(|(k, _)| {
                let k_lower = k.to_lowercase();
                k_lower == lower
                    || k_lower
                        .rsplit('.')
                        .next()
                        .is_some_and(|short| short == lower)
            })
            .map(|(_, v)| v)
    }
}

/// Business annotations for a single database table or collection.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TableSemanticAnnotation {
    /// Business purpose of the table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Business domain or bounded context (e.g. "Billing", "Catalog").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Business meaning of specific columns keyed by column name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub columns: BTreeMap<String, String>,
    /// Implicit foreign keys or join paths (e.g. "customer_ref -> customers.external_id").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implicit_relations: Vec<String>,
}

impl TableSemanticAnnotation {
    pub fn is_empty(&self) -> bool {
        self.description
            .as_deref()
            .is_none_or(|s| s.trim().is_empty())
            && self.domain.as_deref().is_none_or(|s| s.trim().is_empty())
            && self.columns.values().all(|v| v.trim().is_empty())
            && self
                .implicit_relations
                .iter()
                .all(|r| r.trim().is_empty())
    }
}

/// Candidate paths checked in priority order when loading semantic context for a workspace.
pub fn candidate_context_paths(workspace_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![
        workspace_dir.join("context.json"),
        workspace_dir.join("context").join("context.json"),
        workspace_dir.join(".qore-context.json"),
    ];
    if let Some(parent) = workspace_dir.parent() {
        paths.push(parent.join(".qore-context.json"));
    }
    paths
}

/// Primary path where QoreDB persists the semantic context file (`.qoredb/context.json`).
pub fn primary_context_path(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("context.json")
}

/// Loads the semantic context from the active workspace directory, returning an empty
/// `SemanticContextFile` if no file exists yet or if it cannot be parsed.
pub fn load_semantic_context(workspace_dir: &Path) -> SemanticContextFile {
    for path in candidate_context_paths(workspace_dir) {
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(parsed) = serde_json::from_str::<SemanticContextFile>(&raw) {
                return parsed;
            }
        }
    }
    SemanticContextFile::default()
}

/// Persists `ctx` atomically to `.qoredb/context.json` (and updates a sibling
/// `.qore-context.json` in the project root if one already exists).
pub fn save_semantic_context(
    workspace_dir: &Path,
    ctx: &SemanticContextFile,
) -> Result<PathBuf, String> {
    let mut cleaned = ctx.clone();
    cleaned.version = SEMANTIC_CONTEXT_SCHEMA_VERSION;
    cleaned.domain_notes = cleaned
        .domain_notes
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    cleaned.business_rules = cleaned
        .business_rules
        .into_iter()
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty())
        .collect();
    cleaned.tables.retain(|_, ann| {
        ann.description = ann
            .description
            .take()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        ann.domain = ann
            .domain
            .take()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        ann.columns.retain(|_, v| !v.trim().is_empty());
        ann.implicit_relations.retain(|r| !r.trim().is_empty());
        !ann.is_empty()
    });

    let serialized = serde_json::to_string_pretty(&cleaned)
        .map_err(|e| format!("Failed to serialize semantic context: {e}"))?;

    let primary = primary_context_path(workspace_dir);
    crate::atomic_write::write_atomic(&primary, serialized.as_bytes())
        .map_err(|e| format!("Failed to write {}: {e}", primary.display()))?;

    if let Some(parent) = workspace_dir.parent() {
        let dotfile = parent.join(".qore-context.json");
        if dotfile.is_file() {
            let _ = crate::atomic_write::write_atomic(&dotfile, serialized.as_bytes());
        }
    }

    Ok(primary)
}

/// Automatically detects project naming conventions from described table schemas.
pub fn detect_schema_conventions(
    tables: &[(String, TableSchema)],
    redact_sensitive: bool,
) -> Vec<String> {
    let mut snake_case_cols = 0usize;
    let mut camel_case_cols = 0usize;
    let mut at_timestamps = 0usize;
    let mut id_fks = 0usize;
    let mut is_booleans = 0usize;
    let mut cents_amounts = 0usize;
    let mut soft_delete_cols = 0usize;

    for (_, schema) in tables {
        for col in &schema.columns {
            if redact_sensitive && crate::redaction::is_sensitive_column(&col.name) {
                continue;
            }
            let name = col.name.as_str();
            let lower = name.to_lowercase();
            if name.contains('_') {
                snake_case_cols += 1;
            } else if name.chars().any(|c| c.is_ascii_uppercase())
                && name.chars().any(|c| c.is_ascii_lowercase())
            {
                camel_case_cols += 1;
            }
            if lower.ends_with("_at") {
                at_timestamps += 1;
            }
            if lower.ends_with("_id") && !col.is_primary_key {
                id_fks += 1;
            }
            if lower.starts_with("is_") || lower.starts_with("has_") {
                is_booleans += 1;
            }
            if lower.ends_with("_cents") {
                cents_amounts += 1;
            }
            if lower == "deleted_at" || lower == "is_deleted" {
                soft_delete_cols += 1;
            }
        }
    }

    let mut conventions = Vec::new();
    if snake_case_cols >= 3 && snake_case_cols > camel_case_cols * 2 {
        conventions.push("Columns follow snake_case naming".to_string());
    } else if camel_case_cols >= 3 && camel_case_cols > snake_case_cols * 2 {
        conventions.push("Columns follow camelCase naming".to_string());
    }
    if at_timestamps >= 2 {
        conventions.push("Timestamps use `*_at` suffixes".to_string());
    }
    if id_fks >= 2 {
        conventions.push("Foreign key columns use `*_id` suffixes".to_string());
    }
    if is_booleans >= 2 {
        conventions.push("Boolean flags use `is_*` / `has_*` prefixes".to_string());
    }
    if cents_amounts >= 1 {
        conventions.push("Monetary columns ending in `*_cents` store integer minor units".to_string());
    }
    if soft_delete_cols >= 1 {
        conventions.push(
            "Tables with `deleted_at` / `is_deleted` use soft deletion (filter `deleted_at IS NULL` for active records)"
                .to_string(),
        );
    }
    conventions
}

/// Formats table-level semantic annotations to append directly under a table's schema block.
pub fn format_table_semantic_notes(
    table_name: &str,
    ctx: &SemanticContextFile,
    redact_sensitive: bool,
) -> String {
    let Some(ann) = ctx.get_table(table_name) else {
        return String::new();
    };
    if ann.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    if let Some(domain) = ann.domain.as_deref().filter(|s| !s.trim().is_empty()) {
        writeln!(out, "  Business domain: {}", domain.trim()).unwrap();
    }
    if let Some(desc) = ann.description.as_deref().filter(|s| !s.trim().is_empty()) {
        writeln!(out, "  Business meaning: {}", desc.trim()).unwrap();
    }
    if !ann.columns.is_empty() {
        out.push_str("  Column meanings:");
        for (col, meaning) in &ann.columns {
            if meaning.trim().is_empty() {
                continue;
            }
            if redact_sensitive && crate::redaction::is_sensitive_column(col) {
                writeln!(out, "\n    <redacted>: {}", meaning.trim()).unwrap();
            } else {
                writeln!(out, "\n    {}: {}", col, meaning.trim()).unwrap();
            }
        }
    }
    if !ann.implicit_relations.is_empty() {
        out.push_str("  Implicit relations:");
        for rel in &ann.implicit_relations {
            if !rel.trim().is_empty() {
                writeln!(out, "\n    {}", rel.trim()).unwrap();
            }
        }
    }
    out
}

/// Builds a priority, bounded semantic context block for injection into the LLM system prompt.
pub fn format_semantic_block(
    ctx: &SemanticContextFile,
    conventions: &[String],
    frequent_queries: &[String],
    redact_sensitive: bool,
) -> Option<String> {
    let mut sections: Vec<String> = Vec::new();

    if let Some(notes) = ctx.domain_notes.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        sections.push(format!("Domain overview:\n{notes}"));
    }

    if !ctx.business_rules.is_empty() {
        let rules: Vec<String> = ctx
            .business_rules
            .iter()
            .map(|r| r.trim())
            .filter(|r| !r.is_empty())
            .map(|r| format!("- {r}"))
            .collect();
        if !rules.is_empty() {
            sections.push(format!("Business & calculation rules:\n{}", rules.join("\n")));
        }
    }

    // Also include a compact summary of annotated tables so the Agent and schema
    // summarizer see business meanings even before describing individual tables.
    if !ctx.tables.is_empty() {
        let mut table_lines = Vec::new();
        for (table, ann) in &ctx.tables {
            if ann.is_empty() {
                continue;
            }
            let mut parts = Vec::new();
            if let Some(domain) = ann.domain.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                parts.push(format!("[{domain}]"));
            }
            if let Some(desc) = ann
                .description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                parts.push(desc.to_string());
            }
            if !ann.implicit_relations.is_empty() {
                parts.push(format!("rels: {}", ann.implicit_relations.join(", ")));
            }
            if !ann.columns.is_empty() {
                let cols: Vec<String> = ann
                    .columns
                    .iter()
                    .filter(|(_, m)| !m.trim().is_empty())
                    .map(|(c, m)| {
                        let col_name = if redact_sensitive && crate::redaction::is_sensitive_column(c)
                        {
                            "<redacted>"
                        } else {
                            c.as_str()
                        };
                        format!("{col_name}: {}", m.trim())
                    })
                    .collect();
                if !cols.is_empty() {
                    parts.push(format!("columns ({})", cols.join("; ")));
                }
            }
            if !parts.is_empty() {
                table_lines.push(format!("- {table}: {}", parts.join(" — ")));
            }
        }
        if !table_lines.is_empty() {
            sections.push(format!(
                "Annotated tables (.qore-context.json):\n{}",
                table_lines.join("\n")
            ));
        }
    }

    if !conventions.is_empty() {
        let items: Vec<String> = conventions.iter().map(|c| format!("- {c}")).collect();
        sections.push(format!("Detected project conventions:\n{}", items.join("\n")));
    }

    if !frequent_queries.is_empty() {
        let items: Vec<String> = frequent_queries
            .iter()
            .take(5)
            .map(|q| format!("- `{}`", q.replace('\n', " ").trim()))
            .collect();
        sections.push(format!(
            "Frequent project queries (history patterns):\n{}",
            items.join("\n")
        ));
    }

    if sections.is_empty() {
        return None;
    }

    let combined = sections.join("\n\n");
    let bounded = if combined.chars().count() <= MAX_SEMANTIC_CONTEXT_CHARS {
        combined
    } else {
        let truncated: String = combined.chars().take(MAX_SEMANTIC_CONTEXT_CHARS).collect();
        format!("{truncated}…")
    };

    Some(bounded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::TableColumn;

    #[test]
    fn roundtrips_semantic_context_and_cleans_blank_entries() {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = SemanticContextFile {
            version: 1,
            domain_notes: Some("  B2B SaaS billing database  ".to_string()),
            business_rules: vec![
                "MRR excludes churned subscriptions".to_string(),
                "   ".to_string(),
            ],
            tables: BTreeMap::new(),
        };
        let mut cols = BTreeMap::new();
        cols.insert("amount_cents".to_string(), "Price in euro cents".to_string());
        cols.insert("empty_col".to_string(), "   ".to_string());
        ctx.tables.insert(
            "invoices".to_string(),
            TableSemanticAnnotation {
                description: Some("Customer invoices".to_string()),
                domain: Some("Billing".to_string()),
                columns: cols,
                implicit_relations: vec!["org_ref -> organizations.slug".to_string()],
            },
        );

        let saved_path = save_semantic_context(dir.path(), &ctx).unwrap();
        assert!(saved_path.ends_with("context.json"));

        let loaded = load_semantic_context(dir.path());
        assert_eq!(
            loaded.domain_notes.as_deref(),
            Some("B2B SaaS billing database")
        );
        assert_eq!(loaded.business_rules, vec!["MRR excludes churned subscriptions"]);
        let inv = loaded.get_table("INVOICES").unwrap();
        assert_eq!(inv.domain.as_deref(), Some("Billing"));
        assert_eq!(inv.columns.len(), 1);
        assert!(!inv.columns.contains_key("empty_col"));
    }

    #[test]
    fn loads_dot_qore_context_from_parent_project_root() {
        let project = tempfile::tempdir().unwrap();
        let qoredb_dir = project.path().join(".qoredb");
        fs::create_dir_all(&qoredb_dir).unwrap();

        let dotfile = project.path().join(".qore-context.json");
        fs::write(
            &dotfile,
            r#"{"version":1,"domain_notes":"Shared via Git","business_rules":["Net = Gross - Tax"]}"#,
        )
        .unwrap();

        let loaded = load_semantic_context(&qoredb_dir);
        assert_eq!(loaded.domain_notes.as_deref(), Some("Shared via Git"));
        assert_eq!(loaded.business_rules, vec!["Net = Gross - Tax"]);
    }

    #[test]
    fn detects_schema_conventions_and_redacts_sensitive_annotations() {
        let schema = TableSchema {
            columns: vec![
                TableColumn {
                    name: "id".to_string(),
                    data_type: "BIGINT".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                    is_auto_increment: true,
                },
                TableColumn {
                    name: "customer_id".to_string(),
                    data_type: "BIGINT".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
                TableColumn {
                    name: "org_id".to_string(),
                    data_type: "BIGINT".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
                TableColumn {
                    name: "total_cents".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
                TableColumn {
                    name: "created_at".to_string(),
                    data_type: "TIMESTAMPTZ".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
                TableColumn {
                    name: "deleted_at".to_string(),
                    data_type: "TIMESTAMPTZ".to_string(),
                    nullable: true,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            row_count_estimate: None,
            indexes: vec![],
        };

        let conventions =
            detect_schema_conventions(&[("orders".to_string(), schema)], true);
        assert!(conventions.iter().any(|c| c.contains("snake_case")));
        assert!(conventions.iter().any(|c| c.contains("*_at")));
        assert!(conventions.iter().any(|c| c.contains("*_id")));
        assert!(conventions.iter().any(|c| c.contains("*_cents")));
        assert!(conventions.iter().any(|c| c.contains("soft deletion")));

        let mut ctx = SemanticContextFile::default();
        let mut cols = BTreeMap::new();
        cols.insert("email".to_string(), "Primary login address".to_string());
        cols.insert("total_cents".to_string(), "Order total in cents".to_string());
        ctx.tables.insert(
            "orders".to_string(),
            TableSemanticAnnotation {
                description: Some("Customer orders".to_string()),
                domain: Some("Commerce".to_string()),
                columns: cols,
                implicit_relations: vec![],
            },
        );

        let block = format_semantic_block(&ctx, &conventions, &[], true).unwrap();
        assert!(block.contains("total_cents: Order total in cents"));
        assert!(!block.contains("email:"));
        assert!(block.contains("<redacted>: Primary login address"));
    }
}

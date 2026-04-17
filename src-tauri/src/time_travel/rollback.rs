// SPDX-License-Identifier: BUSL-1.1

//! Rollback SQL Generator
//!
//! Generates SQL statements to undo a set of changelog entries.
//! Inverts each operation: INSERT→DELETE, UPDATE→restore before, DELETE→INSERT.
//! Statements are generated in reverse chronological order.

use std::collections::HashMap;

use super::types::{ChangeOperation, ChangelogEntry};

/// Result of rollback SQL generation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RollbackResult {
    /// Complete SQL script (with BEGIN/COMMIT wrapper)
    pub sql: String,
    /// Number of individual statements
    pub statements_count: usize,
    /// Warnings about entries that couldn't be fully reversed
    pub warnings: Vec<String>,
}

/// Generate rollback SQL for a set of changelog entries.
///
/// Entries are processed in reverse chronological order (most recent first).
/// The SQL uses literal values (no placeholders) for manual review in the query editor.
pub fn generate_rollback_statements(
    entries: &[ChangelogEntry],
    driver_id: &str,
) -> RollbackResult {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let quoter = get_quoter(driver_id);
    let mut statements = Vec::new();
    let mut warnings = Vec::new();

    for entry in &sorted {
        let table_ref = format_table_ref(&entry.namespace.database, &entry.namespace.schema, &entry.table_name, &quoter);

        match entry.operation {
            ChangeOperation::Insert => {
                // Undo INSERT → DELETE WHERE pk = values
                let pk_clause = build_pk_where(&entry.primary_key, &quoter);
                if pk_clause.is_empty() {
                    warnings.push(format!(
                        "Entry {}: INSERT has no primary key, cannot generate DELETE",
                        &entry.id.to_string()[..8]
                    ));
                    continue;
                }
                statements.push(format!(
                    "-- Undo INSERT ({}, {})\nDELETE FROM {}\n  WHERE {};",
                    format_pk_summary(&entry.primary_key),
                    entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    table_ref,
                    pk_clause,
                ));
            }
            ChangeOperation::Update => {
                // Undo UPDATE → UPDATE SET columns = before_values WHERE pk
                match &entry.before {
                    Some(before) => {
                        let pk_clause = build_pk_where(&entry.primary_key, &quoter);
                        if pk_clause.is_empty() {
                            warnings.push(format!(
                                "Entry {}: UPDATE has no primary key, cannot generate rollback",
                                &entry.id.to_string()[..8]
                            ));
                            continue;
                        }

                        // Only restore changed columns
                        let set_columns: Vec<String> = if entry.changed_columns.is_empty() {
                            // No changed_columns recorded — restore all non-PK columns
                            before
                                .iter()
                                .filter(|(k, _)| !entry.primary_key.contains_key(*k))
                                .map(|(k, v)| format_set_clause(k, v, &quoter))
                                .collect()
                        } else {
                            entry
                                .changed_columns
                                .iter()
                                .filter_map(|col| {
                                    before.get(col).map(|v| format_set_clause(col, v, &quoter))
                                })
                                .collect()
                        };

                        if set_columns.is_empty() {
                            warnings.push(format!(
                                "Entry {}: UPDATE before-image has no restorable columns",
                                &entry.id.to_string()[..8]
                            ));
                            continue;
                        }

                        statements.push(format!(
                            "-- Undo UPDATE ({}, {})\nUPDATE {}\n  SET {}\n  WHERE {};",
                            format_pk_summary(&entry.primary_key),
                            entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                            table_ref,
                            set_columns.join(", "),
                            pk_clause,
                        ));
                    }
                    None => {
                        warnings.push(format!(
                            "Entry {}: UPDATE has no before-image (skipped)",
                            &entry.id.to_string()[..8]
                        ));
                    }
                }
            }
            ChangeOperation::Delete => {
                // Undo DELETE → INSERT INTO table (columns) VALUES (values)
                match &entry.before {
                    Some(before) => {
                        let (columns, values): (Vec<String>, Vec<String>) = before
                            .iter()
                            .filter_map(|(k, v)| {
                                format_literal(v).map(|lit| (quoter.quote_ident(k), lit))
                            })
                            .unzip();

                        if columns.is_empty() {
                            warnings.push(format!(
                                "Entry {}: DELETE before-image has no restorable columns",
                                &entry.id.to_string()[..8]
                            ));
                            continue;
                        }

                        // Check for binary columns that were skipped
                        let skipped: Vec<&String> = before
                            .iter()
                            .filter(|(_, v)| is_binary_value(v))
                            .map(|(k, _)| k)
                            .collect();
                        for col in skipped {
                            warnings.push(format!(
                                "Entry {}: Column \"{}\" contains binary data (excluded from restore)",
                                &entry.id.to_string()[..8],
                                col
                            ));
                        }

                        statements.push(format!(
                            "-- Undo DELETE ({}, {})\nINSERT INTO {} ({})\n  VALUES ({});",
                            format_pk_summary(&entry.primary_key),
                            entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                            table_ref,
                            columns.join(", "),
                            values.join(", "),
                        ));
                    }
                    None => {
                        warnings.push(format!(
                            "Entry {}: DELETE has no before-image (skipped)",
                            &entry.id.to_string()[..8]
                        ));
                    }
                }
            }
        }
    }

    let statements_count = statements.len();

    // Build the full SQL script
    let mut sql_parts = vec![
        format!("-- Rollback generated by QoreDB Time-Travel"),
        format!("-- Generated at: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
        format!("-- Statements: {}", statements_count),
    ];
    if !warnings.is_empty() {
        sql_parts.push(format!("-- Warnings: {}", warnings.len()));
    }
    sql_parts.push(String::new());
    sql_parts.push("BEGIN;".to_string());
    sql_parts.push(String::new());

    for stmt in &statements {
        sql_parts.push(stmt.clone());
        sql_parts.push(String::new());
    }

    sql_parts.push("COMMIT;".to_string());

    RollbackResult {
        sql: sql_parts.join("\n"),
        statements_count,
        warnings,
    }
}

// ─── SQL Formatting ────────────────────────────────────────────────────────

/// Identifier quoting strategy per driver.
struct Quoter {
    left: &'static str,
    right: &'static str,
}

impl Quoter {
    fn quote_ident(&self, ident: &str) -> String {
        format!("{}{}{}", self.left, ident, self.right)
    }
}

fn get_quoter(driver_id: &str) -> Quoter {
    match driver_id {
        "mysql" | "mariadb" => Quoter {
            left: "`",
            right: "`",
        },
        "sqlserver" => Quoter {
            left: "[",
            right: "]",
        },
        _ => Quoter {
            left: "\"",
            right: "\"",
        },
    }
}

fn format_table_ref(
    database: &str,
    schema: &Option<String>,
    table: &str,
    quoter: &Quoter,
) -> String {
    if let Some(schema) = schema {
        format!(
            "{}.{}",
            quoter.quote_ident(schema),
            quoter.quote_ident(table)
        )
    } else {
        // For MySQL/MariaDB, use database.table
        format!(
            "{}.{}",
            quoter.quote_ident(database),
            quoter.quote_ident(table)
        )
    }
}

fn build_pk_where(pk: &HashMap<String, serde_json::Value>, quoter: &Quoter) -> String {
    let clauses: Vec<String> = pk
        .iter()
        .filter_map(|(k, v)| {
            format_literal(v).map(|lit| format!("{} = {}", quoter.quote_ident(k), lit))
        })
        .collect();
    clauses.join(" AND ")
}

fn format_set_clause(column: &str, value: &serde_json::Value, quoter: &Quoter) -> String {
    let literal = format_literal(value).unwrap_or_else(|| "NULL".to_string());
    format!("{} = {}", quoter.quote_ident(column), literal)
}

fn format_pk_summary(pk: &HashMap<String, serde_json::Value>) -> String {
    let mut pairs: Vec<_> = pk.iter().collect();
    pairs.sort_by_key(|(k, _)| *k);
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", k, format_value_short(v)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Format a JSON value as a SQL literal.
fn format_literal(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => Some("NULL".to_string()),
        serde_json::Value::Bool(b) => Some(if *b { "TRUE" } else { "FALSE" }.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::String(s) => {
            if s.starts_with("<binary ") && s.ends_with(" bytes>") {
                None // Skip binary values
            } else {
                Some(format!("'{}'", s.replace('\'', "''")))
            }
        }
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            // Store as JSON string
            let json_str = serde_json::to_string(value).unwrap_or_default();
            Some(format!("'{}'", json_str.replace('\'', "''")))
        }
    }
}

fn format_value_short(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::String(s) => {
            if s.len() > 20 {
                format!("'{:.17}...'", s)
            } else {
                format!("'{}'", s)
            }
        }
        other => other.to_string(),
    }
}

fn is_binary_value(value: &serde_json::Value) -> bool {
    matches!(value, serde_json::Value::String(s) if s.starts_with("<binary ") && s.ends_with(" bytes>"))
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::Namespace;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_entry(
        op: ChangeOperation,
        pk: HashMap<String, serde_json::Value>,
        before: Option<HashMap<String, serde_json::Value>>,
        after: Option<HashMap<String, serde_json::Value>>,
    ) -> ChangelogEntry {
        ChangelogEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            session_id: "s1".to_string(),
            driver_id: "postgres".to_string(),
            namespace: Namespace {
                database: "mydb".to_string(),
                schema: Some("public".to_string()),
            },
            table_name: "users".to_string(),
            operation: op,
            primary_key: pk,
            before,
            after,
            changed_columns: vec!["name".to_string()],
            connection_name: None,
            environment: "development".to_string(),
        }
    }

    #[test]
    fn test_insert_generates_delete() {
        let pk = HashMap::from([("id".to_string(), serde_json::json!(42))]);
        let entries = vec![make_entry(
            ChangeOperation::Insert,
            pk,
            None,
            Some(HashMap::from([
                ("id".to_string(), serde_json::json!(42)),
                ("name".to_string(), serde_json::json!("Alice")),
            ])),
        )];

        let result = generate_rollback_statements(&entries, "postgres");
        assert_eq!(result.statements_count, 1);
        assert!(result.sql.contains("DELETE FROM"));
        assert!(result.sql.contains("\"id\" = 42"));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_update_restores_before() {
        let pk = HashMap::from([("id".to_string(), serde_json::json!(42))]);
        let before = HashMap::from([
            ("id".to_string(), serde_json::json!(42)),
            ("name".to_string(), serde_json::json!("Alice")),
        ]);
        let after = HashMap::from([
            ("id".to_string(), serde_json::json!(42)),
            ("name".to_string(), serde_json::json!("Bob")),
        ]);
        let entries = vec![make_entry(
            ChangeOperation::Update,
            pk,
            Some(before),
            Some(after),
        )];

        let result = generate_rollback_statements(&entries, "postgres");
        assert_eq!(result.statements_count, 1);
        assert!(result.sql.contains("UPDATE"));
        assert!(result.sql.contains("\"name\" = 'Alice'"));
        assert!(result.sql.contains("WHERE \"id\" = 42"));
    }

    #[test]
    fn test_delete_generates_insert() {
        let pk = HashMap::from([("id".to_string(), serde_json::json!(42))]);
        let before = HashMap::from([
            ("id".to_string(), serde_json::json!(42)),
            ("name".to_string(), serde_json::json!("Alice")),
        ]);
        let entries = vec![make_entry(ChangeOperation::Delete, pk, Some(before), None)];

        let result = generate_rollback_statements(&entries, "postgres");
        assert_eq!(result.statements_count, 1);
        assert!(result.sql.contains("INSERT INTO"));
        assert!(result.sql.contains("'Alice'"));
    }

    #[test]
    fn test_mysql_quoting() {
        let pk = HashMap::from([("id".to_string(), serde_json::json!(1))]);
        let entries = vec![make_entry(ChangeOperation::Insert, pk, None, None)];

        let result = generate_rollback_statements(&entries, "mysql");
        assert!(result.sql.contains("`id`"));
    }

    #[test]
    fn test_sqlserver_quoting() {
        let pk = HashMap::from([("id".to_string(), serde_json::json!(1))]);
        let entries = vec![make_entry(ChangeOperation::Insert, pk, None, None)];

        let result = generate_rollback_statements(&entries, "sqlserver");
        assert!(result.sql.contains("[id]"));
    }

    #[test]
    fn test_update_without_before_warns() {
        let pk = HashMap::from([("id".to_string(), serde_json::json!(42))]);
        let entries = vec![make_entry(ChangeOperation::Update, pk, None, None)];

        let result = generate_rollback_statements(&entries, "postgres");
        assert_eq!(result.statements_count, 0);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("no before-image"));
    }

    #[test]
    fn test_string_escaping() {
        let pk = HashMap::from([("id".to_string(), serde_json::json!(1))]);
        let before = HashMap::from([
            ("id".to_string(), serde_json::json!(1)),
            ("name".to_string(), serde_json::json!("O'Brien")),
        ]);
        let entries = vec![make_entry(ChangeOperation::Delete, pk, Some(before), None)];

        let result = generate_rollback_statements(&entries, "postgres");
        assert!(result.sql.contains("'O''Brien'"));
    }

    #[test]
    fn test_binary_column_warning() {
        let pk = HashMap::from([("id".to_string(), serde_json::json!(1))]);
        let before = HashMap::from([
            ("id".to_string(), serde_json::json!(1)),
            (
                "avatar".to_string(),
                serde_json::json!("<binary 1024 bytes>"),
            ),
        ]);
        let entries = vec![make_entry(ChangeOperation::Delete, pk, Some(before), None)];

        let result = generate_rollback_statements(&entries, "postgres");
        assert!(result.warnings.iter().any(|w| w.contains("binary data")));
    }

    #[test]
    fn test_reverse_chronological_order() {
        let pk1 = HashMap::from([("id".to_string(), serde_json::json!(1))]);
        let pk2 = HashMap::from([("id".to_string(), serde_json::json!(2))]);

        let mut e1 = make_entry(ChangeOperation::Insert, pk1, None, None);
        e1.timestamp = Utc::now() - chrono::Duration::seconds(10);

        let e2 = make_entry(ChangeOperation::Insert, pk2, None, None);

        let result = generate_rollback_statements(&[e1, e2], "postgres");
        assert_eq!(result.statements_count, 2);
        // e2 (most recent) should be first in the SQL
        let delete_positions: Vec<_> = result
            .sql
            .match_indices("Undo INSERT")
            .map(|(pos, _)| pos)
            .collect();
        assert_eq!(delete_positions.len(), 2);
        // id=2 should come before id=1 in the output
        let first_section = &result.sql[delete_positions[0]..delete_positions[1]];
        assert!(first_section.contains("id=2") || first_section.contains("\"id\" = 2"));
    }
}

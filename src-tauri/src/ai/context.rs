// SPDX-License-Identifier: BUSL-1.1

//! Context builder: extracts schema information and builds LLM prompts
//! adapted to the database dialect (SQL, MQL, Redis).

use std::fmt::Write;
use std::sync::Arc;

use tracing::debug;

use crate::engine::types::{
    CollectionListOptions, Namespace, SessionId, TableSchema,
};
use crate::engine::SessionManager;
use crate::virtual_relations::VirtualRelationStore;

/// The dialect determines how the system prompt is phrased
#[derive(Debug, Clone, PartialEq)]
pub enum QueryDialect {
    Sql,
    MongoMql,
    Redis,
}

/// Assembled context for an AI request
pub struct SchemaContext {
    pub system_prompt: String,
    pub schema_description: String,
    pub dialect: QueryDialect,
}

const MAX_TABLES: usize = 30;
const MAX_SCHEMA_WORDS: usize = 4000;

/// Determine the query dialect from a driver ID
pub fn dialect_for_driver(driver_id: &str) -> QueryDialect {
    match driver_id {
        "mongodb" => QueryDialect::MongoMql,
        "redis" => QueryDialect::Redis,
        _ => QueryDialect::Sql,
    }
}

/// Build the full schema context for an AI request.
///
/// Fetches table/collection list and describes each (up to MAX_TABLES),
/// prioritizing tables mentioned in the user prompt.
pub async fn build_context(
    session_manager: &Arc<SessionManager>,
    session_id: SessionId,
    namespace: &Namespace,
    driver_id: &str,
    virtual_relations: &Arc<VirtualRelationStore>,
    connection_id: Option<&str>,
    user_prompt: &str,
) -> Result<SchemaContext, String> {
    let dialect = dialect_for_driver(driver_id);
    let driver = session_manager
        .get_driver(session_id)
        .await
        .map_err(|e| e.to_string())?;

    // 1. List all collections/tables
    let options = CollectionListOptions {
        search: None,
        page: None,
        page_size: Some(200), // Fetch up to 200 table names
    };
    let collection_list = driver
        .list_collections(session_id, namespace, options)
        .await
        .map_err(|e| e.to_string())?;

    let mut table_names: Vec<String> = collection_list
        .collections
        .iter()
        .map(|c| c.name.clone())
        .collect();

    // Prioritize tables mentioned in the user prompt
    let prompt_lower = user_prompt.to_lowercase();
    table_names.sort_by(|a, b| {
        let a_mentioned = prompt_lower.contains(&a.to_lowercase());
        let b_mentioned = prompt_lower.contains(&b.to_lowercase());
        b_mentioned.cmp(&a_mentioned)
    });

    // Limit to MAX_TABLES
    table_names.truncate(MAX_TABLES);

    // 2. Describe each table
    let mut schema_parts: Vec<String> = Vec::new();
    let mut total_words = 0;

    for table_name in &table_names {
        if total_words > MAX_SCHEMA_WORDS {
            break;
        }

        match driver
            .describe_table(session_id, namespace, table_name)
            .await
        {
            Ok(schema) => {
                let desc = format_table_schema(table_name, &schema, driver_id);

                // Append virtual relations if available
                let virtual_fks = if let Some(cid) = connection_id {
                    virtual_relations.get_foreign_keys_for_table(
                        cid,
                        &namespace.database,
                        namespace.schema.as_deref(),
                        table_name,
                    )
                } else {
                    Vec::new()
                };

                let mut full_desc = desc;
                if !virtual_fks.is_empty() {
                    full_desc.push_str("  Virtual relations:");
                    for vfk in &virtual_fks {
                        write!(
                            full_desc,
                            "\n    {} -> {}.{}",
                            vfk.column, vfk.referenced_table, vfk.referenced_column
                        )
                        .unwrap();
                    }
                    full_desc.push('\n');
                }

                total_words += full_desc.split_whitespace().count();
                schema_parts.push(full_desc);
            }
            Err(e) => {
                debug!("Failed to describe table {}: {}", table_name, e);
                schema_parts.push(format!("- {} (schema unavailable)\n", table_name));
            }
        }
    }

    // If there are more tables not described, list them briefly
    if collection_list.collections.len() > table_names.len() {
        let remaining: Vec<String> = collection_list
            .collections
            .iter()
            .skip(table_names.len())
            .take(50)
            .map(|c| c.name.clone())
            .collect();
        if !remaining.is_empty() {
            schema_parts.push(format!(
                "\nOther tables (not described): {}\n",
                remaining.join(", ")
            ));
        }
    }

    let schema_description = schema_parts.join("\n");
    let system_prompt = build_system_prompt(&dialect, driver_id, namespace, &schema_description);

    Ok(SchemaContext {
        system_prompt,
        schema_description,
        dialect,
    })
}

/// Format a single table's schema into a compact text description
fn format_table_schema(table_name: &str, schema: &TableSchema, _driver_id: &str) -> String {
    let mut out = String::new();
    writeln!(out, "- {}", table_name).unwrap();

    for col in &schema.columns {
        let pk_marker = if col.is_primary_key { " PK" } else { "" };
        let null_marker = if col.nullable { " NULL" } else { " NOT NULL" };
        let default_marker = col
            .default_value
            .as_ref()
            .map(|d| format!(" DEFAULT {}", d))
            .unwrap_or_default();
        writeln!(
            out,
            "    {}: {}{}{}{}",
            col.name, col.data_type, pk_marker, null_marker, default_marker
        )
        .unwrap();
    }

    if !schema.foreign_keys.is_empty() {
        out.push_str("  Foreign keys:");
        for fk in &schema.foreign_keys {
            write!(
                out,
                "\n    {} -> {}.{}",
                fk.column, fk.referenced_table, fk.referenced_column
            )
            .unwrap();
        }
        out.push('\n');
    }

    if !schema.indexes.is_empty() {
        out.push_str("  Indexes:");
        for idx in &schema.indexes {
            let unique_marker = if idx.is_unique { " UNIQUE" } else { "" };
            write!(
                out,
                "\n    {}({}){}",
                idx.name,
                idx.columns.join(", "),
                unique_marker
            )
            .unwrap();
        }
        out.push('\n');
    }

    out
}

/// Build the system prompt adapted to the dialect
fn build_system_prompt(
    dialect: &QueryDialect,
    driver_id: &str,
    namespace: &Namespace,
    schema_description: &str,
) -> String {
    let db_context = match &namespace.schema {
        Some(schema) => format!(
            "Database: {} (schema: {}), Driver: {}",
            namespace.database, schema, driver_id
        ),
        None => format!("Database: {}, Driver: {}", namespace.database, driver_id),
    };

    match dialect {
        QueryDialect::Sql => format!(
            r#"You are an expert SQL assistant for a database client application.
{db_context}

Your role:
- Generate valid {driver_id} SQL queries based on the user's request
- Use the exact table and column names from the schema below
- Prefer explicit column names over SELECT *
- Include appropriate WHERE clauses, JOINs, and ORDER BY as needed
- For mutations (INSERT, UPDATE, DELETE), be precise and safe
- Always wrap generated SQL in a ```sql code block

Schema:
{schema_description}"#,
        ),
        QueryDialect::MongoMql => format!(
            r#"You are an expert MongoDB assistant for a database client application.
{db_context}

Your role:
- Generate valid MongoDB shell commands based on the user's request
- Use the exact collection and field names from the schema below
- Wrap generated commands in a ```json or ```js code block
- Support find, aggregate, insertOne, updateOne, deleteOne, and other common operations
- For aggregation pipelines, use proper stage syntax

Schema:
{schema_description}"#,
        ),
        QueryDialect::Redis => format!(
            r#"You are an expert Redis assistant for a database client application.
{db_context}

Your role:
- Generate valid Redis commands based on the user's request
- Use appropriate data structure commands (GET, SET, HGET, LPUSH, etc.)
- Wrap generated commands in a ``` code block

Available key patterns:
{schema_description}"#,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::{ForeignKey, TableColumn, TableIndex};

    #[test]
    fn test_dialect_for_driver() {
        assert_eq!(dialect_for_driver("postgres"), QueryDialect::Sql);
        assert_eq!(dialect_for_driver("mysql"), QueryDialect::Sql);
        assert_eq!(dialect_for_driver("sqlite"), QueryDialect::Sql);
        assert_eq!(dialect_for_driver("mongodb"), QueryDialect::MongoMql);
        assert_eq!(dialect_for_driver("redis"), QueryDialect::Redis);
    }

    #[test]
    fn test_format_table_schema() {
        let schema = TableSchema {
            columns: vec![
                TableColumn {
                    name: "id".to_string(),
                    data_type: "SERIAL".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                },
                TableColumn {
                    name: "name".to_string(),
                    data_type: "VARCHAR(255)".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                },
                TableColumn {
                    name: "email".to_string(),
                    data_type: "VARCHAR(255)".to_string(),
                    nullable: true,
                    default_value: None,
                    is_primary_key: false,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            row_count_estimate: None,
            indexes: vec![TableIndex {
                name: "idx_users_email".to_string(),
                columns: vec!["email".to_string()],
                is_unique: true,
                is_primary: false,
            }],
        };

        let result = format_table_schema("users", &schema, "postgres");
        assert!(result.contains("- users"));
        assert!(result.contains("id: SERIAL PK NOT NULL"));
        assert!(result.contains("email: VARCHAR(255) NULL"));
        assert!(result.contains("idx_users_email(email) UNIQUE"));
    }

    #[test]
    fn test_format_table_schema_with_fks() {
        let schema = TableSchema {
            columns: vec![TableColumn {
                name: "user_id".to_string(),
                data_type: "INT".to_string(),
                nullable: false,
                default_value: None,
                is_primary_key: false,
            }],
            primary_key: None,
            foreign_keys: vec![ForeignKey {
                column: "user_id".to_string(),
                referenced_table: "users".to_string(),
                referenced_column: "id".to_string(),
                referenced_schema: None,
                referenced_database: None,
                constraint_name: None,
                is_virtual: false,
            }],
            row_count_estimate: None,
            indexes: vec![],
        };

        let result = format_table_schema("orders", &schema, "postgres");
        assert!(result.contains("user_id -> users.id"));
    }
}

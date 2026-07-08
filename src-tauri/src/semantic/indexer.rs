// SPDX-License-Identifier: Apache-2.0

//! Builds the embedding corpus from schema metadata. Column names are kept
//! as-is (they are what users search for; nothing leaves localhost) — the
//! `sensitive` flag only feeds a UI badge.

use std::fmt::Write;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tracing::debug;

use crate::engine::types::{CollectionListOptions, Namespace, SessionId, TableSchema};
use crate::engine::SessionManager;

const MAX_NAMESPACES: usize = 50;
const MAX_TABLES: usize = 2000;

#[derive(Debug, Clone)]
pub struct SchemaDoc {
    pub object_id: String,
    pub kind: String,
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
    pub column: Option<String>,
    pub document: String,
    pub sensitive: bool,
}

pub fn fingerprint(model: &str, document: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(model.as_bytes());
    hasher.update(b"\n");
    hasher.update(document.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn qualified_table(namespace: &Namespace, table: &str) -> String {
    match namespace.schema.as_deref() {
        Some(schema) => format!("{}.{}.{}", namespace.database, schema, table),
        None => format!("{}.{}", namespace.database, table),
    }
}

fn object_id(namespace: &Namespace, table: &str, column: Option<&str>) -> String {
    let base = format!(
        "{}|{}|{}",
        namespace.database,
        namespace.schema.as_deref().unwrap_or(""),
        table
    );
    match column {
        Some(col) => format!("{base}|{col}"),
        None => base,
    }
}

fn render_table_doc(namespace: &Namespace, table: &str, schema: &TableSchema) -> String {
    let mut doc = format!("table {}", qualified_table(namespace, table));
    if !schema.columns.is_empty() {
        let cols: Vec<String> = schema
            .columns
            .iter()
            .map(|c| format!("{} ({})", c.name, c.data_type))
            .collect();
        write!(doc, ": columns {}", cols.join(", ")).unwrap();
    }
    if let Some(pk) = schema.primary_key.as_ref().filter(|pk| !pk.is_empty()) {
        write!(doc, "; primary key {}", pk.join(", ")).unwrap();
    }
    for fk in &schema.foreign_keys {
        write!(
            doc,
            "; references {} via {}",
            fk.referenced_table, fk.column
        )
        .unwrap();
    }
    doc
}

fn render_column_doc(
    namespace: &Namespace,
    table: &str,
    schema: &TableSchema,
    index: usize,
) -> String {
    let col = &schema.columns[index];
    let mut doc = format!(
        "column {} of table {}: type {}",
        col.name,
        qualified_table(namespace, table),
        col.data_type
    );
    doc.push_str(if col.nullable { ", nullable" } else { ", not null" });
    if col.is_primary_key {
        doc.push_str(", primary key");
    }
    if schema
        .indexes
        .iter()
        .any(|idx| idx.columns.iter().any(|c| c == &col.name))
    {
        doc.push_str(", indexed");
    }
    if let Some(fk) = schema.foreign_keys.iter().find(|fk| fk.column == col.name) {
        write!(
            doc,
            ", foreign key to {}.{}",
            fk.referenced_table, fk.referenced_column
        )
        .unwrap();
    }
    doc
}

pub fn docs_for_table(namespace: &Namespace, table: &str, schema: &TableSchema) -> Vec<SchemaDoc> {
    let mut docs = Vec::with_capacity(schema.columns.len() + 1);
    docs.push(SchemaDoc {
        object_id: object_id(namespace, table, None),
        kind: "table".to_string(),
        database: namespace.database.clone(),
        schema: namespace.schema.clone(),
        table: table.to_string(),
        column: None,
        document: render_table_doc(namespace, table, schema),
        sensitive: crate::redaction::is_sensitive_column(table),
    });
    for (i, col) in schema.columns.iter().enumerate() {
        docs.push(SchemaDoc {
            object_id: object_id(namespace, table, Some(&col.name)),
            kind: "column".to_string(),
            database: namespace.database.clone(),
            schema: namespace.schema.clone(),
            table: table.to_string(),
            column: Some(col.name.clone()),
            document: render_column_doc(namespace, table, schema, i),
            sensitive: crate::redaction::is_sensitive_column(&col.name),
        });
    }
    docs
}

pub async fn build_corpus(
    session_manager: &Arc<SessionManager>,
    session: SessionId,
) -> Result<Vec<SchemaDoc>, String> {
    let driver = session_manager
        .get_driver(session)
        .await
        .map_err(|e| e.sanitized_message())?;

    let mut namespaces = driver
        .list_namespaces(session)
        .await
        .map_err(|e| e.sanitized_message())?;
    if namespaces.len() > MAX_NAMESPACES {
        debug!(
            "Semantic index: truncating {} namespaces to {MAX_NAMESPACES}",
            namespaces.len()
        );
        namespaces.truncate(MAX_NAMESPACES);
    }

    let mut docs = Vec::new();
    let mut table_count = 0usize;
    'outer: for namespace in &namespaces {
        let options = CollectionListOptions {
            search: None,
            page: None,
            page_size: Some(500),
        };
        let collections = match driver.list_collections(session, namespace, options).await {
            Ok(list) => list.collections,
            Err(e) => {
                debug!(
                    "Semantic index: skipping namespace {}: {}",
                    namespace.database,
                    e.sanitized_message()
                );
                continue;
            }
        };
        for collection in &collections {
            if table_count >= MAX_TABLES {
                debug!("Semantic index: table cap {MAX_TABLES} reached, truncating corpus");
                break 'outer;
            }
            match driver
                .describe_table(session, namespace, &collection.name)
                .await
            {
                Ok(schema) => {
                    docs.extend(docs_for_table(namespace, &collection.name, &schema));
                    table_count += 1;
                }
                Err(e) => debug!(
                    "Semantic index: failed to describe {}: {}",
                    collection.name,
                    e.sanitized_message()
                ),
            }
        }
    }
    Ok(docs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::{ForeignKey, TableColumn, TableIndex};

    fn sample_schema() -> TableSchema {
        TableSchema {
            columns: vec![
                TableColumn {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                    is_auto_increment: true,
                },
                TableColumn {
                    name: "email".to_string(),
                    data_type: "varchar".to_string(),
                    nullable: true,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
                TableColumn {
                    name: "country_id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: true,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![ForeignKey {
                column: "country_id".to_string(),
                referenced_table: "countries".to_string(),
                referenced_column: "id".to_string(),
                referenced_schema: None,
                referenced_database: None,
                constraint_name: None,
                is_virtual: false,
            }],
            indexes: vec![TableIndex {
                name: "customers_email_idx".to_string(),
                columns: vec!["email".to_string()],
                is_unique: true,
                is_primary: false,
                index_type: None,
            }],
            row_count_estimate: None,
        }
    }

    #[test]
    fn renders_stable_documents_and_flags_sensitive() {
        let ns = Namespace {
            database: "shop".to_string(),
            schema: Some("public".to_string()),
        };
        let schema = sample_schema();
        let docs = docs_for_table(&ns, "customers", &schema);

        assert_eq!(docs.len(), 4);
        assert_eq!(docs[0].object_id, "shop|public|customers");
        assert_eq!(
            docs[0].document,
            "table shop.public.customers: columns id (integer), email (varchar), country_id (integer); primary key id; references countries via country_id"
        );
        let email = docs.iter().find(|d| d.column.as_deref() == Some("email")).unwrap();
        assert_eq!(
            email.document,
            "column email of table shop.public.customers: type varchar, nullable, indexed"
        );
        assert!(email.sensitive);
        assert!(!docs[0].sensitive);
        let fk_col = docs.iter().find(|d| d.column.as_deref() == Some("country_id")).unwrap();
        assert!(fk_col.document.ends_with("foreign key to countries.id"));

        let again = docs_for_table(&ns, "customers", &schema);
        assert_eq!(
            fingerprint("m", &docs[0].document),
            fingerprint("m", &again[0].document)
        );
        assert_ne!(
            fingerprint("model-a", &docs[0].document),
            fingerprint("model-b", &docs[0].document)
        );
    }
}

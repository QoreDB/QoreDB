// SPDX-License-Identifier: Apache-2.0

//! Schema DDL Generator
//!
//! Generates CREATE TABLE statements from TableSchema metadata,
//! using SqlDialect for driver-specific formatting.

use qore_sql::generator::SqlDialect;
use qore_core::types::{Namespace, TableSchema};

/// Generates a complete CREATE TABLE DDL statement from a TableSchema,
/// followed by CREATE INDEX statements for non-primary indexes.
pub fn generate_create_table_ddl(
    schema: &TableSchema,
    table_name: &str,
    namespace: &Namespace,
    dialect: SqlDialect,
) -> String {
    let mut output = String::new();
    let qualified = dialect.qualified_table(namespace, table_name);

    // -- Comment header
    output.push_str(&format!("-- Table: {}\n", qualified));
    output.push_str(&format!("CREATE TABLE {} (\n", qualified));

    let mut parts: Vec<String> = Vec::new();

    // Column definitions
    for col in &schema.columns {
        let mut def = format!("  {} {}", dialect.quote_ident(&col.name), col.data_type);

        if !col.nullable {
            def.push_str(" NOT NULL");
        }

        if let Some(ref default_val) = col.default_value {
            if !default_val.is_empty() {
                def.push_str(&format!(" DEFAULT {}", default_val));
            }
        }

        parts.push(def);
    }

    // PRIMARY KEY constraint (composite or multi-column)
    if let Some(ref pk_cols) = schema.primary_key {
        if !pk_cols.is_empty() {
            let pk_quoted: Vec<String> = pk_cols.iter().map(|c| dialect.quote_ident(c)).collect();
            parts.push(format!("  PRIMARY KEY ({})", pk_quoted.join(", ")));
        }
    }

    // FOREIGN KEY constraints (skip virtual relations)
    for fk in &schema.foreign_keys {
        if fk.is_virtual {
            continue;
        }

        let constraint_prefix = if let Some(ref name) = fk.constraint_name {
            format!("  CONSTRAINT {} ", dialect.quote_ident(name))
        } else {
            "  ".to_string()
        };

        let ref_table = if let Some(ref ref_schema) = fk.referenced_schema {
            format!(
                "{}.{}",
                dialect.quote_ident(ref_schema),
                dialect.quote_ident(&fk.referenced_table)
            )
        } else {
            dialect.quote_ident(&fk.referenced_table)
        };

        parts.push(format!(
            "{}FOREIGN KEY ({}) REFERENCES {} ({})",
            constraint_prefix,
            dialect.quote_ident(&fk.column),
            ref_table,
            dialect.quote_ident(&fk.referenced_column),
        ));
    }

    output.push_str(&parts.join(",\n"));
    output.push_str("\n);\n");

    // CREATE INDEX statements (non-primary)
    for idx in &schema.indexes {
        if idx.is_primary {
            continue;
        }

        let idx_cols: Vec<String> = idx.columns.iter().map(|c| dialect.quote_ident(c)).collect();
        let unique = if idx.is_unique { "UNIQUE " } else { "" };

        output.push_str(&format!(
            "CREATE {}INDEX {} ON {} ({});\n",
            unique,
            dialect.quote_ident(&idx.name),
            qualified,
            idx_cols.join(", "),
        ));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use qore_core::types::{ForeignKey, TableColumn, TableIndex};

    #[test]
    fn test_basic_create_table() {
        let schema = TableSchema {
            columns: vec![
                TableColumn {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                },
                TableColumn {
                    name: "name".to_string(),
                    data_type: "varchar(255)".to_string(),
                    nullable: true,
                    default_value: None,
                    is_primary_key: false,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            row_count_estimate: None,
            indexes: vec![],
        };

        let namespace = Namespace {
            database: "testdb".to_string(),
            schema: Some("public".to_string()),
        };

        let ddl = generate_create_table_ddl(&schema, "users", &namespace, SqlDialect::Postgres);

        assert!(ddl.contains("CREATE TABLE"));
        assert!(ddl.contains("\"id\" integer NOT NULL"));
        assert!(ddl.contains("\"name\" varchar(255)"));
        assert!(ddl.contains("PRIMARY KEY (\"id\")"));
    }

    #[test]
    fn test_with_fk_and_index() {
        let schema = TableSchema {
            columns: vec![
                TableColumn {
                    name: "id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                },
                TableColumn {
                    name: "user_id".to_string(),
                    data_type: "integer".to_string(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![ForeignKey {
                column: "user_id".to_string(),
                referenced_table: "users".to_string(),
                referenced_column: "id".to_string(),
                referenced_schema: Some("public".to_string()),
                referenced_database: None,
                constraint_name: Some("fk_user".to_string()),
                is_virtual: false,
            }],
            row_count_estimate: None,
            indexes: vec![TableIndex {
                name: "idx_user_id".to_string(),
                columns: vec!["user_id".to_string()],
                is_unique: false,
                is_primary: false,
            }],
        };

        let namespace = Namespace {
            database: "testdb".to_string(),
            schema: Some("public".to_string()),
        };

        let ddl = generate_create_table_ddl(&schema, "orders", &namespace, SqlDialect::Postgres);

        assert!(ddl.contains("CONSTRAINT \"fk_user\" FOREIGN KEY"));
        assert!(ddl.contains("REFERENCES \"public\".\"users\" (\"id\")"));
        assert!(ddl.contains("CREATE INDEX \"idx_user_id\""));
    }

    #[test]
    fn test_virtual_fk_excluded() {
        let schema = TableSchema {
            columns: vec![TableColumn {
                name: "id".to_string(),
                data_type: "integer".to_string(),
                nullable: false,
                default_value: None,
                is_primary_key: true,
            }],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![ForeignKey {
                column: "id".to_string(),
                referenced_table: "other".to_string(),
                referenced_column: "id".to_string(),
                referenced_schema: None,
                referenced_database: None,
                constraint_name: None,
                is_virtual: true,
            }],
            row_count_estimate: None,
            indexes: vec![],
        };

        let namespace = Namespace {
            database: "testdb".to_string(),
            schema: Some("public".to_string()),
        };

        let ddl = generate_create_table_ddl(&schema, "test", &namespace, SqlDialect::Postgres);

        assert!(!ddl.contains("FOREIGN KEY"));
    }
}

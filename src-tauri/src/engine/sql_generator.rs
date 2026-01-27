//! SQL Generator
//!
//! Generates driver-specific SQL statements for INSERT, UPDATE, and DELETE operations.
//! Used by the sandbox feature to create migration scripts.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::engine::types::{Namespace, RowData, Value};

/// Type of sandbox change operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxChangeType {
    Insert,
    Update,
    Delete,
}

/// DTO for a single sandbox change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxChangeDto {
    pub change_type: SandboxChangeType,
    pub namespace: Namespace,
    pub table_name: String,
    pub primary_key: Option<RowData>,
    pub old_values: Option<HashMap<String, Value>>,
    pub new_values: Option<HashMap<String, Value>>,
}

/// Generated migration script
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationScript {
    pub sql: String,
    pub statement_count: usize,
    pub warnings: Vec<String>,
}

/// SQL dialect for different database drivers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDialect {
    Postgres,
    MySql,
    Sqlite,
}

impl SqlDialect {
    pub fn from_driver_id(driver_id: &str) -> Option<Self> {
        match driver_id.to_lowercase().as_str() {
            "postgres" | "postgresql" => Some(SqlDialect::Postgres),
            "mysql" | "mariadb" => Some(SqlDialect::MySql),
            "sqlite" => Some(SqlDialect::Sqlite),
            _ => None,
        }
    }

    /// Quote an identifier according to the dialect
    pub fn quote_ident(&self, name: &str) -> String {
        match self {
            SqlDialect::Postgres | SqlDialect::Sqlite => {
                format!("\"{}\"", name.replace('"', "\"\""))
            }
            SqlDialect::MySql => {
                format!("`{}`", name.replace('`', "``"))
            }
        }
    }

    /// Format a qualified table name (schema.table or database.table)
    pub fn qualified_table(&self, namespace: &Namespace, table_name: &str) -> String {
        match self {
            SqlDialect::Postgres => {
                let schema = namespace.schema.as_deref().unwrap_or("public");
                format!("{}.{}", self.quote_ident(schema), self.quote_ident(table_name))
            }
            SqlDialect::MySql => {
                format!("{}.{}", self.quote_ident(&namespace.database), self.quote_ident(table_name))
            }
            SqlDialect::Sqlite => {
                self.quote_ident(table_name)
            }
        }
    }

    /// Format a value as a SQL literal
    pub fn format_value(&self, value: &Value) -> String {
        match value {
            Value::Null => "NULL".to_string(),
            Value::Bool(b) => {
                match self {
                    SqlDialect::Postgres => if *b { "TRUE" } else { "FALSE" }.to_string(),
                    SqlDialect::MySql => if *b { "1" } else { "0" }.to_string(),
                    SqlDialect::Sqlite => if *b { "1" } else { "0" }.to_string(),
                }
            }
            Value::Int(i) => i.to_string(),
            Value::Float(f) => {
                if f.is_nan() {
                    "'NaN'".to_string()
                } else if f.is_infinite() {
                    if *f > 0.0 { "'Infinity'" } else { "'-Infinity'" }.to_string()
                } else {
                    format!("{}", f)
                }
            }
            Value::Text(s) => self.escape_string(s),
            Value::Bytes(b) => self.format_bytes(b),
            Value::Json(j) => {
                let json_str = serde_json::to_string(j).unwrap_or_else(|_| "null".to_string());
                self.escape_string(&json_str)
            }
            Value::Array(arr) => {
                match self {
                    SqlDialect::Postgres => {
                        // PostgreSQL array literal
                        let elements: Vec<String> = arr.iter().map(|v| self.format_value(v)).collect();
                        format!("ARRAY[{}]", elements.join(", "))
                    }
                    SqlDialect::MySql | SqlDialect::Sqlite => {
                        // Store as JSON for MySQL/SQLite
                        let json = serde_json::to_string(arr).unwrap_or_else(|_| "[]".to_string());
                        self.escape_string(&json)
                    }
                }
            }
        }
    }

    /// Escape a string for SQL
    fn escape_string(&self, s: &str) -> String {
        match self {
            SqlDialect::Postgres => {
                let mut escaped = String::with_capacity(s.len());
                let mut needs_e_prefix = false;

                for ch in s.chars() {
                    match ch {
                        '\\' => escaped.push_str("\\\\"),
                        '\'' => escaped.push_str("''"),
                        '\n' => {
                            needs_e_prefix = true;
                            escaped.push_str("\\n");
                        }
                        '\r' => {
                            needs_e_prefix = true;
                            escaped.push_str("\\r");
                        }
                        '\t' => {
                            needs_e_prefix = true;
                            escaped.push_str("\\t");
                        }
                        _ => escaped.push(ch),
                    }
                }

                if needs_e_prefix {
                    format!("E'{}'", escaped)
                } else {
                    format!("'{}'", escaped)
                }
            }
            SqlDialect::MySql => {
                let escaped = s
                    .replace('\\', "\\\\")
                    .replace('\'', "''")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t")
                    .replace('\0', "\\0");
                format!("'{}'", escaped)
            }
            SqlDialect::Sqlite => {
                format!("'{}'", s.replace('\'', "''"))
            }
        }
    }

    /// Format bytes as a SQL literal
    fn format_bytes(&self, bytes: &[u8]) -> String {
        let hex_string: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
        match self {
            SqlDialect::Postgres => {
                format!("'\\x{}'", hex_string)
            }
            SqlDialect::MySql => {
                format!("X'{}'", hex_string)
            }
            SqlDialect::Sqlite => {
                format!("X'{}'", hex_string)
            }
        }
    }

    /// Get the statement terminator
    pub fn terminator(&self) -> &'static str {
        ";"
    }
}

/// Generate an INSERT statement
pub fn generate_insert(
    dialect: SqlDialect,
    namespace: &Namespace,
    table_name: &str,
    data: &HashMap<String, Value>,
) -> String {
    let table = dialect.qualified_table(namespace, table_name);

    // Use BTreeMap for deterministic column ordering
    let sorted_data: BTreeMap<&String, &Value> = data.iter().collect();

    let mut columns: Vec<String> = Vec::with_capacity(sorted_data.len());
    let mut values: Vec<String> = Vec::with_capacity(sorted_data.len());

    for (col, val) in sorted_data.iter() {
        columns.push(dialect.quote_ident(col));
        values.push(dialect.format_value(val));
    }

    format!(
        "INSERT INTO {} ({}) VALUES ({})",
        table,
        columns.join(", "),
        values.join(", ")
    )
}

/// Generate an UPDATE statement
pub fn generate_update(
    dialect: SqlDialect,
    namespace: &Namespace,
    table_name: &str,
    primary_key: &RowData,
    data: &HashMap<String, Value>,
) -> Result<String, String> {
    if primary_key.columns.is_empty() {
        return Err("Cannot generate UPDATE without primary key".to_string());
    }

    let table = dialect.qualified_table(namespace, table_name);

    // SET clause
    let set_parts: Vec<String> = data
        .iter()
        .map(|(col, val)| {
            format!("{} = {}", dialect.quote_ident(col), dialect.format_value(val))
        })
        .collect();

    // WHERE clause
    let where_parts: Vec<String> = primary_key
        .columns
        .iter()
        .map(|(col, val)| {
            format!("{} = {}", dialect.quote_ident(col), dialect.format_value(val))
        })
        .collect();

    Ok(format!(
        "UPDATE {} SET {} WHERE {}",
        table,
        set_parts.join(", "),
        where_parts.join(" AND ")
    ))
}

/// Generate a DELETE statement
pub fn generate_delete(
    dialect: SqlDialect,
    namespace: &Namespace,
    table_name: &str,
    primary_key: &RowData,
) -> Result<String, String> {
    if primary_key.columns.is_empty() {
        return Err("Cannot generate DELETE without primary key".to_string());
    }

    let table = dialect.qualified_table(namespace, table_name);

    // WHERE clause
    let where_parts: Vec<String> = primary_key
        .columns
        .iter()
        .map(|(col, val)| {
            format!("{} = {}", dialect.quote_ident(col), dialect.format_value(val))
        })
        .collect();

    Ok(format!(
        "DELETE FROM {} WHERE {}",
        table,
        where_parts.join(" AND ")
    ))
}

/// Generate a MongoDB operation string (for display purposes)
pub fn generate_mongo_operation(
    change: &SandboxChangeDto,
) -> String {
    let collection = &change.table_name;
    let db = &change.namespace.database;

    match change.change_type {
        SandboxChangeType::Insert => {
            let doc = change.new_values.as_ref()
                .map(|v| serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".to_string()))
                .unwrap_or_else(|| "{}".to_string());
            format!("db.getSiblingDB(\"{}\").{}.insertOne({})", db, collection, doc)
        }
        SandboxChangeType::Update => {
            let filter = change.primary_key.as_ref()
                .map(|pk| serde_json::to_string(&pk.columns).unwrap_or_else(|_| "{}".to_string()))
                .unwrap_or_else(|| "{}".to_string());
            let update = change.new_values.as_ref()
                .map(|v| format!("{{ $set: {} }}", serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string())))
                .unwrap_or_else(|| "{ $set: {} }".to_string());
            format!("db.getSiblingDB(\"{}\").{}.updateOne({}, {})", db, collection, filter, update)
        }
        SandboxChangeType::Delete => {
            let filter = change.primary_key.as_ref()
                .map(|pk| serde_json::to_string(&pk.columns).unwrap_or_else(|_| "{}".to_string()))
                .unwrap_or_else(|| "{}".to_string());
            format!("db.getSiblingDB(\"{}\").{}.deleteOne({})", db, collection, filter)
        }
    }
}

/// Generate a complete migration script from a list of changes
pub fn generate_migration_script(
    driver_id: &str,
    changes: &[SandboxChangeDto],
) -> MigrationScript {
    let mut statements: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Check if it's MongoDB
    if driver_id.to_lowercase() == "mongodb" {
        for change in changes {
            statements.push(generate_mongo_operation(change));
        }

        return MigrationScript {
            sql: statements.join("\n\n"),
            statement_count: statements.len(),
            warnings,
        };
    }

    // SQL dialects
    let dialect = match SqlDialect::from_driver_id(driver_id) {
        Some(d) => d,
        None => {
            warnings.push(format!("Unknown driver '{}', defaulting to PostgreSQL syntax", driver_id));
            SqlDialect::Postgres
        }
    };

    for (idx, change) in changes.iter().enumerate() {
        let stmt_result = match change.change_type {
            SandboxChangeType::Insert => {
                if let Some(ref new_values) = change.new_values {
                    Ok(generate_insert(dialect, &change.namespace, &change.table_name, new_values))
                } else {
                    Err("INSERT missing new_values".to_string())
                }
            }
            SandboxChangeType::Update => {
                if let (Some(ref pk), Some(ref new_values)) = (&change.primary_key, &change.new_values) {
                    generate_update(dialect, &change.namespace, &change.table_name, pk, new_values)
                } else {
                    Err("UPDATE missing primary_key or new_values".to_string())
                }
            }
            SandboxChangeType::Delete => {
                if let Some(ref pk) = change.primary_key {
                    generate_delete(dialect, &change.namespace, &change.table_name, pk)
                } else {
                    Err("DELETE missing primary_key".to_string())
                }
            }
        };

        match stmt_result {
            Ok(stmt) => {
                statements.push(format!("{}{}", stmt, dialect.terminator()));
            }
            Err(e) => {
                warnings.push(format!("Change {}: {}", idx + 1, e));
            }
        }
    }

    // Add transaction wrapper comment
    let header = match dialect {
        SqlDialect::Postgres => "-- PostgreSQL Migration Script\n-- Generated by QoreDB Sandbox\n\n",
        SqlDialect::MySql => "-- MySQL Migration Script\n-- Generated by QoreDB Sandbox\n\n",
        SqlDialect::Sqlite => "-- SQLite Migration Script\n-- Generated by QoreDB Sandbox\n\n",
    };

    let sql = if statements.is_empty() {
        format!("{}-- No changes to apply", header)
    } else {
        format!(
            "{}BEGIN;\n\n{}\n\nCOMMIT;",
            header,
            statements.join("\n")
        )
    };

    MigrationScript {
        sql,
        statement_count: statements.len(),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_ident_postgres() {
        let dialect = SqlDialect::Postgres;
        assert_eq!(dialect.quote_ident("users"), "\"users\"");
        assert_eq!(dialect.quote_ident("user\"name"), "\"user\"\"name\"");
    }

    #[test]
    fn test_quote_ident_mysql() {
        let dialect = SqlDialect::MySql;
        assert_eq!(dialect.quote_ident("users"), "`users`");
        assert_eq!(dialect.quote_ident("user`name"), "`user``name`");
    }

    #[test]
    fn test_format_value_string() {
        let dialect = SqlDialect::Postgres;
        assert_eq!(dialect.format_value(&Value::Text("hello".to_string())), "'hello'");
        assert_eq!(dialect.format_value(&Value::Text("it's".to_string())), "'it''s'");
    }

    #[test]
    fn test_generate_insert() {
        let dialect = SqlDialect::Postgres;
        let namespace = Namespace::with_schema("mydb", "public");
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::Text("John".to_string()));
        data.insert("age".to_string(), Value::Int(30));

        let sql = generate_insert(dialect, &namespace, "users", &data);
        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("\"public\".\"users\""));
    }

    #[test]
    fn test_generate_update() {
        let dialect = SqlDialect::Postgres;
        let namespace = Namespace::with_schema("mydb", "public");
        let mut pk = RowData::new();
        pk.columns.insert("id".to_string(), Value::Int(1));
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::Text("Jane".to_string()));

        let sql = generate_update(dialect, &namespace, "users", &pk, &data).unwrap();
        assert!(sql.contains("UPDATE"));
        assert!(sql.contains("SET"));
        assert!(sql.contains("WHERE"));
    }

    #[test]
    fn test_generate_delete() {
        let dialect = SqlDialect::Postgres;
        let namespace = Namespace::with_schema("mydb", "public");
        let mut pk = RowData::new();
        pk.columns.insert("id".to_string(), Value::Int(1));

        let sql = generate_delete(dialect, &namespace, "users", &pk).unwrap();
        assert!(sql.contains("DELETE FROM"));
        assert!(sql.contains("WHERE"));
    }
}

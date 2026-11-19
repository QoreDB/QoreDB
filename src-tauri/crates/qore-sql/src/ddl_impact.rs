// SPDX-License-Identifier: Apache-2.0

//! AST-based DDL impact analysis for Migration Blast Radius.
//!
//! Extracts destructive or data-constraining schema changes from SQL
//! migration scripts and generates read-only preflight probes to detect
//! existing rows that would violate new constraints before applying.

use serde::{Deserialize, Serialize};
use sqlparser::{
    ast::{
        AlterColumnOperation, AlterTableOperation, CharacterLength, ColumnOption, DataType, Expr,
        ObjectName, ObjectType, Statement, TableConstraint,
    },
    parser::Parser,
};

use qore_core::Namespace;

use crate::generator::SqlDialect;
use crate::migration_split::split_migration_statements;
use crate::safety::dialect_for_driver;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DdlHazard {
    DropTable {
        table: String,
    },
    DropColumn {
        table: String,
        column: String,
    },
    AlterColumnType {
        table: String,
        column: String,
        data_type: String,
    },
    AddNotNull {
        table: String,
        column: String,
    },
    AddUnique {
        table: String,
        columns: Vec<String>,
    },
    ShrinkVarchar {
        table: String,
        column: String,
        max_length: u64,
    },
}

impl DdlHazard {
    /// Table name without schema/database qualifier (used for cross-referencing
    /// audit logs and workspace files).
    pub fn table_name(&self) -> &str {
        let full = match self {
            Self::DropTable { table }
            | Self::DropColumn { table, .. }
            | Self::AlterColumnType { table, .. }
            | Self::AddNotNull { table, .. }
            | Self::AddUnique { table, .. }
            | Self::ShrinkVarchar { table, .. } => table.as_str(),
        };
        full.rsplit('.').next().unwrap_or(full)
    }

    pub fn columns(&self) -> Vec<&str> {
        match self {
            Self::DropTable { .. } => Vec::new(),
            Self::DropColumn { column, .. }
            | Self::AlterColumnType { column, .. }
            | Self::AddNotNull { column, .. }
            | Self::ShrinkVarchar { column, .. } => vec![column.as_str()],
            Self::AddUnique { columns, .. } => columns.iter().map(String::as_str).collect(),
        }
    }

    /// Severity level for UI presentation: `"high"` for data/availability loss
    /// operations (`DROP`), `"medium"` for constraint/type changes.
    pub fn severity(&self) -> &'static str {
        match self {
            Self::DropTable { .. } | Self::DropColumn { .. } => "high",
            Self::AlterColumnType { .. }
            | Self::AddNotNull { .. }
            | Self::AddUnique { .. }
            | Self::ShrinkVarchar { .. } => "medium",
        }
    }

    /// Generates a read-only `SELECT COUNT(*)` probe to count existing rows
    /// that violate the constraint before running the migration.
    pub fn preflight_probe_sql(&self, dialect: &SqlDialect, namespace: &Namespace) -> Option<String> {
        let qualified = qualified_from_name(dialect, namespace, self.table_part_tuple());
        match self {
            Self::AddNotNull { column, .. } => {
                let col = dialect.quote_ident(column);
                Some(format!(
                    "SELECT COUNT(*) AS violations FROM {qualified} WHERE {col} IS NULL"
                ))
            }
            Self::AddUnique { columns, .. } if !columns.is_empty() => {
                let quoted_cols: Vec<String> =
                    columns.iter().map(|c| dialect.quote_ident(c)).collect();
                let not_null = quoted_cols
                    .iter()
                    .map(|c| format!("{c} IS NOT NULL"))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let group_by = quoted_cols.join(", ");
                Some(format!(
                    "SELECT COUNT(*) AS violations FROM (SELECT 1 AS _v FROM {qualified} WHERE {not_null} GROUP BY {group_by} HAVING COUNT(*) > 1) AS _qore_dups"
                ))
            }
            Self::ShrinkVarchar {
                column, max_length, ..
            } => {
                let col = dialect.quote_ident(column);
                let len_expr = match dialect {
                    SqlDialect::MySql => format!("CHAR_LENGTH({col})"),
                    SqlDialect::SqlServer => format!("LEN(CAST({col} AS NVARCHAR(MAX)))"),
                    SqlDialect::Postgres
                    | SqlDialect::Sqlite
                    | SqlDialect::Snowflake
                    | SqlDialect::BigQuery => format!("LENGTH(CAST({col} AS VARCHAR))"),
                };
                Some(format!(
                    "SELECT COUNT(*) AS violations FROM {qualified} WHERE {col} IS NOT NULL AND {len_expr} > {max_length}"
                ))
            }
            _ => None,
        }
    }

    fn table_part_tuple(&self) -> &str {
        match self {
            Self::DropTable { table }
            | Self::DropColumn { table, .. }
            | Self::AlterColumnType { table, .. }
            | Self::AddNotNull { table, .. }
            | Self::AddUnique { table, .. }
            | Self::ShrinkVarchar { table, .. } => table.as_str(),
        }
    }
}

fn qualified_from_name(dialect: &SqlDialect, namespace: &Namespace, raw_table: &str) -> String {
    if let Some((schema, table)) = raw_table.rsplit_once('.') {
        if matches!(dialect, SqlDialect::Sqlite) {
            return dialect.quote_ident(table);
        }
        return format!(
            "{}.{}",
            dialect.quote_ident(schema),
            dialect.quote_ident(table)
        );
    }
    dialect.qualified_table(namespace, raw_table)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DdlImpactSummary {
    pub hazards: Vec<DdlHazard>,
    pub affected_tables: Vec<String>,
    pub parse_error: Option<String>,
}

fn object_name_str(name: &ObjectName) -> String {
    name.0
        .iter()
        .filter_map(|part| part.as_ident())
        .map(|ident| ident.value.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn char_length(dt: &DataType) -> Option<u64> {
    let opt_len = match dt {
        DataType::Varchar(len) | DataType::Nvarchar(len) => len.as_ref(),
        DataType::Char(len) | DataType::Character(len) | DataType::CharacterVarying(len) => {
            len.as_ref()
        }
        DataType::CharVarying(len) => len.as_ref(),
        _ => return None,
    };
    match opt_len {
        Some(CharacterLength::IntegerLength { length, .. }) => Some(*length),
        _ => None,
    }
}

fn inspect_type_change(table: &str, column: &str, data_type: &DataType, out: &mut Vec<DdlHazard>) {
    if let Some(max_length) = char_length(data_type) {
        out.push(DdlHazard::ShrinkVarchar {
            table: table.to_string(),
            column: column.to_string(),
            max_length,
        });
    } else {
        out.push(DdlHazard::AlterColumnType {
            table: table.to_string(),
            column: column.to_string(),
            data_type: data_type.to_string(),
        });
    }
}

/// Parses a SQL migration script and extracts schema hazards and impacted tables.
pub fn analyze_ddl_impact(driver_id: &str, sql: &str) -> DdlImpactSummary {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return DdlImpactSummary::default();
    }

    let dialect = dialect_for_driver(driver_id);
    let mut hazards = Vec::new();
    let mut parse_error = None;

    match Parser::parse_sql(&*dialect, trimmed) {
        Ok(statements) => {
            for stmt in statements {
                collect_hazards(stmt, &mut hazards);
            }
        }
        Err(whole_err) => match split_migration_statements(driver_id, trimmed) {
            Ok(pieces) => {
                let mut failures = Vec::new();
                for piece in pieces {
                    match Parser::parse_sql(&*dialect, piece.text) {
                        Ok(statements) => {
                            for stmt in statements {
                                collect_hazards(stmt, &mut hazards);
                            }
                        }
                        Err(err) => failures.push(format!("statement {}: {err}", piece.index)),
                    }
                }
                if !failures.is_empty() {
                    parse_error = Some(failures.join("; "));
                }
            }
            Err(_) => parse_error = Some(whole_err.to_string()),
        },
    }

    let mut affected_tables: Vec<String> = Vec::new();
    for hazard in &hazards {
        let name = hazard.table_name().to_string();
        if !affected_tables.contains(&name) {
            affected_tables.push(name);
        }
    }

    DdlImpactSummary {
        hazards,
        affected_tables,
        parse_error,
    }
}

fn collect_hazards(stmt: Statement, hazards: &mut Vec<DdlHazard>) {
    match stmt {
        Statement::Drop {
            object_type: ObjectType::Table,
            names,
            ..
        } => {
            for name in names {
                let table = object_name_str(&name);
                if !table.is_empty() {
                    hazards.push(DdlHazard::DropTable { table });
                }
            }
        }
        Statement::AlterTable(alter) => {
            let table = object_name_str(&alter.name);
            if table.is_empty() {
                return;
            }
            for op in alter.operations {
                match op {
                    AlterTableOperation::DropColumn { column_names, .. } => {
                        for ident in column_names {
                            hazards.push(DdlHazard::DropColumn {
                                table: table.clone(),
                                column: ident.value,
                            });
                        }
                    }
                    AlterTableOperation::AlterColumn { column_name, op } => match op {
                        AlterColumnOperation::SetNotNull => {
                            hazards.push(DdlHazard::AddNotNull {
                                table: table.clone(),
                                column: column_name.value,
                            });
                        }
                        AlterColumnOperation::SetDataType { data_type, .. } => {
                            inspect_type_change(
                                &table,
                                &column_name.value,
                                &data_type,
                                hazards,
                            );
                        }
                        _ => {}
                    },
                    AlterTableOperation::ModifyColumn {
                        col_name,
                        data_type,
                        options,
                        ..
                    } => {
                        inspect_type_change(&table, &col_name.value, &data_type, hazards);
                        if options.iter().any(|o| matches!(o, ColumnOption::NotNull)) {
                            hazards.push(DdlHazard::AddNotNull {
                                table: table.clone(),
                                column: col_name.value,
                            });
                        }
                    }
                    AlterTableOperation::ChangeColumn {
                        old_name,
                        data_type,
                        options,
                        ..
                    } => {
                        inspect_type_change(&table, &old_name.value, &data_type, hazards);
                        if options.iter().any(|o| matches!(o, ColumnOption::NotNull)) {
                            hazards.push(DdlHazard::AddNotNull {
                                table: table.clone(),
                                column: old_name.value,
                            });
                        }
                    }
                    AlterTableOperation::AddConstraint {
                        constraint: TableConstraint::Unique(uc),
                        ..
                    } => {
                        let columns: Vec<String> = uc
                            .columns
                            .iter()
                            .filter_map(|ic| match &ic.column.expr {
                                Expr::Identifier(id) => Some(id.value.clone()),
                                _ => None,
                            })
                            .collect();
                        if !columns.is_empty() {
                            hazards.push(DdlHazard::AddUnique {
                                table: table.clone(),
                                columns,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        Statement::CreateIndex(idx) if idx.unique => {
            let table = object_name_str(&idx.table_name);
            let columns: Vec<String> = idx
                .columns
                .iter()
                .filter_map(|ic| match &ic.column.expr {
                    Expr::Identifier(id) => Some(id.value.clone()),
                    _ => None,
                })
                .collect();
            if !table.is_empty() && !columns.is_empty() {
                hazards.push(DdlHazard::AddUnique { table, columns });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_drop_alter_not_null_unique_and_varchar_shrink() {
        let sql = r#"
            ALTER TABLE public.users DROP COLUMN legacy_role;
            ALTER TABLE users ALTER COLUMN email SET NOT NULL;
            ALTER TABLE users ALTER COLUMN code TYPE VARCHAR(16);
            ALTER TABLE users ALTER COLUMN age TYPE BIGINT;
            CREATE UNIQUE INDEX idx_users_email ON users (email);
            DROP TABLE old_sessions;
        "#;

        let summary = analyze_ddl_impact("postgres", sql);
        assert!(summary.parse_error.is_none());
        assert_eq!(summary.affected_tables, vec!["users", "old_sessions"]);
        assert_eq!(
            summary.hazards,
            vec![
                DdlHazard::DropColumn {
                    table: "public.users".to_string(),
                    column: "legacy_role".to_string(),
                },
                DdlHazard::AddNotNull {
                    table: "users".to_string(),
                    column: "email".to_string(),
                },
                DdlHazard::ShrinkVarchar {
                    table: "users".to_string(),
                    column: "code".to_string(),
                    max_length: 16,
                },
                DdlHazard::AlterColumnType {
                    table: "users".to_string(),
                    column: "age".to_string(),
                    data_type: "BIGINT".to_string(),
                },
                DdlHazard::AddUnique {
                    table: "users".to_string(),
                    columns: vec!["email".to_string()],
                },
                DdlHazard::DropTable {
                    table: "old_sessions".to_string(),
                },
            ]
        );
    }

    #[test]
    fn builds_dialect_aware_preflight_probes() {
        let ns = Namespace {
            database: "app".to_string(),
            schema: Some("public".to_string()),
        };

        let not_null = DdlHazard::AddNotNull {
            table: "users".to_string(),
            column: "email".to_string(),
        };
        assert_eq!(
            not_null
                .preflight_probe_sql(&SqlDialect::Postgres, &ns)
                .unwrap(),
            "SELECT COUNT(*) AS violations FROM \"public\".\"users\" WHERE \"email\" IS NULL"
        );

        let shrink = DdlHazard::ShrinkVarchar {
            table: "users".to_string(),
            column: "code".to_string(),
            max_length: 12,
        };
        assert_eq!(
            shrink
                .preflight_probe_sql(&SqlDialect::MySql, &ns)
                .unwrap(),
            "SELECT COUNT(*) AS violations FROM `app`.`users` WHERE `code` IS NOT NULL AND CHAR_LENGTH(`code`) > 12"
        );

        let unique = DdlHazard::AddUnique {
            table: "users".to_string(),
            columns: vec!["email".to_string()],
        };
        assert_eq!(
            unique
                .preflight_probe_sql(&SqlDialect::Sqlite, &ns)
                .unwrap(),
            "SELECT COUNT(*) AS violations FROM (SELECT 1 AS _v FROM \"users\" WHERE \"email\" IS NOT NULL GROUP BY \"email\" HAVING COUNT(*) > 1) AS _qore_dups"
        );
    }

    #[test]
    fn keeps_hazards_from_parseable_statements_when_one_statement_fails() {
        let sql = "ALTER TABLE users DROP COLUMN legacy;\nTHIS IS NOT SQL;\nDROP TABLE old_sessions;";
        let summary = analyze_ddl_impact("postgres", sql);
        let error = summary.parse_error.expect("unparseable statement must be reported");
        assert!(error.starts_with("statement 2:"), "{error}");
        assert_eq!(
            summary.hazards,
            vec![
                DdlHazard::DropColumn {
                    table: "users".to_string(),
                    column: "legacy".to_string(),
                },
                DdlHazard::DropTable {
                    table: "old_sessions".to_string(),
                },
            ]
        );
    }
}

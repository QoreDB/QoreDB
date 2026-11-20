// SPDX-License-Identifier: Apache-2.0

//! AST-based DDL impact analysis for Migration Blast Radius.
//!
//! Extracts destructive or data-constraining schema changes from SQL
//! migration scripts and generates read-only preflight probes to detect
//! existing rows that would violate new constraints before applying.

use serde::{Deserialize, Serialize};
use sqlparser::{
    ast::{
        AlterColumnOperation, AlterTableOperation, CharacterLength, ColumnDef, ColumnOption,
        DataType, Expr, Ident, ObjectName, ObjectType, RenameTableNameKind, Statement,
        TableConstraint,
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
    TruncateTable {
        table: String,
    },
    RenameTable {
        table: String,
        new_name: String,
    },
    DropColumn {
        table: String,
        column: String,
    },
    RenameColumn {
        table: String,
        column: String,
        new_name: String,
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
    AddNotNullColumn {
        table: String,
        column: String,
    },
    AddUnique {
        table: String,
        columns: Vec<String>,
    },
    AddForeignKey {
        table: String,
        columns: Vec<String>,
        referenced_table: String,
        referenced_columns: Vec<String>,
    },
    AddCheck {
        table: String,
        expression: String,
    },
    /// The target length is known, the current one is not: the preflight
    /// probe decides whether existing values would be truncated.
    ShrinkVarchar {
        table: String,
        column: String,
        max_length: u64,
    },
}

impl DdlHazard {
    fn table(&self) -> &str {
        match self {
            Self::DropTable { table }
            | Self::TruncateTable { table }
            | Self::RenameTable { table, .. }
            | Self::DropColumn { table, .. }
            | Self::RenameColumn { table, .. }
            | Self::AlterColumnType { table, .. }
            | Self::AddNotNull { table, .. }
            | Self::AddNotNullColumn { table, .. }
            | Self::AddUnique { table, .. }
            | Self::AddForeignKey { table, .. }
            | Self::AddCheck { table, .. }
            | Self::ShrinkVarchar { table, .. } => table.as_str(),
        }
    }

    /// Table name without schema/database qualifier (used for cross-referencing
    /// audit logs and workspace files).
    pub fn table_name(&self) -> &str {
        let full = self.table();
        full.rsplit('.').next().unwrap_or(full)
    }

    pub fn columns(&self) -> Vec<&str> {
        match self {
            Self::DropTable { .. }
            | Self::TruncateTable { .. }
            | Self::RenameTable { .. }
            | Self::AddCheck { .. } => Vec::new(),
            Self::DropColumn { column, .. }
            | Self::RenameColumn { column, .. }
            | Self::AlterColumnType { column, .. }
            | Self::AddNotNull { column, .. }
            | Self::AddNotNullColumn { column, .. }
            | Self::ShrinkVarchar { column, .. } => vec![column.as_str()],
            Self::AddUnique { columns, .. } | Self::AddForeignKey { columns, .. } => {
                columns.iter().map(String::as_str).collect()
            }
        }
    }

    /// `"high"` when existing data or every query on the object breaks;
    /// `"medium"` when the change only fails on rows that violate it.
    pub fn severity(&self) -> &'static str {
        match self {
            Self::DropTable { .. }
            | Self::TruncateTable { .. }
            | Self::RenameTable { .. }
            | Self::DropColumn { .. }
            | Self::RenameColumn { .. } => "high",
            Self::AlterColumnType { .. }
            | Self::AddNotNull { .. }
            | Self::AddNotNullColumn { .. }
            | Self::AddUnique { .. }
            | Self::AddForeignKey { .. }
            | Self::AddCheck { .. }
            | Self::ShrinkVarchar { .. } => "medium",
        }
    }

    /// Generates a read-only `SELECT COUNT(*)` probe to count existing rows
    /// that violate the constraint before running the migration.
    pub fn preflight_probe_sql(
        &self,
        dialect: &SqlDialect,
        namespace: &Namespace,
    ) -> Option<String> {
        let qualified = qualified_from_name(dialect, namespace, self.table());
        match self {
            Self::AddNotNull { column, .. } => {
                let col = dialect.quote_ident(column);
                Some(format!(
                    "SELECT COUNT(*) AS violations FROM {qualified} WHERE {col} IS NULL"
                ))
            }
            Self::AddNotNullColumn { .. } => {
                Some(format!("SELECT COUNT(*) AS violations FROM {qualified}"))
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
            Self::AddForeignKey {
                columns,
                referenced_table,
                referenced_columns,
                ..
            } if !columns.is_empty() && columns.len() == referenced_columns.len() => {
                let parent = qualified_from_name(dialect, namespace, referenced_table);
                let not_null = columns
                    .iter()
                    .map(|c| format!("_qore_c.{} IS NOT NULL", dialect.quote_ident(c)))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let join = columns
                    .iter()
                    .zip(referenced_columns)
                    .map(|(c, r)| {
                        format!(
                            "_qore_p.{} = _qore_c.{}",
                            dialect.quote_ident(r),
                            dialect.quote_ident(c)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" AND ");
                Some(format!(
                    "SELECT COUNT(*) AS violations FROM {qualified} AS _qore_c WHERE {not_null} AND NOT EXISTS (SELECT 1 FROM {parent} AS _qore_p WHERE {join})"
                ))
            }
            Self::AddCheck { expression, .. } => Some(format!(
                "SELECT COUNT(*) AS violations FROM {qualified} WHERE NOT ({expression})"
            )),
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
}

fn qualified_from_name(dialect: &SqlDialect, namespace: &Namespace, raw_table: &str) -> String {
    let parts: Vec<&str> = raw_table.split('.').collect();
    if parts.len() > 1 {
        if matches!(dialect, SqlDialect::Sqlite) {
            return dialect.quote_ident(parts[parts.len() - 1]);
        }
        return parts
            .iter()
            .map(|part| dialect.quote_ident(part))
            .collect::<Vec<_>>()
            .join(".");
    }
    // An unqualified name resolves like the migration itself does: against the
    // session's search_path / current database, unless a namespace is explicit.
    let explicit = match dialect {
        SqlDialect::MySql => !namespace.database.is_empty(),
        SqlDialect::Postgres | SqlDialect::SqlServer => namespace.schema.is_some(),
        SqlDialect::Sqlite | SqlDialect::Snowflake | SqlDialect::BigQuery => false,
    };
    if explicit {
        dialect.qualified_table(namespace, raw_table)
    } else {
        dialect.quote_ident(raw_table)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DdlImpactSummary {
    pub hazards: Vec<DdlHazard>,
    pub affected_tables: Vec<String>,
    pub parse_error: Option<String>,
}

/// How the dialect stores unquoted identifiers in its catalog. Hazards carry
/// catalog names so the quoted preflight probes target the real object.
#[derive(Clone, Copy)]
enum IdentFold {
    Lower,
    Upper,
    Preserve,
}

impl IdentFold {
    fn for_driver(driver_id: &str) -> Self {
        match SqlDialect::from_driver_id(driver_id) {
            Some(SqlDialect::Postgres) => Self::Lower,
            Some(SqlDialect::Snowflake) => Self::Upper,
            _ => Self::Preserve,
        }
    }

    fn name(self, ident: &Ident) -> String {
        if ident.quote_style.is_some() {
            return ident.value.clone();
        }
        match self {
            Self::Lower => ident.value.to_lowercase(),
            Self::Upper => ident.value.to_uppercase(),
            Self::Preserve => ident.value.clone(),
        }
    }

    fn object(self, name: &ObjectName) -> String {
        name.0
            .iter()
            .filter_map(|part| part.as_ident())
            .map(|ident| self.name(ident))
            .collect::<Vec<_>>()
            .join(".")
    }

    fn column_expr(self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Identifier(id) => Some(self.name(id)),
            _ => None,
        }
    }
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

/// A new `NOT NULL` column without a default fails (or is silently
/// back-filled, on MySQL) as soon as the table has rows.
fn is_not_null_without_default(column: &ColumnDef) -> bool {
    let options = || column.options.iter().map(|o| &o.option);
    options().any(|o| matches!(o, ColumnOption::NotNull))
        && !options().any(|o| {
            matches!(
                o,
                ColumnOption::Default(_)
                    | ColumnOption::Generated { .. }
                    | ColumnOption::Identity(_)
                    | ColumnOption::Materialized(_)
                    | ColumnOption::Alias(_)
                    | ColumnOption::Ephemeral(_)
            )
        })
}

fn push_constraint(
    table: &str,
    constraint: &TableConstraint,
    fold: IdentFold,
    hazards: &mut Vec<DdlHazard>,
) {
    match constraint {
        TableConstraint::Unique(uc) => {
            let columns: Vec<String> = uc
                .columns
                .iter()
                .filter_map(|ic| fold.column_expr(&ic.column.expr))
                .collect();
            if !columns.is_empty() {
                hazards.push(DdlHazard::AddUnique {
                    table: table.to_string(),
                    columns,
                });
            }
        }
        TableConstraint::ForeignKey(fk) => {
            hazards.push(DdlHazard::AddForeignKey {
                table: table.to_string(),
                columns: fk.columns.iter().map(|c| fold.name(c)).collect(),
                referenced_table: fold.object(&fk.foreign_table),
                referenced_columns: fk.referred_columns.iter().map(|c| fold.name(c)).collect(),
            });
        }
        TableConstraint::Check(check) => {
            hazards.push(DdlHazard::AddCheck {
                table: table.to_string(),
                expression: check.expr.to_string(),
            });
        }
        _ => {}
    }
}

/// Parses a SQL migration script and extracts schema hazards and impacted tables.
pub fn analyze_ddl_impact(driver_id: &str, sql: &str) -> DdlImpactSummary {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return DdlImpactSummary::default();
    }

    let dialect = dialect_for_driver(driver_id);
    let fold = IdentFold::for_driver(driver_id);
    let mut hazards = Vec::new();
    let mut parse_error = None;

    match Parser::parse_sql(&*dialect, trimmed) {
        Ok(statements) => {
            for stmt in statements {
                collect_hazards(stmt, fold, &mut hazards);
            }
        }
        Err(whole_err) => match split_migration_statements(driver_id, trimmed) {
            Ok(pieces) => {
                let mut failures = Vec::new();
                for piece in pieces {
                    match Parser::parse_sql(&*dialect, piece.text) {
                        Ok(statements) => {
                            for stmt in statements {
                                collect_hazards(stmt, fold, &mut hazards);
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

fn collect_hazards(stmt: Statement, fold: IdentFold, hazards: &mut Vec<DdlHazard>) {
    match stmt {
        Statement::Drop {
            object_type: ObjectType::Table,
            names,
            ..
        } => {
            for name in names {
                let table = fold.object(&name);
                if !table.is_empty() {
                    hazards.push(DdlHazard::DropTable { table });
                }
            }
        }
        Statement::Truncate(truncate) => {
            for target in truncate.table_names {
                let table = fold.object(&target.name);
                if !table.is_empty() {
                    hazards.push(DdlHazard::TruncateTable { table });
                }
            }
        }
        Statement::RenameTable(renames) => {
            for rename in renames {
                hazards.push(DdlHazard::RenameTable {
                    table: fold.object(&rename.old_name),
                    new_name: fold.object(&rename.new_name),
                });
            }
        }
        Statement::AlterTable(alter) => {
            let table = fold.object(&alter.name);
            if table.is_empty() {
                return;
            }
            for op in alter.operations {
                match op {
                    AlterTableOperation::DropColumn { column_names, .. } => {
                        for ident in column_names {
                            hazards.push(DdlHazard::DropColumn {
                                table: table.clone(),
                                column: fold.name(&ident),
                            });
                        }
                    }
                    AlterTableOperation::RenameColumn {
                        old_column_name,
                        new_column_name,
                    } => hazards.push(DdlHazard::RenameColumn {
                        table: table.clone(),
                        column: fold.name(&old_column_name),
                        new_name: fold.name(&new_column_name),
                    }),
                    AlterTableOperation::RenameTable { table_name } => {
                        let new_name = match table_name {
                            RenameTableNameKind::As(name) | RenameTableNameKind::To(name) => {
                                fold.object(&name)
                            }
                        };
                        hazards.push(DdlHazard::RenameTable {
                            table: table.clone(),
                            new_name,
                        });
                    }
                    AlterTableOperation::AddColumn { column_def, .. } => {
                        if is_not_null_without_default(&column_def) {
                            hazards.push(DdlHazard::AddNotNullColumn {
                                table: table.clone(),
                                column: fold.name(&column_def.name),
                            });
                        }
                    }
                    AlterTableOperation::AlterColumn { column_name, op } => match op {
                        AlterColumnOperation::SetNotNull => {
                            hazards.push(DdlHazard::AddNotNull {
                                table: table.clone(),
                                column: fold.name(&column_name),
                            });
                        }
                        AlterColumnOperation::SetDataType { data_type, .. } => {
                            inspect_type_change(
                                &table,
                                &fold.name(&column_name),
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
                        let column = fold.name(&col_name);
                        inspect_type_change(&table, &column, &data_type, hazards);
                        if options.iter().any(|o| matches!(o, ColumnOption::NotNull)) {
                            hazards.push(DdlHazard::AddNotNull {
                                table: table.clone(),
                                column,
                            });
                        }
                    }
                    AlterTableOperation::ChangeColumn {
                        old_name,
                        new_name,
                        data_type,
                        options,
                        ..
                    } => {
                        let column = fold.name(&old_name);
                        let renamed = fold.name(&new_name);
                        if renamed != column {
                            hazards.push(DdlHazard::RenameColumn {
                                table: table.clone(),
                                column: column.clone(),
                                new_name: renamed,
                            });
                        }
                        inspect_type_change(&table, &column, &data_type, hazards);
                        if options.iter().any(|o| matches!(o, ColumnOption::NotNull)) {
                            hazards.push(DdlHazard::AddNotNull {
                                table: table.clone(),
                                column,
                            });
                        }
                    }
                    AlterTableOperation::AddConstraint { constraint, .. } => {
                        push_constraint(&table, &constraint, fold, hazards);
                    }
                    _ => {}
                }
            }
        }
        Statement::CreateIndex(idx) if idx.unique => {
            let table = fold.object(&idx.table_name);
            let columns: Vec<String> = idx
                .columns
                .iter()
                .filter_map(|ic| fold.column_expr(&ic.column.expr))
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
            shrink.preflight_probe_sql(&SqlDialect::MySql, &ns).unwrap(),
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
        let sql =
            "ALTER TABLE users DROP COLUMN legacy;\nTHIS IS NOT SQL;\nDROP TABLE old_sessions;";
        let summary = analyze_ddl_impact("postgres", sql);
        let error = summary
            .parse_error
            .expect("unparseable statement must be reported");
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

    #[test]
    fn folds_unquoted_identifiers_to_the_catalog_case() {
        let pg = analyze_ddl_impact(
            "postgres",
            r#"ALTER TABLE Users ALTER COLUMN Email SET NOT NULL; ALTER TABLE "Audit" ALTER COLUMN "Who" SET NOT NULL;"#,
        );
        assert_eq!(
            pg.hazards,
            vec![
                DdlHazard::AddNotNull {
                    table: "users".to_string(),
                    column: "email".to_string(),
                },
                DdlHazard::AddNotNull {
                    table: "Audit".to_string(),
                    column: "Who".to_string(),
                },
            ]
        );

        let snowflake = analyze_ddl_impact(
            "snowflake",
            "ALTER TABLE users ALTER COLUMN email SET NOT NULL",
        );
        assert_eq!(
            snowflake.hazards,
            vec![DdlHazard::AddNotNull {
                table: "USERS".to_string(),
                column: "EMAIL".to_string(),
            }]
        );
    }

    #[test]
    fn extracts_renames_truncate_new_not_null_columns_fk_and_check() {
        let sql = r#"
            ALTER TABLE users RENAME COLUMN mail TO email;
            ALTER TABLE users RENAME TO members;
            TRUNCATE TABLE sessions;
            ALTER TABLE orders ADD COLUMN status TEXT NOT NULL;
            ALTER TABLE orders ADD COLUMN note TEXT NOT NULL DEFAULT '';
            ALTER TABLE orders ADD CONSTRAINT fk_user FOREIGN KEY (user_id) REFERENCES users (id);
            ALTER TABLE orders ADD CONSTRAINT positive CHECK (total >= 0);
        "#;
        let summary = analyze_ddl_impact("postgres", sql);
        assert!(summary.parse_error.is_none());
        assert_eq!(
            summary.hazards,
            vec![
                DdlHazard::RenameColumn {
                    table: "users".to_string(),
                    column: "mail".to_string(),
                    new_name: "email".to_string(),
                },
                DdlHazard::RenameTable {
                    table: "users".to_string(),
                    new_name: "members".to_string(),
                },
                DdlHazard::TruncateTable {
                    table: "sessions".to_string(),
                },
                DdlHazard::AddNotNullColumn {
                    table: "orders".to_string(),
                    column: "status".to_string(),
                },
                DdlHazard::AddForeignKey {
                    table: "orders".to_string(),
                    columns: vec!["user_id".to_string()],
                    referenced_table: "users".to_string(),
                    referenced_columns: vec!["id".to_string()],
                },
                DdlHazard::AddCheck {
                    table: "orders".to_string(),
                    expression: "total >= 0".to_string(),
                },
            ]
        );
        assert_eq!(summary.affected_tables, vec!["users", "sessions", "orders"]);
    }

    #[test]
    fn unqualified_tables_follow_the_session_default_without_explicit_namespace() {
        let ns = Namespace {
            database: String::new(),
            schema: None,
        };
        let hazard = DdlHazard::AddNotNull {
            table: "users".to_string(),
            column: "email".to_string(),
        };
        assert_eq!(
            hazard.preflight_probe_sql(&SqlDialect::Postgres, &ns).unwrap(),
            "SELECT COUNT(*) AS violations FROM \"users\" WHERE \"email\" IS NULL"
        );
        assert_eq!(
            hazard.preflight_probe_sql(&SqlDialect::MySql, &ns).unwrap(),
            "SELECT COUNT(*) AS violations FROM `users` WHERE `email` IS NULL"
        );
    }

    #[test]
    fn mysql_rename_table_and_change_column_rename() {
        let summary = analyze_ddl_impact(
            "mysql",
            "RENAME TABLE a TO b; ALTER TABLE t CHANGE COLUMN old_c new_c INT NOT NULL;",
        );
        assert!(summary.hazards.contains(&DdlHazard::RenameTable {
            table: "a".to_string(),
            new_name: "b".to_string(),
        }));
        assert!(summary.hazards.contains(&DdlHazard::RenameColumn {
            table: "t".to_string(),
            column: "old_c".to_string(),
            new_name: "new_c".to_string(),
        }));
    }

    #[test]
    fn builds_foreign_key_check_and_new_column_probes() {
        let ns = Namespace {
            database: String::new(),
            schema: Some("public".to_string()),
        };
        let fk = DdlHazard::AddForeignKey {
            table: "orders".to_string(),
            columns: vec!["user_id".to_string()],
            referenced_table: "users".to_string(),
            referenced_columns: vec!["id".to_string()],
        };
        assert_eq!(
            fk.preflight_probe_sql(&SqlDialect::Postgres, &ns).unwrap(),
            "SELECT COUNT(*) AS violations FROM \"public\".\"orders\" AS _qore_c WHERE _qore_c.\"user_id\" IS NOT NULL AND NOT EXISTS (SELECT 1 FROM \"public\".\"users\" AS _qore_p WHERE _qore_p.\"id\" = _qore_c.\"user_id\")"
        );
        let check = DdlHazard::AddCheck {
            table: "orders".to_string(),
            expression: "total >= 0".to_string(),
        };
        assert_eq!(
            check.preflight_probe_sql(&SqlDialect::MySql, &ns).unwrap(),
            "SELECT COUNT(*) AS violations FROM `orders` WHERE NOT (total >= 0)"
        );
        let column = DdlHazard::AddNotNullColumn {
            table: "app.public.orders".to_string(),
            column: "status".to_string(),
        };
        assert_eq!(
            column
                .preflight_probe_sql(&SqlDialect::Postgres, &ns)
                .unwrap(),
            "SELECT COUNT(*) AS violations FROM \"app\".\"public\".\"orders\""
        );
        assert!(
            DdlHazard::RenameTable {
                table: "a".to_string(),
                new_name: "b".to_string()
            }
            .preflight_probe_sql(&SqlDialect::Postgres, &ns)
            .is_none()
        );
    }
}

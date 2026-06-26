// SPDX-License-Identifier: Apache-2.0

//! Per-driver argument builders for the backup CLIs.
//!
//! Identifier-style fields (database / table names) go through
//! [`safe_identifier`] before reaching `Command::arg`. Output paths come
//! from a file picker so we treat them as user intent, not as untrusted input.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::path_to_string;
use super::tools::BackupTool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupMode {
    /// Full dump — schema + data.
    Full,
    /// `--schema-only` style.
    SchemaOnly,
    /// `--data-only` style.
    DataOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupFormat {
    /// Plain SQL text (default for SQL drivers).
    Sql,
    /// `pg_dump --format=custom` — only valid for PostgreSQL.
    PostgresCustom,
    /// `mongodump` BSON archive.
    MongoArchive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupOptions {
    /// Driver short name (`postgres`, `mysql`, `mariadb`, `sqlite`, `mongodb`).
    pub driver: String,
    pub mode: BackupMode,
    pub format: BackupFormat,
    /// Connection coordinates, validated by the caller.
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    /// Sent via env (`PGPASSWORD`, `MYSQL_PWD`) or `--password` (Mongo) — never logged.
    pub password: Option<String>,
    pub database: Option<String>,
    /// Empty = dump everything. Each entry must be a bare identifier.
    pub tables: Vec<String>,
    pub output_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreOptions {
    pub driver: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
    pub input_path: PathBuf,
    pub format: BackupFormat,
}

/// Build the CLI args for a backup, in the order they should be passed to
/// `Command::arg`. Returns the args plus any environment variables to set
/// on the child (e.g. `PGPASSWORD`).
pub fn build_backup_args(
    tool: BackupTool,
    opts: &BackupOptions,
) -> Result<(Vec<String>, Vec<(String, String)>), String> {
    match tool {
        BackupTool::PgDump => build_pg_dump_args(opts),
        BackupTool::MysqlDump => build_mysql_dump_args(opts, false),
        BackupTool::MariaDbDump => build_mysql_dump_args(opts, true),
        BackupTool::MongoDump => build_mongo_dump_args(opts),
        BackupTool::Sqlite3 => build_sqlite_dump_args(opts),
        other => Err(format!("Tool {:?} is not a backup binary", other)),
    }
}

pub fn build_restore_args(
    tool: BackupTool,
    opts: &RestoreOptions,
) -> Result<(Vec<String>, Vec<(String, String)>), String> {
    match tool {
        BackupTool::PgRestore => build_pg_restore_args(opts),
        BackupTool::Psql => build_psql_restore_args(opts),
        BackupTool::Mysql => build_mysql_restore_args(opts),
        BackupTool::MongoRestore => build_mongo_restore_args(opts),
        BackupTool::Sqlite3 => build_sqlite_restore_args(opts),
        other => Err(format!("Tool {:?} is not a restore binary", other)),
    }
}

fn build_pg_dump_args(
    opts: &BackupOptions,
) -> Result<(Vec<String>, Vec<(String, String)>), String> {
    let host = safe_arg_value(&opts.host, "Host")?;
    let mut args = vec![
        "--host".into(),
        host,
        "--port".into(),
        opts.port.to_string(),
    ];

    if let Some(user) = opts.username.as_ref().filter(|u| !u.is_empty()) {
        args.push("--username".into());
        args.push(safe_arg_value(user, "Username")?);
        args.push("--no-password".into()); // password supplied via env
    }

    match opts.mode {
        BackupMode::Full => {}
        BackupMode::SchemaOnly => args.push("--schema-only".into()),
        BackupMode::DataOnly => args.push("--data-only".into()),
    }

    if matches!(opts.format, BackupFormat::PostgresCustom) {
        args.push("--format=custom".into());
    }

    for table in &opts.tables {
        let ident = safe_identifier(table)?;
        args.push("--table".into());
        args.push(ident);
    }

    args.push("--file".into());
    args.push(path_to_string(&opts.output_path)?);

    if let Some(db) = opts.database.as_ref().filter(|d| !d.is_empty()) {
        args.push(safe_identifier(db)?);
    }

    let env = opts
        .password
        .as_ref()
        .filter(|p| !p.is_empty())
        .map(|p| ("PGPASSWORD".to_string(), p.clone()))
        .into_iter()
        .collect();

    Ok((args, env))
}

fn build_mysql_dump_args(
    opts: &BackupOptions,
    _mariadb: bool,
) -> Result<(Vec<String>, Vec<(String, String)>), String> {
    let host = safe_arg_value(&opts.host, "Host")?;
    let mut args = vec![format!("--host={}", host), format!("--port={}", opts.port)];

    if let Some(user) = opts.username.as_ref().filter(|u| !u.is_empty()) {
        let user = safe_arg_value(user, "Username")?;
        args.push(format!("--user={}", user));
    }

    match opts.mode {
        BackupMode::Full => {}
        BackupMode::SchemaOnly => args.push("--no-data".into()),
        BackupMode::DataOnly => {
            args.push("--no-create-info".into());
            args.push("--skip-triggers".into());
        }
    }

    args.push(format!(
        "--result-file={}",
        path_to_string(&opts.output_path)?
    ));

    let db = opts
        .database
        .as_ref()
        .filter(|d| !d.is_empty())
        .ok_or_else(|| "Database name is required for MySQL/MariaDB dump".to_string())?;
    args.push(safe_identifier(db)?);

    for table in &opts.tables {
        args.push(safe_identifier(table)?);
    }

    let env = opts
        .password
        .as_ref()
        .filter(|p| !p.is_empty())
        .map(|p| ("MYSQL_PWD".to_string(), p.clone()))
        .into_iter()
        .collect();

    Ok((args, env))
}

fn build_mongo_dump_args(
    opts: &BackupOptions,
) -> Result<(Vec<String>, Vec<(String, String)>), String> {
    let host = safe_arg_value(&opts.host, "Host")?;
    let mut args = vec![
        format!("--host={}", host),
        format!("--port={}", opts.port),
        "--archive".to_string(),
        format!("--archive={}", path_to_string(&opts.output_path)?),
    ];
    // Drop the bare `--archive` if we already specified a path (cli accepts both).
    args.retain(|a| a != "--archive");

    if let Some(user) = opts.username.as_ref().filter(|u| !u.is_empty()) {
        let user = safe_arg_value(user, "Username")?;
        args.push(format!("--username={}", user));
        args.push("--authenticationDatabase=admin".into());
        if let Some(pass) = opts.password.as_ref().filter(|p| !p.is_empty()) {
            args.push(format!("--password={}", pass));
        }
    }

    if let Some(db) = opts.database.as_ref().filter(|d| !d.is_empty()) {
        args.push(format!("--db={}", safe_identifier(db)?));
    }

    Ok((args, Vec::new()))
}

fn build_sqlite_dump_args(
    opts: &BackupOptions,
) -> Result<(Vec<String>, Vec<(String, String)>), String> {
    let db_file = opts
        .database
        .as_ref()
        .filter(|d| !d.is_empty())
        .ok_or_else(|| "Database file path is required for SQLite dump".to_string())?;

    let dump_cmd = match opts.mode {
        BackupMode::Full => ".dump",
        BackupMode::SchemaOnly => ".schema",
        BackupMode::DataOnly => {
            return Err("SQLite does not support --data-only dumps".into());
        }
    };

    let args = vec![db_file.clone(), dump_cmd.to_string()];
    Ok((args, Vec::new()))
}

fn build_pg_restore_args(
    opts: &RestoreOptions,
) -> Result<(Vec<String>, Vec<(String, String)>), String> {
    let host = safe_arg_value(&opts.host, "Host")?;
    let mut args = vec![
        "--host".into(),
        host,
        "--port".into(),
        opts.port.to_string(),
    ];

    if let Some(user) = opts.username.as_ref().filter(|u| !u.is_empty()) {
        args.push("--username".into());
        args.push(safe_arg_value(user, "Username")?);
        args.push("--no-password".into());
    }

    if let Some(db) = opts.database.as_ref().filter(|d| !d.is_empty()) {
        args.push("--dbname".into());
        args.push(safe_identifier(db)?);
    }

    args.push(path_to_string(&opts.input_path)?);

    let env = opts
        .password
        .as_ref()
        .filter(|p| !p.is_empty())
        .map(|p| ("PGPASSWORD".to_string(), p.clone()))
        .into_iter()
        .collect();

    Ok((args, env))
}

fn build_psql_restore_args(
    opts: &RestoreOptions,
) -> Result<(Vec<String>, Vec<(String, String)>), String> {
    let host = safe_arg_value(&opts.host, "Host")?;
    let mut args = vec![
        "--host".into(),
        host,
        "--port".into(),
        opts.port.to_string(),
    ];

    if let Some(user) = opts.username.as_ref().filter(|u| !u.is_empty()) {
        args.push("--username".into());
        args.push(safe_arg_value(user, "Username")?);
    }

    if let Some(db) = opts.database.as_ref().filter(|d| !d.is_empty()) {
        args.push(safe_identifier(db)?);
    }

    args.push("--file".into());
    args.push(path_to_string(&opts.input_path)?);

    let env = opts
        .password
        .as_ref()
        .filter(|p| !p.is_empty())
        .map(|p| ("PGPASSWORD".to_string(), p.clone()))
        .into_iter()
        .collect();

    Ok((args, env))
}

fn build_mysql_restore_args(
    opts: &RestoreOptions,
) -> Result<(Vec<String>, Vec<(String, String)>), String> {
    let host = safe_arg_value(&opts.host, "Host")?;
    let mut args = vec![format!("--host={}", host), format!("--port={}", opts.port)];

    if let Some(user) = opts.username.as_ref().filter(|u| !u.is_empty()) {
        let user = safe_arg_value(user, "Username")?;
        args.push(format!("--user={}", user));
    }

    if let Some(db) = opts.database.as_ref().filter(|d| !d.is_empty()) {
        args.push(safe_identifier(db)?);
    }

    let env = opts
        .password
        .as_ref()
        .filter(|p| !p.is_empty())
        .map(|p| ("MYSQL_PWD".to_string(), p.clone()))
        .into_iter()
        .collect();

    Ok((args, env))
}

fn build_mongo_restore_args(
    opts: &RestoreOptions,
) -> Result<(Vec<String>, Vec<(String, String)>), String> {
    let host = safe_arg_value(&opts.host, "Host")?;
    let mut args = vec![
        format!("--host={}", host),
        format!("--port={}", opts.port),
        format!("--archive={}", path_to_string(&opts.input_path)?),
    ];

    if let Some(user) = opts.username.as_ref().filter(|u| !u.is_empty()) {
        let user = safe_arg_value(user, "Username")?;
        args.push(format!("--username={}", user));
        args.push("--authenticationDatabase=admin".into());
        if let Some(pass) = opts.password.as_ref().filter(|p| !p.is_empty()) {
            args.push(format!("--password={}", pass));
        }
    }

    if let Some(db) = opts.database.as_ref().filter(|d| !d.is_empty()) {
        args.push(format!("--db={}", safe_identifier(db)?));
    }

    Ok((args, Vec::new()))
}

fn build_sqlite_restore_args(
    opts: &RestoreOptions,
) -> Result<(Vec<String>, Vec<(String, String)>), String> {
    let db_file = opts
        .database
        .as_ref()
        .filter(|d| !d.is_empty())
        .ok_or_else(|| "Database file path is required for SQLite restore".to_string())?;

    Ok((vec![db_file.clone()], Vec::new()))
}

/// Reject values that would be parsed as a CLI flag by the backup binaries.
/// Used for `host` and `username` which are passed verbatim to `Command::arg`.
/// We still accept dots, hyphens, underscores so things like
/// `db.internal-corp.local` or `service-account-1` keep working — we only
/// guard against `--flag` injection.
fn safe_arg_value(value: &str, field: &str) -> Result<String, String> {
    if value.starts_with('-') {
        return Err(format!("{} '{}' starts with a dash", field, value));
    }
    if value.contains('\0') {
        return Err(format!("{} contains a null byte", field));
    }
    Ok(value.to_string())
}

/// Allow only ASCII identifier characters plus dot (qualified names) and
/// hyphen (some DBs accept those when quoted, but here we want the bare
/// identifier passed as a single arg). This stops `--something` style
/// injection masquerading as a database name.
pub fn safe_identifier(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err("Identifier cannot be empty".into());
    }
    if value.starts_with('-') {
        return Err(format!("Identifier '{}' starts with a dash", value));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
    {
        return Err(format!(
            "Identifier '{}' contains unsupported characters",
            value
        ));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> BackupOptions {
        BackupOptions {
            driver: "postgres".into(),
            mode: BackupMode::Full,
            format: BackupFormat::Sql,
            host: "localhost".into(),
            port: 5432,
            username: Some("admin".into()),
            password: Some("hunter2".into()),
            database: Some("orders".into()),
            tables: Vec::new(),
            output_path: PathBuf::from("/tmp/orders.sql"),
        }
    }

    #[test]
    fn pg_dump_builds_args_and_env() {
        let (args, env) = build_pg_dump_args(&opts()).unwrap();
        assert!(args.iter().any(|a| a == "--host"));
        assert!(args.iter().any(|a| a == "localhost"));
        assert!(args.iter().any(|a| a == "--username"));
        assert!(args.iter().any(|a| a == "--no-password"));
        assert!(args.iter().any(|a| a == "--file"));
        assert!(args.iter().any(|a| a == "/tmp/orders.sql"));
        assert!(args.iter().any(|a| a == "orders"));
        assert_eq!(env, vec![("PGPASSWORD".to_string(), "hunter2".to_string())]);
    }

    #[test]
    fn pg_dump_schema_only_flag() {
        let mut o = opts();
        o.mode = BackupMode::SchemaOnly;
        let (args, _) = build_pg_dump_args(&o).unwrap();
        assert!(args.iter().any(|a| a == "--schema-only"));
    }

    #[test]
    fn pg_dump_table_filter_validates_identifier() {
        let mut o = opts();
        o.tables = vec!["valid_table".into(), "--bad".into()];
        let err = build_pg_dump_args(&o).unwrap_err();
        assert!(err.contains("starts with a dash"));
    }

    #[test]
    fn mysql_dump_uses_env_password() {
        let mut o = opts();
        o.driver = "mysql".into();
        let (args, env) = build_mysql_dump_args(&o, false).unwrap();
        assert!(args.iter().any(|a| a == "--host=localhost"));
        assert!(args.iter().any(|a| a == "--user=admin"));
        assert_eq!(env, vec![("MYSQL_PWD".to_string(), "hunter2".to_string())]);
    }

    #[test]
    fn pg_dump_rejects_dash_prefixed_host() {
        let mut o = opts();
        o.host = "--malicious".into();
        let err = build_pg_dump_args(&o).unwrap_err();
        assert!(err.contains("Host"));
        assert!(err.contains("starts with a dash"));
    }

    #[test]
    fn pg_dump_rejects_dash_prefixed_username() {
        let mut o = opts();
        o.username = Some("--evil".into());
        let err = build_pg_dump_args(&o).unwrap_err();
        assert!(err.contains("Username"));
    }

    #[test]
    fn mysql_dump_rejects_dash_prefixed_username() {
        let mut o = opts();
        o.driver = "mysql".into();
        o.username = Some("--evil".into());
        let err = build_mysql_dump_args(&o, false).unwrap_err();
        assert!(err.contains("Username"));
    }

    #[test]
    fn safe_arg_value_accepts_realistic_hostnames() {
        // hostnames with dots and hyphens must still pass
        assert!(safe_arg_value("db.internal-corp.local", "Host").is_ok());
        assert!(safe_arg_value("service-account-1", "Username").is_ok());
    }

    #[test]
    fn safe_identifier_rejects_injection() {
        assert!(safe_identifier("--drop-table").is_err());
        assert!(safe_identifier("a;b").is_err());
        assert!(safe_identifier("$(rm)").is_err());
        assert!(safe_identifier("schema.table").is_ok());
        assert!(safe_identifier("orders_2024").is_ok());
    }

    #[test]
    fn sqlite_data_only_is_rejected() {
        let mut o = opts();
        o.driver = "sqlite".into();
        o.mode = BackupMode::DataOnly;
        o.database = Some("/tmp/foo.db".into());
        let err = build_sqlite_dump_args(&o).unwrap_err();
        assert!(err.contains("data-only"));
    }

    #[test]
    fn mongo_dump_passes_password_on_cli_not_env() {
        let mut o = opts();
        o.driver = "mongodb".into();
        o.format = BackupFormat::MongoArchive;
        o.port = 27017;
        o.output_path = PathBuf::from("/tmp/dump.archive");
        let (args, env) = build_mongo_dump_args(&o).unwrap();
        assert!(args.iter().any(|a| a == "--username=admin"));
        assert!(args.iter().any(|a| a == "--password=hunter2"));
        assert!(args.iter().any(|a| a == "--authenticationDatabase=admin"));
        assert!(env.is_empty());
    }

    #[test]
    fn mongo_restore_passes_password_on_cli_not_env() {
        let o = RestoreOptions {
            driver: "mongodb".into(),
            host: "localhost".into(),
            port: 27017,
            username: Some("admin".into()),
            password: Some("hunter2".into()),
            database: Some("orders".into()),
            input_path: PathBuf::from("/tmp/dump.archive"),
            format: BackupFormat::MongoArchive,
        };
        let (args, env) = build_mongo_restore_args(&o).unwrap();
        assert!(args.iter().any(|a| a == "--password=hunter2"));
        assert!(env.is_empty());
    }
}

// SPDX-License-Identifier: Apache-2.0

//! SQL dialect selection.
//!
//! The public [`Dialect`] enum is the user-facing choice. Internally, each
//! variant resolves to a static [`DialectOps`] implementation that carries
//! all per-dialect behaviour (quoting, placeholders, LIMIT/FETCH style,
//! `ILIKE` support, …). Concrete implementations live in
//! `compiler/{postgres,mysql,sqlite,mssql,duckdb,snowflake,bigquery}.rs`.
//!
//! **CockroachDB** is Postgres wire-compatible; pick [`Dialect::Postgres`]
//! until a truly divergent feature requires its own variant.

use crate::compiler::{
    DialectOps, bigquery::BigQueryOps, duckdb::DuckDbOps, mssql::SqlServerOps, mysql::MySqlOps,
    postgres::PostgresOps, snowflake::SnowflakeOps, sqlite::SqliteOps,
};

/// Target SQL dialect for a compiled query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Postgres,
    MySql,
    Sqlite,
    SqlServer,
    DuckDb,
    Snowflake,
    BigQuery,
}

impl Dialect {
    /// Resolve the static [`DialectOps`] implementation for this variant.
    pub(crate) fn ops(self) -> &'static dyn DialectOps {
        match self {
            Dialect::Postgres => &PostgresOps,
            Dialect::MySql => &MySqlOps,
            Dialect::Sqlite => &SqliteOps,
            Dialect::SqlServer => &SqlServerOps,
            Dialect::DuckDb => &DuckDbOps,
            Dialect::Snowflake => &SnowflakeOps,
            Dialect::BigQuery => &BigQueryOps,
        }
    }

    /// Best-effort mapping from a QoreDB driver id. Returns `None` for
    /// drivers that have no SQL query-builder representation
    /// (MongoDB, Redis).
    pub fn from_driver_id(driver_id: &str) -> Option<Self> {
        match driver_id.to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" | "cockroachdb" | "cockroach" | "yugabytedb" => {
                Some(Dialect::Postgres)
            }
            "mysql" | "mariadb" | "planetscale" | "tidb" | "starrocks" | "doris"
            | "singlestore" => Some(Dialect::MySql),
            "sqlite" => Some(Dialect::Sqlite),
            "sqlserver" | "mssql" | "azuresql" | "synapse" => Some(Dialect::SqlServer),
            "duckdb" | "motherduck" => Some(Dialect::DuckDb),
            "snowflake" => Some(Dialect::Snowflake),
            "bigquery" => Some(Dialect::BigQuery),
            _ => None,
        }
    }
}

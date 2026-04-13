// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

pub type QueryResult<T> = Result<T, QueryError>;

#[derive(Debug, Error)]
pub enum QueryError {
    /// A SELECT was built without specifying a source table.
    #[error("SELECT query has no FROM clause")]
    MissingFrom,

    /// A SELECT was built with no projection (no columns, no `*`).
    #[error("SELECT query has an empty projection — call .all() or .columns([...])")]
    EmptyProjection,

    /// A literal value could not be emitted safely into SQL
    /// (NaN, Infinity, or other dialect-incompatible value).
    #[error("invalid literal: {0}")]
    InvalidLiteral(&'static str),

    /// Expression semantics are invalid (e.g. malformed BETWEEN bounds).
    #[error("invalid expression: {0}")]
    InvalidExpr(&'static str),

    /// The requested feature is not supported by the target dialect.
    #[error("feature not supported by dialect: {0}")]
    Unsupported(&'static str),

    /// MSSQL requires `ORDER BY` when `OFFSET`/`FETCH NEXT` is used.
    #[error("MSSQL OFFSET/FETCH requires ORDER BY")]
    MssqlOffsetRequiresOrderBy,
}

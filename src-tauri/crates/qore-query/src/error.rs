// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

pub type QueryResult<T> = Result<T, QueryError>;

#[derive(Debug, Error)]
pub enum QueryError {
    #[error("invalid expression: {0}")]
    InvalidExpr(String),

    #[error("invalid literal: {0}")]
    InvalidLiteral(String),

    #[error("feature not supported by dialect: {0}")]
    Unsupported(String),

    #[error("MSSQL OFFSET requires ORDER BY")]
    MssqlOffsetRequiresOrderBy,
}

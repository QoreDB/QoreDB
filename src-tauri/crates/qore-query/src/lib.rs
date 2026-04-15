// SPDX-License-Identifier: Apache-2.0

//! # QoreQuery — type-safe, multi-dialect SQL query builder.
//!
//! QoreQuery compiles an in-memory query AST into parameterised SQL for
//! PostgreSQL, MySQL/MariaDB, SQLite, Microsoft SQL Server, and DuckDB.
//! It is the query layer of the QoreDB platform and the foundation for
//! the upcoming QoreORM.
//!
//! ## Design goals
//!
//! - **SQL injection is structurally impossible**: every literal becomes
//!   a bound parameter, every identifier is quoted through a per-dialect
//!   escape routine.
//! - **One expression, N dialects**: the same builder produces correct
//!   SQL for each backend; dialect-specific fallbacks (e.g. `ILIKE` on
//!   MySQL) and syntactic variants (`OFFSET…FETCH` on MSSQL) are applied
//!   by the compiler.
//! - **Ready for QoreORM**: [`Column<T>`] is phantom-typed, so future
//!   `#[derive(Model)]` macros can emit typed column accessors without
//!   breaking this API.
//!
//! ## Example
//!
//! ```
//! use qore_query::prelude::*;
//! use qore_query::query::{Order, Query};
//! use qore_query::ident::tcol;
//!
//! let q = Query::select()
//!     .from_as("users", "u")
//!     .columns(["id", "name", "email"])
//!     .inner_join_as("orders", "o", tcol("u", "id").eq(tcol("o", "user_id")))
//!     .filter(col("age").gt(18).and(col("active").eq(true)))
//!     .order_by("name", Order::Asc)
//!     .limit(10)
//!     .build(Dialect::Postgres)
//!     .unwrap();
//!
//! assert!(q.sql.starts_with(r#"SELECT "id", "name", "email" FROM "users" AS "u""#));
//! assert_eq!(q.params.len(), 2); // 18 and true
//! ```
//!
//! ## Crate layout
//!
//! | Module | Role |
//! | --- | --- |
//! | [`ident`] | `Column<T>`, `col()`, `tcol()`, `IntoOperand` trait |
//! | [`expr`] | `Expr` AST, `BinOp`/`UnOp`, composition (`and`, `or`, `not`) |
//! | [`query`] | `SelectQuery` fluent builder, joins, ordering |
//! | [`compiler`] | `DialectOps` trait + one file per dialect, `SqlCompiler` |
//! | [`dialect`] | Public `Dialect` enum + driver-id mapping |
//! | [`built`] | `BuiltQuery { sql, params }` — ready for `DataEngine::execute` |
//! | [`error`] | Typed `QueryError` variants |
//!
//! See `doc/QoreQuery_Builder_Plan.md` for the full roadmap through the
//! Phase 2 MVP and beyond.

pub mod built;
pub mod compiler;
pub mod dialect;
pub mod error;
pub mod expr;
pub mod ident;
pub mod prelude;
pub mod query;
pub mod sql_type;

pub use built::BuiltQuery;
pub use dialect::Dialect;
pub use error::{QueryError, QueryResult};
pub use expr::{cast, coalesce, exists, not_exists};
pub use ident::{col, tcol, Column, IntoOperand};
pub use sql_type::SqlType;

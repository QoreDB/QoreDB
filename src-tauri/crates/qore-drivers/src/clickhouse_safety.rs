// SPDX-License-Identifier: Apache-2.0

//! Re-export of the ClickHouse classifier from `qore-sql`.
//!
//! The classifier lives in `qore-sql` so it can be wired into the SQL
//! safety pipeline without taking a driver dep. This module is kept for
//! backward compatibility with existing imports.

pub use qore_sql::clickhouse_safety::{classify, ClickHouseQueryClass};

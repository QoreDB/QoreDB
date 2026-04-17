// SPDX-License-Identifier: Apache-2.0

use qore_core::Value;

/// Compiled query ready for execution against a `DataEngine`.
///
/// All literal values are bound via `params` — the SQL string contains
/// only placeholders, never inlined values.
#[derive(Debug, Clone)]
pub struct BuiltQuery {
    pub sql: String,
    pub params: Vec<Value>,
}

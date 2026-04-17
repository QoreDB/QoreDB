// SPDX-License-Identifier: Apache-2.0

//! Portable SQL type names for `CAST(... AS type)` expressions.
//!
//! The variants here cover the common cases we need for cross-dialect
//! query building. Each dialect renders its own syntax via
//! [`crate::compiler::DialectOps::write_sql_type`]; see that trait
//! for the defaults and per-dialect overrides.
//!
//! We keep the set small on purpose: `CAST` is a tool for coercing
//! within a query, not for modelling a full schema — that's Phase 3
//! (qore-orm). When a cast target has no portable spelling (e.g.
//! MySQL `SIGNED` vs `INT`, SQLite type-affinity quirks), the
//! dialect impl picks the closest idiom.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlType {
    /// 32-bit signed integer.
    Int,
    /// 64-bit signed integer.
    BigInt,
    /// Single-precision float.
    Real,
    /// Double-precision float.
    Double,
    /// Variable-length text.
    Text,
    /// Boolean.
    Bool,
    /// Calendar date (no time component).
    Date,
    /// Timestamp (date + time, no time-zone).
    Timestamp,
    /// Opaque bytes.
    Blob,
}

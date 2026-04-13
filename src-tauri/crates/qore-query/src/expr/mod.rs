// SPDX-License-Identifier: Apache-2.0

//! Expression tree. Implementation lands in Semaine 1-2.

use qore_core::Value;

pub mod ops;

#[derive(Debug, Clone)]
pub enum Expr {
    // Populated in Semaine 1-2 — see doc/QoreQuery_Builder_Plan.md §3.2
    Literal(Value),
    Placeholder, // stub to keep variants non-empty
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Like,
    ILike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    IsNull,
    IsNotNull,
}

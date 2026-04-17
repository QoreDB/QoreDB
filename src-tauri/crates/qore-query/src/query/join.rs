// SPDX-License-Identifier: Apache-2.0

//! JOIN clause AST.

use std::borrow::Cow;

use crate::expr::Expr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    Inner,
    Left,
    Right,
    Full,
}

impl JoinKind {
    pub(crate) fn sql_keyword(self) -> &'static str {
        match self {
            JoinKind::Inner => "INNER JOIN",
            JoinKind::Left => "LEFT JOIN",
            JoinKind::Right => "RIGHT JOIN",
            JoinKind::Full => "FULL JOIN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Join {
    pub(crate) kind: JoinKind,
    pub(crate) table: Cow<'static, str>,
    pub(crate) alias: Option<Cow<'static, str>>,
    pub(crate) on: Expr,
}

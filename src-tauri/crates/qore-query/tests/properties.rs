// SPDX-License-Identifier: Apache-2.0

//! Property-based tests.
//!
//! These guard two structural invariants of the compiler that must hold
//! for **every** well-formed AST:
//!
//! 1. Compilation is **total** — it either returns `Ok(BuiltQuery)` or
//!    a typed `QueryError`. No panic, no infinite loop.
//! 2. When compilation succeeds, the emitted SQL is **parseable** by
//!    `sqlparser` under the corresponding dialect. That catches bugs
//!    like unbalanced parentheses, stray keywords, or malformed JOIN
//!    syntax that snapshot tests might miss.
//!
//! We keep the expression space small on purpose: three columns, a
//! handful of literals, depth bounded at 4. The goal is to exercise
//! the combinatorial explosion of *operators* on a manageable leaf
//! set, not to fuzz the entire API.

use proptest::prelude::*;
use qore_query::prelude::*;
use qore_query::query::Query;
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

/// Strategy producing a bounded-depth [`Expr`] tree over a fixed
/// alphabet of columns and small integer literals.
fn expr_strategy() -> impl Strategy<Value = Expr> {
    let leaf = prop_oneof![
        Just("a"), Just("b"), Just("c"),
    ]
    .prop_map(|name| col(name).into_operand());

    let lit = any::<i32>().prop_map(|n| Expr::Literal((n as i64).into()));

    let atom = prop_oneof![leaf, lit];

    atom.prop_recursive(
        4,   // depth
        32,  // max nodes
        4,   // max items per collection
        |inner| {
            prop_oneof![
                (inner.clone(), inner.clone()).prop_map(|(a, b)| a.and(b)),
                (inner.clone(), inner.clone()).prop_map(|(a, b)| a.or(b)),
                (inner.clone(), inner.clone()).prop_map(|(a, b)| a.eq(b)),
                (inner.clone(), inner.clone()).prop_map(|(a, b)| a.gt(b)),
                inner.clone().prop_map(|a| a.not()),
            ]
        },
    )
}

proptest! {
    /// Compilation on any generated AST must either succeed or produce
    /// a typed `QueryError`. No panic.
    #[test]
    fn compilation_is_total(e in expr_strategy()) {
        let result = Query::select()
            .from("t")
            .all()
            .filter(e)
            .build(Dialect::Postgres);
        // Accept both success and typed error — neither is a crash.
        let _ = result;
    }

    /// When compilation succeeds, the emitted SQL must parse cleanly
    /// under the Postgres dialect. Catches structural bugs that
    /// fixture tests might miss.
    #[test]
    fn compiled_sql_is_parseable(e in expr_strategy()) {
        if let Ok(built) = Query::select()
            .from("t")
            .all()
            .filter(e)
            .build(Dialect::Postgres)
        {
            let parsed = Parser::parse_sql(&PostgreSqlDialect {}, &built.sql);
            prop_assert!(
                parsed.is_ok(),
                "sqlparser rejected compiled SQL:\n  {}\n  error: {:?}",
                built.sql,
                parsed.err()
            );
        }
    }

    /// The number of `$N` placeholders in the compiled Postgres SQL
    /// must equal `params.len()`.
    #[test]
    fn placeholders_match_params_for_postgres(e in expr_strategy()) {
        if let Ok(built) = Query::select()
            .from("t")
            .all()
            .filter(e)
            .build(Dialect::Postgres)
        {
            // Count occurrences of `$` followed by a digit. A bare `$`
            // would indicate a bug, so count those too via any `$`.
            let n = built.sql.matches('$').count();
            prop_assert_eq!(n, built.params.len());
        }
    }
}

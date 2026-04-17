// SPDX-License-Identifier: Apache-2.0

//! Hardening tests: depth and parameter-count bounds, plus the extended
//! `IntoOperand` surface on `in_` / `not_in` / `between`.

use qore_query::compiler::{MAX_AST_DEPTH, MAX_PARAMS};
use qore_query::ident::tcol;
use qore_query::prelude::*;
use qore_query::query::{Order, Query};

fn pg() -> Dialect {
    Dialect::Postgres
}

#[test]
fn deeply_nested_beyond_limit_errors_cleanly() {
    // Build a tree far beyond MAX_AST_DEPTH by repeated AND nesting.
    let mut e: Expr = col("x").eq(1i64);
    for _ in 0..(MAX_AST_DEPTH + 100) {
        e = e.and(col("x").eq(1i64));
    }
    let err = Query::select()
        .from("t")
        .all()
        .filter(e)
        .build(pg())
        .unwrap_err();
    match err {
        QueryError::AstTooDeep(limit) => assert_eq!(limit, MAX_AST_DEPTH),
        other => panic!("expected AstTooDeep, got {:?}", other),
    }
}

#[test]
fn reasonable_depth_still_compiles() {
    // A 50-level deep AND chain must stay well within the limit.
    let mut e: Expr = col("x").eq(1i64);
    for _ in 0..50 {
        e = e.and(col("x").eq(1i64));
    }
    let q = Query::select()
        .from("t")
        .all()
        .filter(e)
        .build(pg())
        .unwrap();
    assert_eq!(q.params.len(), 51);
}

#[test]
fn too_many_parameters_errors_cleanly() {
    // A 70k-element IN list exceeds MAX_PARAMS (65535).
    let huge: Vec<i64> = (0..(MAX_PARAMS as i64 + 100)).collect();
    let err = Query::select()
        .from("t")
        .all()
        .filter(col("id").in_(huge))
        .build(pg())
        .unwrap_err();
    match err {
        QueryError::TooManyParameters(limit) => assert_eq!(limit, MAX_PARAMS),
        other => panic!("expected TooManyParameters, got {:?}", other),
    }
}

#[test]
fn in_accepts_column_references_for_future_subquery_compat() {
    // `col IN (col_a, col_b, col_c)` is legal SQL — same shape as the
    // future `col IN (subquery)` path. Migration to IntoOperand enables
    // both without further API changes.
    let q = Query::select()
        .from("t")
        .all()
        .filter(col("status").in_([col("primary_status"), col("secondary_status")]))
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "t" WHERE ("status" IN ("primary_status", "secondary_status"))"#
    );
    assert!(q.params.is_empty(), "column refs must not become params");
}

#[test]
fn between_accepts_columns_as_bounds() {
    // Common pattern: `ts BETWEEN start_ts AND end_ts` where bounds are
    // other columns, not literals.
    let q = Query::select()
        .from("events")
        .all()
        .filter(col("ts").between(col("window_start"), col("window_end")))
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "events" WHERE ("ts" BETWEEN "window_start" AND "window_end")"#
    );
    assert!(q.params.is_empty());
}

#[test]
fn between_mixes_columns_and_literals() {
    let q = Query::select()
        .from("events")
        .all()
        .filter(col("ts").between(col("floor"), 1_000_000i64))
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "events" WHERE ("ts" BETWEEN "floor" AND $1)"#
    );
    assert_eq!(q.params.len(), 1);
}

#[test]
fn order_by_qualified_column() {
    let q = Query::select()
        .from_as("users", "u")
        .all()
        .order_by_qualified("u", "created_at", Order::Desc)
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "users" AS "u" ORDER BY "u"."created_at" DESC"#
    );
}

#[test]
fn crate_docs_example_compiles_and_produces_expected_shape() {
    // Mirrors the `lib.rs` doc example — serves as a regression test
    // that the headline example keeps working as the API evolves.
    let q = Query::select()
        .from_as("users", "u")
        .columns(["id", "name", "email"])
        .inner_join_as("orders", "o", tcol("u", "id").eq(tcol("o", "user_id")))
        .filter(col("age").gt(18).and(col("active").eq(true)))
        .order_by("name", Order::Asc)
        .limit(10)
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT "id", "name", "email" FROM "users" AS "u" INNER JOIN "orders" AS "o" ON ("u"."id" = "o"."user_id") WHERE (("age" > $1) AND ("active" = $2)) ORDER BY "name" ASC LIMIT 10"#
    );
    assert_eq!(q.params.len(), 2);
}

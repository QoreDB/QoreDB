// SPDX-License-Identifier: Apache-2.0

//! Semaine 4 coverage: JOINs, table aliases, NULLS FIRST/LAST ordering.

use qore_query::prelude::*;
use qore_query::query::{Nulls, Order, Query};

fn pg() -> Dialect {
    Dialect::Postgres
}
fn my() -> Dialect {
    Dialect::MySql
}
fn sl() -> Dialect {
    Dialect::Sqlite
}
fn ms() -> Dialect {
    Dialect::SqlServer
}
fn dd() -> Dialect {
    Dialect::DuckDb
}

// ============================================================================
// JOINs
// ============================================================================

#[test]
fn inner_join_with_aliases_postgres() {
    use qore_query::ident::tcol;
    let q = Query::select()
        .from_as("users", "u")
        .columns(["id", "name"])
        .inner_join_as("orders", "o", tcol("u", "id").eq(tcol("o", "user_id")))
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT "id", "name" FROM "users" AS "u" INNER JOIN "orders" AS "o" ON ("u"."id" = "o"."user_id")"#
    );
    assert!(q.params.is_empty());
}

#[test]
fn left_and_right_joins_mysql() {
    use qore_query::ident::tcol;
    let q = Query::select()
        .from_as("a", "a")
        .all()
        .left_join_as("b", "b", tcol("a", "id").eq(tcol("b", "a_id")))
        .right_join_as("c", "c", tcol("a", "id").eq(tcol("c", "a_id")))
        .build(my())
        .unwrap();
    assert_eq!(
        q.sql,
        "SELECT * FROM `a` AS `a` \
         LEFT JOIN `b` AS `b` ON (`a`.`id` = `b`.`a_id`) \
         RIGHT JOIN `c` AS `c` ON (`a`.`id` = `c`.`a_id`)"
    );
}

#[test]
fn full_join_rejected_on_mysql() {
    let err = Query::select()
        .from("a")
        .all()
        .full_join("b", col("x").eq(1i64))
        .build(my())
        .unwrap_err();
    assert!(matches!(err, QueryError::Unsupported(_)));
}

#[test]
fn full_join_rejected_on_sqlite() {
    let err = Query::select()
        .from("a")
        .all()
        .full_join("b", col("x").eq(1i64))
        .build(sl())
        .unwrap_err();
    assert!(matches!(err, QueryError::Unsupported(_)));
}

#[test]
fn right_join_rejected_on_sqlite() {
    let err = Query::select()
        .from("a")
        .all()
        .right_join("b", col("x").eq(1i64))
        .build(sl())
        .unwrap_err();
    assert!(matches!(err, QueryError::Unsupported(_)));
}

#[test]
fn full_join_accepted_on_postgres_and_mssql_and_duckdb() {
    for d in [pg(), ms(), dd()] {
        let q = Query::select()
            .from("a")
            .all()
            .full_join("b", col("x").eq(1i64))
            .order_by("x", Order::Asc) // MSSQL-safe; others ignore
            .build(d)
            .unwrap();
        assert!(q.sql.contains("FULL JOIN"), "dialect: {:?}", d);
    }
}

#[test]
fn join_on_clause_parameters_are_contiguous_with_where() {
    use qore_query::ident::tcol;
    let q = Query::select()
        .from_as("users", "u")
        .all()
        .inner_join_as("orders", "o", tcol("u", "id").eq(tcol("o", "user_id")))
        .filter(tcol("o", "total").gt(100i64))
        .filter(col("active").eq(true))
        .build(pg())
        .unwrap();
    assert!(q.sql.contains("$1"));
    assert!(q.sql.contains("$2"));
    assert_eq!(q.params.len(), 2);
}

#[test]
fn multiple_joins_preserve_order() {
    use qore_query::ident::tcol;
    let q = Query::select()
        .from("a")
        .all()
        .inner_join("b", tcol("a", "id").eq(tcol("b", "a_id")))
        .left_join("c", tcol("b", "id").eq(tcol("c", "b_id")))
        .build(pg())
        .unwrap();
    let inner_idx = q.sql.find("INNER JOIN").unwrap();
    let left_idx = q.sql.find("LEFT JOIN").unwrap();
    assert!(
        inner_idx < left_idx,
        "joins must compile in insertion order"
    );
}

// ============================================================================
// Table alias on FROM
// ============================================================================

#[test]
fn from_as_renders_with_as_keyword() {
    let q = Query::select()
        .from_as("users", "u")
        .all()
        .build(pg())
        .unwrap();
    assert_eq!(q.sql, r#"SELECT * FROM "users" AS "u""#);
}

#[test]
fn from_after_from_as_resets_alias() {
    // Calling .from() after .from_as() should clear the alias (no stale state).
    let q = Query::select()
        .from_as("users", "u")
        .from("products") // overrides and drops alias
        .all()
        .build(pg())
        .unwrap();
    assert_eq!(q.sql, r#"SELECT * FROM "products""#);
}

// ============================================================================
// NULLS FIRST / LAST
// ============================================================================

#[test]
fn nulls_ordering_native_on_postgres_sqlite_duckdb() {
    for d in [pg(), sl(), dd()] {
        let q = Query::select()
            .from("t")
            .all()
            .order_by_nulls("name", Order::Asc, Nulls::Last)
            .build(d)
            .unwrap();
        assert!(
            q.sql.contains("NULLS LAST"),
            "native NULLS LAST expected for {:?}, got: {}",
            d,
            q.sql
        );
    }
}

#[test]
fn nulls_ordering_emulated_on_mysql() {
    let q = Query::select()
        .from("t")
        .all()
        .order_by_nulls("name", Order::Asc, Nulls::Last)
        .build(my())
        .unwrap();
    // NULLS LAST: non-NULLs (key 0) sort before NULLs (key 1)
    assert_eq!(
        q.sql,
        "SELECT * FROM `t` ORDER BY CASE WHEN `name` IS NULL THEN 1 ELSE 0 END, `name` ASC"
    );
}

#[test]
fn nulls_first_emulated_on_mssql() {
    let q = Query::select()
        .from("t")
        .all()
        .order_by_nulls("created_at", Order::Desc, Nulls::First)
        .build(ms())
        .unwrap();
    // NULLS FIRST: NULLs (key 0) sort before non-NULLs (key 1)
    assert_eq!(
        q.sql,
        "SELECT * FROM [t] ORDER BY CASE WHEN [created_at] IS NULL THEN 0 ELSE 1 END, [created_at] DESC"
    );
}

#[test]
fn order_by_without_nulls_clause_is_untouched() {
    // Sanity: regular order_by() should not emit any CASE WHEN prefix,
    // even on a dialect that lacks NULLS support.
    let q = Query::select()
        .from("t")
        .all()
        .order_by("name", Order::Asc)
        .build(my())
        .unwrap();
    assert_eq!(q.sql, "SELECT * FROM `t` ORDER BY `name` ASC");
}

#[test]
fn mixed_order_by_with_and_without_nulls() {
    // Multiple order keys where only some have NULLS clause.
    let q = Query::select()
        .from("t")
        .all()
        .order_by("id", Order::Asc)
        .order_by_nulls("name", Order::Desc, Nulls::Last)
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "t" ORDER BY "id" ASC, "name" DESC NULLS LAST"#
    );
}

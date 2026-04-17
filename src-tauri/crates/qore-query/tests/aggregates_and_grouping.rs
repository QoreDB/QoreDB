// SPDX-License-Identifier: Apache-2.0

//! Semaine 7 coverage: aggregate functions, GROUP BY, HAVING.

use qore_query::ident::tcol;
use qore_query::prelude::*;
use qore_query::query::{Order, Query};

fn pg() -> Dialect {
    Dialect::Postgres
}
fn my() -> Dialect {
    Dialect::MySql
}
fn ms() -> Dialect {
    Dialect::SqlServer
}

// ============================================================================
// Aggregate functions
// ============================================================================

#[test]
fn count_star_renders_as_count_asterisk() {
    let q = Query::select()
        .from("users")
        .select_expr_as(count_all(), "n")
        .build(pg())
        .unwrap();
    assert_eq!(q.sql, r#"SELECT COUNT(*) AS "n" FROM "users""#);
}

#[test]
fn count_column_renders_with_quoted_name() {
    let q = Query::select()
        .from("users")
        .select_expr_as(count(col("email")), "emails")
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT COUNT("email") AS "emails" FROM "users""#
    );
}

#[test]
fn count_distinct_emits_distinct_keyword() {
    let q = Query::select()
        .from("orders")
        .select_expr_as(count_distinct(col("user_id")), "unique_buyers")
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT COUNT(DISTINCT "user_id") AS "unique_buyers" FROM "orders""#
    );
}

#[test]
fn sum_avg_min_max() {
    let q = Query::select()
        .from("orders")
        .select_expr_as(sum(col("total")), "s")
        .select_expr_as(avg(col("total")), "a")
        .select_expr_as(min(col("total")), "mn")
        .select_expr_as(max(col("total")), "mx")
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT SUM("total") AS "s", AVG("total") AS "a", MIN("total") AS "mn", MAX("total") AS "mx" FROM "orders""#
    );
}

#[test]
fn aggregates_dialect_agnostic() {
    // Aggregate function names are standard ANSI SQL — same across
    // all five dialects, only quoting differs.
    let dialects = [pg(), my(), ms(), Dialect::Sqlite, Dialect::DuckDb];
    for d in dialects {
        let q = Query::select()
            .from("t")
            .select_expr(count_all())
            .build(d)
            .unwrap();
        assert!(q.sql.contains("COUNT(*)"), "dialect {:?}", d);
    }
}

// ============================================================================
// GROUP BY
// ============================================================================

#[test]
fn group_by_single_column() {
    let q = Query::select()
        .from("orders")
        .column("user_id")
        .select_expr_as(count_all(), "n")
        .group_by("user_id")
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT "user_id", COUNT(*) AS "n" FROM "orders" GROUP BY "user_id""#
    );
}

#[test]
fn group_by_multiple_columns_in_order() {
    let q = Query::select()
        .from("t")
        .all()
        .group_by("a")
        .group_by("b")
        .group_by("c")
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "t" GROUP BY "a", "b", "c""#
    );
}

#[test]
fn group_by_qualified_column() {
    let q = Query::select()
        .from_as("orders", "o")
        .select_expr(tcol("o", "user_id"))
        .select_expr(count_all())
        .group_by_qualified("o", "user_id")
        .build(pg())
        .unwrap();
    assert!(q.sql.contains(r#"GROUP BY "o"."user_id""#));
}

#[test]
fn group_by_expression() {
    // GROUP BY CAST("created" AS DATE) — a common real-world pattern
    // for aggregating by day.
    let q = Query::select()
        .from("events")
        .select_expr(col("created").cast(SqlType::Date))
        .select_expr(count_all())
        .group_by_expr(col("created").cast(SqlType::Date))
        .build(pg())
        .unwrap();
    assert!(q.sql.contains(r#"GROUP BY CAST("created" AS DATE)"#));
}

#[test]
fn group_by_appears_after_where_and_before_order_by() {
    let q = Query::select()
        .from("orders")
        .column("user_id")
        .select_expr(count_all())
        .filter(col("status").eq("completed"))
        .group_by("user_id")
        .order_by("user_id", Order::Asc)
        .build(pg())
        .unwrap();
    let where_idx = q.sql.find("WHERE").unwrap();
    let group_idx = q.sql.find("GROUP BY").unwrap();
    let order_idx = q.sql.find("ORDER BY").unwrap();
    assert!(where_idx < group_idx);
    assert!(group_idx < order_idx);
}

// ============================================================================
// HAVING
// ============================================================================

#[test]
fn having_combined_with_group_by() {
    let q = Query::select()
        .from("orders")
        .column("user_id")
        .select_expr_as(count_all(), "n")
        .group_by("user_id")
        .having(count_all().gt(5i64))
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT "user_id", COUNT(*) AS "n" FROM "orders" GROUP BY "user_id" HAVING (COUNT(*) > $1)"#
    );
    assert_eq!(q.params.len(), 1);
}

#[test]
fn multiple_having_calls_combine_with_and() {
    let q = Query::select()
        .from("orders")
        .column("user_id")
        .group_by("user_id")
        .having(count_all().gt(5i64))
        .having(sum(col("total")).ge(1000i64))
        .build(pg())
        .unwrap();
    assert!(q.sql.contains("HAVING ((COUNT(*) > $1) AND (SUM(\"total\") >= $2))"));
    assert_eq!(q.params.len(), 2);
}

#[test]
fn having_without_group_by_is_a_compile_error() {
    let err = Query::select()
        .from("t")
        .all()
        .having(count_all().gt(0i64))
        .build(pg())
        .unwrap_err();
    assert!(matches!(err, QueryError::InvalidExpr(_)));
}

#[test]
fn having_appears_after_group_by_and_before_order_by() {
    let q = Query::select()
        .from("t")
        .all()
        .group_by("k")
        .having(count_all().gt(0i64))
        .order_by("k", Order::Asc)
        .build(pg())
        .unwrap();
    let group_idx = q.sql.find("GROUP BY").unwrap();
    let having_idx = q.sql.find("HAVING").unwrap();
    let order_idx = q.sql.find("ORDER BY").unwrap();
    assert!(group_idx < having_idx);
    assert!(having_idx < order_idx);
}

// ============================================================================
// Combined real-world scenario
// ============================================================================

#[test]
fn analytical_query_with_join_group_having_order_limit() {
    // SELECT o.user_id, COUNT(*) AS n, SUM(o.total) AS revenue
    // FROM orders AS o
    // INNER JOIN users AS u ON u.id = o.user_id
    // WHERE u.active = true
    // GROUP BY o.user_id
    // HAVING COUNT(*) > 3
    // ORDER BY revenue DESC
    // LIMIT 20
    let q = Query::select()
        .from_as("orders", "o")
        .select_expr(tcol("o", "user_id"))
        .select_expr_as(count_all(), "n")
        .select_expr_as(sum(tcol("o", "total")), "revenue")
        .inner_join_as("users", "u", tcol("u", "id").eq(tcol("o", "user_id")))
        .filter(tcol("u", "active").eq(true))
        .group_by_qualified("o", "user_id")
        .having(count_all().gt(3i64))
        .order_by("revenue", Order::Desc)
        .limit(20)
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT "o"."user_id", COUNT(*) AS "n", SUM("o"."total") AS "revenue" FROM "orders" AS "o" INNER JOIN "users" AS "u" ON ("u"."id" = "o"."user_id") WHERE ("u"."active" = $1) GROUP BY "o"."user_id" HAVING (COUNT(*) > $2) ORDER BY "revenue" DESC LIMIT 20"#
    );
    assert_eq!(q.params.len(), 2);
}

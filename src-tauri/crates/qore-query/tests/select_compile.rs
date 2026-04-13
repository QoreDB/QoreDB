// SPDX-License-Identifier: Apache-2.0

//! Integration tests for SELECT compilation across all four dialects.
//!
//! Covers Semaine 1-2 scope of the Phase 2 plan:
//! - projection, FROM, WHERE, ORDER BY, LIMIT/OFFSET
//! - AND / OR / NOT composition
//! - IN / NOT IN / BETWEEN
//! - IS NULL / IS NOT NULL
//! - LIKE / ILIKE fallback on non-PG dialects
//! - MSSQL `OFFSET..FETCH NEXT` with required ORDER BY

use qore_core::Value;
use qore_query::prelude::*;
use qore_query::query::{Order, Query};

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

#[test]
fn simple_select_all_postgres() {
    let q = Query::select().from("users").all().build(pg()).unwrap();
    assert_eq!(q.sql, r#"SELECT * FROM "users""#);
    assert!(q.params.is_empty());
}

#[test]
fn select_columns_quoted_per_dialect() {
    let build = |d| {
        Query::select()
            .from("users")
            .columns(["id", "name"])
            .build(d)
            .unwrap()
            .sql
    };
    assert_eq!(build(pg()), r#"SELECT "id", "name" FROM "users""#);
    assert_eq!(build(my()), "SELECT `id`, `name` FROM `users`");
    assert_eq!(build(sl()), r#"SELECT "id", "name" FROM "users""#);
    assert_eq!(build(ms()), "SELECT [id], [name] FROM [users]");
}

#[test]
fn where_eq_is_parameterized_postgres() {
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("age").gt(18))
        .build(pg())
        .unwrap();
    assert_eq!(q.sql, r#"SELECT * FROM "users" WHERE ("age" > $1)"#);
    assert!(matches!(q.params.as_slice(), [Value::Int(18)]));
}

#[test]
fn mysql_and_sqlite_use_question_mark_placeholders() {
    let q_my = Query::select()
        .from("users")
        .all()
        .filter(col("age").gt(18))
        .build(my())
        .unwrap();
    assert_eq!(q_my.sql, "SELECT * FROM `users` WHERE (`age` > ?)");

    let q_sl = Query::select()
        .from("users")
        .all()
        .filter(col("age").gt(18))
        .build(sl())
        .unwrap();
    assert_eq!(q_sl.sql, r#"SELECT * FROM "users" WHERE ("age" > ?)"#);
}

#[test]
fn mssql_uses_at_pn_placeholders() {
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("age").gt(18))
        .build(ms())
        .unwrap();
    assert_eq!(q.sql, "SELECT * FROM [users] WHERE ([age] > @p1)");
}

#[test]
fn multiple_filters_combine_with_and() {
    let q = Query::select()
        .from("users")
        .columns(["id"])
        .filter(col("age").gt(18))
        .filter(col("active").eq(true))
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT "id" FROM "users" WHERE (("age" > $1) AND ("active" = $2))"#
    );
    assert_eq!(q.params.len(), 2);
}

#[test]
fn explicit_and_or_composition_preserves_parentheses() {
    let expr = col("age").gt(18).and(col("role").eq("admin").or(col("vip").eq(true)));
    let q = Query::select()
        .from("users")
        .all()
        .filter(expr)
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "users" WHERE (("age" > $1) AND (("role" = $2) OR ("vip" = $3)))"#
    );
    assert_eq!(q.params.len(), 3);
}

#[test]
fn not_and_null_checks() {
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("email").is_null().or(col("deleted").eq(true).not()))
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "users" WHERE (("email" IS NULL) OR NOT (("deleted" = $1)))"#
    );
}

#[test]
fn in_list_parameterizes_each_value() {
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("status").in_(["active", "pending"]))
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "users" WHERE ("status" IN ($1, $2))"#
    );
    assert_eq!(q.params.len(), 2);
}

#[test]
fn empty_in_list_becomes_always_false() {
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("id").in_(Vec::<i64>::new()))
        .build(pg())
        .unwrap();
    assert_eq!(q.sql, r#"SELECT * FROM "users" WHERE (1 = 0)"#);
    assert!(q.params.is_empty());
}

#[test]
fn not_in_empty_becomes_always_true() {
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("id").not_in(Vec::<i64>::new()))
        .build(pg())
        .unwrap();
    assert_eq!(q.sql, r#"SELECT * FROM "users" WHERE (1 = 1)"#);
}

#[test]
fn between_bounds_are_parameterized_in_order() {
    let q = Query::select()
        .from("events")
        .all()
        .filter(col("ts").between(100i64, 200i64))
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "events" WHERE ("ts" BETWEEN $1 AND $2)"#
    );
    assert!(matches!(
        q.params.as_slice(),
        [Value::Int(100), Value::Int(200)]
    ));
}

#[test]
fn like_is_native_in_all_dialects() {
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("name").like("a%"))
        .build(my())
        .unwrap();
    assert_eq!(q.sql, "SELECT * FROM `users` WHERE (`name` LIKE ?)");
}

#[test]
fn ilike_is_native_on_postgres() {
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("name").ilike("a%"))
        .build(pg())
        .unwrap();
    assert_eq!(q.sql, r#"SELECT * FROM "users" WHERE ("name" ILIKE $1)"#);
}

#[test]
fn ilike_falls_back_to_lower_on_mysql_and_sqlite_and_mssql() {
    let expected = [
        (my(), "SELECT * FROM `users` WHERE (LOWER(`name`) LIKE LOWER(?))"),
        (sl(), r#"SELECT * FROM "users" WHERE (LOWER("name") LIKE LOWER(?))"#),
        (ms(), "SELECT * FROM [users] WHERE (LOWER([name]) LIKE LOWER(@p1))"),
    ];
    for (d, sql) in expected {
        let q = Query::select()
            .from("users")
            .all()
            .filter(col("name").ilike("a%"))
            .build(d)
            .unwrap();
        assert_eq!(q.sql, sql);
    }
}

#[test]
fn order_by_single_column_asc_desc() {
    let q = Query::select()
        .from("users")
        .all()
        .order_by("name", Order::Asc)
        .order_by("created_at", Order::Desc)
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "users" ORDER BY "name" ASC, "created_at" DESC"#
    );
}

#[test]
fn limit_offset_on_postgres_mysql_sqlite() {
    for d in [pg(), my(), sl()] {
        let q = Query::select()
            .from("users")
            .all()
            .limit(10)
            .offset(20)
            .build(d)
            .unwrap();
        assert!(q.sql.ends_with(" LIMIT 10 OFFSET 20"), "got: {}", q.sql);
    }
}

#[test]
fn mssql_limit_requires_order_by() {
    let err = Query::select()
        .from("users")
        .all()
        .limit(10)
        .build(ms())
        .unwrap_err();
    assert!(matches!(err, QueryError::MssqlOffsetRequiresOrderBy));
}

#[test]
fn mssql_emits_offset_fetch_syntax() {
    let q = Query::select()
        .from("users")
        .all()
        .order_by("id", Order::Asc)
        .limit(10)
        .offset(5)
        .build(ms())
        .unwrap();
    assert_eq!(
        q.sql,
        "SELECT * FROM [users] ORDER BY [id] ASC OFFSET 5 ROWS FETCH NEXT 10 ROWS ONLY"
    );
}

#[test]
fn mssql_offset_only_without_fetch() {
    let q = Query::select()
        .from("users")
        .all()
        .order_by("id", Order::Asc)
        .offset(5)
        .build(ms())
        .unwrap();
    assert_eq!(
        q.sql,
        "SELECT * FROM [users] ORDER BY [id] ASC OFFSET 5 ROWS"
    );
}

#[test]
fn placeholder_count_matches_params_vector() {
    // Three literals = three placeholders = three params, in order of appearance.
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("age").gt(18).and(col("name").eq("alice").or(col("vip").eq(true))))
        .build(pg())
        .unwrap();
    let n_placeholders = q.sql.matches('$').count();
    assert_eq!(n_placeholders, q.params.len());
    assert_eq!(n_placeholders, 3);
}

#[test]
fn select_without_from_is_a_user_error() {
    let err = Query::select().all().build(pg()).unwrap_err();
    assert!(matches!(err, QueryError::MissingFrom));
}

#[test]
fn select_without_projection_is_a_user_error() {
    let err = Query::select().from("users").build(pg()).unwrap_err();
    assert!(matches!(err, QueryError::EmptyProjection));
}

#[test]
fn nan_and_infinity_are_rejected() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let err = Query::select()
            .from("t")
            .all()
            .filter(col("x").gt(bad))
            .build(pg())
            .unwrap_err();
        assert!(matches!(err, QueryError::InvalidLiteral(_)));
    }
}

#[test]
fn in_accepts_borrowed_slice() {
    // Regression: `&[T]` yields `&T`, which requires `From<&T> for Value`.
    let ids: &[i64] = &[1, 2, 3];
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("id").in_(ids.iter().copied()))
        .build(pg())
        .unwrap();
    assert_eq!(q.params.len(), 3);

    // Also accept a direct array literal.
    let q2 = Query::select()
        .from("users")
        .all()
        .filter(col("id").in_([10i64, 20, 30]))
        .build(pg())
        .unwrap();
    assert_eq!(q2.params.len(), 3);
}

#[test]
fn null_literal_via_option_none() {
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("email").eq(Option::<&str>::None))
        .build(pg())
        .unwrap();
    // NOTE: `= NULL` never matches in SQL — users should prefer .is_null().
    // We still emit it faithfully: catching this semantic pitfall is the
    // caller's job, not the builder's. Test pins the mechanical behaviour.
    assert_eq!(q.sql, r#"SELECT * FROM "users" WHERE ("email" = $1)"#);
    assert!(matches!(q.params.as_slice(), [Value::Null]));
}

#[test]
fn qualified_column_reference() {
    use qore_query::ident::tcol;
    let q = Query::select()
        .from("users")
        .all()
        .filter(tcol("u", "id").eq(42i64))
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "users" WHERE ("u"."id" = $1)"#
    );
}

#[test]
fn identifier_with_embedded_quote_is_escaped_per_dialect() {
    // Malicious-looking names must be quoted safely (embedded quote character
    // is doubled per ANSI / dialect rules). This is the injection defence for
    // identifiers — it is delegated to qore_sql::SqlDialect::quote_ident.
    let q_pg = Query::select()
        .from(r#"we"ird"#)
        .all()
        .build(pg())
        .unwrap();
    assert_eq!(q_pg.sql, r#"SELECT * FROM "we""ird""#);

    let q_my = Query::select()
        .from("back`tick")
        .all()
        .build(my())
        .unwrap();
    assert_eq!(q_my.sql, "SELECT * FROM `back``tick`");

    let q_ms = Query::select()
        .from("bra]cket")
        .all()
        .build(ms())
        .unwrap();
    assert_eq!(q_ms.sql, "SELECT * FROM [bra]]cket]");
}

#[test]
fn deeply_nested_and_or_preserves_all_parentheses() {
    // Build: (((a=1 AND b=2) OR c=3) AND (d=4 OR (e=5 AND f=6)))
    let expr = col("a")
        .eq(1i64)
        .and(col("b").eq(2i64))
        .or(col("c").eq(3i64))
        .and(col("d").eq(4i64).or(col("e").eq(5i64).and(col("f").eq(6i64))));
    let q = Query::select()
        .from("t")
        .all()
        .filter(expr)
        .build(pg())
        .unwrap();
    // Every binary node is wrapped — no ambiguity regardless of reader's
    // assumed precedence.
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "t" WHERE (((("a" = $1) AND ("b" = $2)) OR ("c" = $3)) AND (("d" = $4) OR (("e" = $5) AND ("f" = $6))))"#
    );
    assert_eq!(q.params.len(), 6);
}

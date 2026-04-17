// SPDX-License-Identifier: Apache-2.0

//! Semaine 5 coverage: text search helpers with wildcard escaping and
//! N-ary combinators.

use qore_core::Value;
use qore_query::prelude::*;
use qore_query::query::Query;

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
// Text search helpers - wildcards are pre-escaped in the bound value
// ============================================================================

#[test]
fn starts_with_wraps_with_percent_suffix_and_escape() {
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("name").starts_with("ali"))
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "users" WHERE ("name" LIKE $1 ESCAPE '\')"#
    );
    assert!(matches!(
        q.params.as_slice(),
        [Value::Text(s)] if s == "ali%"
    ));
}

#[test]
fn ends_with_prefixes_with_percent() {
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("name").ends_with("son"))
        .build(pg())
        .unwrap();
    assert!(matches!(
        q.params.as_slice(),
        [Value::Text(s)] if s == "%son"
    ));
}

#[test]
fn contains_wraps_both_sides() {
    let q = Query::select()
        .from("users")
        .all()
        .filter(col("name").contains("bob"))
        .build(pg())
        .unwrap();
    assert!(matches!(
        q.params.as_slice(),
        [Value::Text(s)] if s == "%bob%"
    ));
}

#[test]
fn user_wildcards_in_input_are_neutralised() {
    // User search for literal "50%" must NOT match "500 things" etc.
    // The `%` in input is escaped so it matches literally.
    let q = Query::select()
        .from("offers")
        .all()
        .filter(col("label").contains("50%"))
        .build(pg())
        .unwrap();
    assert!(matches!(
        q.params.as_slice(),
        [Value::Text(s)] if s == "%50\\%%"
    ));
    // The ESCAPE clause is what makes \\% literal.
    assert!(q.sql.contains("ESCAPE '\\'"));
}

#[test]
fn underscore_in_input_is_neutralised() {
    // `_` matches a single char in LIKE — must be escaped for literal search.
    let q = Query::select()
        .from("t")
        .all()
        .filter(col("x").starts_with("a_b"))
        .build(pg())
        .unwrap();
    assert!(matches!(
        q.params.as_slice(),
        [Value::Text(s)] if s == "a\\_b%"
    ));
}

#[test]
fn backslash_in_input_is_neutralised() {
    // The escape character itself must also be escaped.
    let q = Query::select()
        .from("t")
        .all()
        .filter(col("x").contains("a\\b"))
        .build(pg())
        .unwrap();
    assert!(matches!(
        q.params.as_slice(),
        [Value::Text(s)] if s == "%a\\\\b%"
    ));
}

#[test]
fn text_helpers_work_on_all_dialects() {
    let expected = [
        (pg(), r#"WHERE ("name" LIKE $1 ESCAPE '\')"#),
        (my(), "WHERE (`name` LIKE ? ESCAPE '\\')"),
        (sl(), r#"WHERE ("name" LIKE ? ESCAPE '\')"#),
        (ms(), "WHERE ([name] LIKE @p1 ESCAPE '\\')"),
        (dd(), r#"WHERE ("name" LIKE ? ESCAPE '\')"#),
    ];
    for (d, fragment) in expected {
        let q = Query::select()
            .from("users")
            .all()
            .filter(col("name").starts_with("al"))
            .build(d)
            .unwrap();
        assert!(
            q.sql.contains(fragment),
            "dialect {:?}: expected {:?} in {:?}",
            d,
            fragment,
            q.sql
        );
    }
}

// ============================================================================
// ILIKE unchanged by the refactor — regression guard
// ============================================================================

#[test]
fn plain_like_still_has_no_escape_clause() {
    // `.like(pattern)` is a low-level pass-through; users opt into escaping
    // by calling `.starts_with`/etc. instead.
    let q = Query::select()
        .from("t")
        .all()
        .filter(col("x").like("raw%pattern"))
        .build(pg())
        .unwrap();
    assert_eq!(q.sql, r#"SELECT * FROM "t" WHERE ("x" LIKE $1)"#);
}

#[test]
fn plain_ilike_still_falls_back_to_lower_on_mysql() {
    let q = Query::select()
        .from("t")
        .all()
        .filter(col("x").ilike("foo%"))
        .build(my())
        .unwrap();
    assert_eq!(q.sql, "SELECT * FROM `t` WHERE (LOWER(`x`) LIKE LOWER(?))");
}

// ============================================================================
// N-ary combinators
// ============================================================================

#[test]
fn and_all_folds_expressions_left_to_right() {
    let parts = vec![col("a").eq(1i64), col("b").eq(2i64), col("c").eq(3i64)];
    let combined = Expr::and_all(parts).unwrap();
    let q = Query::select()
        .from("t")
        .all()
        .filter(combined)
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "t" WHERE ((("a" = $1) AND ("b" = $2)) AND ("c" = $3))"#
    );
}

#[test]
fn or_any_folds_expressions_left_to_right() {
    let parts = [col("a").eq(1i64), col("b").eq(2i64)];
    let combined = Expr::or_any(parts).unwrap();
    let q = Query::select()
        .from("t")
        .all()
        .filter(combined)
        .build(pg())
        .unwrap();
    assert_eq!(
        q.sql,
        r#"SELECT * FROM "t" WHERE (("a" = $1) OR ("b" = $2))"#
    );
}

#[test]
fn and_all_with_single_element_returns_that_element_alone() {
    let combined = Expr::and_all([col("a").eq(1i64)]).unwrap();
    let q = Query::select()
        .from("t")
        .all()
        .filter(combined)
        .build(pg())
        .unwrap();
    assert_eq!(q.sql, r#"SELECT * FROM "t" WHERE ("a" = $1)"#);
}

#[test]
fn and_all_with_empty_iter_returns_none() {
    let combined: Option<Expr> = Expr::and_all(std::iter::empty());
    assert!(combined.is_none());
}

#[test]
fn or_any_with_empty_iter_returns_none() {
    let combined: Option<Expr> = Expr::or_any(std::iter::empty());
    assert!(combined.is_none());
}

#[test]
fn dynamic_filter_composition_pattern() {
    // Real use case: conditionally build filters and combine.
    let name_filter: Option<&str> = Some("bob");
    let min_age: Option<i64> = Some(18);
    let only_active: bool = true;

    let mut filters: Vec<Expr> = Vec::new();
    if let Some(n) = name_filter {
        filters.push(col("name").contains(n));
    }
    if let Some(a) = min_age {
        filters.push(col("age").ge(a));
    }
    if only_active {
        filters.push(col("active").eq(true));
    }

    let mut q = Query::select().from("users").all();
    if let Some(combined) = Expr::and_all(filters) {
        q = q.filter(combined);
    }
    let built = q.build(pg()).unwrap();
    assert_eq!(built.params.len(), 3); // name pattern, age, active
    assert!(built.sql.contains("LIKE"));
    assert!(built.sql.contains("ESCAPE"));
}

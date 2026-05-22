// SPDX-License-Identifier: BUSL-1.1

//! Referential integrity rule: `foreign_key_integrity`.
//!
//! Counts rows in the source table whose FK value has no match in the
//! referenced table. NULL FK values are skipped (a missing FK is not a
//! referential violation per SQL semantics — use `not_null_pct` if you
//! want to require presence).

use super::{dialect::Dialect, RuleSql, RuleSqlKind, SqlBuildError};

pub fn build_foreign_key_integrity(
    dialect: Dialect,
    source_table_sql: &str,
    source_column_sql: &str,
    ref_table_sql: &str,
    ref_column_sql: &str,
    sample_limit: u32,
) -> Result<RuleSql, SqlBuildError> {
    let exists_sub = format!(
        "SELECT 1 FROM {ref_table_sql} ref WHERE ref.{ref_column_sql} = src.{source_column_sql}"
    );
    let predicate = format!("(src.{source_column_sql} IS NOT NULL AND NOT EXISTS ({exists_sub}))");
    let metric_query = format!(
        "SELECT (SELECT count(*) FROM {source_table_sql} src WHERE {predicate}) AS violations, \
         (SELECT count(*) FROM {source_table_sql}) AS total"
    );
    let samples_query = Some(format!(
        "SELECT src.* FROM {source_table_sql} src WHERE {predicate} LIMIT {sample_limit}"
    ));
    let _ = dialect;
    Ok(RuleSql {
        kind: RuleSqlKind::ViolationsCount,
        metric_query,
        samples_query,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fk_emits_not_exists() {
        let r = build_foreign_key_integrity(
            Dialect::Postgres,
            "\"public\".\"orders\"",
            "\"customer_id\"",
            "\"public\".\"customers\"",
            "\"id\"",
            10,
        )
        .unwrap();
        assert!(r.metric_query.contains("NOT EXISTS"));
        assert!(r.metric_query.contains("ref.\"id\" = src.\"customer_id\""));
        assert!(r.metric_query.contains("src.\"customer_id\" IS NOT NULL"));
    }
}

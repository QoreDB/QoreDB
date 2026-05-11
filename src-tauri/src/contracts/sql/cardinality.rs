// SPDX-License-Identifier: BUSL-1.1

//! Cardinality rule: `row_count`.

use super::{dialect::Dialect, RuleSql, RuleSqlKind, SqlBuildError};

pub fn build_row_count(_dialect: Dialect, table_sql: &str) -> Result<RuleSql, SqlBuildError> {
    let metric_query = format!("SELECT count(*) AS metric_value FROM {table_sql}");
    Ok(RuleSql {
        kind: RuleSqlKind::SingleMetric,
        metric_query,
        samples_query: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_count_basic() {
        let r = build_row_count(Dialect::Postgres, "\"public\".\"orders\"").unwrap();
        assert_eq!(
            r.metric_query,
            "SELECT count(*) AS metric_value FROM \"public\".\"orders\""
        );
        assert!(matches!(r.kind, RuleSqlKind::SingleMetric));
    }
}

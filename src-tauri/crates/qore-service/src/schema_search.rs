// SPDX-License-Identifier: Apache-2.0

//! One-round-trip column listings per engine family, so a schema search does
//! not describe every table one by one. Names are filtered on our side: the
//! only user-controlled text that reaches the engine is the namespace, as a
//! quoted literal.

use qore_core::Namespace;

pub fn sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

/// Query returning `(table, column, type)` rows for the whole namespace, or
/// `None` for engines without a cheap catalogue (the caller falls back to
/// describing tables one by one).
pub fn columns_query(driver_id: &str, namespace: &Namespace) -> Option<String> {
    let schema = namespace.schema.as_deref().map(sql_literal);
    let database = sql_literal(&namespace.database);
    let query = match driver_id.to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" | "cockroachdb" | "yugabytedb" | "timescaledb" | "neon"
        | "supabase" => format!(
            "SELECT table_name, column_name, data_type FROM information_schema.columns \
             WHERE table_schema = {} ORDER BY table_name, ordinal_position",
            schema.unwrap_or_else(|| "'public'".to_string())
        ),
        "mysql" | "mariadb" | "tidb" | "starrocks" | "doris" | "singlestore" | "planetscale" => {
            format!(
                "SELECT table_name, column_name, data_type FROM information_schema.columns \
                 WHERE table_schema = {database} ORDER BY table_name, ordinal_position"
            )
        }
        "sqlserver" | "azuresql" | "synapse" => format!(
            "SELECT table_name, column_name, data_type FROM information_schema.columns \
             WHERE table_schema = {} ORDER BY table_name, ordinal_position",
            schema.unwrap_or_else(|| "'dbo'".to_string())
        ),
        "snowflake" => format!(
            "SELECT table_name, column_name, data_type FROM information_schema.columns \
             WHERE table_catalog = {database} AND table_schema = {} \
             ORDER BY table_name, ordinal_position",
            schema?
        ),
        "duckdb" | "motherduck" => format!(
            "SELECT table_name, column_name, data_type FROM information_schema.columns \
             WHERE table_schema = {} ORDER BY table_name, ordinal_position",
            schema.unwrap_or_else(|| "current_schema()".to_string())
        ),
        "sqlite" => "SELECT m.name, p.name, p.type FROM sqlite_master m \
             JOIN pragma_table_info(m.name) p \
             WHERE m.type IN ('table', 'view') AND m.name NOT LIKE 'sqlite_%' \
             ORDER BY m.name, p.cid"
            .to_string(),
        "clickhouse" => format!(
            "SELECT table, name, type FROM system.columns WHERE database = {database} \
             ORDER BY table, position"
        ),
        "cassandra" | "scylladb" | "keyspaces" => format!(
            "SELECT table_name, column_name, type FROM system_schema.columns \
             WHERE keyspace_name = {database}"
        ),
        _ => return None,
    };
    Some(query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_are_quoted() {
        assert_eq!(sql_literal("it's"), "'it''s'");
        assert_eq!(sql_literal("a\\b"), "'a\\\\b'");
    }

    #[test]
    fn families_get_a_catalogue_query_or_none() {
        let ns = Namespace::new("shop");
        assert!(columns_query("postgres", &ns).unwrap().contains("'public'"));
        assert!(
            columns_query("mysql", &ns)
                .unwrap()
                .contains("table_schema = 'shop'")
        );
        assert!(
            columns_query("sqlite", &ns)
                .unwrap()
                .contains("pragma_table_info")
        );
        assert!(
            columns_query("cassandra", &ns)
                .unwrap()
                .contains("keyspace_name = 'shop'")
        );
        assert!(columns_query("snowflake", &ns).is_none());
        assert!(columns_query("mongodb", &ns).is_none());

        let scoped = Namespace::with_schema("shop", "sales");
        assert!(
            columns_query("postgres", &scoped)
                .unwrap()
                .contains("'sales'")
        );
        assert!(
            columns_query("snowflake", &scoped)
                .unwrap()
                .contains("'sales'")
        );
    }
}

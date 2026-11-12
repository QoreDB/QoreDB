// SPDX-License-Identifier: Apache-2.0

use super::*;
use qore_core::types::{Hypertable, HypertableDimension};

const HYPERTABLES_SQL: &str = r#"
    SELECT h.hypertable_name::text,
           h.num_chunks::bigint,
           COALESCE(h.compression_enabled, false),
           (SELECT json_agg(json_build_object(
                       'column_name', d.column_name::text,
                       'dimension_type', d.dimension_type::text,
                       'time_interval', d.time_interval::text,
                       'integer_interval', d.integer_interval,
                       'num_partitions', d.num_partitions)
                   ORDER BY d.dimension_number)::text
              FROM timescaledb_information.dimensions d
             WHERE d.hypertable_schema = h.hypertable_schema
               AND d.hypertable_name = h.hypertable_name),
           (SELECT j.config->>'compress_after'
              FROM timescaledb_information.jobs j
             WHERE j.proc_name = 'policy_compression'
               AND j.hypertable_schema = h.hypertable_schema
               AND j.hypertable_name = h.hypertable_name
             LIMIT 1),
           (SELECT j.config->>'drop_after'
              FROM timescaledb_information.jobs j
             WHERE j.proc_name = 'policy_retention'
               AND j.hypertable_schema = h.hypertable_schema
               AND j.hypertable_name = h.hypertable_name
             LIMIT 1)
      FROM timescaledb_information.hypertables h
     WHERE h.hypertable_schema = $1
     ORDER BY h.hypertable_name
"#;

type HypertableRow = (
    String,
    Option<i64>,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
);

pub async fn list_hypertables(
    sessions: &SessionMap,
    session: SessionId,
    namespace: &Namespace,
) -> EngineResult<Vec<Hypertable>> {
    let pg = get_session(sessions, session).await?;
    let pool = &pg.pool;
    let installed: Option<(i32,)> =
        sqlx::query_as("SELECT 1 FROM pg_extension WHERE extname = 'timescaledb'")
            .fetch_optional(pool)
            .await
            .map_err(|e| EngineError::execution_error(e.to_string()))?;
    if installed.is_none() {
        return Ok(Vec::new());
    }

    let schema = namespace.schema.as_deref().unwrap_or("public");
    let rows: Vec<HypertableRow> = sqlx::query_as(HYPERTABLES_SQL)
        .bind(schema)
        .fetch_all(pool)
        .await
        .map_err(|e| EngineError::execution_error(e.to_string()))?;

    rows.into_iter().map(hypertable_from_row).collect()
}

fn hypertable_from_row(row: HypertableRow) -> EngineResult<Hypertable> {
    let (name, num_chunks, compression_enabled, dimensions, compress_after, drop_after) = row;
    Ok(Hypertable {
        name,
        num_chunks,
        compression_enabled,
        dimensions: parse_dimensions(dimensions.as_deref())?,
        compress_after,
        drop_after,
    })
}

fn parse_dimensions(raw: Option<&str>) -> EngineResult<Vec<HypertableDimension>> {
    match raw {
        None => Ok(Vec::new()),
        Some(json) => serde_json::from_str(json).map_err(|e| {
            EngineError::execution_error(format!("Invalid TimescaleDB dimension metadata: {e}"))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_hypertable_with_time_and_space_dimensions_and_policies() {
        let dims = r#"[
            {"column_name":"time","dimension_type":"Time","time_interval":"7 days",
             "integer_interval":null,"num_partitions":null},
            {"column_name":"device_id","dimension_type":"Space","time_interval":null,
             "integer_interval":null,"num_partitions":4}
        ]"#;
        let table = hypertable_from_row((
            "conditions".into(),
            Some(12),
            true,
            Some(dims.into()),
            Some("7 days".into()),
            Some("90 days".into()),
        ))
        .unwrap();
        assert_eq!(table.name, "conditions");
        assert_eq!(table.dimensions.len(), 2);
        assert_eq!(table.dimensions[0].time_interval.as_deref(), Some("7 days"));
        assert_eq!(table.dimensions[1].num_partitions, Some(4));
        assert_eq!(table.compress_after.as_deref(), Some("7 days"));
        assert_eq!(table.drop_after.as_deref(), Some("90 days"));
    }

    #[test]
    fn missing_dimensions_yield_an_empty_list() {
        assert!(parse_dimensions(None).unwrap().is_empty());
        assert!(parse_dimensions(Some("not json")).is_err());
    }
}

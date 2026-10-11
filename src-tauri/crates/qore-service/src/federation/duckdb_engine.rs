// SPDX-License-Identifier: BUSL-1.1

//! DuckDB in-memory engine wrapper for federation queries.
//!
//! Each federation query creates a fresh DuckDB connection, loads source data
//! into temporary tables, executes the rewritten query, and returns results.
//! The connection is dropped after use, freeing all memory.

use std::time::Instant;

use duckdb::{Connection, params_from_iter, types::Value as DuckValue};

use qore_core::error::{EngineError, EngineResult};
use qore_core::types::{ColumnInfo, QueryResult, Row, Value};
use qore_drivers::drivers::duckdb::{duckdb_owned_value_to_qoredb, logical_type_name};

/// Batch size for inserting rows into DuckDB temp tables.
const INSERT_BATCH_SIZE: usize = 1000;

/// Wraps an in-memory DuckDB connection for a single federation query.
pub struct DuckDbEngine {
    conn: Connection,
}

impl DuckDbEngine {
    /// Creates a new in-memory DuckDB instance.
    pub fn new() -> EngineResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| EngineError::internal(format!("Failed to open DuckDB: {e}")))?;
        Ok(Self { conn })
    }

    /// Once the sources are loaded, the query may only read the temp tables:
    /// no local files, URLs or extensions, and no way to turn that back on.
    pub fn disable_external_access(&self) -> EngineResult<()> {
        self.conn
            .execute_batch("SET enable_external_access = false; SET lock_configuration = true;")
            .map_err(|e| EngineError::internal(format!("Failed to restrict DuckDB: {e}")))
    }

    /// Creates a temporary table with the given schema.
    pub fn create_temp_table(
        &self,
        name: &str,
        columns: &[ColumnInfo],
        rows: &[Row],
    ) -> EngineResult<()> {
        if columns.is_empty() {
            return Err(EngineError::validation(format!(
                "Cannot create temp table '{name}': no columns"
            )));
        }

        let col_defs: Vec<String> = columns
            .iter()
            .enumerate()
            .map(|(index, c)| {
                let duck_type = if c.masked {
                    "VARCHAR".to_string()
                } else if is_exact_decimal(&c.data_type) {
                    exact_decimal_type(c, index, rows)?
                } else {
                    map_type_to_duckdb(&c.data_type).to_string()
                };
                Ok(format!("\"{}\" {}", c.name.replace('"', "\"\""), duck_type))
            })
            .collect::<EngineResult<_>>()?;

        let sql = format!(
            "CREATE TEMP TABLE \"{}\" ({})",
            name.replace('"', "\"\""),
            col_defs.join(", ")
        );

        self.conn.execute_batch(&sql).map_err(|e| {
            EngineError::internal(format!("Failed to create temp table '{name}': {e}"))
        })?;

        Ok(())
    }

    /// Inserts rows into a temp table in batches.
    pub fn insert_batch(
        &self,
        table: &str,
        rows: &[Row],
        columns: &[ColumnInfo],
    ) -> EngineResult<()> {
        if rows.is_empty() || columns.is_empty() {
            return Ok(());
        }

        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "INSERT INTO \"{}\" VALUES ({})",
            table.replace('"', "\"\""),
            placeholders.join(", ")
        );

        for chunk in rows.chunks(INSERT_BATCH_SIZE) {
            let tx = self
                .conn
                .unchecked_transaction()
                .map_err(|e| EngineError::internal(format!("DuckDB transaction failed: {e}")))?;

            {
                let mut stmt = tx
                    .prepare_cached(&sql)
                    .map_err(|e| EngineError::internal(format!("DuckDB prepare failed: {e}")))?;

                for row in chunk {
                    let duck_values: Vec<DuckValue> =
                        row.values.iter().map(value_to_duckdb).collect();

                    stmt.execute(params_from_iter(duck_values.iter()))
                        .map_err(|e| EngineError::internal(format!("DuckDB insert failed: {e}")))?;
                }
            }

            tx.commit()
                .map_err(|e| EngineError::internal(format!("DuckDB commit failed: {e}")))?;
        }

        Ok(())
    }

    /// Executes a query and returns a `QueryResult`.
    pub fn execute_query(&self, sql: &str) -> EngineResult<QueryResult> {
        let start = Instant::now();
        let (columns, rows) = self.execute_and_collect(sql)?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;

        Ok(QueryResult {
            columns,
            rows,
            affected_rows: None,
            execution_time_ms: elapsed,
        })
    }

    /// Executes a query and returns columns + rows for streaming.
    ///
    /// This is synchronous because DuckDB types are not `Send`/`Sync`.
    /// The caller is responsible for sending results through the stream channel.
    pub fn execute_query_for_stream(&self, sql: &str) -> EngineResult<(Vec<ColumnInfo>, Vec<Row>)> {
        self.execute_and_collect(sql)
    }

    /// Shared implementation: uses `conn.prepare` + `stmt.query` to execute,
    /// then reads column metadata and rows from the executed statement.
    ///
    /// In DuckDB 1.4+, `column_count()` and `column_name()` both require
    /// the statement to have been executed first (they access `result_unwrap()`).
    fn execute_and_collect(&self, sql: &str) -> EngineResult<(Vec<ColumnInfo>, Vec<Row>)> {
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(|e| EngineError::execution_error(format!("Federation query failed: {e}")))?;

        // DuckDB 1.4+ panics on column_count/column_name if the statement has not run yet.
        stmt.execute([])
            .map_err(|e| EngineError::execution_error(format!("Federation query failed: {e}")))?;

        let column_count = stmt.column_count();
        let columns: Vec<ColumnInfo> = (0..column_count)
            .map(|i| ColumnInfo {
                name: stmt
                    .column_name(i)
                    .map(|s| s.into())
                    .unwrap_or_else(|_| format!("col_{i}").into()),
                data_type: logical_type_name(&stmt.column_logical_type(i)).into(),
                nullable: true,
                masked: false,
            })
            .collect();

        let rows_iter = stmt
            .query_map([], |row| {
                let values: Vec<Value> = (0..column_count)
                    .map(|i| {
                        row.get_ref(i)
                            .map(|value| duckdb_owned_value_to_qoredb(value.to_owned()))
                    })
                    .collect::<duckdb::Result<_>>()?;
                Ok(Row { values })
            })
            .map_err(|e| EngineError::execution_error(format!("Federation query failed: {e}")))?;

        let mut rows = Vec::new();
        for row_result in rows_iter {
            let row = row_result
                .map_err(|e| EngineError::execution_error(format!("Row fetch failed: {e}")))?;
            rows.push(row);
        }

        Ok((columns, rows))
    }
}

fn is_exact_decimal(data_type: &str) -> bool {
    let lower = data_type.trim().to_ascii_lowercase();
    matches!(
        lower.split('(').next().unwrap_or("").trim(),
        "numeric" | "decimal" | "money"
    )
}

/// Infer from the complete fetched column, including scale differences between
/// rows. DuckDB's Rust decoder uses a 96-bit decimal, so cap precision at 28
/// digits rather than rounding into DOUBLE or risking a decoder overflow.
fn exact_decimal_type(column: &ColumnInfo, index: usize, rows: &[Row]) -> EngineResult<String> {
    let mut integer_digits = 0_i128;
    let mut scale = 0_i128;
    for row in rows {
        let value = row
            .values
            .get(index)
            .ok_or_else(|| EngineError::validation("Federation row has missing columns"))?;
        let text = match value {
            Value::Null => continue,
            Value::Text(text) => text.clone(),
            Value::Int(value) => value.to_string(),
            _ => {
                return Err(EngineError::validation(format!(
                    "Federation column '{}' requires an exact decimal value",
                    column.name
                )));
            }
        };
        let decimal = text.parse::<bigdecimal::BigDecimal>().map_err(|_| {
            EngineError::validation(format!(
                "Federation column '{}' contains an unsupported decimal",
                column.name
            ))
        })?;
        let (_, exponent) = decimal.as_bigint_and_exponent();
        integer_digits =
            integer_digits.max((i128::from(decimal.digits()) - i128::from(exponent)).max(0));
        scale = scale.max(i128::from(exponent).max(0));
    }
    let precision = (integer_digits + scale).max(1);
    if precision > 28 {
        return Err(EngineError::validation(format!(
            "Federation column '{}' exceeds the supported exact decimal precision of 28 digits; no rounded result was produced",
            column.name
        )));
    }
    Ok(format!("DECIMAL({precision},{scale})"))
}

/// Maps a source database type string to a DuckDB type.
fn map_type_to_duckdb(data_type: &str) -> &'static str {
    let lower = data_type.to_lowercase();
    let normalized = lower.trim();

    if normalized.ends_with("[]") || normalized.starts_with("array") {
        return "VARCHAR"; // Serialize arrays as JSON strings.
    }

    match normalized {
        // Boolean
        "boolean" | "bool" => "BOOLEAN",

        // Integer types
        "smallint" | "int2" | "smallserial" | "serial2" | "tinyint" => "SMALLINT",
        "integer" | "int" | "int4" | "serial" | "serial4" | "mediumint" => "INTEGER",
        "bigint" | "int8" | "bigserial" | "serial8" => "BIGINT",

        // Float types
        "real" | "float4" | "float" => "FLOAT",
        "double precision" | "double" | "float8" => "DOUBLE",

        // String types
        "text" | "character varying" | "varchar" | "char" | "character" | "bpchar" | "citext"
        | "name" | "longtext" | "mediumtext" | "tinytext" | "enum" | "set" => "VARCHAR",

        // Date/Time
        "timestamp" | "timestamp without time zone" | "datetime" => "TIMESTAMP",
        "timestamp with time zone" | "timestamptz" => "TIMESTAMPTZ",
        "date" => "DATE",
        "time" | "time without time zone" => "TIME",
        "time with time zone" | "timetz" => "VARCHAR", // DuckDB has limited TIMETZ support
        "interval" => "INTERVAL",

        // Binary
        "bytea" | "blob" | "binary" | "varbinary" | "longblob" | "mediumblob" | "tinyblob" => {
            "BLOB"
        }

        // JSON
        "json" | "jsonb" => "VARCHAR",

        // UUID
        "uuid" => "VARCHAR",

        // Network types (PostgreSQL)
        "inet" | "cidr" | "macaddr" | "macaddr8" => "VARCHAR",

        // Geometric types
        "point" | "line" | "lseg" | "box" | "path" | "polygon" | "circle" => "VARCHAR",

        // Other
        "xml" | "tsvector" | "tsquery" | "bit" | "bit varying" | "varbit" => "VARCHAR",

        _ => {
            // Parameterized fallbacks: varchar(255), numeric(10,2), timestamp(6), etc.
            if normalized.starts_with("varchar")
                || normalized.starts_with("character varying")
                || normalized.starts_with("char")
                || normalized.starts_with("character")
            {
                return "VARCHAR";
            }
            if normalized.starts_with("timestamp") {
                return "TIMESTAMP";
            }
            if normalized.starts_with("time") {
                return "VARCHAR";
            }
            if normalized.starts_with("bit") {
                return "VARCHAR";
            }
            "VARCHAR"
        }
    }
}

/// Converts a QoreDB `Value` to a DuckDB `Value`.
fn value_to_duckdb(value: &Value) -> DuckValue {
    match value {
        Value::Null => DuckValue::Null,
        Value::Bool(b) => DuckValue::Boolean(*b),
        Value::Int(i) => DuckValue::BigInt(*i),
        Value::Float(f) => DuckValue::Double(*f),
        Value::Text(s) => DuckValue::Text(s.clone()),
        Value::Bytes(b) => DuckValue::Blob(b.clone()),
        Value::Json(j) => DuckValue::Text(j.to_string()),
        Value::Array(arr) => DuckValue::Text(serde_json::to_string(arr).unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn federation_preserves_result_types_and_exact_values() {
        let engine = DuckDbEngine::new().unwrap();
        let result = engine.execute_query("SELECT '00123' AS code, 1.25::DOUBLE AS fraction, 123.45::DECIMAL(10,2) AS amount, 18446744073709551615::UBIGINT AS large, DATE '2026-10-11' AS day, NULL AS empty").unwrap();
        assert_eq!(
            serde_json::to_value(&result.rows[0].values).unwrap(),
            serde_json::to_value(vec![
                Value::Text("00123".into()),
                Value::Float(1.25),
                Value::Text("123.45".into()),
                Value::Text("18446744073709551615".into()),
                Value::Text("2026-10-11".into()),
                Value::Null,
            ])
            .unwrap()
        );
        assert_eq!(result.columns[1].data_type.as_str(), "DOUBLE");
        let (_, rows) = engine
            .execute_query_for_stream("SELECT '00123', 1.25::DOUBLE")
            .unwrap();
        assert_eq!(
            serde_json::to_value(&rows[0].values).unwrap(),
            serde_json::to_value(vec![Value::Text("00123".into()), Value::Float(1.25)]).unwrap()
        );
    }

    #[test]
    fn federation_quotes_source_identifiers() {
        let engine = DuckDbEngine::new().unwrap();
        let columns = vec![ColumnInfo {
            name: "a\"b".into(),
            data_type: "text".into(),
            nullable: true,
            masked: false,
        }];
        engine.create_temp_table("t\"x", &columns, &[]).unwrap();
        engine
            .insert_batch(
                "t\"x",
                &[Row {
                    values: vec![Value::Text("00123".into())],
                }],
                &columns,
            )
            .unwrap();
        let result = engine
            .execute_query("SELECT \"a\"\"b\" FROM \"t\"\"x\"")
            .unwrap();
        assert!(matches!(&result.rows[0].values[0], Value::Text(value) if value == "00123"));
    }

    #[test]
    fn federation_does_not_round_decimal_sources() {
        let engine = DuckDbEngine::new().unwrap();
        let columns = vec![ColumnInfo {
            name: "amount".into(),
            data_type: "numeric".into(),
            nullable: true,
            masked: false,
        }];
        let rows = vec![Row {
            values: vec![Value::Text("9007199254740993.123456789".into())],
        }];
        engine
            .create_temp_table("amounts", &columns, &rows)
            .unwrap();
        engine.insert_batch("amounts", &rows, &columns).unwrap();
        let result = engine.execute_query("SELECT amount FROM amounts").unwrap();
        assert!(
            matches!(&result.rows[0].values[0], Value::Text(value) if value == "9007199254740993.123456789")
        );
    }

    #[test]
    fn federation_decimal_schema_handles_scale_and_rejects_unsupported_precision() {
        let column = ColumnInfo {
            name: "amount".into(),
            data_type: "DECIMAL(30,10)".into(),
            nullable: true,
            masked: false,
        };
        let rows = ["1e3", "0.0001", "12.3400"].map(|value| Row {
            values: vec![Value::Text(value.into())],
        });
        assert_eq!(
            exact_decimal_type(&column, 0, &rows).unwrap(),
            "DECIMAL(8,4)"
        );
        let engine = DuckDbEngine::new().unwrap();
        engine
            .create_temp_table("scaled", &[column.clone()], &rows)
            .unwrap();
        engine
            .insert_batch("scaled", &rows, &[column.clone()])
            .unwrap();
        assert!(
            matches!(&engine.execute_query("SELECT SUM(amount) FROM scaled").unwrap().rows[0].values[0], Value::Text(value) if value == "1012.3401")
        );
        for value in [
            "12345678901234567890123456789",
            "1e100",
            "0.00000000000000000000000000001",
            "NaN",
            "invalid",
        ] {
            let rows = [Row {
                values: vec![Value::Text(value.into())],
            }];
            assert!(
                engine
                    .create_temp_table("invalid", &[column.clone()], &rows)
                    .is_err(),
                "{value}"
            );
        }
        let mut masked = column;
        masked.masked = true;
        let rows = [Row {
            values: vec![Value::Text("••••••".into())],
        }];
        engine
            .create_temp_table("masked", &[masked.clone()], &rows)
            .unwrap();
        engine.insert_batch("masked", &rows, &[masked]).unwrap();
        assert!(
            matches!(&engine.execute_query("SELECT * FROM masked").unwrap().rows[0].values[0], Value::Text(value) if value == "••••••")
        );
    }

    #[test]
    fn restricted_engine_reads_temp_tables_but_not_files() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("secret.csv");
        std::fs::write(&csv, "a\n1\n").unwrap();
        let read_file = format!("SELECT * FROM read_csv('{}')", csv.display());

        let engine = DuckDbEngine::new().unwrap();
        let columns = vec![ColumnInfo {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            masked: false,
        }];
        engine.create_temp_table("t", &columns, &[]).unwrap();
        engine
            .insert_batch(
                "t",
                &[Row {
                    values: vec![Value::Int(7)],
                }],
                &columns,
            )
            .unwrap();
        assert!(engine.execute_query(&read_file).is_ok());

        engine.disable_external_access().unwrap();
        assert_eq!(
            engine.execute_query("SELECT id FROM t").unwrap().rows.len(),
            1
        );
        assert!(engine.execute_query(&read_file).is_err());
        assert!(
            engine
                .execute_query("SET enable_external_access = true")
                .is_err()
        );
    }
}

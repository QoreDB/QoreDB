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
    pub fn create_temp_table(&self, name: &str, columns: &[ColumnInfo]) -> EngineResult<()> {
        if columns.is_empty() {
            return Err(EngineError::validation(format!(
                "Cannot create temp table '{name}': no columns"
            )));
        }

        let col_defs: Vec<String> = columns
            .iter()
            .map(|c| {
                let duck_type = map_type_to_duckdb(&c.data_type);
                format!("\"{}\" {}", c.name, duck_type)
            })
            .collect();

        let sql = format!("CREATE TEMP TABLE \"{}\" ({})", name, col_defs.join(", "));

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
            table,
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
                data_type: "VARCHAR".into(),
                nullable: true,
                masked: false,
            })
            .collect();

        let rows_iter = stmt
            .query_map([], |row| {
                let values: Vec<Value> = (0..column_count)
                    .map(|i| duckdb_value_to_qoredb(row, i))
                    .collect();
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
        "numeric" | "decimal" | "money" => "DOUBLE",

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
            if normalized.starts_with("numeric") || normalized.starts_with("decimal") {
                return "DOUBLE";
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

/// Extracts a value from a DuckDB row and converts it to a QoreDB `Value`.
fn duckdb_value_to_qoredb(row: &duckdb::Row<'_>, idx: usize) -> Value {
    if let Ok(v) = row.get::<_, Option<i64>>(idx) {
        return match v {
            Some(i) => Value::Int(i),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.get::<_, Option<f64>>(idx) {
        return match v {
            Some(f) => Value::Float(f),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.get::<_, Option<bool>>(idx) {
        return match v {
            Some(b) => Value::Bool(b),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.get::<_, Option<String>>(idx) {
        return match v {
            Some(s) => Value::Text(s),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.get::<_, Option<Vec<u8>>>(idx) {
        return match v {
            Some(b) => Value::Bytes(b),
            None => Value::Null,
        };
    }
    Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;

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
        engine.create_temp_table("t", &columns).unwrap();
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

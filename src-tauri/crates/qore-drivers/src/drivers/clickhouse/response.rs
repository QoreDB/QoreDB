// SPDX-License-Identifier: Apache-2.0

//! Parse ClickHouse `JSONCompactEachRowWithNamesAndTypes` responses.
//!
//! Format:
//! ```text
//! ["name1", "name2"]              <- column names
//! ["Int32",  "String"]            <- column types
//! [1, "alpha"]                    <- row values
//! [2, "beta"]
//! ```
//! Each line is a complete JSON array. DDL/mutation statements emit an empty
//! body — we treat that as "no result set" rather than an error.

use qore_core::error::{EngineError, EngineResult};
use qore_core::types::{ColumnInfo, QueryResult, Row, Value};
use serde_json::Value as JsonValue;

use super::types::{build_column_info, json_to_value};

pub fn parse_query_result(body: &str, execution_time_ms: f64) -> EngineResult<QueryResult> {
    let mut lines = body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .into_iter();

    // No body → DDL or write-only statement with no result set.
    let Some(names_line) = lines.next() else {
        return Ok(QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            affected_rows: None,
            execution_time_ms,
        });
    };

    let names: Vec<String> = serde_json::from_str(names_line).map_err(|e| {
        EngineError::execution_error(format!("Invalid ClickHouse names line: {e}"))
    })?;

    let Some(types_line) = lines.next() else {
        return Err(EngineError::execution_error(
            "ClickHouse response missing type header",
        ));
    };
    let types: Vec<String> = serde_json::from_str(types_line).map_err(|e| {
        EngineError::execution_error(format!("Invalid ClickHouse types line: {e}"))
    })?;

    if names.len() != types.len() {
        return Err(EngineError::execution_error(format!(
            "ClickHouse names/types mismatch: {} vs {}",
            names.len(),
            types.len()
        )));
    }

    let columns: Vec<ColumnInfo> = names
        .iter()
        .zip(types.iter())
        .map(|(n, t)| build_column_info(n, t))
        .collect();

    let mut rows = Vec::new();
    for line in lines {
        let cells: Vec<JsonValue> = serde_json::from_str(line).map_err(|e| {
            EngineError::execution_error(format!("Invalid ClickHouse row: {e}"))
        })?;
        if cells.len() != types.len() {
            return Err(EngineError::execution_error(format!(
                "ClickHouse row arity mismatch: {} vs {}",
                cells.len(),
                types.len()
            )));
        }
        let values: Vec<Value> = cells
            .iter()
            .zip(types.iter())
            .map(|(cell, t)| json_to_value(t, cell))
            .collect();
        rows.push(Row { values });
    }

    Ok(QueryResult {
        columns,
        rows,
        affected_rows: None,
        execution_time_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_row_response() {
        let body = "[\"id\",\"name\"]\n[\"Int32\",\"String\"]\n[1,\"alice\"]\n[2,\"bob\"]\n";
        let qr = parse_query_result(body, 0.0).unwrap();
        assert_eq!(qr.columns.len(), 2);
        assert_eq!(qr.columns[0].name.as_str(), "id");
        assert_eq!(qr.columns[0].data_type.as_str(), "Int32");
        assert_eq!(qr.rows.len(), 2);
        assert!(matches!(qr.rows[0].values[0], Value::Int(1)));
        assert!(matches!(qr.rows[1].values[1], Value::Text(ref s) if s == "bob"));
    }

    #[test]
    fn parses_empty_body_as_no_result_set() {
        let qr = parse_query_result("", 0.0).unwrap();
        assert!(qr.columns.is_empty());
        assert!(qr.rows.is_empty());
    }

    #[test]
    fn parses_zero_row_response_with_schema() {
        let body = "[\"x\"]\n[\"Int32\"]\n";
        let qr = parse_query_result(body, 0.0).unwrap();
        assert_eq!(qr.columns.len(), 1);
        assert_eq!(qr.rows.len(), 0);
    }

    #[test]
    fn rejects_missing_types_header() {
        let body = "[\"x\"]\n";
        let err = parse_query_result(body, 0.0).unwrap_err();
        let EngineError::ExecutionError { message } = err else {
            panic!("expected ExecutionError");
        };
        assert!(message.contains("type header"));
    }

    #[test]
    fn rejects_arity_mismatch() {
        let body = "[\"a\",\"b\"]\n[\"Int32\",\"String\"]\n[1]\n";
        let err = parse_query_result(body, 0.0).unwrap_err();
        let EngineError::ExecutionError { message } = err else {
            panic!("expected ExecutionError");
        };
        assert!(message.contains("arity"));
    }

    #[test]
    fn detects_nullable_columns() {
        let body = "[\"x\"]\n[\"Nullable(Int32)\"]\n[null]\n";
        let qr = parse_query_result(body, 0.0).unwrap();
        assert!(qr.columns[0].nullable);
        assert!(matches!(qr.rows[0].values[0], Value::Null));
    }
}

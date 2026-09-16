// SPDX-License-Identifier: Apache-2.0

//! The SQL API's `jsonv2` result format: every cell is a string, or null,
//! and the row type says what the string means. Dates are day counts,
//! times and timestamps are epoch seconds with a fraction, and a
//! `TIMESTAMP_TZ` carries its offset in minutes shifted by 1440.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone, Utc};
use compact_str::CompactString;
use qore_core::error::{EngineError, EngineResult};
use qore_core::types::{ColumnInfo, Row, Value};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellType {
    Fixed { scale: u32 },
    Real,
    Text,
    Boolean,
    Date,
    Time,
    TimestampNtz,
    TimestampLtz,
    TimestampTz,
    Binary,
    Semi,
    Other,
}

impl CellType {
    fn from_row_type(kind: &str, scale: Option<u32>) -> Self {
        match kind.to_ascii_lowercase().as_str() {
            "fixed" => CellType::Fixed {
                scale: scale.unwrap_or(0),
            },
            "real" => CellType::Real,
            "text" => CellType::Text,
            "boolean" => CellType::Boolean,
            "date" => CellType::Date,
            "time" => CellType::Time,
            "timestamp_ntz" => CellType::TimestampNtz,
            "timestamp_ltz" => CellType::TimestampLtz,
            "timestamp_tz" => CellType::TimestampTz,
            "binary" => CellType::Binary,
            "variant" | "object" | "array" => CellType::Semi,
            _ => CellType::Other,
        }
    }
}

#[derive(Deserialize)]
struct RowType {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default = "default_true")]
    nullable: bool,
    precision: Option<u32>,
    scale: Option<u32>,
    length: Option<u64>,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
struct PartitionInfo {}

#[derive(Deserialize)]
struct ResultSetMetaData {
    #[serde(rename = "rowType", default)]
    row_type: Vec<RowType>,
    #[serde(rename = "partitionInfo", default)]
    partition_info: Vec<PartitionInfo>,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "resultSetMetaData")]
    result_set_meta_data: Option<ResultSetMetaData>,
    #[serde(default)]
    data: Vec<Vec<Option<String>>>,
}

pub struct StatementBody {
    pub columns: Vec<ColumnInfo>,
    types: Vec<CellType>,
    pub rows: Vec<Row>,
    pub partitions: usize,
}

impl StatementBody {
    pub fn parse(text: &str) -> EngineResult<Self> {
        let payload: Payload = serde_json::from_str(text)
            .map_err(|e| EngineError::internal(format!("Unexpected Snowflake result: {e}")))?;
        let meta = payload.result_set_meta_data.unwrap_or(ResultSetMetaData {
            row_type: Vec::new(),
            partition_info: Vec::new(),
        });
        let types: Vec<CellType> = meta
            .row_type
            .iter()
            .map(|c| CellType::from_row_type(&c.kind, c.scale))
            .collect();
        let columns = meta
            .row_type
            .iter()
            .map(|c| ColumnInfo {
                name: CompactString::new(&c.name),
                data_type: CompactString::new(declared_type(c)),
                nullable: c.nullable,
                masked: false,
            })
            .collect();
        let mut body = Self {
            columns,
            types,
            rows: Vec::new(),
            partitions: meta.partition_info.len().max(1),
        };
        body.push_rows(payload.data)?;
        Ok(body)
    }

    pub fn append_partition(&mut self, text: &str) -> EngineResult<()> {
        let payload: Payload = serde_json::from_str(text)
            .map_err(|e| EngineError::internal(format!("Unexpected Snowflake partition: {e}")))?;
        self.push_rows(payload.data)
    }

    fn push_rows(&mut self, data: Vec<Vec<Option<String>>>) -> EngineResult<()> {
        for cells in data {
            if cells.len() != self.types.len() {
                return Err(EngineError::internal(format!(
                    "Snowflake row has {} cells for {} columns",
                    cells.len(),
                    self.types.len()
                )));
            }
            let values = cells
                .into_iter()
                .zip(&self.types)
                .map(|(cell, ty)| decode(*ty, cell.as_deref()))
                .collect();
            self.rows.push(Row { values });
        }
        Ok(())
    }
}

fn declared_type(c: &RowType) -> String {
    let kind = c.kind.to_ascii_uppercase();
    match (kind.as_str(), c.precision, c.scale, c.length) {
        ("FIXED", Some(p), Some(s), _) => format!("NUMBER({p},{s})"),
        ("FIXED", _, _, _) => "NUMBER".into(),
        ("REAL", ..) => "FLOAT".into(),
        ("TEXT", _, _, Some(len)) => format!("VARCHAR({len})"),
        ("TEXT", ..) => "VARCHAR".into(),
        _ => kind,
    }
}

/// The wire never carries a number: a `FIXED` with no scale is parsed
/// exactly when it fits an `i64` and kept as text past that, a scaled one
/// stays text so no digit is lost.
pub fn decode(ty: CellType, cell: Option<&str>) -> Value {
    let Some(raw) = cell else {
        return Value::Null;
    };
    match ty {
        CellType::Fixed { scale: 0 } => raw
            .parse::<i64>()
            .map(Value::Int)
            .unwrap_or_else(|_| Value::Text(raw.to_string())),
        CellType::Fixed { .. } => Value::Text(raw.to_string()),
        CellType::Real => raw
            .parse::<f64>()
            .map(Value::Float)
            .unwrap_or_else(|_| Value::Text(raw.to_string())),
        CellType::Boolean => match raw {
            "true" | "1" | "TRUE" => Value::Bool(true),
            "false" | "0" | "FALSE" => Value::Bool(false),
            other => Value::Text(other.to_string()),
        },
        CellType::Date => decode_date(raw).unwrap_or_else(|| Value::Text(raw.to_string())),
        CellType::Time => decode_time(raw).unwrap_or_else(|| Value::Text(raw.to_string())),
        CellType::TimestampNtz => {
            decode_timestamp(raw, None, false).unwrap_or_else(|| Value::Text(raw.to_string()))
        }
        CellType::TimestampLtz => {
            decode_timestamp(raw, None, true).unwrap_or_else(|| Value::Text(raw.to_string()))
        }
        CellType::TimestampTz => {
            let (epoch, offset) = raw.split_once(' ').unwrap_or((raw, "1440"));
            offset
                .parse::<i32>()
                .ok()
                .and_then(|minutes| decode_timestamp(epoch, Some(minutes - 1440), true))
                .unwrap_or_else(|| Value::Text(raw.to_string()))
        }
        CellType::Binary => decode_hex(raw)
            .map(Value::Bytes)
            .unwrap_or_else(|| Value::Text(raw.to_string())),
        CellType::Semi => serde_json::from_str(raw)
            .map(Value::Json)
            .unwrap_or_else(|_| Value::Text(raw.to_string())),
        CellType::Text | CellType::Other => Value::Text(raw.to_string()),
    }
}

fn decode_date(raw: &str) -> Option<Value> {
    let days: i64 = raw.parse().ok()?;
    let date =
        NaiveDate::from_ymd_opt(1970, 1, 1)?.checked_add_signed(chrono::Duration::days(days))?;
    Some(Value::Text(date.format("%Y-%m-%d").to_string()))
}

/// Seconds since midnight, with a fraction. Zero fractions are dropped so
/// `09:30:00` reads as a time and not as a measurement.
fn decode_time(raw: &str) -> Option<Value> {
    let (secs, fraction) = split_seconds(raw)?;
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    Some(Value::Text(match fraction {
        Some(f) => format!("{h:02}:{m:02}:{s:02}.{f}"),
        None => format!("{h:02}:{m:02}:{s:02}"),
    }))
}

fn decode_timestamp(raw: &str, offset_minutes: Option<i32>, zoned: bool) -> Option<Value> {
    let (secs, fraction) = split_seconds(raw)?;
    let nanos: u32 = fraction
        .map(|f| format!("{f:0<9}")[..9].parse().ok())
        .unwrap_or(Some(0))?;
    let utc: DateTime<Utc> = Utc.timestamp_opt(secs, nanos).single()?;
    let text = match (zoned, offset_minutes) {
        (false, _) => format!("{}", utc.format("%Y-%m-%dT%H:%M:%S%.f")),
        (true, None) => utc.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
        (true, Some(minutes)) => {
            let offset = FixedOffset::east_opt(minutes * 60)?;
            utc.with_timezone(&offset)
                .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, false)
        }
    };
    Some(Value::Text(text))
}

fn split_seconds(raw: &str) -> Option<(i64, Option<&str>)> {
    let (whole, fraction) = match raw.split_once('.') {
        Some((w, f)) => (w, Some(f.trim_end_matches('0')).filter(|f| !f.is_empty())),
        None => (raw, None),
    };
    Some((whole.parse().ok()?, fraction))
}

fn decode_hex(raw: &str) -> Option<Vec<u8>> {
    if !raw.len().is_multiple_of(2) {
        return None;
    }
    (0..raw.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&raw[i..i + 2], 16).ok())
        .collect()
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Binding {
    #[serde(rename = "type")]
    kind: &'static str,
    value: String,
}

/// Positional bindings, keyed `"1"`, `"2"`, … as the API wants.
pub type Bindings = BTreeMap<String, Binding>;

/// A bindable value, or `None` for a NULL, which is inlined in the SQL
/// because the API has no typed null.
pub fn bind(value: &Value) -> Option<Binding> {
    let (kind, text) = match value {
        Value::Null => return None,
        Value::Bool(b) => ("BOOLEAN", b.to_string()),
        Value::Int(i) => ("FIXED", i.to_string()),
        Value::Float(f) => ("REAL", f.to_string()),
        Value::Text(s) => ("TEXT", s.clone()),
        Value::Bytes(b) => (
            "BINARY",
            b.iter().map(|byte| format!("{byte:02X}")).collect(),
        ),
        Value::Json(j) => ("TEXT", j.to_string()),
        Value::Array(items) => (
            "TEXT",
            serde_json::to_string(items).unwrap_or_else(|_| "[]".into()),
        ),
    };
    Some(Binding { kind, value: text })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(v: Value) -> String {
        match v {
            Value::Text(s) => s,
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn numbers_stay_exact() {
        assert!(matches!(
            decode(CellType::Fixed { scale: 0 }, Some("42")),
            Value::Int(42)
        ));
        assert_eq!(
            text(decode(
                CellType::Fixed { scale: 0 },
                Some("99999999999999999999")
            )),
            "99999999999999999999"
        );
        assert_eq!(
            text(decode(CellType::Fixed { scale: 2 }, Some("12.30"))),
            "12.30"
        );
        assert!(matches!(decode(CellType::Real, Some("1.5")), Value::Float(f) if f == 1.5));
        assert!(matches!(decode(CellType::Real, None), Value::Null));
    }

    #[test]
    fn booleans_accept_both_spellings() {
        assert!(matches!(
            decode(CellType::Boolean, Some("true")),
            Value::Bool(true)
        ));
        assert!(matches!(
            decode(CellType::Boolean, Some("0")),
            Value::Bool(false)
        ));
    }

    #[test]
    fn temporal_cells_render_as_iso_text() {
        assert_eq!(text(decode(CellType::Date, Some("19782"))), "2024-02-29");
        assert_eq!(text(decode(CellType::Date, Some("-1"))), "1969-12-31");
        assert_eq!(text(decode(CellType::Time, Some("34200"))), "09:30:00");
        assert_eq!(
            text(decode(CellType::Time, Some("39735.123456789"))),
            "11:02:15.123456789"
        );
        assert_eq!(
            text(decode(CellType::TimestampNtz, Some("1700000000.500000000"))),
            "2023-11-14T22:13:20.500"
        );
        assert_eq!(
            text(decode(CellType::TimestampLtz, Some("1700000000"))),
            "2023-11-14T22:13:20Z"
        );
        // Offset is minutes plus 1440: 1500 is UTC+01:00.
        assert_eq!(
            text(decode(CellType::TimestampTz, Some("1700000000.123 1500"))),
            "2023-11-14T23:13:20.123+01:00"
        );
        assert_eq!(
            text(decode(CellType::TimestampTz, Some("garbage"))),
            "garbage"
        );
    }

    #[test]
    fn binary_and_semi_structured_cells() {
        assert!(matches!(
            decode(CellType::Binary, Some("DEADBEEF")),
            Value::Bytes(ref b) if b == &[0xDE, 0xAD, 0xBE, 0xEF]
        ));
        assert!(matches!(
            decode(CellType::Semi, Some(r#"{"a":[1,2]}"#)),
            Value::Json(ref j) if j["a"][1] == 2
        ));
        assert_eq!(text(decode(CellType::Semi, Some("not json"))), "not json");
    }

    #[test]
    fn a_result_parses_its_columns_and_rows() {
        let body = StatementBody::parse(
            r#"{"resultSetMetaData":{"numRows":1,"format":"jsonv2","partitionInfo":[{"rowCount":1}],
                "rowType":[{"name":"ID","type":"fixed","nullable":false,"precision":38,"scale":0},
                           {"name":"NAME","type":"text","nullable":true,"length":16777216},
                           {"name":"PRICE","type":"fixed","nullable":true,"precision":10,"scale":2}]},
               "data":[["7","x",null]],"statementHandle":"h"}"#,
        )
        .unwrap();
        assert_eq!(body.partitions, 1);
        assert_eq!(body.columns[0].data_type.as_str(), "NUMBER(38,0)");
        assert!(!body.columns[0].nullable);
        assert_eq!(body.columns[1].data_type.as_str(), "VARCHAR(16777216)");
        assert_eq!(body.columns[2].data_type.as_str(), "NUMBER(10,2)");
        assert!(matches!(body.rows[0].values[0], Value::Int(7)));
        assert!(matches!(body.rows[0].values[2], Value::Null));

        // A DDL answer has no metadata at all.
        let ddl = StatementBody::parse(r#"{"statementHandle":"h","message":"ok"}"#).unwrap();
        assert!(ddl.columns.is_empty());
        assert!(ddl.rows.is_empty());

        assert!(StatementBody::parse(
            r#"{"resultSetMetaData":{"rowType":[{"name":"A","type":"text"}]},"data":[["1","2"]]}"#
        )
        .is_err());
    }

    #[test]
    fn bindings_carry_the_api_type_and_null_is_not_bindable() {
        assert_eq!(
            bind(&Value::Int(3)),
            Some(Binding {
                kind: "FIXED",
                value: "3".into()
            })
        );
        assert_eq!(
            bind(&Value::Bytes(vec![1, 255])),
            Some(Binding {
                kind: "BINARY",
                value: "01FF".into()
            })
        );
        assert_eq!(
            bind(&Value::Json(serde_json::json!({"k": 1}))),
            Some(Binding {
                kind: "TEXT",
                value: r#"{"k":1}"#.into()
            })
        );
        assert!(bind(&Value::Null).is_none());
    }
}

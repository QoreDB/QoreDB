// SPDX-License-Identifier: Apache-2.0

//! BigQuery's row format: every cell is `{"v": …}`, a scalar is a string,
//! a `RECORD` nests `{"f": [cells]}`, and a `REPEATED` field is a list of
//! cells. The schema, not the cell, says what a string means.

use base64::Engine as _;
use chrono::{DateTime, Utc};
use compact_str::CompactString;
use qore_core::error::{EngineError, EngineResult};
use qore_core::types::{ColumnInfo, Row, Value};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use super::client::JobRef;

#[derive(Debug, Clone, Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub fields: Vec<Field>,
}

impl Field {
    pub fn repeated(&self) -> bool {
        self.mode.eq_ignore_ascii_case("REPEATED")
    }

    pub fn nullable(&self) -> bool {
        !self.mode.eq_ignore_ascii_case("REQUIRED")
    }

    /// `ARRAY<STRING>`, `STRUCT<…>`, `INT64`: what a user would write.
    pub fn declared_type(&self) -> String {
        let base = match self.kind.to_ascii_uppercase().as_str() {
            "INTEGER" => "INT64".to_string(),
            "FLOAT" => "FLOAT64".to_string(),
            "BOOLEAN" => "BOOL".to_string(),
            "RECORD" | "STRUCT" => format!(
                "STRUCT<{}>",
                self.fields
                    .iter()
                    .map(|f| format!("{} {}", f.name, f.declared_type()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            other => other.to_string(),
        };
        if self.repeated() {
            format!("ARRAY<{base}>")
        } else {
            base
        }
    }

    pub fn column(&self) -> ColumnInfo {
        ColumnInfo {
            name: CompactString::new(&self.name),
            data_type: CompactString::new(self.declared_type()),
            nullable: self.nullable(),
            masked: false,
        }
    }
}

#[derive(Deserialize, Default)]
pub struct Schema {
    #[serde(default)]
    pub fields: Vec<Field>,
}

#[derive(Deserialize)]
struct Cell {
    v: Json,
}

#[derive(Deserialize)]
struct RawRow {
    f: Vec<Cell>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPage {
    #[serde(default)]
    job_complete: bool,
    job_reference: Option<JobRef>,
    schema: Option<Schema>,
    #[serde(default)]
    rows: Vec<RawRow>,
    total_rows: Option<String>,
    page_token: Option<String>,
    total_bytes_processed: Option<String>,
    cache_hit: Option<bool>,
    num_dml_affected_rows: Option<String>,
}

pub struct QueryPage {
    pub complete: bool,
    pub job: Option<JobRef>,
    pub columns: Vec<ColumnInfo>,
    fields: Vec<Field>,
    /// Rows that arrived before the schema did; a page after the first
    /// carries no schema of its own.
    pending: Vec<RawRow>,
    pub rows: Vec<Row>,
    pub total_rows: Option<u64>,
    pub page_token: Option<String>,
    pub total_bytes_processed: Option<u64>,
    pub cache_hit: Option<bool>,
    pub affected_rows: Option<u64>,
}

impl QueryPage {
    pub fn parse(text: &str) -> EngineResult<Self> {
        let raw: RawPage = serde_json::from_str(text)
            .map_err(|e| EngineError::internal(format!("Unexpected BigQuery result: {e}")))?;
        let fields = raw.schema.unwrap_or_default().fields;
        let mut page = Self {
            complete: raw.job_complete,
            job: raw.job_reference,
            columns: fields.iter().map(Field::column).collect(),
            fields,
            pending: raw.rows,
            rows: Vec::new(),
            total_rows: raw.total_rows.and_then(|t| t.parse().ok()),
            page_token: raw.page_token,
            total_bytes_processed: raw.total_bytes_processed.and_then(|t| t.parse().ok()),
            cache_hit: raw.cache_hit,
            affected_rows: raw.num_dml_affected_rows.and_then(|t| t.parse().ok()),
        };
        page.decode_pending()?;
        Ok(page)
    }

    /// A later page or poll answer: the schema arrives with the first
    /// complete page, the rows accumulate.
    pub fn absorb(&mut self, next: QueryPage) -> EngineResult<()> {
        self.complete = next.complete;
        if self.fields.is_empty() && !next.fields.is_empty() {
            self.fields = next.fields;
            self.columns = next.columns;
        }
        self.rows.extend(next.rows);
        self.pending.extend(next.pending);
        self.total_rows = next.total_rows.or(self.total_rows);
        self.page_token = next.page_token;
        self.total_bytes_processed = next.total_bytes_processed.or(self.total_bytes_processed);
        self.cache_hit = next.cache_hit.or(self.cache_hit);
        self.affected_rows = next.affected_rows.or(self.affected_rows);
        self.decode_pending()
    }

    fn decode_pending(&mut self) -> EngineResult<()> {
        if self.fields.is_empty() {
            return Ok(());
        }
        for row in self.pending.drain(..) {
            self.rows.push(decode_row(&self.fields, row)?);
        }
        Ok(())
    }

    /// Once the job is complete and every page is in, rows still waiting
    /// for a schema mean the answer was malformed.
    pub fn finalize(&mut self) -> EngineResult<()> {
        self.decode_pending()?;
        if self.pending.is_empty() {
            Ok(())
        } else {
            Err(EngineError::internal(
                "BigQuery returned rows without a schema",
            ))
        }
    }
}

/// `tabledata.list`: rows without a schema, which comes from `tables.get`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableData {
    total_rows: Option<String>,
    #[serde(default)]
    rows: Vec<RawRow>,
}

impl TableData {
    pub fn total_rows(&self) -> Option<u64> {
        self.total_rows.as_deref().and_then(|t| t.parse().ok())
    }

    pub fn into_rows(self, fields: &[Field]) -> EngineResult<Vec<Row>> {
        self.rows
            .into_iter()
            .map(|row| decode_row(fields, row))
            .collect()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    #[serde(default)]
    pub schema: Schema,
    pub num_rows: Option<String>,
    #[serde(default)]
    pub table_constraints: TableConstraints,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableConstraints {
    pub primary_key: Option<PrimaryKey>,
    #[serde(default)]
    pub foreign_keys: Vec<ForeignKeyInfo>,
}

#[derive(Deserialize)]
pub struct PrimaryKey {
    #[serde(default)]
    pub columns: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyInfo {
    pub name: Option<String>,
    pub referenced_table: ReferencedTable,
    #[serde(default)]
    pub column_references: Vec<ColumnReference>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencedTable {
    pub project_id: Option<String>,
    pub dataset_id: Option<String>,
    pub table_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnReference {
    pub referencing_column: String,
    pub referenced_column: String,
}

fn decode_row(fields: &[Field], row: RawRow) -> EngineResult<Row> {
    if row.f.len() != fields.len() {
        return Err(EngineError::internal(format!(
            "BigQuery row has {} cells for {} columns",
            row.f.len(),
            fields.len()
        )));
    }
    Ok(Row {
        values: row
            .f
            .into_iter()
            .zip(fields)
            .map(|(cell, field)| decode(field, cell.v))
            .collect(),
    })
}

pub fn decode(field: &Field, cell: Json) -> Value {
    if field.repeated() {
        return match cell {
            Json::Array(items) => Value::Array(
                items
                    .into_iter()
                    .map(|item| decode_scalar(field, unwrap_v(item)))
                    .collect(),
            ),
            Json::Null => Value::Null,
            other => decode_scalar(field, other),
        };
    }
    decode_scalar(field, cell)
}

fn unwrap_v(item: Json) -> Json {
    match item {
        Json::Object(mut o) => o.remove("v").unwrap_or(Json::Null),
        other => other,
    }
}

fn decode_scalar(field: &Field, cell: Json) -> Value {
    if cell.is_null() {
        return Value::Null;
    }
    let kind = field.kind.to_ascii_uppercase();
    if kind == "RECORD" || kind == "STRUCT" {
        let cells = match cell {
            Json::Object(mut o) => match o.remove("f") {
                Some(Json::Array(cells)) => cells,
                _ => return Value::Null,
            },
            _ => return Value::Null,
        };
        let object: serde_json::Map<String, Json> = field
            .fields
            .iter()
            .zip(cells)
            .map(|(f, cell)| (f.name.clone(), to_json(decode(f, unwrap_v(cell)))))
            .collect();
        return Value::Json(Json::Object(object));
    }
    let Some(raw) = cell.as_str() else {
        return Value::Json(cell);
    };
    match kind.as_str() {
        "INTEGER" | "INT64" => raw
            .parse::<i64>()
            .map(Value::Int)
            .unwrap_or_else(|_| Value::Text(raw.to_string())),
        "FLOAT" | "FLOAT64" => raw
            .parse::<f64>()
            .map(Value::Float)
            .unwrap_or_else(|_| Value::Text(raw.to_string())),
        "BOOLEAN" | "BOOL" => match raw {
            "true" | "TRUE" => Value::Bool(true),
            "false" | "FALSE" => Value::Bool(false),
            other => Value::Text(other.to_string()),
        },
        "BYTES" => base64::engine::general_purpose::STANDARD
            .decode(raw)
            .map(Value::Bytes)
            .unwrap_or_else(|_| Value::Text(raw.to_string())),
        "TIMESTAMP" => decode_timestamp(raw).unwrap_or_else(|| Value::Text(raw.to_string())),
        "JSON" => serde_json::from_str(raw)
            .map(Value::Json)
            .unwrap_or_else(|_| Value::Text(raw.to_string())),
        _ => Value::Text(raw.to_string()),
    }
}

/// Epoch seconds as a decimal string, sometimes in exponent form
/// (`1.7E9`). Microsecond precision is what BigQuery stores, and what
/// survives the float.
fn decode_timestamp(raw: &str) -> Option<Value> {
    let seconds: f64 = raw.parse().ok()?;
    let micros = (seconds * 1_000_000.0).round() as i64;
    let utc: DateTime<Utc> = DateTime::from_timestamp_micros(micros)?;
    Some(Value::Text(
        utc.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
    ))
}

fn to_json(value: Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(b),
        Value::Int(i) => Json::from(i),
        Value::Float(f) => serde_json::Number::from_f64(f)
            .map(Json::Number)
            .unwrap_or(Json::Null),
        Value::Text(s) => Json::String(s),
        Value::Bytes(b) => Json::String(base64::engine::general_purpose::STANDARD.encode(b)),
        Value::Json(j) => j,
        Value::Array(items) => Json::Array(items.into_iter().map(to_json).collect()),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    parameter_type: ParamType,
    parameter_value: ParamValue,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ParamType {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ParamValue {
    value: String,
}

/// A positional query parameter, or `None` for a NULL, which is inlined.
pub fn param(value: &Value) -> Option<Param> {
    let (kind, text) = match value {
        Value::Null => return None,
        Value::Bool(b) => ("BOOL", b.to_string()),
        Value::Int(i) => ("INT64", i.to_string()),
        Value::Float(f) => ("FLOAT64", f.to_string()),
        Value::Text(s) => ("STRING", s.clone()),
        Value::Bytes(b) => ("BYTES", base64::engine::general_purpose::STANDARD.encode(b)),
        Value::Json(j) => ("STRING", j.to_string()),
        Value::Array(items) => (
            "STRING",
            serde_json::to_string(items).unwrap_or_else(|_| "[]".into()),
        ),
    };
    Some(Param {
        parameter_type: ParamType { kind },
        parameter_value: ParamValue { value: text },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(kind: &str, mode: &str) -> Field {
        Field {
            name: "c".into(),
            kind: kind.into(),
            mode: mode.into(),
            fields: vec![],
        }
    }

    #[test]
    fn scalars_follow_the_schema_type() {
        assert!(matches!(
            decode(&field("INTEGER", ""), Json::from("42")),
            Value::Int(42)
        ));
        assert!(
            matches!(decode(&field("FLOAT", ""), Json::from("1.5")), Value::Float(f) if f == 1.5)
        );
        assert!(matches!(
            decode(&field("BOOLEAN", ""), Json::from("true")),
            Value::Bool(true)
        ));
        assert!(
            matches!(decode(&field("NUMERIC", ""), Json::from("12.30")), Value::Text(ref s) if s == "12.30")
        );
        assert!(
            matches!(decode(&field("BYTES", ""), Json::from("3q2+7w==")), Value::Bytes(ref b) if b == &[0xDE, 0xAD, 0xBE, 0xEF])
        );
        assert!(matches!(
            decode(&field("STRING", "NULLABLE"), Json::Null),
            Value::Null
        ));
        assert!(
            matches!(decode(&field("JSON", ""), Json::from(r#"{"a":1}"#)), Value::Json(ref j) if j["a"] == 1)
        );
    }

    #[test]
    fn timestamps_accept_exponent_form() {
        assert!(matches!(
            decode(&field("TIMESTAMP", ""), Json::from("1.7E9")),
            Value::Text(ref s) if s == "2023-11-14T22:13:20Z"
        ));
        assert!(matches!(
            decode(&field("TIMESTAMP", ""), Json::from("1700000000.123456")),
            Value::Text(ref s) if s == "2023-11-14T22:13:20.123456Z"
        ));
    }

    #[test]
    fn repeated_and_record_fields_nest() {
        let tags = field("STRING", "REPEATED");
        let decoded = decode(&tags, serde_json::json!([{"v": "a"}, {"v": "b"}]));
        assert!(matches!(decoded, Value::Array(ref items) if items.len() == 2));

        let address = Field {
            name: "address".into(),
            kind: "RECORD".into(),
            mode: "NULLABLE".into(),
            fields: vec![field("STRING", ""), {
                let mut zip = field("INTEGER", "");
                zip.name = "zip".into();
                zip
            }],
        };
        let decoded = decode(
            &address,
            serde_json::json!({"f": [{"v": "rue"}, {"v": "75001"}]}),
        );
        assert!(matches!(decoded, Value::Json(ref j) if j["zip"] == 75001 && j["c"] == "rue"));
        assert_eq!(address.declared_type(), "STRUCT<c STRING, zip INT64>");
        assert_eq!(tags.declared_type(), "ARRAY<STRING>");
    }

    #[test]
    fn a_page_parses_and_later_pages_accumulate() {
        let mut first = QueryPage::parse(
            r#"{"jobComplete":true,"jobReference":{"projectId":"p","jobId":"j"},
                "schema":{"fields":[{"name":"n","type":"INT64","mode":"REQUIRED"}]},
                "rows":[{"f":[{"v":"1"}]}],"totalRows":"2","pageToken":"t",
                "totalBytesProcessed":"10","numDmlAffectedRows":"0"}"#,
        )
        .unwrap();
        assert!(!first.columns[0].nullable);
        let next = QueryPage::parse(r#"{"jobComplete":true,"rows":[{"f":[{"v":"2"}]}]}"#).unwrap();
        first.absorb(next).unwrap();
        assert_eq!(first.rows.len(), 2);
        assert!(first.page_token.is_none());
        assert_eq!(first.total_rows, Some(2));

        assert!(QueryPage::parse(
            r#"{"schema":{"fields":[{"name":"a","type":"STRING"}]},"rows":[{"f":[{"v":"1"},{"v":"2"}]}]}"#
        )
        .is_err());
        let mut orphan =
            QueryPage::parse(r#"{"jobComplete":true,"rows":[{"f":[{"v":"1"}]}]}"#).unwrap();
        assert!(
            orphan.finalize().is_err(),
            "rows with no schema are malformed"
        );
    }

    #[test]
    fn params_are_typed_and_null_is_not_a_param() {
        let p = param(&Value::Int(3)).unwrap();
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({
                "parameterType": {"type": "INT64"}, "parameterValue": {"value": "3"}
            })
        );
        assert_eq!(
            serde_json::to_value(param(&Value::Bytes(vec![1])).unwrap()).unwrap()["parameterValue"]
                ["value"],
            "AQ=="
        );
        assert!(param(&Value::Null).is_none());
    }
}

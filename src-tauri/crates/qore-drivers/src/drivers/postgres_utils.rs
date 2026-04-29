// SPDX-License-Identifier: Apache-2.0

//! PostgreSQL driver helpers

use std::collections::HashMap;

use bigdecimal::BigDecimal;
use bigdecimal::ToPrimitive as BigDecimalToPrimitive;
use rust_decimal::Decimal;
use sqlx::postgres::{PgColumn, PgRow, PgTypeKind, PgValueFormat, Postgres};
use sqlx::{Column, Executor, Row, TypeInfo, ValueRef};
use uuid::Uuid;

use qore_core::error::{EngineError, EngineResult};
use qore_core::types::{ColumnInfo, Row as QRow, Value};

/// Map from enum value OID to its label string
pub(crate) type EnumLabelMap = HashMap<u32, String>;

pub(crate) fn read_i32(buf: &mut &[u8]) -> Option<i32> {
    if buf.len() < 4 {
        return None;
    }
    let (bytes, rest) = buf.split_at(4);
    *buf = rest;
    Some(i32::from_be_bytes(bytes.try_into().ok()?))
}

pub(crate) fn read_u32(buf: &mut &[u8]) -> Option<u32> {
    if buf.len() < 4 {
        return None;
    }
    let (bytes, rest) = buf.split_at(4);
    *buf = rest;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

/// Collect all enum type OIDs from column metadata (both scalar enums and enum arrays)
pub(crate) fn collect_enum_type_oids(columns: &[PgColumn]) -> Vec<u32> {
    let mut oids = Vec::new();
    for col in columns {
        let type_info = col.type_info();
        match type_info.kind() {
            PgTypeKind::Enum(_) => {
                if let Some(oid) = type_info.oid() {
                    oids.push(oid.0);
                }
            }
            PgTypeKind::Array(elem_type) => {
                if matches!(elem_type.kind(), PgTypeKind::Enum(_)) {
                    if let Some(oid) = elem_type.oid() {
                        oids.push(oid.0);
                    }
                }
            }
            _ => {}
        }
    }
    oids.sort();
    oids.dedup();
    oids
}

/// Load enum labels from pg_enum for the given type OIDs
pub(crate) async fn load_enum_labels<'e, E>(
    executor: E,
    enum_type_oids: &[u32],
) -> EngineResult<EnumLabelMap>
where
    E: Executor<'e, Database = Postgres>,
{
    if enum_type_oids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT oid::bigint, enumlabel::text FROM pg_enum WHERE enumtypid = ANY($1::oid[])",
    )
    .bind(enum_type_oids.iter().map(|&o| o as i32).collect::<Vec<_>>())
    .fetch_all(executor)
    .await
    .map_err(|e| EngineError::execution_error(format!("Failed to load enum labels: {}", e)))?;

    let map: EnumLabelMap = rows
        .into_iter()
        .map(|(oid, label)| (oid as u32, label))
        .collect();

    Ok(map)
}

/// Decode a scalar enum value from raw bytes
pub(crate) fn decode_enum_value(
    raw: &sqlx::postgres::PgValueRef<'_>,
    labels: &EnumLabelMap,
) -> Option<Value> {
    if raw.is_null() {
        return Some(Value::Null);
    }

    match raw.format() {
        PgValueFormat::Text => raw.as_str().ok().map(|s| Value::Text(s.to_string())),
        PgValueFormat::Binary => {
            let bytes = raw.as_bytes().ok()?;
            if bytes.len() < 4 {
                return None;
            }
            let oid = u32::from_be_bytes(bytes[0..4].try_into().ok()?);
            labels.get(&oid).map(|label| Value::Text(label.clone()))
        }
    }
}

/// Decode an enum array from binary format
pub(crate) fn decode_enum_array_binary(
    raw: &sqlx::postgres::PgValueRef<'_>,
    labels: &EnumLabelMap,
) -> Option<Value> {
    if raw.is_null() {
        return Some(Value::Null);
    }

    match raw.format() {
        PgValueFormat::Text => {
            let text = raw.as_str().ok()?;
            let text = text.trim();
            if !text.starts_with('{') || !text.ends_with('}') {
                return None;
            }
            let inner = &text[1..text.len() - 1];
            if inner.is_empty() {
                return Some(Value::Array(vec![]));
            }
            let values: Vec<Value> = inner
                .split(',')
                .map(|s| {
                    let s = s.trim();
                    if s.eq_ignore_ascii_case("null") {
                        Value::Null
                    } else {
                        let label = if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                            &s[1..s.len() - 1]
                        } else {
                            s
                        };
                        Value::Text(label.to_string())
                    }
                })
                .collect();
            Some(Value::Array(values))
        }
        PgValueFormat::Binary => {
            let bytes = raw.as_bytes().ok()?;
            let mut buf = bytes;

            let ndim = read_i32(&mut buf)?;
            let _flags = read_i32(&mut buf)?;
            let _elem_oid = read_u32(&mut buf)?;

            if ndim == 0 {
                return Some(Value::Array(vec![]));
            }

            let ndim = ndim as usize;
            let mut dims = Vec::with_capacity(ndim);
            for _ in 0..ndim {
                let len = read_i32(&mut buf)?;
                let _lower_bound = read_i32(&mut buf)?;
                if len < 0 {
                    return None;
                }
                dims.push(len as usize);
            }

            let total_elems = dims
                .iter()
                .try_fold(1usize, |acc, &len| acc.checked_mul(len))?;

            let mut values = Vec::with_capacity(total_elems);
            for _ in 0..total_elems {
                let elem_len = read_i32(&mut buf)?;
                if elem_len == -1 {
                    values.push(Value::Null);
                } else {
                    let elem_len = elem_len as usize;
                    if buf.len() < elem_len {
                        return None;
                    }
                    if elem_len == 4 {
                        let oid = u32::from_be_bytes(buf[0..4].try_into().ok()?);
                        if let Some(label) = labels.get(&oid) {
                            values.push(Value::Text(label.clone()));
                        } else {
                            values.push(Value::Text(format!("enum_oid:{}", oid)));
                        }
                    } else {
                        values.push(Value::Null);
                    }
                    buf = &buf[elem_len..];
                }
            }

            fn build_array(iter: &mut std::vec::IntoIter<Value>, dims: &[usize]) -> Option<Value> {
                if dims.is_empty() {
                    return None;
                }
                let len = dims[0];
                if dims.len() == 1 {
                    let mut vals = Vec::with_capacity(len);
                    for _ in 0..len {
                        vals.push(iter.next()?);
                    }
                    return Some(Value::Array(vals));
                }
                let mut vals = Vec::with_capacity(len);
                for _ in 0..len {
                    vals.push(build_array(iter, &dims[1..])?);
                }
                Some(Value::Array(vals))
            }

            let mut iter = values.into_iter();
            build_array(&mut iter, &dims)
        }
    }
}

/// Bind a Value to a Postgres query
pub(crate) fn bind_param<'q>(
    query: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    value: &'q Value,
) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
    match value {
        Value::Null => query.bind(Option::<String>::None),
        Value::Bool(b) => query.bind(b),
        Value::Int(i) => query.bind(i),
        Value::Float(f) => query.bind(f),
        Value::Text(s) => query.bind(s),
        Value::Bytes(b) => query.bind(b),
        Value::Json(j) => query.bind(j),
        Value::Array(_) => query.bind(Option::<String>::None),
    }
}

/// Hot-path conversion using a precomputed per-column decoder.
pub(crate) fn convert_row_with_decoders(
    pg_row: &PgRow,
    decoders: &[PgDecoder],
    enum_labels: &EnumLabelMap,
) -> QRow {
    let mut values = Vec::with_capacity(decoders.len());
    for (idx, decoder) in decoders.iter().enumerate() {
        values.push(decoder.decode(pg_row, idx, enum_labels));
    }
    QRow { values }
}

pub(crate) fn columns_and_rows(
    pg_rows: &[PgRow],
    enum_labels: &EnumLabelMap,
) -> (Vec<ColumnInfo>, Vec<QRow>) {
    if pg_rows.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let columns = get_column_info(&pg_rows[0]);
    let decoders = build_decoders(pg_rows[0].columns());
    let rows = pg_rows
        .iter()
        .map(|r| convert_row_with_decoders(r, &decoders, enum_labels))
        .collect();
    (columns, rows)
}

/// Inspect column type info once per result to pick a fast decoder per column.
pub(crate) fn build_decoders(columns: &[PgColumn]) -> Vec<PgDecoder> {
    columns
        .iter()
        .map(|col| {
            let type_info = col.type_info();
            match type_info.kind() {
                PgTypeKind::Enum(_) => PgDecoder::Fallback,
                PgTypeKind::Array(elem) if matches!(elem.kind(), PgTypeKind::Enum(_)) => {
                    PgDecoder::Fallback
                }
                _ => PgDecoder::for_type(type_info.name()),
            }
        })
        .collect()
}

/// Extracts a value from a PgRow at the given index
pub(crate) fn extract_value(row: &PgRow, idx: usize, enum_labels: &EnumLabelMap) -> Value {
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
        return v.map(Value::Int).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<i32>, _>(idx) {
        return v.map(|i| Value::Int(i as i64)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<i16>, _>(idx) {
        return v.map(|i| Value::Int(i as i64)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
        return v.map(Value::Bool).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
        return v
            .map(|f| {
                if f.is_finite() {
                    Value::Float(f)
                } else {
                    Value::Text(f.to_string())
                }
            })
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f32>, _>(idx) {
        return v
            .map(|f| {
                let f64_val = f as f64;
                if f64_val.is_finite() {
                    Value::Float(f64_val)
                } else {
                    Value::Text(f.to_string())
                }
            })
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<BigDecimal>, _>(idx) {
        return v
            .map(|d| match d.to_f64() {
                Some(f) if f.is_finite() => Value::Float(f),
                _ => Value::Text(d.to_string()),
            })
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Decimal>, _>(idx) {
        return v
            .map(|d| {
                use rust_decimal::prelude::ToPrimitive;
                match d.to_f64() {
                    Some(f) if f.is_finite() => Value::Float(f),
                    _ => Value::Text(d.to_string()),
                }
            })
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Uuid>, _>(idx) {
        return v.map(|u| Value::Text(u.to_string())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return v.map(Value::Text).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
        return v.map(Value::Bytes).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(idx) {
        return v.map(Value::Json).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(idx) {
        return v
            .map(|dt| Value::Text(dt.to_rfc3339()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::DateTime<chrono::FixedOffset>>, _>(idx) {
        return v
            .map(|dt| Value::Text(dt.to_rfc3339()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDateTime>, _>(idx) {
        return v
            .map(|dt| Value::Text(dt.format("%Y-%m-%d %H:%M:%S").to_string()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDate>, _>(idx) {
        return v
            .map(|d| Value::Text(d.format("%Y-%m-%d").to_string()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveTime>, _>(idx) {
        return v
            .map(|t| Value::Text(t.format("%H:%M:%S").to_string()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<i64>>, _>(idx) {
        return v
            .map(|vals| Value::Array(vals.into_iter().map(Value::Int).collect()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<i32>>, _>(idx) {
        return v
            .map(|vals| Value::Array(vals.into_iter().map(|i| Value::Int(i as i64)).collect()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<f64>>, _>(idx) {
        return v
            .map(|vals| Value::Array(vals.into_iter().map(Value::Float).collect()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<f32>>, _>(idx) {
        return v
            .map(|vals| Value::Array(vals.into_iter().map(|f| Value::Float(f as f64)).collect()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<bool>>, _>(idx) {
        return v
            .map(|vals| Value::Array(vals.into_iter().map(Value::Bool).collect()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<String>>, _>(idx) {
        return v
            .map(|vals| Value::Array(vals.into_iter().map(Value::Text).collect()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<Uuid>>, _>(idx) {
        return v
            .map(|vals| {
                Value::Array(
                    vals.into_iter()
                        .map(|u| Value::Text(u.to_string()))
                        .collect(),
                )
            })
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<serde_json::Value>>, _>(idx) {
        return v
            .map(|vals| Value::Array(vals.into_iter().map(Value::Json).collect()))
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<Option<String>>>, _>(idx) {
        return v
            .map(|vals| {
                Value::Array(
                    vals.into_iter()
                        .map(|item| item.map(Value::Text).unwrap_or(Value::Null))
                        .collect(),
                )
            })
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<Option<i64>>>, _>(idx) {
        return v
            .map(|vals| {
                Value::Array(
                    vals.into_iter()
                        .map(|item| item.map(Value::Int).unwrap_or(Value::Null))
                        .collect(),
                )
            })
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Vec<Option<f64>>>, _>(idx) {
        return v
            .map(|vals| {
                Value::Array(
                    vals.into_iter()
                        .map(|item| item.map(Value::Float).unwrap_or(Value::Null))
                        .collect(),
                )
            })
            .unwrap_or(Value::Null);
    }

    if let Ok(raw) = row.try_get_raw(idx) {
        let col = row.columns().get(idx);
        if let Some(col) = col {
            let type_info = col.type_info();
            match type_info.kind() {
                PgTypeKind::Enum(_) => {
                    if let Some(value) = decode_enum_value(&raw, enum_labels) {
                        return value;
                    }
                }
                PgTypeKind::Array(elem_type) => {
                    if matches!(elem_type.kind(), PgTypeKind::Enum(_)) {
                        if let Some(value) = decode_enum_array_binary(&raw, enum_labels) {
                            return value;
                        }
                    }
                }
                _ => {}
            }
        }

        if !raw.is_null() {
            if let Ok(text) = raw.as_str() {
                return Value::Text(text.to_string());
            }
            if let Ok(bytes) = raw.as_bytes() {
                if !bytes.is_empty() {
                    return Value::Text(String::from_utf8_lossy(bytes).to_string());
                }
            }
        }
    }
    Value::Null
}

/// Gets column info from a PgRow
pub(crate) fn get_column_info(row: &PgRow) -> Vec<ColumnInfo> {
    row.columns()
        .iter()
        .map(|col| ColumnInfo {
            name: col.name().into(),
            data_type: col.type_info().name().into(),
            nullable: true,
        })
        .collect()
}

/// Per-column typed decoder. Built once per result from `type_info().name()`;
#[derive(Clone, Copy)]
pub(crate) enum PgDecoder {
    Int8,
    Int4,
    Int2,
    Bool,
    Float8,
    Float4,
    Numeric,
    Uuid,
    Text,
    Bytea,
    Json,
    TimestampTz,
    Timestamp,
    Date,
    Time,
    ArrayInt8,
    ArrayInt4,
    ArrayFloat8,
    ArrayFloat4,
    ArrayBool,
    ArrayText,
    ArrayUuid,
    ArrayJson,
    Fallback,
}

impl PgDecoder {
    fn for_type(name: &str) -> Self {
        // sqlx's Postgres type_info().name() returns names like "INT4", "BOOL",
        // "VARCHAR", "_INT4" for arrays. Unknown names → Fallback.
        match name {
            "INT8" | "BIGSERIAL" => Self::Int8,
            "INT4" | "SERIAL" | "OID" => Self::Int4,
            "INT2" | "SMALLSERIAL" => Self::Int2,
            "BOOL" => Self::Bool,
            "FLOAT8" => Self::Float8,
            "FLOAT4" => Self::Float4,
            "NUMERIC" | "MONEY" => Self::Numeric,
            "UUID" => Self::Uuid,
            "TEXT" | "VARCHAR" | "CHAR" | "BPCHAR" | "NAME" | "CITEXT" => Self::Text,
            "BYTEA" => Self::Bytea,
            "JSON" | "JSONB" => Self::Json,
            "TIMESTAMPTZ" => Self::TimestampTz,
            "TIMESTAMP" => Self::Timestamp,
            "DATE" => Self::Date,
            "TIME" | "TIMETZ" => Self::Time,
            // Arrays are named with a "_" prefix in Postgres catalog; sqlx
            // surfaces them either as "_INT4" / "_TEXT" / ... or (depending
            // on version) "INT4[]" / "TEXT[]".
            "INT8[]" | "_INT8" => Self::ArrayInt8,
            "INT4[]" | "_INT4" | "OID[]" | "_OID" => Self::ArrayInt4,
            "FLOAT8[]" | "_FLOAT8" => Self::ArrayFloat8,
            "FLOAT4[]" | "_FLOAT4" => Self::ArrayFloat4,
            "BOOL[]" | "_BOOL" => Self::ArrayBool,
            "TEXT[]" | "_TEXT" | "VARCHAR[]" | "_VARCHAR" => Self::ArrayText,
            "UUID[]" | "_UUID" => Self::ArrayUuid,
            "JSON[]" | "_JSON" | "JSONB[]" | "_JSONB" => Self::ArrayJson,
            _ => Self::Fallback,
        }
    }

    fn decode(self, row: &PgRow, idx: usize, enum_labels: &EnumLabelMap) -> Value {
        match self {
            Self::Int8 => match row.try_get::<Option<i64>, _>(idx) {
                Ok(Some(v)) => Value::Int(v),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::Int4 => match row.try_get::<Option<i32>, _>(idx) {
                Ok(Some(v)) => Value::Int(v as i64),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::Int2 => match row.try_get::<Option<i16>, _>(idx) {
                Ok(Some(v)) => Value::Int(v as i64),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::Bool => match row.try_get::<Option<bool>, _>(idx) {
                Ok(Some(v)) => Value::Bool(v),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::Float8 => match row.try_get::<Option<f64>, _>(idx) {
                Ok(Some(f)) => {
                    if f.is_finite() {
                        Value::Float(f)
                    } else {
                        Value::Text(f.to_string())
                    }
                }
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::Float4 => match row.try_get::<Option<f32>, _>(idx) {
                Ok(Some(f)) => {
                    let f64_val = f as f64;
                    if f64_val.is_finite() {
                        Value::Float(f64_val)
                    } else {
                        Value::Text(f.to_string())
                    }
                }
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::Numeric => {
                // NUMERIC decodes to BigDecimal; try it first, then Decimal as
                // a secondary path (some sqlx versions accept either).
                match row.try_get::<Option<BigDecimal>, _>(idx) {
                    Ok(Some(d)) => match d.to_f64() {
                        Some(f) if f.is_finite() => Value::Float(f),
                        _ => Value::Text(d.to_string()),
                    },
                    Ok(None) => Value::Null,
                    Err(_) => match row.try_get::<Option<Decimal>, _>(idx) {
                        Ok(Some(d)) => {
                            use rust_decimal::prelude::ToPrimitive;
                            match d.to_f64() {
                                Some(f) if f.is_finite() => Value::Float(f),
                                _ => Value::Text(d.to_string()),
                            }
                        }
                        Ok(None) => Value::Null,
                        Err(_) => extract_value(row, idx, enum_labels),
                    },
                }
            }
            Self::Uuid => match row.try_get::<Option<Uuid>, _>(idx) {
                Ok(Some(v)) => Value::Text(v.to_string()),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::Text => match row.try_get::<Option<String>, _>(idx) {
                Ok(Some(v)) => Value::Text(v),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::Bytea => match row.try_get::<Option<Vec<u8>>, _>(idx) {
                Ok(Some(v)) => Value::Bytes(v),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::Json => match row.try_get::<Option<serde_json::Value>, _>(idx) {
                Ok(Some(v)) => Value::Json(v),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::TimestampTz => {
                if let Ok(opt) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(idx) {
                    return opt
                        .map(|dt| Value::Text(dt.to_rfc3339()))
                        .unwrap_or(Value::Null);
                }
                if let Ok(opt) =
                    row.try_get::<Option<chrono::DateTime<chrono::FixedOffset>>, _>(idx)
                {
                    return opt
                        .map(|dt| Value::Text(dt.to_rfc3339()))
                        .unwrap_or(Value::Null);
                }
                extract_value(row, idx, enum_labels)
            }
            Self::Timestamp => match row.try_get::<Option<chrono::NaiveDateTime>, _>(idx) {
                Ok(Some(v)) => Value::Text(v.format("%Y-%m-%d %H:%M:%S").to_string()),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::Date => match row.try_get::<Option<chrono::NaiveDate>, _>(idx) {
                Ok(Some(v)) => Value::Text(v.format("%Y-%m-%d").to_string()),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::Time => match row.try_get::<Option<chrono::NaiveTime>, _>(idx) {
                Ok(Some(v)) => Value::Text(v.format("%H:%M:%S").to_string()),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::ArrayInt8 => match row.try_get::<Option<Vec<i64>>, _>(idx) {
                Ok(Some(vals)) => Value::Array(vals.into_iter().map(Value::Int).collect()),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::ArrayInt4 => match row.try_get::<Option<Vec<i32>>, _>(idx) {
                Ok(Some(vals)) => {
                    Value::Array(vals.into_iter().map(|i| Value::Int(i as i64)).collect())
                }
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::ArrayFloat8 => match row.try_get::<Option<Vec<f64>>, _>(idx) {
                Ok(Some(vals)) => Value::Array(vals.into_iter().map(Value::Float).collect()),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::ArrayFloat4 => match row.try_get::<Option<Vec<f32>>, _>(idx) {
                Ok(Some(vals)) => {
                    Value::Array(vals.into_iter().map(|f| Value::Float(f as f64)).collect())
                }
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::ArrayBool => match row.try_get::<Option<Vec<bool>>, _>(idx) {
                Ok(Some(vals)) => Value::Array(vals.into_iter().map(Value::Bool).collect()),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::ArrayText => match row.try_get::<Option<Vec<String>>, _>(idx) {
                Ok(Some(vals)) => Value::Array(vals.into_iter().map(Value::Text).collect()),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::ArrayUuid => match row.try_get::<Option<Vec<Uuid>>, _>(idx) {
                Ok(Some(vals)) => Value::Array(
                    vals.into_iter()
                        .map(|u| Value::Text(u.to_string()))
                        .collect(),
                ),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::ArrayJson => match row.try_get::<Option<Vec<serde_json::Value>>, _>(idx) {
                Ok(Some(vals)) => Value::Array(vals.into_iter().map(Value::Json).collect()),
                Ok(None) => Value::Null,
                Err(_) => extract_value(row, idx, enum_labels),
            },
            Self::Fallback => extract_value(row, idx, enum_labels),
        }
    }
}

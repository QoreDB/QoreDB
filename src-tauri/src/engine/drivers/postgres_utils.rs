//! PostgreSQL driver helpers

use std::collections::HashMap;

use sqlx::postgres::{PgColumn, PgRow, PgTypeKind, PgValueFormat, Postgres};
use sqlx::{Column, Executor, Row, TypeInfo, ValueRef};
use uuid::Uuid;
use rust_decimal::Decimal;
use bigdecimal::BigDecimal;
use bigdecimal::ToPrimitive as BigDecimalToPrimitive;

use crate::engine::error::{EngineError, EngineResult};
use crate::engine::types::{ColumnInfo, Row as QRow, Value};

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

            fn build_array(
                iter: &mut std::vec::IntoIter<Value>,
                dims: &[usize],
            ) -> Option<Value> {
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

/// Converts a SQLx row to the universal Row type
pub(crate) fn convert_row(pg_row: &PgRow, enum_labels: &EnumLabelMap) -> QRow {
    let values: Vec<Value> = pg_row
        .columns()
        .iter()
        .map(|col| extract_value(pg_row, col.ordinal(), enum_labels))
        .collect();

    QRow { values }
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
        return v.map(|dt| Value::Text(dt.to_rfc3339())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<chrono::DateTime<chrono::FixedOffset>>, _>(idx) {
        return v.map(|dt| Value::Text(dt.to_rfc3339())).unwrap_or(Value::Null);
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
                Value::Array(vals.into_iter().map(|u| Value::Text(u.to_string())).collect())
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
            name: col.name().to_string(),
            data_type: col.type_info().name().to_string(),
            nullable: true,
        })
        .collect()
}

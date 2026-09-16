// SPDX-License-Identifier: Apache-2.0

//! CQL type specs and value decoding.
//!
//! This is the one part of the driver where a mistake is silent: a misread
//! value renders in the grid without raising anything. Every branch below is
//! covered by a unit test built from the wire encoding the server actually
//! sends, not from a round-trip through our own writer.

use std::net::{Ipv4Addr, Ipv6Addr};

use bigdecimal::num_bigint::{BigInt, Sign};
use chrono::{DateTime, NaiveDate, SecondsFormat};
use qore_core::error::{EngineError, EngineResult};
use qore_core::types::Value;

use super::frame::Reader;

#[derive(Clone, Debug, PartialEq)]
pub enum CqlType {
    Custom(String),
    Ascii,
    Bigint,
    Blob,
    Boolean,
    Counter,
    Decimal,
    Double,
    Float,
    Int,
    Timestamp,
    Uuid,
    Varchar,
    Varint,
    Timeuuid,
    Inet,
    Date,
    Time,
    Smallint,
    Tinyint,
    Duration,
    List(Box<CqlType>),
    Map(Box<CqlType>, Box<CqlType>),
    Set(Box<CqlType>),
    Udt {
        name: String,
        fields: Vec<(String, CqlType)>,
    },
    Tuple(Vec<CqlType>),
}

impl CqlType {
    /// CQL spelling of the type, for `ColumnInfo::data_type`. Matches what
    /// `DESCRIBE TABLE` shows so a user can compare the two without translating.
    pub fn name(&self) -> String {
        match self {
            CqlType::Custom(class) => class
                .rsplit('.')
                .next()
                .unwrap_or(class.as_str())
                .to_string(),
            CqlType::Ascii => "ascii".into(),
            CqlType::Bigint => "bigint".into(),
            CqlType::Blob => "blob".into(),
            CqlType::Boolean => "boolean".into(),
            CqlType::Counter => "counter".into(),
            CqlType::Decimal => "decimal".into(),
            CqlType::Double => "double".into(),
            CqlType::Float => "float".into(),
            CqlType::Int => "int".into(),
            CqlType::Timestamp => "timestamp".into(),
            CqlType::Uuid => "uuid".into(),
            CqlType::Varchar => "text".into(),
            CqlType::Varint => "varint".into(),
            CqlType::Timeuuid => "timeuuid".into(),
            CqlType::Inet => "inet".into(),
            CqlType::Date => "date".into(),
            CqlType::Time => "time".into(),
            CqlType::Smallint => "smallint".into(),
            CqlType::Tinyint => "tinyint".into(),
            CqlType::Duration => "duration".into(),
            CqlType::List(inner) => format!("list<{}>", inner.name()),
            CqlType::Set(inner) => format!("set<{}>", inner.name()),
            CqlType::Map(k, v) => format!("map<{}, {}>", k.name(), v.name()),
            CqlType::Udt { name, .. } => name.clone(),
            CqlType::Tuple(items) => {
                let inner: Vec<String> = items.iter().map(CqlType::name).collect();
                format!("tuple<{}>", inner.join(", "))
            }
        }
    }
}

/// Reads an `<option>` from a RESULT metadata block.
pub fn read_type(r: &mut Reader<'_>) -> EngineResult<CqlType> {
    let id = r.u16()?;
    Ok(match id {
        // `duration` only got its own id in protocol v5; a v4 server sends it
        // as a custom type named after its Java marshaller.
        0x0000 => match r.string()?.as_str() {
            "org.apache.cassandra.db.marshal.DurationType" => CqlType::Duration,
            class => CqlType::Custom(class.to_string()),
        },
        0x0001 => CqlType::Ascii,
        0x0002 => CqlType::Bigint,
        0x0003 => CqlType::Blob,
        0x0004 => CqlType::Boolean,
        0x0005 => CqlType::Counter,
        0x0006 => CqlType::Decimal,
        0x0007 => CqlType::Double,
        0x0008 => CqlType::Float,
        0x0009 => CqlType::Int,
        0x000B => CqlType::Timestamp,
        0x000C => CqlType::Uuid,
        0x000D => CqlType::Varchar,
        0x000E => CqlType::Varint,
        0x000F => CqlType::Timeuuid,
        0x0010 => CqlType::Inet,
        0x0011 => CqlType::Date,
        0x0012 => CqlType::Time,
        0x0013 => CqlType::Smallint,
        0x0014 => CqlType::Tinyint,
        0x0015 => CqlType::Duration,
        0x0020 => CqlType::List(Box::new(read_type(r)?)),
        0x0021 => {
            let key = read_type(r)?;
            let value = read_type(r)?;
            CqlType::Map(Box::new(key), Box::new(value))
        }
        0x0022 => CqlType::Set(Box::new(read_type(r)?)),
        0x0030 => {
            let _keyspace = r.string()?;
            let name = r.string()?;
            let count = r.u16()? as usize;
            let mut fields = Vec::with_capacity(count);
            for _ in 0..count {
                let field = r.string()?;
                fields.push((field, read_type(r)?));
            }
            CqlType::Udt { name, fields }
        }
        0x0031 => {
            let count = r.u16()? as usize;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(read_type(r)?);
            }
            CqlType::Tuple(items)
        }
        other => {
            return Err(EngineError::internal(format!(
                "Unsupported CQL type id 0x{other:04X}"
            )));
        }
    })
}

/// Decodes one cell. `None` is the wire's NULL and stays distinct from an empty
/// blob or an empty collection.
pub fn decode(ty: &CqlType, raw: Option<&[u8]>) -> EngineResult<Value> {
    let Some(bytes) = raw else {
        return Ok(Value::Null);
    };
    Ok(match ty {
        CqlType::Ascii | CqlType::Varchar => Value::Text(
            String::from_utf8(bytes.to_vec())
                .map_err(|_| EngineError::internal("CQL text column is not valid UTF-8"))?,
        ),
        CqlType::Custom(_) | CqlType::Blob => Value::Bytes(bytes.to_vec()),
        CqlType::Boolean => Value::Bool(bytes.first().is_some_and(|b| *b != 0)),
        CqlType::Tinyint => Value::Int(fixed::<1>(bytes, "tinyint")?[0] as i8 as i64),
        CqlType::Smallint => Value::Int(i16::from_be_bytes(fixed::<2>(bytes, "smallint")?) as i64),
        CqlType::Int => Value::Int(i32::from_be_bytes(fixed::<4>(bytes, "int")?) as i64),
        CqlType::Bigint | CqlType::Counter => {
            Value::Int(i64::from_be_bytes(fixed::<8>(bytes, "bigint")?))
        }
        CqlType::Float => Value::Float(f32::from_be_bytes(fixed::<4>(bytes, "float")?) as f64),
        CqlType::Double => Value::Float(f64::from_be_bytes(fixed::<8>(bytes, "double")?)),
        CqlType::Uuid | CqlType::Timeuuid => Value::Text(format_uuid(bytes)?),
        CqlType::Varint => Value::Text(BigInt::from_signed_bytes_be(bytes).to_string()),
        CqlType::Decimal => Value::Text(decode_decimal(bytes)?),
        CqlType::Timestamp => {
            let millis = i64::from_be_bytes(fixed::<8>(bytes, "timestamp")?);
            match DateTime::from_timestamp_millis(millis) {
                Some(dt) => Value::Text(dt.to_rfc3339_opts(SecondsFormat::Millis, true)),
                // Out of chrono's range: keep the raw epoch rather than invent one.
                None => Value::Int(millis),
            }
        }
        CqlType::Date => Value::Text(decode_date(bytes)?),
        CqlType::Time => Value::Text(decode_time(bytes)?),
        CqlType::Inet => Value::Text(decode_inet(bytes)?),
        CqlType::Duration => Value::Text(decode_duration(bytes)?),
        CqlType::List(inner) | CqlType::Set(inner) => Value::Array(decode_sequence(inner, bytes)?),
        CqlType::Map(key, value) => Value::Json(decode_map(key, value, bytes)?),
        CqlType::Tuple(items) => {
            let mut r = Reader::new(bytes);
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                // A tuple may arrive short when trailing fields were never set.
                if r.remaining() == 0 {
                    out.push(Value::Null);
                    continue;
                }
                out.push(decode(item, r.bytes()?)?);
            }
            Value::Array(out)
        }
        CqlType::Udt { fields, .. } => {
            let mut r = Reader::new(bytes);
            let mut object = serde_json::Map::with_capacity(fields.len());
            for (name, field_ty) in fields {
                let value = if r.remaining() == 0 {
                    Value::Null
                } else {
                    decode(field_ty, r.bytes()?)?
                };
                object.insert(name.clone(), to_json(value));
            }
            Value::Json(serde_json::Value::Object(object))
        }
    })
}

/// Encodes a bound value for EXECUTE. Binding rather than interpolating is what
/// keeps a value out of the statement text: there is no CQL literal escaping
/// anywhere in this driver, and there must not be.
///
/// The grid hands back strings for most cells, so a `Text` is accepted wherever
/// it parses into the column's type; anything that does not parse is refused
/// rather than coerced into a value the user did not type.
pub fn encode(ty: &CqlType, value: &Value) -> EngineResult<Option<Vec<u8>>> {
    if matches!(value, Value::Null) {
        return Ok(None);
    }
    let bytes = match ty {
        CqlType::Ascii | CqlType::Varchar => as_text(value).into_bytes(),
        CqlType::Blob => match value {
            Value::Bytes(b) => b.clone(),
            other => decode_hex(&as_text(other))?,
        },
        CqlType::Boolean => vec![u8::from(as_bool(value)?)],
        CqlType::Tinyint => {
            vec![i8::try_from(as_int(value)?).map_err(|_| out_of_range("tinyint", value))? as u8]
        }
        CqlType::Smallint => i16::try_from(as_int(value)?)
            .map_err(|_| out_of_range("smallint", value))?
            .to_be_bytes()
            .to_vec(),
        CqlType::Int => i32::try_from(as_int(value)?)
            .map_err(|_| out_of_range("int", value))?
            .to_be_bytes()
            .to_vec(),
        CqlType::Bigint | CqlType::Counter => as_int(value)?.to_be_bytes().to_vec(),
        CqlType::Float => (as_float(value)? as f32).to_be_bytes().to_vec(),
        CqlType::Double => as_float(value)?.to_be_bytes().to_vec(),
        CqlType::Uuid | CqlType::Timeuuid => uuid::Uuid::parse_str(as_text(value).trim())
            .map_err(|_| invalid("uuid", value))?
            .into_bytes()
            .to_vec(),
        CqlType::Varint => parse_bigint(&as_text(value))?.to_signed_bytes_be(),
        CqlType::Decimal => encode_decimal(&as_text(value))?,
        CqlType::Timestamp => encode_timestamp(value)?.to_be_bytes().to_vec(),
        CqlType::Date => encode_date(&as_text(value))?.to_be_bytes().to_vec(),
        CqlType::Time => encode_time(&as_text(value))?.to_be_bytes().to_vec(),
        CqlType::Inet => match as_text(value)
            .trim()
            .parse::<std::net::IpAddr>()
            .map_err(|_| invalid("inet", value))?
        {
            std::net::IpAddr::V4(v4) => v4.octets().to_vec(),
            std::net::IpAddr::V6(v6) => v6.octets().to_vec(),
        },
        CqlType::List(inner) | CqlType::Set(inner) => encode_sequence(inner, value)?,
        CqlType::Map(key, val) => encode_map(key, val, value)?,
        CqlType::Tuple(_) | CqlType::Udt { .. } | CqlType::Duration | CqlType::Custom(_) => {
            return Err(EngineError::not_supported(format!(
                "Binding a {} value is not implemented; edit the row with CQL instead",
                ty.name()
            )));
        }
    };
    Ok(Some(bytes))
}

fn invalid(what: &str, value: &Value) -> EngineError {
    EngineError::validation(format!("`{}` is not a valid {what}", as_text(value)))
}

fn out_of_range(what: &str, value: &Value) -> EngineError {
    EngineError::validation(format!("`{}` does not fit in a {what}", as_text(value)))
}

fn as_text(value: &Value) -> String {
    match value {
        Value::Text(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        other => to_json(other.clone()).to_string(),
    }
}

fn as_int(value: &Value) -> EngineResult<i64> {
    match value {
        Value::Int(i) => Ok(*i),
        Value::Bool(b) => Ok(i64::from(*b)),
        Value::Float(f) if f.fract() == 0.0 => Ok(*f as i64),
        other => as_text(other)
            .trim()
            .parse::<i64>()
            .map_err(|_| invalid("integer", other)),
    }
}

fn as_float(value: &Value) -> EngineResult<f64> {
    match value {
        Value::Float(f) => Ok(*f),
        Value::Int(i) => Ok(*i as f64),
        other => as_text(other)
            .trim()
            .parse::<f64>()
            .map_err(|_| invalid("number", other)),
    }
}

fn as_bool(value: &Value) -> EngineResult<bool> {
    match value {
        Value::Bool(b) => Ok(*b),
        Value::Int(i) => Ok(*i != 0),
        other => match as_text(other).trim().to_ascii_lowercase().as_str() {
            "true" | "t" | "1" | "yes" => Ok(true),
            "false" | "f" | "0" | "no" => Ok(false),
            _ => Err(invalid("boolean", other)),
        },
    }
}

fn parse_bigint(text: &str) -> EngineResult<BigInt> {
    text.trim()
        .parse::<BigInt>()
        .map_err(|_| EngineError::validation(format!("`{text}` is not a valid varint")))
}

fn decode_hex(text: &str) -> EngineResult<Vec<u8>> {
    let trimmed = text.trim().trim_start_matches("0x");
    if !trimmed.len().is_multiple_of(2) {
        return Err(EngineError::validation(
            "A blob literal needs an even number of hex digits",
        ));
    }
    (0..trimmed.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&trimmed[i..i + 2], 16)
                .map_err(|_| EngineError::validation(format!("`{text}` is not a hex blob")))
        })
        .collect()
}

fn encode_decimal(text: &str) -> EngineResult<Vec<u8>> {
    let trimmed = text.trim();
    let (digits, scale) = match trimmed.split_once('.') {
        Some((whole, frac)) => (format!("{whole}{frac}"), frac.len() as i32),
        None => (trimmed.to_string(), 0),
    };
    let unscaled = parse_bigint(&digits)
        .map_err(|_| EngineError::validation(format!("`{text}` is not a valid decimal")))?;
    let mut out = scale.to_be_bytes().to_vec();
    out.extend_from_slice(&unscaled.to_signed_bytes_be());
    Ok(out)
}

fn encode_timestamp(value: &Value) -> EngineResult<i64> {
    if let Value::Int(millis) = value {
        return Ok(*millis);
    }
    let text = as_text(value);
    let trimmed = text.trim();
    if let Ok(millis) = trimmed.parse::<i64>() {
        return Ok(millis);
    }
    DateTime::parse_from_rfc3339(trimmed)
        .map(|dt| dt.timestamp_millis())
        .map_err(|_| EngineError::validation(format!("`{trimmed}` is not an RFC 3339 timestamp")))
}

fn encode_date(text: &str) -> EngineResult<u32> {
    let parsed = text
        .trim()
        .parse::<NaiveDate>()
        .map_err(|_| EngineError::validation(format!("`{text}` is not a YYYY-MM-DD date")))?;
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("1970-01-01 is a date");
    let days = (parsed - epoch).num_days();
    u32::try_from(days + i64::from(u32::MAX / 2) + 1)
        .map_err(|_| EngineError::validation(format!("`{text}` is outside the CQL date range")))
}

fn encode_time(text: &str) -> EngineResult<i64> {
    let trimmed = text.trim();
    let (hms, fraction) = match trimmed.split_once('.') {
        Some((hms, frac)) => (hms, frac),
        None => (trimmed, ""),
    };
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.len() != 3 {
        return Err(EngineError::validation(format!(
            "`{text}` is not an HH:MM:SS time"
        )));
    }
    let bad = || EngineError::validation(format!("`{text}` is not a valid time"));
    let hours: i64 = parts[0].parse().map_err(|_| bad())?;
    let minutes: i64 = parts[1].parse().map_err(|_| bad())?;
    let seconds: i64 = parts[2].parse().map_err(|_| bad())?;
    if !(0..24).contains(&hours) || !(0..60).contains(&minutes) || !(0..60).contains(&seconds) {
        return Err(bad());
    }
    let nanos: i64 = if fraction.is_empty() {
        0
    } else {
        let padded = format!("{fraction:0<9}");
        padded
            .get(..9)
            .ok_or_else(bad)?
            .parse()
            .map_err(|_| bad())?
    };
    Ok(((hours * 3600 + minutes * 60 + seconds) * 1_000_000_000) + nanos)
}

fn encode_sequence(inner: &CqlType, value: &Value) -> EngineResult<Vec<u8>> {
    let items = match value {
        Value::Array(items) => items.clone(),
        Value::Json(serde_json::Value::Array(items)) => {
            items.iter().cloned().map(from_json).collect()
        }
        other => {
            return Err(EngineError::validation(format!(
                "`{}` is not a list",
                as_text(other)
            )));
        }
    };
    let mut out = (items.len() as i32).to_be_bytes().to_vec();
    for item in &items {
        match encode(inner, item)? {
            Some(bytes) => {
                out.extend_from_slice(&(bytes.len() as i32).to_be_bytes());
                out.extend_from_slice(&bytes);
            }
            None => out.extend_from_slice(&(-1i32).to_be_bytes()),
        }
    }
    Ok(out)
}

fn encode_map(key: &CqlType, val: &CqlType, value: &Value) -> EngineResult<Vec<u8>> {
    let Value::Json(serde_json::Value::Object(entries)) = value else {
        return Err(EngineError::validation(format!(
            "`{}` is not a map",
            as_text(value)
        )));
    };
    let mut out = (entries.len() as i32).to_be_bytes().to_vec();
    for (k, v) in entries {
        for bytes in [
            encode(key, &Value::Text(k.clone()))?,
            encode(val, &from_json(v.clone()))?,
        ] {
            match bytes {
                Some(b) => {
                    out.extend_from_slice(&(b.len() as i32).to_be_bytes());
                    out.extend_from_slice(&b);
                }
                None => out.extend_from_slice(&(-1i32).to_be_bytes()),
            }
        }
    }
    Ok(out)
}

fn from_json(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => Value::Int(i),
            None => Value::Float(n.as_f64().unwrap_or_default()),
        },
        serde_json::Value::String(s) => Value::Text(s),
        other => Value::Json(other),
    }
}

fn fixed<const N: usize>(bytes: &[u8], what: &str) -> EngineResult<[u8; N]> {
    bytes.try_into().map_err(|_| {
        EngineError::internal(format!(
            "CQL {what} column carries {} bytes, expected {N}",
            bytes.len()
        ))
    })
}

fn decode_sequence(inner: &CqlType, bytes: &[u8]) -> EngineResult<Vec<Value>> {
    let mut r = Reader::new(bytes);
    let count = r.i32()?;
    if count < 0 {
        return Err(EngineError::internal("Negative CQL collection length"));
    }
    let mut out = Vec::with_capacity(count.min(1024) as usize);
    for _ in 0..count {
        out.push(decode(inner, r.bytes()?)?);
    }
    Ok(out)
}

fn decode_map(key: &CqlType, value: &CqlType, bytes: &[u8]) -> EngineResult<serde_json::Value> {
    let mut r = Reader::new(bytes);
    let count = r.i32()?;
    if count < 0 {
        return Err(EngineError::internal("Negative CQL map length"));
    }
    let mut object = serde_json::Map::with_capacity(count.min(1024) as usize);
    for _ in 0..count {
        let k = decode(key, r.bytes()?)?;
        let v = decode(value, r.bytes()?)?;
        object.insert(map_key(&k), to_json(v));
    }
    Ok(serde_json::Value::Object(object))
}

/// JSON objects only key on strings. A CQL map may key on anything, so the key
/// is rendered the way the grid would show it rather than dropped.
fn map_key(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Text(s) => s.clone(),
        Value::Bytes(b) => b.iter().map(|byte| format!("{byte:02x}")).collect(),
        other => to_json(other.clone()).to_string(),
    }
}

fn to_json(value: Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::Int(i) => serde_json::Value::from(i),
        Value::Float(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::Text(s) => serde_json::Value::String(s),
        Value::Bytes(b) => {
            serde_json::Value::String(b.iter().map(|byte| format!("{byte:02x}")).collect())
        }
        Value::Json(j) => j,
        Value::Array(items) => serde_json::Value::Array(items.into_iter().map(to_json).collect()),
    }
}

fn format_uuid(bytes: &[u8]) -> EngineResult<String> {
    let raw = fixed::<16>(bytes, "uuid")?;
    Ok(uuid::Uuid::from_bytes(raw).to_string())
}

/// `decimal` is a 4-byte scale followed by an arbitrary-precision unscaled
/// integer. Rendering it through `f64` would lose digits, so the decimal point
/// is inserted into the exact digit string instead.
fn decode_decimal(bytes: &[u8]) -> EngineResult<String> {
    if bytes.len() < 4 {
        return Err(EngineError::internal("CQL decimal is shorter than 4 bytes"));
    }
    let scale = i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let unscaled = BigInt::from_signed_bytes_be(&bytes[4..]);
    if scale == 0 {
        return Ok(unscaled.to_string());
    }
    if scale < 0 {
        // A negative scale multiplies; let BigInt do the exact widening.
        let factor = BigInt::from(10u8).pow(scale.unsigned_abs());
        return Ok((unscaled * factor).to_string());
    }

    let negative = unscaled.sign() == Sign::Minus;
    let digits = unscaled.magnitude().to_string();
    let scale = scale as usize;
    let text = if digits.len() > scale {
        let split = digits.len() - scale;
        format!("{}.{}", &digits[..split], &digits[split..])
    } else {
        format!("0.{}{}", "0".repeat(scale - digits.len()), digits)
    };
    Ok(if negative { format!("-{text}") } else { text })
}

/// `date` is days since the epoch, biased by 2^31 so the wire value is unsigned.
fn decode_date(bytes: &[u8]) -> EngineResult<String> {
    let raw = u32::from_be_bytes(fixed::<4>(bytes, "date")?);
    let days = raw as i64 - i64::from(u32::MAX / 2) - 1;
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("1970-01-01 is a date");
    epoch
        .checked_add_signed(chrono::Duration::days(days))
        .map(|d| d.to_string())
        .ok_or_else(|| EngineError::internal(format!("CQL date is out of range ({days} days)")))
}

/// `time` is nanoseconds since midnight; there is no date part to attach.
fn decode_time(bytes: &[u8]) -> EngineResult<String> {
    let nanos = i64::from_be_bytes(fixed::<8>(bytes, "time")?);
    if !(0..86_400_000_000_000).contains(&nanos) {
        return Err(EngineError::internal(format!(
            "CQL time out of range: {nanos}ns"
        )));
    }
    let seconds = nanos / 1_000_000_000;
    let rest = nanos % 1_000_000_000;
    Ok(format!(
        "{:02}:{:02}:{:02}.{:09}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60,
        rest
    ))
}

fn decode_inet(bytes: &[u8]) -> EngineResult<String> {
    match bytes.len() {
        4 => Ok(Ipv4Addr::from(fixed::<4>(bytes, "inet")?).to_string()),
        16 => Ok(Ipv6Addr::from(fixed::<16>(bytes, "inet")?).to_string()),
        n => Err(EngineError::internal(format!(
            "CQL inet carries {n} bytes, expected 4 or 16"
        ))),
    }
}

/// `duration` is three Cassandra vints: months, days, nanoseconds. It is not the
/// protobuf varint — the count of leading 1-bits in the first byte gives the
/// number of extra bytes, and the result is zigzag-encoded.
fn decode_duration(bytes: &[u8]) -> EngineResult<String> {
    let mut pos = 0usize;
    let months = read_vint(bytes, &mut pos)?;
    let days = read_vint(bytes, &mut pos)?;
    let nanos = read_vint(bytes, &mut pos)?;
    if months == 0 && days == 0 && nanos == 0 {
        return Ok("0s".to_string());
    }
    let mut out = String::new();
    if months != 0 {
        out.push_str(&format!("{months}mo"));
    }
    if days != 0 {
        out.push_str(&format!("{days}d"));
    }
    if nanos != 0 {
        out.push_str(&format!("{nanos}ns"));
    }
    Ok(out)
}

fn read_vint(bytes: &[u8], pos: &mut usize) -> EngineResult<i64> {
    let first = *bytes
        .get(*pos)
        .ok_or_else(|| EngineError::internal("Truncated CQL vint"))?;
    *pos += 1;
    let extra = first.leading_ones() as usize;
    if extra > 8 {
        return Err(EngineError::internal("Malformed CQL vint"));
    }
    // The leading 1-bits and their terminating 0 are the length marker; what is
    // left of the first byte is the most significant part of the value.
    let mut value: u64 = if extra >= 8 {
        0
    } else {
        u64::from(first & (0xFFu8 >> (extra + 1)))
    };
    for _ in 0..extra {
        let byte = *bytes
            .get(*pos)
            .ok_or_else(|| EngineError::internal("Truncated CQL vint"))?;
        *pos += 1;
        value = (value << 8) | u64::from(byte);
    }
    Ok(((value >> 1) as i64) ^ -((value & 1) as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Value` carries a `serde_json::Value` arm and derives no `PartialEq`, so
    /// assertions compare the debug rendering — which still separates every
    /// arm, including `Null` from an empty `Bytes`.
    macro_rules! assert_value {
        ($actual:expr, $expected:expr $(, $($arg:tt)+)?) => {
            assert_eq!(format!("{:?}", $actual), format!("{:?}", $expected) $(, $($arg)+)?)
        };
    }

    fn cell(ty: CqlType, bytes: &[u8]) -> Value {
        decode(&ty, Some(bytes)).expect("decodes")
    }

    #[test]
    fn null_is_not_an_empty_blob() {
        assert_value!(decode(&CqlType::Blob, None).unwrap(), Value::Null);
        assert_value!(
            decode(&CqlType::Blob, Some(&[])).unwrap(),
            Value::Bytes(vec![])
        );
        assert_value!(decode(&CqlType::Varchar, None).unwrap(), Value::Null);
        assert_value!(
            decode(&CqlType::List(Box::new(CqlType::Int)), None).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn signed_integers_keep_their_sign_at_every_width() {
        assert_value!(cell(CqlType::Tinyint, &[0xFF]), Value::Int(-1));
        assert_value!(cell(CqlType::Smallint, &[0xFF, 0xFF]), Value::Int(-1));
        assert_value!(
            cell(CqlType::Int, &[0xFF, 0xFF, 0xFF, 0xFF]),
            Value::Int(-1)
        );
        assert_value!(cell(CqlType::Bigint, &[0xFF; 8]), Value::Int(-1));
        assert_value!(cell(CqlType::Tinyint, &[0x80]), Value::Int(-128));
        assert_value!(
            cell(CqlType::Int, &[0x7F, 0xFF, 0xFF, 0xFF]),
            Value::Int(i32::MAX as i64)
        );
    }

    #[test]
    fn a_wrong_width_is_an_error_not_a_silent_zero() {
        assert!(decode(&CqlType::Int, Some(&[0x00, 0x01])).is_err());
        assert!(decode(&CqlType::Bigint, Some(&[0x00; 4])).is_err());
        assert!(decode(&CqlType::Uuid, Some(&[0x00; 8])).is_err());
    }

    #[test]
    fn floats_decode_from_big_endian_ieee754() {
        assert_value!(
            cell(CqlType::Float, &1.5f32.to_be_bytes()),
            Value::Float(1.5)
        );
        assert_value!(
            cell(CqlType::Double, &(-0.25f64).to_be_bytes()),
            Value::Float(-0.25)
        );
    }

    #[test]
    fn uuid_renders_hyphenated() {
        let raw = [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc,
            0xde, 0xf0,
        ];
        assert_value!(
            cell(CqlType::Uuid, &raw),
            Value::Text("12345678-9abc-def0-1234-56789abcdef0".to_string())
        );
    }

    #[test]
    fn varint_keeps_precision_beyond_i64() {
        // 2^70, which no fixed-width integer would hold.
        let mut bytes = vec![0x40];
        bytes.extend_from_slice(&[0u8; 8]);
        assert_value!(
            cell(CqlType::Varint, &bytes),
            Value::Text("1180591620717411303424".to_string())
        );
        assert_value!(
            cell(CqlType::Varint, &[0xFF]),
            Value::Text("-1".to_string())
        );
        assert_value!(cell(CqlType::Varint, &[0x00]), Value::Text("0".to_string()));
    }

    #[test]
    fn decimal_places_the_point_without_going_through_f64() {
        // scale 2, unscaled 12345 -> 123.45
        let mut bytes = 2i32.to_be_bytes().to_vec();
        bytes.extend_from_slice(&BigInt::from(12345).to_signed_bytes_be());
        assert_value!(cell(CqlType::Decimal, &bytes), Value::Text("123.45".into()));

        // scale larger than the digit count pads on the left.
        let mut bytes = 5i32.to_be_bytes().to_vec();
        bytes.extend_from_slice(&BigInt::from(42).to_signed_bytes_be());
        assert_value!(
            cell(CqlType::Decimal, &bytes),
            Value::Text("0.00042".into())
        );

        // negative values keep the sign outside the point.
        let mut bytes = 2i32.to_be_bytes().to_vec();
        bytes.extend_from_slice(&BigInt::from(-12345).to_signed_bytes_be());
        assert_value!(
            cell(CqlType::Decimal, &bytes),
            Value::Text("-123.45".into())
        );

        // scale 0 is a plain integer.
        let mut bytes = 0i32.to_be_bytes().to_vec();
        bytes.extend_from_slice(&BigInt::from(7).to_signed_bytes_be());
        assert_value!(cell(CqlType::Decimal, &bytes), Value::Text("7".into()));
    }

    #[test]
    fn timestamp_renders_as_rfc3339() {
        assert_value!(
            cell(CqlType::Timestamp, &0i64.to_be_bytes()),
            Value::Text("1970-01-01T00:00:00.000Z".to_string())
        );
        assert_value!(
            cell(CqlType::Timestamp, &1_700_000_000_123i64.to_be_bytes()),
            Value::Text("2023-11-14T22:13:20.123Z".to_string())
        );
    }

    #[test]
    fn date_is_biased_by_two_to_the_31() {
        let epoch = 1u32 << 31;
        assert_value!(
            cell(CqlType::Date, &epoch.to_be_bytes()),
            Value::Text("1970-01-01".to_string())
        );
        assert_value!(
            cell(CqlType::Date, &(epoch + 1).to_be_bytes()),
            Value::Text("1970-01-02".to_string())
        );
        assert_value!(
            cell(CqlType::Date, &(epoch - 1).to_be_bytes()),
            Value::Text("1969-12-31".to_string())
        );
    }

    #[test]
    fn time_is_nanoseconds_since_midnight() {
        assert_value!(
            cell(CqlType::Time, &0i64.to_be_bytes()),
            Value::Text("00:00:00.000000000".to_string())
        );
        let one_pm = 13i64 * 3600 * 1_000_000_000 + 5 * 1_000_000_000 + 7;
        assert_value!(
            cell(CqlType::Time, &one_pm.to_be_bytes()),
            Value::Text("13:00:05.000000007".to_string())
        );
        assert!(decode(&CqlType::Time, Some(&(-1i64).to_be_bytes())).is_err());
    }

    #[test]
    fn inet_handles_both_address_families() {
        assert_value!(
            cell(CqlType::Inet, &[127, 0, 0, 1]),
            Value::Text("127.0.0.1".to_string())
        );
        let mut v6 = [0u8; 16];
        v6[15] = 1;
        assert_value!(cell(CqlType::Inet, &v6), Value::Text("::1".to_string()));
        assert!(decode(&CqlType::Inet, Some(&[1, 2, 3])).is_err());
    }

    #[test]
    fn collections_decode_with_int_sized_counts() {
        // list<int> [1, 2]
        let mut bytes = 2i32.to_be_bytes().to_vec();
        for n in [1i32, 2] {
            bytes.extend_from_slice(&4i32.to_be_bytes());
            bytes.extend_from_slice(&n.to_be_bytes());
        }
        assert_value!(
            cell(CqlType::List(Box::new(CqlType::Int)), &bytes),
            Value::Array(vec![Value::Int(1), Value::Int(2)])
        );

        // An empty collection is an empty array, not NULL.
        assert_value!(
            cell(CqlType::Set(Box::new(CqlType::Int)), &0i32.to_be_bytes()),
            Value::Array(vec![])
        );
    }

    #[test]
    fn a_null_inside_a_collection_survives() {
        let mut bytes = 2i32.to_be_bytes().to_vec();
        bytes.extend_from_slice(&4i32.to_be_bytes());
        bytes.extend_from_slice(&7i32.to_be_bytes());
        bytes.extend_from_slice(&(-1i32).to_be_bytes());
        assert_value!(
            cell(CqlType::List(Box::new(CqlType::Int)), &bytes),
            Value::Array(vec![Value::Int(7), Value::Null])
        );
    }

    #[test]
    fn map_renders_as_a_json_object_even_with_non_string_keys() {
        // map<int, text> {1: 'a'}
        let mut bytes = 1i32.to_be_bytes().to_vec();
        bytes.extend_from_slice(&4i32.to_be_bytes());
        bytes.extend_from_slice(&1i32.to_be_bytes());
        bytes.extend_from_slice(&1i32.to_be_bytes());
        bytes.push(b'a');
        let decoded = cell(
            CqlType::Map(Box::new(CqlType::Int), Box::new(CqlType::Varchar)),
            &bytes,
        );
        assert_value!(decoded, Value::Json(serde_json::json!({ "1": "a" })));
    }

    #[test]
    fn udt_fields_keep_their_names_and_a_short_payload_pads_with_null() {
        let ty = CqlType::Udt {
            name: "address".to_string(),
            fields: vec![
                ("street".to_string(), CqlType::Varchar),
                ("zip".to_string(), CqlType::Int),
            ],
        };
        let mut bytes = 2i32.to_be_bytes().to_vec();
        bytes.extend_from_slice(b"rd");
        bytes.extend_from_slice(&4i32.to_be_bytes());
        bytes.extend_from_slice(&75000i32.to_be_bytes());
        assert_value!(
            decode(&ty, Some(&bytes)).unwrap(),
            Value::Json(serde_json::json!({ "street": "rd", "zip": 75000 }))
        );

        // Trailing fields the server never wrote come back as null.
        let mut short = 2i32.to_be_bytes().to_vec();
        short.extend_from_slice(b"rd");
        assert_value!(
            decode(&ty, Some(&short)).unwrap(),
            Value::Json(serde_json::json!({ "street": "rd", "zip": null }))
        );
    }

    #[test]
    fn tuple_decodes_positionally() {
        let ty = CqlType::Tuple(vec![CqlType::Int, CqlType::Varchar]);
        let mut bytes = 4i32.to_be_bytes().to_vec();
        bytes.extend_from_slice(&9i32.to_be_bytes());
        bytes.extend_from_slice(&1i32.to_be_bytes());
        bytes.push(b'x');
        assert_value!(
            decode(&ty, Some(&bytes)).unwrap(),
            Value::Array(vec![Value::Int(9), Value::Text("x".to_string())])
        );
    }

    #[test]
    fn duration_reads_three_zigzag_vints() {
        // 0 months, 0 days, 0 nanos: three single-byte vints.
        assert_value!(
            cell(CqlType::Duration, &[0x00, 0x00, 0x00]),
            Value::Text("0s".to_string())
        );
        // 1 month (zigzag 2), 2 days (zigzag 4), 3 nanos (zigzag 6).
        assert_value!(
            cell(CqlType::Duration, &[0x02, 0x04, 0x06]),
            Value::Text("1mo2d3ns".to_string())
        );
        // A negative component: -1 zigzags to 1.
        assert_value!(
            cell(CqlType::Duration, &[0x01, 0x00, 0x00]),
            Value::Text("-1mo".to_string())
        );
    }

    fn round_trip(ty: CqlType, input: Value) -> Value {
        let encoded = encode(&ty, &input)
            .unwrap_or_else(|e| panic!("{} should encode: {e}", ty.name()))
            .expect("not null");
        decode(&ty, Some(&encoded)).expect("decodes back")
    }

    #[test]
    fn scalars_survive_an_encode_decode_round_trip() {
        assert_value!(round_trip(CqlType::Int, Value::Int(-42)), Value::Int(-42));
        assert_value!(
            round_trip(CqlType::Bigint, Value::Int(i64::MIN)),
            Value::Int(i64::MIN)
        );
        assert_value!(
            round_trip(CqlType::Tinyint, Value::Int(-128)),
            Value::Int(-128)
        );
        assert_value!(
            round_trip(CqlType::Boolean, Value::Bool(true)),
            Value::Bool(true)
        );
        assert_value!(
            round_trip(CqlType::Double, Value::Float(1.25)),
            Value::Float(1.25)
        );
        assert_value!(
            round_trip(CqlType::Varchar, Value::Text("héllo".into())),
            Value::Text("héllo".into())
        );
        assert_value!(
            round_trip(CqlType::Inet, Value::Text("10.0.0.1".into())),
            Value::Text("10.0.0.1".into())
        );
        assert_value!(
            round_trip(
                CqlType::Uuid,
                Value::Text("12345678-9abc-def0-1234-56789abcdef0".into())
            ),
            Value::Text("12345678-9abc-def0-1234-56789abcdef0".into())
        );
        assert_value!(
            round_trip(CqlType::Date, Value::Text("2024-02-29".into())),
            Value::Text("2024-02-29".into())
        );
        assert_value!(
            round_trip(CqlType::Time, Value::Text("13:00:05.000000007".into())),
            Value::Text("13:00:05.000000007".into())
        );
        assert_value!(
            round_trip(
                CqlType::Timestamp,
                Value::Text("2023-11-14T22:13:20.123Z".into())
            ),
            Value::Text("2023-11-14T22:13:20.123Z".into())
        );
        assert_value!(
            round_trip(
                CqlType::Varint,
                Value::Text("1180591620717411303424".into())
            ),
            Value::Text("1180591620717411303424".into())
        );
        assert_value!(
            round_trip(CqlType::Decimal, Value::Text("-123.45".into())),
            Value::Text("-123.45".into())
        );
    }

    #[test]
    fn a_null_binds_to_the_wire_null_not_to_a_default() {
        assert!(encode(&CqlType::Int, &Value::Null).unwrap().is_none());
        assert!(encode(&CqlType::Varchar, &Value::Null).unwrap().is_none());
    }

    #[test]
    fn the_grid_string_form_of_a_value_is_accepted() {
        // Cells come back from the UI as text; a value that parses is bound,
        // and one that does not is refused rather than coerced.
        assert_value!(
            round_trip(CqlType::Int, Value::Text("7".into())),
            Value::Int(7)
        );
        assert_value!(
            round_trip(CqlType::Boolean, Value::Text("false".into())),
            Value::Bool(false)
        );
        assert!(encode(&CqlType::Int, &Value::Text("seven".into())).is_err());
        assert!(encode(&CqlType::Uuid, &Value::Text("not-a-uuid".into())).is_err());
        assert!(encode(&CqlType::Inet, &Value::Text("999.1.1.1".into())).is_err());
        assert!(encode(&CqlType::Date, &Value::Text("29/02/2024".into())).is_err());
    }

    #[test]
    fn a_value_too_wide_for_its_column_is_refused() {
        assert!(encode(&CqlType::Tinyint, &Value::Int(300)).is_err());
        assert!(encode(&CqlType::Smallint, &Value::Int(70_000)).is_err());
        assert!(encode(&CqlType::Int, &Value::Int(i64::MAX)).is_err());
    }

    #[test]
    fn collections_round_trip_with_their_nulls() {
        let ty = CqlType::List(Box::new(CqlType::Int));
        assert_value!(
            round_trip(ty.clone(), Value::Array(vec![Value::Int(1), Value::Null])),
            Value::Array(vec![Value::Int(1), Value::Null])
        );

        let map = CqlType::Map(Box::new(CqlType::Varchar), Box::new(CqlType::Int));
        assert_value!(
            round_trip(map, Value::Json(serde_json::json!({ "a": 1 }))),
            Value::Json(serde_json::json!({ "a": 1 }))
        );
    }

    #[test]
    fn blob_accepts_bytes_or_a_hex_string() {
        assert_value!(
            round_trip(CqlType::Blob, Value::Bytes(vec![0xDE, 0xAD])),
            Value::Bytes(vec![0xDE, 0xAD])
        );
        assert_value!(
            round_trip(CqlType::Blob, Value::Text("0xdead".into())),
            Value::Bytes(vec![0xDE, 0xAD])
        );
        assert!(encode(&CqlType::Blob, &Value::Text("0xabc".into())).is_err());
    }

    #[test]
    fn types_with_no_binder_say_so_rather_than_guess() {
        let udt = CqlType::Udt {
            name: "address".into(),
            fields: vec![],
        };
        assert!(encode(&udt, &Value::Text("{}".into())).is_err());
        assert!(encode(&CqlType::Duration, &Value::Text("1mo".into())).is_err());
    }

    #[test]
    fn a_v4_server_sends_duration_as_a_custom_type() {
        let class = "org.apache.cassandra.db.marshal.DurationType";
        let mut bytes = 0u16.to_be_bytes().to_vec();
        bytes.extend_from_slice(&(class.len() as u16).to_be_bytes());
        bytes.extend_from_slice(class.as_bytes());
        let ty = read_type(&mut Reader::new(&bytes)).unwrap();
        assert!(matches!(ty, CqlType::Duration));
    }

    #[test]
    fn type_names_read_like_cql() {
        assert_value!(CqlType::Varchar.name(), "text");
        assert_value!(CqlType::List(Box::new(CqlType::Int)).name(), "list<int>");
        assert_value!(
            CqlType::Map(Box::new(CqlType::Varchar), Box::new(CqlType::Bigint)).name(),
            "map<text, bigint>"
        );
        assert_value!(
            CqlType::Tuple(vec![CqlType::Int, CqlType::Uuid]).name(),
            "tuple<int, uuid>"
        );
        assert_value!(
            CqlType::Custom("org.apache.cassandra.db.marshal.FooType".to_string()).name(),
            "FooType"
        );
    }

    #[test]
    fn nested_collection_types_parse_from_metadata() {
        use crate::drivers::cql::frame::Writer;
        let mut w = Writer::new();
        w.u16(0x0021); // map
        w.u16(0x000D); // varchar key
        w.u16(0x0020); // list value
        w.u16(0x0009); // int element
        let buf = w.finish();
        let ty = read_type(&mut Reader::new(&buf)).expect("parses");
        assert_value!(ty.name(), "map<text, list<int>>");
    }
}

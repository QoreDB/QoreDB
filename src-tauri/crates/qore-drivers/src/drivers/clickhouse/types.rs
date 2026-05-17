// SPDX-License-Identifier: Apache-2.0

//! ClickHouse → QoreDB type mapping.
//!
//! ClickHouse exposes a rich type system: integers (Int8..Int128, UInt*), floats,
//! Decimal, String/FixedString, Date/DateTime/DateTime64, UUID, Enum, plus the
//! composite forms `Nullable(T)`, `LowCardinality(T)`, `Array(T)`, `Tuple(...)`,
//! `Map(K,V)`. With `JSONCompactEachRowWithNamesAndTypes` we receive the type
//! string verbatim — we just classify it for the QoreDB column metadata.

use compact_str::CompactString;
use qore_core::types::{ColumnInfo, Value};
use serde_json::Value as JsonValue;

/// Strip the outermost `Nullable(...)` and `LowCardinality(...)` wrappers and
/// return whether the column is nullable. Mirrors how ClickHouse reports
/// nullability — there is no separate column flag.
pub fn unwrap_modifiers(declared: &str) -> (String, bool) {
    let mut current = declared.trim().to_string();
    let mut nullable = false;
    loop {
        let upper = current.to_ascii_uppercase();
        if let Some(rest) = upper.strip_prefix("NULLABLE(") {
            if let Some(stripped) = strip_matching_paren(&current[9..]) {
                current = stripped.to_string();
                nullable = true;
                continue;
            }
            // Fall through if parens are unbalanced (shouldn't happen).
            let _ = rest;
        }
        if let Some(rest) = upper.strip_prefix("LOWCARDINALITY(") {
            if let Some(stripped) = strip_matching_paren(&current[15..]) {
                current = stripped.to_string();
                continue;
            }
            let _ = rest;
        }
        break;
    }
    (current, nullable)
}

fn strip_matching_paren(input: &str) -> Option<&str> {
    if !input.ends_with(')') {
        return None;
    }
    Some(&input[..input.len() - 1])
}

pub fn build_column_info(name: &str, declared_type: &str) -> ColumnInfo {
    let (inner, nullable) = unwrap_modifiers(declared_type);
    ColumnInfo {
        name: CompactString::new(name),
        data_type: CompactString::new(inner),
        nullable,
    }
}

/// Map a JSON cell from ClickHouse's `JSONCompactEachRowWithNamesAndTypes`
/// response to a `Value`.
///
/// The format encodes integers and floats as JSON numbers, but for types that
/// don't fit (Int128, UInt64 over 2^53, Decimal, Date, DateTime, UUID, Enum,
/// FixedString, Tuple, Map) the value is emitted as a JSON string. We try to
/// pick the most informative QoreDB representation per category — `Int` /
/// `Float` for numerics that fit, `Text` for the rest, `Array` for `Array(T)`,
/// and `Json` for nested objects (Tuple/Map serialized objects).
pub fn json_to_value(declared_type: &str, json: &JsonValue) -> Value {
    let (inner, nullable) = unwrap_modifiers(declared_type);
    if matches!(json, JsonValue::Null) {
        // Whether or not the column is nullable, a JSON null maps to Null.
        let _ = nullable;
        return Value::Null;
    }
    json_to_value_inner(&inner, json)
}

fn json_to_value_inner(declared_type: &str, json: &JsonValue) -> Value {
    let upper = declared_type.to_ascii_uppercase();

    if let Some(inner) = strip_prefix_paren(&upper, "ARRAY(") {
        if let JsonValue::Array(items) = json {
            let inner_decl = original_inside(declared_type, "Array(", inner.len());
            return Value::Array(
                items
                    .iter()
                    .map(|v| json_to_value_inner(inner_decl, v))
                    .collect(),
            );
        }
    }

    // Map(K, V) and Tuple(...) — surfaced as JSON to keep structure visible.
    if upper.starts_with("MAP(")
        || upper.starts_with("TUPLE(")
        || upper.starts_with("VARIANT(")
        || upper.starts_with("DYNAMIC")
    {
        return Value::Json(json.clone());
    }

    match json {
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(u) = n.as_u64() {
                if u <= i64::MAX as u64 {
                    Value::Int(u as i64)
                } else {
                    Value::Text(u.to_string())
                }
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Text(n.to_string())
            }
        }
        JsonValue::String(s) => {
            // Numeric ClickHouse types may arrive as strings when they exceed
            // JSON's safe integer range or when the user opted into output
            // strings via setting `output_format_json_quote_64bit_integers=1`.
            if is_integer_type(&upper) {
                if let Ok(i) = s.parse::<i64>() {
                    return Value::Int(i);
                }
            }
            if is_float_type(&upper) {
                if let Ok(f) = s.parse::<f64>() {
                    return Value::Float(f);
                }
            }
            Value::Text(s.clone())
        }
        JsonValue::Null => Value::Null,
        JsonValue::Array(_) | JsonValue::Object(_) => Value::Json(json.clone()),
    }
}

fn is_integer_type(upper: &str) -> bool {
    matches!(
        upper,
        "INT8"
            | "INT16"
            | "INT32"
            | "INT64"
            | "INT128"
            | "INT256"
            | "UINT8"
            | "UINT16"
            | "UINT32"
            | "UINT64"
            | "UINT128"
            | "UINT256"
    )
}

fn is_float_type(upper: &str) -> bool {
    matches!(upper, "FLOAT32" | "FLOAT64") || upper.starts_with("DECIMAL")
}

fn strip_prefix_paren<'a>(upper: &'a str, prefix: &str) -> Option<&'a str> {
    upper
        .strip_prefix(prefix)
        .and_then(|inner| inner.strip_suffix(')'))
}

/// Return the original-cased substring inside the outer `Wrapper(...)`. The
/// upper-case match has already verified that the pattern fits — we just
/// reslice the original to keep type-name casing intact (matters for `String`
/// vs `STRING` in nested errors / display).
fn original_inside<'a>(declared: &'a str, prefix: &str, _inner_len: usize) -> &'a str {
    let start = prefix.len();
    let end = declared.len().saturating_sub(1);
    &declared[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unwraps_nullable() {
        let (t, n) = unwrap_modifiers("Nullable(String)");
        assert_eq!(t, "String");
        assert!(n);
    }

    #[test]
    fn unwraps_low_cardinality_nullable() {
        let (t, n) = unwrap_modifiers("LowCardinality(Nullable(String))");
        assert_eq!(t, "String");
        assert!(n);
    }

    #[test]
    fn keeps_plain_types() {
        let (t, n) = unwrap_modifiers("UInt32");
        assert_eq!(t, "UInt32");
        assert!(!n);
    }

    #[test]
    fn maps_int_number() {
        let v = json_to_value("Int32", &json!(42));
        match v {
            Value::Int(42) => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn maps_uint64_string_to_int() {
        let v = json_to_value("UInt64", &json!("123456789"));
        match v {
            Value::Int(123_456_789) => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn maps_decimal_string_to_float() {
        let v = json_to_value("Decimal(10, 2)", &json!("1.50"));
        match v {
            Value::Float(f) if (f - 1.5).abs() < 1e-9 => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn maps_array_of_strings() {
        let v = json_to_value("Array(String)", &json!(["a", "b"]));
        match v {
            Value::Array(items) => {
                assert_eq!(items.len(), 2);
                matches!(items[0], Value::Text(_));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn maps_nested_array() {
        let v = json_to_value("Array(Array(Int32))", &json!([[1, 2], [3]]));
        match v {
            Value::Array(items) => {
                assert_eq!(items.len(), 2);
                if let Value::Array(inner) = &items[0] {
                    assert_eq!(inner.len(), 2);
                } else {
                    panic!("expected nested array");
                }
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn null_propagates() {
        assert!(matches!(
            json_to_value("Nullable(Int32)", &json!(null)),
            Value::Null
        ));
    }

    #[test]
    fn map_type_kept_as_json() {
        let v = json_to_value("Map(String, Int32)", &json!({"a": 1}));
        assert!(matches!(v, Value::Json(_)));
    }

    #[test]
    fn build_column_info_marks_nullable() {
        let info = build_column_info("name", "Nullable(String)");
        assert!(info.nullable);
        assert_eq!(info.data_type.as_str(), "String");
    }
}

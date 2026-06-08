// SPDX-License-Identifier: Apache-2.0

//! ClickHouse SQL literal formatter for mutations.
//!
//! ClickHouse's HTTP protocol has no client-side parameter binding for
//! arbitrary dynamic SQL — the closest equivalent is the `{name:Type}`
//! placeholder which requires a declared type per parameter. Since QoreDB
//! row mutations carry only `Value` variants (no schema-side type), we
//! format values into safe SQL literals at the call site.
//!
//! String escaping follows ClickHouse's documented rules: backslash escapes
//! `\\`, `\'`, `\b`, `\f`, `\r`, `\n`, `\t`, `\0`. Other characters are
//! kept verbatim. Bytes are emitted as `unhex('…')` so binary payloads
//! survive the round-trip.

use qore_core::types::Value;

pub fn format_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => {
            if *b {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.is_nan() {
                "nan".to_string()
            } else if f.is_infinite() {
                if *f > 0.0 {
                    "inf".to_string()
                } else {
                    "-inf".to_string()
                }
            } else {
                f.to_string()
            }
        }
        Value::Text(s) => format_string(s),
        Value::Bytes(b) => {
            let hex: String = b.iter().map(|byte| format!("{byte:02X}")).collect();
            format!("unhex('{hex}')")
        }
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(format_literal).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Json(j) => format_string(&j.to_string()),
    }
}

fn format_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn null_is_keyword() {
        assert_eq!(format_literal(&Value::Null), "NULL");
    }

    #[test]
    fn bool_is_one_or_zero() {
        assert_eq!(format_literal(&Value::Bool(true)), "1");
        assert_eq!(format_literal(&Value::Bool(false)), "0");
    }

    #[test]
    fn integers_are_bare() {
        assert_eq!(format_literal(&Value::Int(42)), "42");
        assert_eq!(format_literal(&Value::Int(-7)), "-7");
    }

    #[test]
    fn floats_handle_specials() {
        assert_eq!(format_literal(&Value::Float(1.5)), "1.5");
        assert_eq!(format_literal(&Value::Float(f64::NAN)), "nan");
        assert_eq!(format_literal(&Value::Float(f64::INFINITY)), "inf");
        assert_eq!(format_literal(&Value::Float(f64::NEG_INFINITY)), "-inf");
    }

    #[test]
    fn strings_escape_quotes_and_backslashes() {
        assert_eq!(format_literal(&Value::Text("abc".into())), "'abc'");
        assert_eq!(
            format_literal(&Value::Text("it's a test".into())),
            "'it\\'s a test'"
        );
        assert_eq!(
            format_literal(&Value::Text("back\\slash".into())),
            "'back\\\\slash'"
        );
        assert_eq!(
            format_literal(&Value::Text("line\nbreak".into())),
            "'line\\nbreak'"
        );
    }

    #[test]
    fn bytes_emit_unhex() {
        assert_eq!(
            format_literal(&Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF])),
            "unhex('DEADBEEF')"
        );
    }

    #[test]
    fn arrays_recurse() {
        let v = Value::Array(vec![Value::Int(1), Value::Text("two".into()), Value::Null]);
        assert_eq!(format_literal(&v), "[1, 'two', NULL]");
    }

    #[test]
    fn arrays_can_nest() {
        let v = Value::Array(vec![
            Value::Array(vec![Value::Int(1), Value::Int(2)]),
            Value::Array(vec![Value::Int(3)]),
        ]);
        assert_eq!(format_literal(&v), "[[1, 2], [3]]");
    }

    #[test]
    fn json_is_emitted_as_string_literal() {
        let v = Value::Json(json!({"a": 1, "b": "x"}));
        let out = format_literal(&v);
        assert!(out.starts_with('\''));
        assert!(out.ends_with('\''));
        // contents preserve the JSON, with the embedded double-quotes intact
        assert!(out.contains("\"a\":1"));
    }

    #[test]
    fn rejects_sql_injection_via_quotes() {
        // The crucial property: a malicious string can never escape the literal.
        let v = Value::Text("'; DROP TABLE users; --".into());
        let out = format_literal(&v);
        assert_eq!(out, "'\\'; DROP TABLE users; --'");
        // The surrounding quotes remain balanced and embedded `'` is escaped.
        assert!(out.starts_with('\''));
        assert!(out.ends_with('\''));
    }
}

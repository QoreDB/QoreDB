// SPDX-License-Identifier: BUSL-1.1

//! Per-dialect SQL literal formatting for values that originate from
//! contract YAML (allowed_values, ranges, regex patterns, dates).
//!
//! These values are author-controlled, but we still escape them so a
//! malformed contract can never produce a parse error or accidental
//! SQL injection vector inside the generated query.

use crate::contracts::AllowedValue;

use super::dialect::Dialect;

pub fn escape_string_literal(dialect: Dialect, s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        match ch {
            '\'' => out.push_str("''"),
            // MySQL and ClickHouse honor backslash escapes by default;
            // double them to keep the literal verbatim.
            '\\' if matches!(dialect, Dialect::MySql | Dialect::ClickHouse) => {
                out.push_str("\\\\");
            }
            '\0' => out.push_str("\\0"),
            _ => out.push(ch),
        }
    }
    out.push('\'');
    out
}

pub fn format_number(n: f64) -> Option<String> {
    if !n.is_finite() {
        return None;
    }
    if n.fract() == 0.0 && n.abs() < 1e18 {
        Some(format!("{}", n as i64))
    } else {
        Some(format!("{n}"))
    }
}

pub fn format_int(n: i64) -> String {
    n.to_string()
}

pub fn format_allowed_value(dialect: Dialect, v: &AllowedValue) -> Option<String> {
    Some(match v {
        AllowedValue::Null => "NULL".to_string(),
        AllowedValue::Bool(b) => match dialect {
            Dialect::SqlServer | Dialect::ClickHouse => if *b { "1" } else { "0" }.to_string(),
            _ => if *b { "TRUE" } else { "FALSE" }.to_string(),
        },
        AllowedValue::Int(i) => i.to_string(),
        AllowedValue::Float(f) => format_number(*f)?,
        AllowedValue::Text(s) => escape_string_literal(dialect, s),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_single_quote() {
        assert_eq!(escape_string_literal(Dialect::Postgres, "O'Brien"), "'O''Brien'");
    }

    #[test]
    fn mysql_doubles_backslashes() {
        assert_eq!(escape_string_literal(Dialect::MySql, "a\\b"), "'a\\\\b'");
    }

    #[test]
    fn postgres_keeps_backslashes_verbatim() {
        // Standard PG (standard_conforming_strings=on) treats \ literally.
        assert_eq!(escape_string_literal(Dialect::Postgres, "a\\b"), "'a\\b'");
    }

    #[test]
    fn injection_attempt_neutralized() {
        let evil = "'; DROP TABLE users; --";
        let out = escape_string_literal(Dialect::Postgres, evil);
        assert_eq!(out, "'''; DROP TABLE users; --'");
    }

    #[test]
    fn format_number_handles_integers() {
        assert_eq!(format_number(42.0).unwrap(), "42");
        assert_eq!(format_number(-7.5).unwrap(), "-7.5");
        assert!(format_number(f64::NAN).is_none());
        assert!(format_number(f64::INFINITY).is_none());
    }

    #[test]
    fn allowed_value_bool_dialect_specific() {
        assert_eq!(
            format_allowed_value(Dialect::Postgres, &AllowedValue::Bool(true)).unwrap(),
            "TRUE"
        );
        assert_eq!(
            format_allowed_value(Dialect::ClickHouse, &AllowedValue::Bool(true)).unwrap(),
            "1"
        );
        assert_eq!(
            format_allowed_value(Dialect::SqlServer, &AllowedValue::Bool(false)).unwrap(),
            "0"
        );
    }

    #[test]
    fn allowed_value_null() {
        assert_eq!(
            format_allowed_value(Dialect::Postgres, &AllowedValue::Null).unwrap(),
            "NULL"
        );
    }
}

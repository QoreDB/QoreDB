// SPDX-License-Identifier: Apache-2.0

//! Query redaction for audit and profiling persistence.
//!
//! Replaces sensitive literals in SQL queries before they are written to disk
//! or stored in memory. The raw query remains available in `QueryContext` for
//! safety analysis — redaction is only applied at storage boundaries.

use regex::Regex;
use std::sync::OnceLock;

/// Redacts sensitive literals from a SQL query string.
///
/// Replaces:
/// - Connection URI credentials (`postgres://user:pass@host` → `postgres://***@host`)
/// - Secret assignments (`password=xxx` → `password=***`)
/// - SQL string literals (`'value'` → `'[REDACTED]'`)
pub fn redact_query_literals(query: &str) -> String {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();

    let patterns = PATTERNS.get_or_init(|| {
        vec![
            // Connection strings with credentials
            (
                Regex::new(r"(?i)((?:postgres|mysql|mongodb|redis|rediss)://)([^@]+)@")
                    .unwrap(),
                "${1}***@",
            ),
            // Secret assignments: password=xxx, token=xxx, etc.
            (
                Regex::new(r"(?i)(password|passwd|secret|token|api[_\-]?key)\s*=\s*\S+")
                    .unwrap(),
                "${1}=***",
            ),
            // SQL string literals: 'value' → '[REDACTED]' (handles escaped quotes '')
            (
                Regex::new(r"'(?:''|[^'])*'").unwrap(),
                "'[REDACTED]'",
            ),
        ]
    });

    let mut result = query.to_string();
    for (re, replacement) in patterns {
        result = re.replace_all(&result, *replacement).into_owned();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_string_literals() {
        let q = "SELECT * FROM users WHERE name = 'Alice' AND age > 30";
        let r = redact_query_literals(q);
        assert_eq!(r, "SELECT * FROM users WHERE name = '[REDACTED]' AND age > 30");
    }

    #[test]
    fn test_redact_escaped_quotes() {
        let q = "INSERT INTO t VALUES ('O''Brien')";
        let r = redact_query_literals(q);
        assert_eq!(r, "INSERT INTO t VALUES ('[REDACTED]')");
    }

    #[test]
    fn test_redact_connection_uri() {
        let q = "-- connecting to postgres://admin:s3cret@db.host:5432/mydb";
        let r = redact_query_literals(q);
        assert!(r.contains("postgres://***@db.host"));
        assert!(!r.contains("s3cret"));
    }

    #[test]
    fn test_redact_password_assignment() {
        let q = "SET password=hunter2";
        let r = redact_query_literals(q);
        assert_eq!(r, "SET password=***");
    }

    #[test]
    fn test_no_redaction_needed() {
        let q = "SELECT count(*) FROM orders";
        let r = redact_query_literals(q);
        assert_eq!(r, q);
    }
}

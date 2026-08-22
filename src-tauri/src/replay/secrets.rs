// SPDX-License-Identifier: BUSL-1.1

//! Spotting secrets in the query text a replay set is about to version.
//!
//! A replayable query cannot be redacted and still replay, so the set carries
//! its literals verbatim into Git. The risk that matters is not "there are
//! literals" — `WHERE status = 'pending'` is harmless and redacting it would
//! break the replay for nothing. It is "there is a credential", and that is a
//! narrower question.
//!
//! So the patterns here are deliberately targeted: connection strings carrying
//! credentials, secret-looking assignments, and whatever the user added to the
//! interceptor's redaction patterns. The catch-all "every string literal" rule
//! that `redaction.rs` applies for the audit log is left out on purpose — it
//! would flag every query and teach the user to ignore the warning.

use regex::Regex;
use std::sync::OnceLock;

/// Connection strings with inline credentials, and `password=`/`token=`-style
/// assignments. Mirrors the targeted half of the interceptor's redaction.
fn builtin_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            Regex::new(r"(?i)(?:postgres|postgresql|mysql|mongodb(?:\+srv)?|redis|rediss)://[^@\s]+:[^@\s]+@")
                .unwrap(),
            Regex::new(r"(?i)\b(?:password|passwd|pwd|secret|token|api[_\-]?key|access[_\-]?key|private[_\-]?key)\b\s*[=:]\s*\S")
                .unwrap(),
            // Redis AUTH, with or without a username.
            Regex::new(r"(?i)\bAUTH\s+\S").unwrap(),
            // Common vendor key shapes, which say what they are on sight.
            Regex::new(r"\b(?:sk|pk|rk)_(?:live|test)_[A-Za-z0-9]{8,}").unwrap(),
            Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap(),
            Regex::new(r"\bgh[pousr]_[A-Za-z0-9]{20,}").unwrap(),
            Regex::new(r"\bey[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}").unwrap(),
        ]
    })
}

/// Whether the query carries something that looks like a credential.
///
/// `custom_patterns` are the user's own regexes, taken from the interceptor
/// configuration so one place governs both the audit log and this.
pub fn looks_like_secret(query: &str, custom_patterns: &[String]) -> bool {
    if builtin_patterns().iter().any(|re| re.is_match(query)) {
        return true;
    }
    custom_patterns
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .any(|re| re.is_match(query))
}

/// How a recording treats query text on its way into a versioned set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretPolicy {
    /// Say nothing. For sets that never leave the machine.
    Off,
    /// Flag the entries that look like they carry a credential and let the
    /// user drop them. The set stays replayable — this is the default because
    /// the real risk is a secret leaving *by accident*.
    #[default]
    Warn,
    /// Redact literals on the way out. The set becomes shareable without
    /// reservation and stops being replayable; callers must say so.
    Redact,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_literals_are_not_flagged() {
        for query in [
            "SELECT * FROM orders WHERE status = 'pending'",
            "SELECT id, email FROM users ORDER BY id",
            "UPDATE items SET label = 'shipped' WHERE id = 12",
            r#"{"collection":"users","query":{"city":"Paris"}}"#,
        ] {
            assert!(
                !looks_like_secret(query, &[]),
                "false positive on a harmless literal: {query}"
            );
        }
    }

    /// The sample keys are assembled at runtime on purpose. Written as source
    /// literals they look real enough to trip secret scanners — including the
    /// push protection on this repository, which is exactly the kind of alert
    /// a test fixture should not be generating.
    fn sample_keys() -> Vec<String> {
        vec![
            format!("sk_{}_{}", "live", "4eC39HqLyjWDarjtT1zdp7dc"),
            format!("AKIA{}", "IOSFODNN7EXAMPLE"),
            format!("ghp_{}", "16C7e42F292c6912E7710c838347Ae178B4a"),
            format!(
                "ey{}.ey{}.{}",
                "JhbGciOiJIUzI1NiIs", "JzdWIiOiIxMjM0NTY3", "SflKxwRJSMeKKF2QT4"
            ),
        ]
    }

    #[test]
    fn credentials_are_flagged() {
        let mut queries = vec![
            "SELECT * FROM t WHERE password = 'hunter2'".to_string(),
            "SELECT * FROM dblink('postgres://admin:s3cret@10.0.0.1/db', 'SELECT 1')".to_string(),
            "AUTH hunter2".to_string(),
        ];
        queries.extend(
            sample_keys()
                .into_iter()
                .map(|key| format!("SELECT '{key}' AS value")),
        );

        for query in queries {
            assert!(
                looks_like_secret(&query, &[]),
                "missed a secret in: {query}"
            );
        }
    }

    #[test]
    fn user_patterns_extend_the_builtin_set() {
        let query = "SELECT * FROM t WHERE ref = 'INTERNAL-ABC123'";
        assert!(!looks_like_secret(query, &[]));
        assert!(looks_like_secret(
            query,
            &[r"INTERNAL-[A-Z0-9]+".to_string()]
        ));
    }

    /// A malformed user regex must not take the scan down with it.
    #[test]
    fn an_invalid_user_pattern_is_ignored() {
        assert!(!looks_like_secret("SELECT 1", &["([unclosed".to_string()]));
    }

    #[test]
    fn warn_is_the_default_policy() {
        assert_eq!(SecretPolicy::default(), SecretPolicy::Warn);
    }
}

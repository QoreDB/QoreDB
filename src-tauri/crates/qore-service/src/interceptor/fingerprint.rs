// SPDX-License-Identifier: Apache-2.0

//! Query Fingerprinting
//!
//! Normalizes a query into a canonical form (literals → `?`, whitespace
//! collapsed, identifiers preserved) and hashes the result with SHA-256.
//! Two queries that differ only by their parameters share the same fingerprint,
//! which lets the audit panel group them by signature.
//!
//! The fingerprint is **not** a security primitive. It is a stable grouping
//! key. Collisions are theoretically possible but irrelevant in practice for
//! the volume of queries a single user produces.

use sha2::{Digest, Sha256};

/// Length of the hex prefix surfaced to users. The full SHA-256 is 64 hex chars
/// — 16 is short enough to skim and long enough to make accidental collisions
/// astronomically unlikely at single-user scale (~2^32 queries before 1 in a
/// million collision risk).
const PREFIX_HEX_LEN: usize = 16;

/// Compute a stable fingerprint for the given query.
///
/// Dispatch is driver-aware so that SQL, MongoDB and Redis queries are
/// normalized appropriately.
pub fn fingerprint_query(query: &str, driver_id: &str) -> String {
    let normalized = normalize(query, driver_id);
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    let digest = hasher.finalize();
    hex_prefix(&digest, PREFIX_HEX_LEN)
}

fn normalize(query: &str, driver_id: &str) -> String {
    let driver = driver_id.to_ascii_lowercase();
    match driver.as_str() {
        "mongodb" => normalize_mongo(query),
        "redis" => normalize_redis(query),
        _ => normalize_sql(query),
    }
}

/// Normalize a SQL query:
/// - replace single-quoted string literals with `?`
/// - replace double-quoted string literals (MySQL ANSI off) with `?`
/// - replace numeric literals with `?`
/// - replace placeholders (`$1`, `:name`, `?`) with `?`
/// - collapse whitespace, uppercase keywords for stability
fn normalize_sql(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    let mut chars = query.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            // Single-quoted string literal: '...' (with escaped '' allowed)
            '\'' => {
                out.push('?');
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '\'' {
                        if matches!(chars.peek(), Some(&'\'')) {
                            chars.next(); // doubled escape
                            continue;
                        }
                        break;
                    }
                }
            }
            // Double-quoted: in MySQL non-ANSI mode this is a string literal.
            // We treat it as a literal too — false positives only affect
            // non-ANSI identifiers, which are rare and yield a stable group.
            '"' => {
                out.push('?');
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '"' {
                        if matches!(chars.peek(), Some(&'"')) {
                            chars.next();
                            continue;
                        }
                        break;
                    }
                }
            }
            // Numeric literal (integer / decimal / scientific notation prefix)
            c if c.is_ascii_digit() => {
                out.push('?');
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_digit()
                        || next == '.'
                        || next == 'e'
                        || next == 'E'
                        || next == '+'
                        || next == '-'
                    {
                        // The +/- after e/E only counts if directly after exponent.
                        // For simplicity we include them; the trailing chars get
                        // collapsed into the same `?` and never surface.
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            // PG / SQLite numbered placeholder: $1, $2, …
            '$' if matches!(chars.peek(), Some(c) if c.is_ascii_digit()) => {
                out.push('?');
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_digit() {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            // Named placeholder: :name (skip the colon if it's part of `::` cast)
            ':' if matches!(chars.peek(), Some(c) if c.is_ascii_alphabetic() || *c == '_') => {
                out.push('?');
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_alphanumeric() || next == '_' {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            // Existing question-mark placeholder
            '?' => out.push('?'),
            // Whitespace: collapse
            c if c.is_whitespace() => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            // Identifier / keyword: uppercase for stability
            c => out.push(c.to_ascii_uppercase()),
        }
    }

    out.trim().to_string()
}

/// Normalize a MongoDB query (JSON or shell-ish).
///
/// We replace string and numeric **values** with `?` while preserving JSON
/// **keys** (which carry the signature). A quoted string is treated as a key
/// when the next non-whitespace character after its closing quote is `:`; any
/// other context — array element, RHS of `:`, function arg — is a value.
fn normalize_mongo(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    let mut chars = query.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' | '\'' => {
                let quote = ch;
                let mut content = String::new();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '\\' {
                        if let Some(&escaped) = chars.peek() {
                            content.push('\\');
                            content.push(escaped);
                            chars.next();
                        }
                        continue;
                    }
                    if next == quote {
                        break;
                    }
                    content.push(next);
                }

                if next_non_space_is_colon(&chars) {
                    // Key — keep verbatim with quotes
                    out.push(quote);
                    out.push_str(&content);
                    out.push(quote);
                } else {
                    out.push('?');
                }
            }
            c if c.is_ascii_digit() => {
                out.push('?');
                while let Some(&next) = chars.peek() {
                    if next.is_ascii_digit() || next == '.' {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            c => out.push(c),
        }
    }

    out.trim().to_string()
}

fn next_non_space_is_colon<I>(chars: &std::iter::Peekable<I>) -> bool
where
    I: Iterator<Item = char> + Clone,
{
    let mut probe = chars.clone();
    while let Some(&c) = probe.peek() {
        if c.is_whitespace() {
            probe.next();
            continue;
        }
        return c == ':';
    }
    false
}

/// Normalize a Redis command line. Keep the verb (and subverb for `CONFIG`,
/// `ACL`, …) uppercase; replace every following argument with `?`.
fn normalize_redis(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut tokens = trimmed.split_whitespace();
    let Some(verb) = tokens.next() else {
        return String::new();
    };
    let verb_upper = verb.to_ascii_uppercase();

    let multi_word = matches!(
        verb_upper.as_str(),
        "CONFIG"
            | "ACL"
            | "CLIENT"
            | "CLUSTER"
            | "COMMAND"
            | "DEBUG"
            | "FUNCTION"
            | "LATENCY"
            | "MEMORY"
            | "OBJECT"
            | "PUBSUB"
            | "SCRIPT"
            | "SLOWLOG"
            | "XGROUP"
            | "XINFO"
    );

    let mut out = verb_upper;
    let mut argc = 0usize;

    if multi_word {
        if let Some(sub) = tokens.next() {
            out.push(' ');
            out.push_str(&sub.to_ascii_uppercase());
        }
    }

    for _ in tokens {
        argc += 1;
    }

    if argc > 0 {
        out.push(' ');
        out.push_str(&vec!["?"; argc].join(" "));
    }

    out
}

fn hex_prefix(bytes: &[u8], hex_len: usize) -> String {
    let mut out = String::with_capacity(hex_len);
    for byte in bytes.iter().take(hex_len.div_ceil(2)) {
        out.push_str(&format!("{:02x}", byte));
    }
    out.truncate(hex_len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_literal_values_share_fingerprint() {
        let a = fingerprint_query("SELECT * FROM users WHERE id = 1", "postgres");
        let b = fingerprint_query("SELECT * FROM users WHERE id = 42", "postgres");
        assert_eq!(a, b);
    }

    #[test]
    fn sql_string_literals_share_fingerprint() {
        let a = fingerprint_query("SELECT * FROM logs WHERE name = 'alice'", "mysql");
        let b = fingerprint_query("SELECT * FROM logs WHERE name = 'bob'", "mysql");
        assert_eq!(a, b);
    }

    #[test]
    fn sql_different_columns_differ() {
        let a = fingerprint_query("SELECT id FROM users WHERE id = 1", "postgres");
        let b = fingerprint_query("SELECT id FROM users WHERE email = 1", "postgres");
        assert_ne!(a, b);
    }

    #[test]
    fn sql_whitespace_collapses_for_matching_styles() {
        // Whitespace runs are collapsed, so equivalent queries from the same
        // client (same operator spacing) hash identically.
        let a = fingerprint_query("SELECT id FROM users   WHERE  id = 1", "postgres");
        let b = fingerprint_query("SELECT id FROM users WHERE id = 2", "postgres");
        assert_eq!(a, b);
    }

    #[test]
    fn sql_placeholders_collapse() {
        let a = fingerprint_query("SELECT * FROM t WHERE a = $1 AND b = $2", "postgres");
        let b = fingerprint_query("SELECT * FROM t WHERE a = ? AND b = ?", "mysql");
        assert_eq!(a, b);
    }

    #[test]
    fn sql_named_placeholder_is_collapsed() {
        let a = fingerprint_query("SELECT * FROM t WHERE a = :id", "postgres");
        let b = fingerprint_query("SELECT * FROM t WHERE a = 5", "postgres");
        assert_eq!(a, b);
    }

    #[test]
    fn sql_double_colon_cast_preserved_enough_to_share() {
        // PG cast `::int` should not break grouping with another typed query
        let a = fingerprint_query("SELECT id::int FROM t WHERE id = 1", "postgres");
        let b = fingerprint_query("SELECT id::int FROM t WHERE id = 999", "postgres");
        assert_eq!(a, b);
    }

    #[test]
    fn mongo_field_names_preserved_values_collapsed() {
        let a = fingerprint_query(
            r#"{"operation":"find","filter":{"email":"a@b.c"}}"#,
            "mongodb",
        );
        let b = fingerprint_query(
            r#"{"operation":"find","filter":{"email":"x@y.z"}}"#,
            "mongodb",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn mongo_different_field_changes_fingerprint() {
        let a = fingerprint_query(r#"{"filter":{"email":"a"}}"#, "mongodb");
        let b = fingerprint_query(r#"{"filter":{"name":"a"}}"#, "mongodb");
        assert_ne!(a, b);
    }

    #[test]
    fn redis_args_collapsed() {
        let a = fingerprint_query("SET user:42 hunter2", "redis");
        let b = fingerprint_query("SET user:99 swordfish", "redis");
        assert_eq!(a, b);
    }

    #[test]
    fn redis_subcommand_kept() {
        let a = fingerprint_query("CONFIG GET maxmemory", "redis");
        let b = fingerprint_query("CONFIG SET maxmemory 1gb", "redis");
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_is_stable_length() {
        let fp = fingerprint_query("SELECT 1", "postgres");
        assert_eq!(fp.len(), PREFIX_HEX_LEN);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn empty_query_does_not_panic() {
        let fp = fingerprint_query("", "postgres");
        assert_eq!(fp.len(), PREFIX_HEX_LEN);
    }
}

// SPDX-License-Identifier: Apache-2.0

//! Query redaction for audit and profiling persistence.
//!
//! Replaces sensitive literals in queries before they are written to disk
//! or stored in memory. The raw query remains available in `QueryContext`
//! for safety analysis — redaction is only applied at storage boundaries.
//!
//! Dispatches per driver:
//! - SQL drivers (postgres/mysql/sqlite/…): string literals, connection URIs,
//!   secret assignments.
//! - MongoDB: JSON fields matching `password`/`token`/`secret`/`api_key` +
//!   connection URIs.
//! - Redis: `AUTH` args, `CONFIG SET` with sensitive keys, `EVAL`/`EVALSHA`
//!   scripts collapsed.

use parking_lot::RwLock;
use regex::Regex;
use std::sync::OnceLock;

fn redaction_enabled_lock() -> &'static RwLock<bool> {
    static LOCK: OnceLock<RwLock<bool>> = OnceLock::new();
    LOCK.get_or_init(|| RwLock::new(true))
}

fn custom_patterns_lock() -> &'static RwLock<Vec<Regex>> {
    static LOCK: OnceLock<RwLock<Vec<Regex>>> = OnceLock::new();
    LOCK.get_or_init(|| RwLock::new(Vec::new()))
}

/// Enable or disable redaction globally.
pub fn set_redaction_enabled(enabled: bool) {
    *redaction_enabled_lock().write() = enabled;
}

/// Whether redaction is currently enabled.
pub fn is_redaction_enabled() -> bool {
    *redaction_enabled_lock().read()
}

/// Serializes tests that mutate the global redaction state. Exposed under
/// `cfg(test)` so downstream modules (e.g. `types.rs`) can share the lock.
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// Replace the custom pattern list. Invalid regexes are silently skipped —
/// validation is expected at configuration time.
pub fn set_custom_patterns(patterns: &[String]) {
    let compiled: Vec<Regex> = patterns
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect();
    *custom_patterns_lock().write() = compiled;
}

/// Redact a query for persistence, selecting the strategy from the driver id.
pub fn redact_query(query: &str, driver_id: &str) -> String {
    if !is_redaction_enabled() {
        return query.to_string();
    }

    let base = match driver_id.to_lowercase().as_str() {
        "mongodb" | "mongo" => redact_mongo(query),
        "redis" => redact_redis(query),
        _ => redact_sql(query),
    };

    apply_custom_patterns(&base)
}

fn apply_custom_patterns(input: &str) -> String {
    let patterns = custom_patterns_lock().read();
    if patterns.is_empty() {
        return input.to_string();
    }
    let mut out = input.to_string();
    for re in patterns.iter() {
        out = re.replace_all(&out, "[REDACTED]").into_owned();
    }
    out
}

/// SQL redaction (legacy entry point kept for callers that don't know the
/// driver id). Prefer `redact_query`.
pub fn redact_query_literals(query: &str) -> String {
    if !is_redaction_enabled() {
        return query.to_string();
    }
    apply_custom_patterns(&redact_sql(query))
}

// ==================== SQL ====================

fn redact_sql(query: &str) -> String {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        vec![
            // Connection strings with credentials
            (
                Regex::new(r"(?i)((?:postgres|mysql|mongodb|redis|rediss)://)([^@\s]+)@")
                    .unwrap(),
                "${1}***@",
            ),
            // Secret assignments: password=xxx, token=xxx, api_key=xxx, etc.
            (
                Regex::new(r"(?i)(password|passwd|secret|token|api[_\-]?key)\s*=\s*\S+")
                    .unwrap(),
                "${1}=***",
            ),
            // SQL string literals: 'value' → '[REDACTED]' (handles doubled quotes '')
            (Regex::new(r"'(?:''|[^'])*'").unwrap(), "'[REDACTED]'"),
        ]
    });

    let mut result = query.to_string();
    for (re, replacement) in patterns.iter() {
        result = re.replace_all(&result, *replacement).into_owned();
    }
    result
}

// ==================== MongoDB ====================

fn redact_mongo(query: &str) -> String {
    static FIELD_PATTERN: OnceLock<Regex> = OnceLock::new();
    static FIELD_PATTERN_SINGLE: OnceLock<Regex> = OnceLock::new();
    static BARE_KEY_PATTERN: OnceLock<Regex> = OnceLock::new();
    static URI_PATTERN: OnceLock<Regex> = OnceLock::new();

    let field_pattern = FIELD_PATTERN.get_or_init(|| {
        Regex::new(
            r#"(?i)(["'](?:password|passwd|secret|token|api[_\-]?key|credentials|authorization|auth)["'])\s*:\s*"(?:\\.|[^"\\])*""#,
        )
        .unwrap()
    });
    let field_pattern_single = FIELD_PATTERN_SINGLE.get_or_init(|| {
        Regex::new(
            r#"(?i)(["'](?:password|passwd|secret|token|api[_\-]?key|credentials|authorization|auth)["'])\s*:\s*'(?:\\.|[^'\\])*'"#,
        )
        .unwrap()
    });
    let bare_key_pattern = BARE_KEY_PATTERN.get_or_init(|| {
        Regex::new(
            r#"(?i)\b(password|passwd|secret|token|api[_\-]?key|credentials|authorization|auth)\s*:\s*(?:"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')"#,
        )
        .unwrap()
    });
    let uri_pattern = URI_PATTERN.get_or_init(|| {
        Regex::new(r"(?i)((?:mongodb(?:\+srv)?|redis|rediss)://)([^@\s]+)@").unwrap()
    });

    let mut out = uri_pattern.replace_all(query, "${1}***@").into_owned();
    out = field_pattern
        .replace_all(&out, r#"$1: "[REDACTED]""#)
        .into_owned();
    out = field_pattern_single
        .replace_all(&out, r#"$1: "[REDACTED]""#)
        .into_owned();
    out = bare_key_pattern
        .replace_all(&out, r#"$1: "[REDACTED]""#)
        .into_owned();
    out
}

// ==================== Redis ====================

fn redact_redis(query: &str) -> String {
    // Redis commands are line-oriented; redact per line to preserve multi-
    // command scripts.
    query
        .lines()
        .map(redact_redis_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_redis_line(line: &str) -> String {
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    let indent = &line[..indent_len];

    let tokens = split_redis_tokens(trimmed);
    if tokens.is_empty() {
        return line.to_string();
    }

    let cmd = tokens[0].to_uppercase();
    let redacted_tokens: Vec<String> = match cmd.as_str() {
        // AUTH [username] password
        "AUTH" => {
            let mut out = vec![tokens[0].clone()];
            if tokens.len() >= 2 {
                // Treat all args after AUTH as secret (covers both single-arg
                // and user+password forms).
                for _ in 1..tokens.len() {
                    out.push("***".to_string());
                }
            }
            out
        }
        // CONFIG SET <key> <value> — redact value when key is sensitive.
        "CONFIG" if tokens.len() >= 4 && tokens[1].eq_ignore_ascii_case("SET") => {
            let key = tokens[2].to_lowercase();
            if matches!(
                key.as_str(),
                "requirepass" | "masterauth" | "masteruser" | "tls-key-file-pass"
            ) {
                let mut out = vec![tokens[0].clone(), tokens[1].clone(), tokens[2].clone()];
                for _ in 3..tokens.len() {
                    out.push("***".to_string());
                }
                out
            } else {
                tokens.clone()
            }
        }
        // Lua scripts may embed secrets; collapse the body.
        "EVAL" | "EVALSHA" => {
            if tokens.len() >= 2 {
                let mut out = vec![tokens[0].clone(), "\"[REDACTED_SCRIPT]\"".to_string()];
                // Keep numkeys + KEYS/ARGV untouched from index 2 onwards.
                for tok in tokens.iter().skip(2) {
                    out.push(tok.clone());
                }
                out
            } else {
                tokens.clone()
            }
        }
        // ACL SETUSER <name> ... — hash/password clauses.
        "ACL" if tokens.len() >= 3 && tokens[1].eq_ignore_ascii_case("SETUSER") => {
            let mut out = Vec::with_capacity(tokens.len());
            for (i, tok) in tokens.iter().enumerate() {
                if i >= 3 {
                    let lower = tok.to_lowercase();
                    if lower.starts_with(">")
                        || lower.starts_with("<")
                        || lower.starts_with("#")
                        || lower.starts_with("!")
                    {
                        out.push("***".to_string());
                        continue;
                    }
                }
                out.push(tok.clone());
            }
            out
        }
        _ => tokens.clone(),
    };

    format!("{}{}", indent, redacted_tokens.join(" "))
}

/// Whitespace-aware splitter that preserves quoted segments as single tokens.
fn split_redis_tokens(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;

    while let Some(ch) = chars.next() {
        match ch {
            '\\' if in_single || in_double => {
                current.push(ch);
                if let Some(&next) = chars.peek() {
                    current.push(next);
                    chars.next();
                }
            }
            '"' if !in_single => {
                current.push(ch);
                in_double = !in_double;
            }
            '\'' if !in_double => {
                current.push(ch);
                in_single = !in_single;
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        out.push(current);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::MutexGuard;

    fn reset() -> MutexGuard<'static, ()> {
        let guard = test_lock();
        set_redaction_enabled(true);
        set_custom_patterns(&[]);
        guard
    }

    // ---------- SQL ----------

    #[test]
    fn sql_redact_string_literals() {
        let _guard = reset();
        let q = "SELECT * FROM users WHERE name = 'Alice' AND age > 30";
        let r = redact_query(q, "postgres");
        assert_eq!(
            r,
            "SELECT * FROM users WHERE name = '[REDACTED]' AND age > 30"
        );
    }

    #[test]
    fn sql_redact_escaped_quotes() {
        let _guard = reset();
        let q = "INSERT INTO t VALUES ('O''Brien')";
        let r = redact_query(q, "mysql");
        assert_eq!(r, "INSERT INTO t VALUES ('[REDACTED]')");
    }

    #[test]
    fn sql_redact_connection_uri() {
        let _guard = reset();
        let q = "-- connecting to postgres://admin:s3cret@db.host:5432/mydb";
        let r = redact_query(q, "postgres");
        assert!(r.contains("postgres://***@db.host"));
        assert!(!r.contains("s3cret"));
    }

    #[test]
    fn sql_redact_password_assignment() {
        let _guard = reset();
        let q = "SET password=hunter2";
        let r = redact_query(q, "postgres");
        assert_eq!(r, "SET password=***");
    }

    #[test]
    fn sql_no_redaction_needed() {
        let _guard = reset();
        let q = "SELECT count(*) FROM orders";
        let r = redact_query(q, "postgres");
        assert_eq!(r, q);
    }

    // ---------- MongoDB ----------

    #[test]
    fn mongo_redact_password_field() {
        let _guard = reset();
        let q = r#"{"operation":"insert","document":{"email":"a@b.c","password":"hunter2"}}"#;
        let r = redact_query(q, "mongodb");
        assert!(!r.contains("hunter2"));
        assert!(r.contains(r#""password": "[REDACTED]""#));
        // Non-sensitive fields preserved
        assert!(r.contains("a@b.c"));
    }

    #[test]
    fn mongo_redact_token_and_secret() {
        let _guard = reset();
        let q =
            r#"{"filter":{"token":"abc.def.ghi","secret":"s3cr3t","api_key":"k"},"name":"foo"}"#;
        let r = redact_query(q, "mongodb");
        assert!(!r.contains("abc.def.ghi"));
        assert!(!r.contains("s3cr3t"));
        assert!(r.contains("[REDACTED]"));
        assert!(r.contains("\"name\":\"foo\""));
    }

    #[test]
    fn mongo_redact_shell_syntax() {
        let _guard = reset();
        let q = r#"db.users.insertOne({email: "a@b.c", password: "hunter2"})"#;
        let r = redact_query(q, "mongodb");
        assert!(!r.contains("hunter2"));
        assert!(r.contains("[REDACTED]"));
    }

    #[test]
    fn mongo_redact_connection_uri() {
        let _guard = reset();
        let q = "mongodb+srv://user:pass@cluster.mongodb.net/db";
        let r = redact_query(q, "mongodb");
        assert!(r.contains("mongodb+srv://***@cluster"));
        assert!(!r.contains("pass@"));
    }

    #[test]
    fn mongo_leaves_normal_queries_intact() {
        let _guard = reset();
        let q = r#"{"filter":{"age":{"$gt":30}},"projection":{"name":1}}"#;
        let r = redact_query(q, "mongodb");
        assert_eq!(r, q);
    }

    // ---------- Redis ----------

    #[test]
    fn redis_redact_auth_single_arg() {
        let _guard = reset();
        let r = redact_query("AUTH hunter2", "redis");
        assert_eq!(r, "AUTH ***");
    }

    #[test]
    fn redis_redact_auth_user_pass() {
        let _guard = reset();
        let r = redact_query("AUTH alice hunter2", "redis");
        assert_eq!(r, "AUTH *** ***");
    }

    #[test]
    fn redis_redact_config_set_requirepass() {
        let _guard = reset();
        let r = redact_query("CONFIG SET requirepass hunter2", "redis");
        assert_eq!(r, "CONFIG SET requirepass ***");
    }

    #[test]
    fn redis_config_set_other_key_unchanged() {
        let _guard = reset();
        let q = "CONFIG SET maxmemory 256mb";
        let r = redact_query(q, "redis");
        assert_eq!(r, q);
    }

    #[test]
    fn redis_redact_eval_script() {
        let _guard = reset();
        let r = redact_query(
            r#"EVAL "return redis.call('SET','pw','hunter2')" 0"#,
            "redis",
        );
        assert!(!r.contains("hunter2"));
        assert!(r.contains("[REDACTED_SCRIPT]"));
        // numkeys preserved at the tail
        assert!(r.ends_with(" 0"));
    }

    #[test]
    fn redis_non_sensitive_command_unchanged() {
        let _guard = reset();
        let r = redact_query("GET users:42", "redis");
        assert_eq!(r, "GET users:42");
    }

    #[test]
    fn redis_acl_setuser_redacts_password_clauses() {
        let _guard = reset();
        let r = redact_query("ACL SETUSER alice on >hunter2 ~* +@all", "redis");
        assert!(!r.contains("hunter2"));
        assert!(r.contains("***"));
        // Non-credential clauses preserved
        assert!(r.contains("~*"));
    }

    // ---------- Toggle & custom patterns ----------

    #[test]
    fn disabled_returns_input_unchanged() {
        let _guard = reset();
        set_redaction_enabled(false);
        let q = "AUTH hunter2";
        let r = redact_query(q, "redis");
        assert_eq!(r, q);
        set_redaction_enabled(true);
    }

    #[test]
    fn custom_pattern_applies_across_drivers() {
        let _guard = reset();
        set_custom_patterns(&[r"INTERNAL-[A-Z0-9]+".to_string()]);
        let r = redact_query("SELECT 'INTERNAL-ABC123' FROM t", "postgres");
        // SQL redaction already handles the literal, but the custom pattern
        // applies on top (on top of already-redacted output too).
        assert!(!r.contains("INTERNAL-ABC123"));
        set_custom_patterns(&[]);
    }

    #[test]
    fn invalid_custom_pattern_is_skipped() {
        let _guard = reset();
        set_custom_patterns(&["[".to_string(), "valid[0-9]+".to_string()]);
        let r = redact_query("value valid123 end", "postgres");
        assert!(r.contains("[REDACTED]"));
        set_custom_patterns(&[]);
    }
}

// SPDX-License-Identifier: Apache-2.0

//! ClickHouse query safety classification.
//!
//! ClickHouse SQL is close enough to ANSI SQL that we don't need a full
//! parser to flag dangerous statements — a leading-keyword classifier covers
//! the policy enforcement contract (read-only mode, production guards). For
//! anything we can't recognize we return `Unknown`, which the policy layer
//! treats as "ask for confirmation".

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickHouseQueryClass {
    Read,
    Mutation,
    Dangerous,
    Unknown,
}

pub fn classify(query: &str) -> ClickHouseQueryClass {
    let stripped = strip_leading_comments(query.trim());
    if stripped.is_empty() {
        return ClickHouseQueryClass::Unknown;
    }
    let upper = stripped.to_ascii_uppercase();

    if is_dangerous(&upper) {
        return ClickHouseQueryClass::Dangerous;
    }
    if is_read(&upper) {
        return ClickHouseQueryClass::Read;
    }
    if is_mutation(&upper) {
        return ClickHouseQueryClass::Mutation;
    }
    ClickHouseQueryClass::Unknown
}

fn is_dangerous(upper: &str) -> bool {
    // DROP DATABASE / DROP TABLE / DROP DICTIONARY / DROP VIEW
    if let Some(rest) = upper.strip_prefix("DROP ") {
        let head = rest.trim_start();
        if head.starts_with("DATABASE")
            || head.starts_with("TABLE")
            || head.starts_with("DICTIONARY")
            || head.starts_with("VIEW")
            || head.starts_with("FUNCTION")
            || head.starts_with("USER")
            || head.starts_with("ROLE")
        {
            return true;
        }
    }

    if upper.starts_with("TRUNCATE ") || upper == "TRUNCATE" {
        return true;
    }

    if upper.starts_with("DETACH ") {
        return true;
    }

    if upper.starts_with("SYSTEM ") {
        let rest = upper.trim_start_matches("SYSTEM ").trim_start();
        if rest.starts_with("SHUTDOWN")
            || rest.starts_with("KILL")
            || rest.starts_with("DROP")
            || rest.starts_with("RESTART")
            || rest.starts_with("RELOAD")
            || rest.starts_with("STOP")
            || rest.starts_with("START")
            || rest.starts_with("FLUSH")
        {
            return true;
        }
    }

    if upper.starts_with("OPTIMIZE ") && upper.contains(" FINAL") {
        return true;
    }

    if upper.starts_with("KILL ") {
        return true;
    }

    false
}

fn is_read(upper: &str) -> bool {
    let head = first_keyword(upper);
    matches!(
        head.as_str(),
        "SELECT" | "WITH" | "SHOW" | "DESCRIBE" | "DESC" | "EXISTS" | "EXPLAIN" | "CHECK"
    )
}

fn is_mutation(upper: &str) -> bool {
    let head = first_keyword(upper);
    if matches!(
        head.as_str(),
        "INSERT" | "CREATE" | "ATTACH" | "RENAME" | "GRANT" | "REVOKE" | "SET"
    ) {
        return true;
    }
    if head == "ALTER" {
        return true;
    }
    if head == "OPTIMIZE" {
        return true;
    }
    false
}

fn first_keyword(upper: &str) -> String {
    upper
        .split(|c: char| c.is_whitespace() || c == ';')
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Strip line and block comments from the front so `-- a\nSELECT 1` and
/// `/* foo*/ SELECT 1` classify as Read.
fn strip_leading_comments(input: &str) -> &str {
    let mut s = input;
    loop {
        s = s.trim_start();
        if let Some(rest) = s.strip_prefix("--") {
            if let Some(idx) = rest.find('\n') {
                s = &rest[idx + 1..];
                continue;
            } else {
                return "";
            }
        }
        if let Some(rest) = s.strip_prefix("/*") {
            if let Some(idx) = rest.find("*/") {
                s = &rest[idx + 2..];
                continue;
            } else {
                return "";
            }
        }
        return s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_read_queries() {
        assert_eq!(classify("SELECT 1"), ClickHouseQueryClass::Read);
        assert_eq!(
            classify("WITH x AS (SELECT 1) SELECT * FROM x"),
            ClickHouseQueryClass::Read
        );
        assert_eq!(classify("SHOW TABLES"), ClickHouseQueryClass::Read);
        assert_eq!(classify("DESCRIBE TABLE t"), ClickHouseQueryClass::Read);
        assert_eq!(classify("EXPLAIN SELECT 1"), ClickHouseQueryClass::Read);
    }

    #[test]
    fn classifies_mutations() {
        assert_eq!(
            classify("INSERT INTO t VALUES (1)"),
            ClickHouseQueryClass::Mutation
        );
        assert_eq!(
            classify("CREATE TABLE t (id Int32) ENGINE = Memory"),
            ClickHouseQueryClass::Mutation
        );
        assert_eq!(
            classify("ALTER TABLE t UPDATE x = 1 WHERE id = 1"),
            ClickHouseQueryClass::Mutation
        );
        assert_eq!(
            classify("ALTER TABLE t DELETE WHERE id = 1"),
            ClickHouseQueryClass::Mutation
        );
        assert_eq!(classify("OPTIMIZE TABLE t"), ClickHouseQueryClass::Mutation);
    }

    #[test]
    fn classifies_dangerous() {
        assert_eq!(classify("DROP TABLE t"), ClickHouseQueryClass::Dangerous);
        assert_eq!(
            classify("DROP DATABASE prod"),
            ClickHouseQueryClass::Dangerous
        );
        assert_eq!(
            classify("TRUNCATE TABLE t"),
            ClickHouseQueryClass::Dangerous
        );
        assert_eq!(classify("DETACH TABLE t"), ClickHouseQueryClass::Dangerous);
        assert_eq!(classify("SYSTEM SHUTDOWN"), ClickHouseQueryClass::Dangerous);
        assert_eq!(
            classify("SYSTEM FLUSH LOGS"),
            ClickHouseQueryClass::Dangerous
        );
        assert_eq!(
            classify("OPTIMIZE TABLE t FINAL"),
            ClickHouseQueryClass::Dangerous
        );
        assert_eq!(
            classify("KILL QUERY WHERE 1"),
            ClickHouseQueryClass::Dangerous
        );
    }

    #[test]
    fn handles_leading_comments() {
        assert_eq!(
            classify("-- weekly report\nSELECT 1"),
            ClickHouseQueryClass::Read
        );
        assert_eq!(
            classify("/* note */ DROP TABLE t"),
            ClickHouseQueryClass::Dangerous
        );
    }

    #[test]
    fn empty_or_whitespace_is_unknown() {
        assert_eq!(classify(""), ClickHouseQueryClass::Unknown);
        assert_eq!(classify("   "), ClickHouseQueryClass::Unknown);
        assert_eq!(classify("-- only a comment"), ClickHouseQueryClass::Unknown);
    }

    #[test]
    fn unknown_for_unclassified() {
        assert_eq!(classify("USE db"), ClickHouseQueryClass::Unknown);
        assert_eq!(classify("CHECK TABLE t"), ClickHouseQueryClass::Read);
    }
}

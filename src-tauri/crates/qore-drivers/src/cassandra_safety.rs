// SPDX-License-Identifier: Apache-2.0

//! CQL statement classification for the read-only and production guards.
//!
//! CQL is not SQL, so `qore-sql`'s parser has nothing to say about it. The
//! classification is lexical on purpose: this decides whether a statement is
//! allowed to run, and a guard that fails to parse must refuse rather than
//! wave the statement through.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CqlQueryClass {
    Read,
    Mutation,
    Dangerous,
    Unknown,
}

pub fn classify(query: &str) -> CqlQueryClass {
    let statement = strip_comments(query);
    let mut words = statement.split_whitespace();
    let Some(first) = words.next() else {
        return CqlQueryClass::Unknown;
    };
    let head = first.to_ascii_uppercase();
    let second = words.next().map(|w| w.to_ascii_uppercase());

    match head.as_str() {
        "TRUNCATE" => CqlQueryClass::Dangerous,
        "DROP" => CqlQueryClass::Dangerous,
        "ALTER" | "CREATE" => match second.as_deref() {
            // Creating an index rebuilds it across the ring; on a large table
            // that is an operational event, not a schema tweak.
            Some("INDEX" | "CUSTOM") => CqlQueryClass::Dangerous,
            Some(_) => CqlQueryClass::Mutation,
            None => CqlQueryClass::Unknown,
        },
        "GRANT" | "REVOKE" => CqlQueryClass::Dangerous,
        "SELECT" => CqlQueryClass::Read,
        "INSERT" | "UPDATE" | "DELETE" | "BATCH" | "BEGIN" | "APPLY" => CqlQueryClass::Mutation,
        "USE" | "DESCRIBE" | "DESC" | "LIST" => CqlQueryClass::Read,
        _ => CqlQueryClass::Unknown,
    }
}

/// `ALLOW FILTERING` turns a bounded lookup into a ring-wide scan whose cost
/// grows with the data, not with the result. It is a legitimate tool while
/// exploring, and a way to take a cluster down in production.
pub fn uses_allow_filtering(query: &str) -> bool {
    let upper = strip_comments(query).to_ascii_uppercase();
    let mut rest = upper.as_str();
    while let Some(at) = rest.find("ALLOW") {
        let after = rest[at + "ALLOW".len()..].trim_start();
        if after.starts_with("FILTERING") {
            return true;
        }
        rest = &rest[at + "ALLOW".len()..];
    }
    false
}

/// A `SELECT` with no `WHERE` reads every partition on every replica. Cassandra
/// answers it — slowly, and at the cost of the whole ring — which is why the
/// driver refuses it outside development rather than letting the grid issue it.
pub fn is_unbounded_select(query: &str) -> bool {
    let statement = strip_comments(query);
    let upper = statement.to_ascii_uppercase();
    if !upper.trim_start().starts_with("SELECT") {
        return false;
    }
    if upper.contains(" WHERE ") {
        return false;
    }
    // A `LIMIT` bounds the answer even without a partition key, which is what
    // `preview_table` relies on.
    !upper.contains(" LIMIT ")
}

/// Statements refused outright in a production environment, with the reason the
/// UI shows. `read_only` is enforced separately by the caller.
pub fn production_refusal(query: &str) -> Option<&'static str> {
    let statement = strip_comments(query);
    let upper = statement.to_ascii_uppercase();
    let head: Vec<&str> = upper.split_whitespace().take(3).collect();
    match head.as_slice() {
        ["TRUNCATE", ..] => {
            Some("TRUNCATE erases every row in the table and cannot be rolled back")
        }
        ["DROP", "KEYSPACE", ..] => Some("DROP KEYSPACE erases the whole keyspace"),
        ["DROP", "TABLE", ..] => Some("DROP TABLE erases the table and its data"),
        _ if uses_allow_filtering(&statement) => {
            Some("ALLOW FILTERING scans the entire ring; run it against a development cluster")
        }
        _ if is_unbounded_select(&statement) => {
            Some("This SELECT has no partition key and no LIMIT, so it scans every partition")
        }
        _ => None,
    }
}

/// Drops `--` and `//` line comments and `/* */` blocks so a keyword cannot be
/// hidden behind one.
fn strip_comments(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    let bytes: Vec<char> = query.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            out.push(c);
            if c == '\'' {
                // A doubled quote is an escaped quote, not the end of the literal.
                if bytes.get(i + 1) == Some(&'\'') {
                    out.push('\'');
                    i += 2;
                    continue;
                }
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' => {
                in_string = true;
                out.push(c);
                i += 1;
            }
            '-' if bytes.get(i + 1) == Some(&'-') => {
                while i < bytes.len() && bytes[i] != '\n' {
                    i += 1;
                }
                out.push(' ');
            }
            '/' if bytes.get(i + 1) == Some(&'/') => {
                while i < bytes.len() && bytes[i] != '\n' {
                    i += 1;
                }
                out.push(' ');
            }
            '/' if bytes.get(i + 1) == Some(&'*') => {
                i += 2;
                while i < bytes.len() && !(bytes[i] == '*' && bytes.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
                out.push(' ');
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    // Pad the ends so ` WHERE ` and ` LIMIT ` match at a statement boundary.
    format!(" {} ", out.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_the_common_statements() {
        assert_eq!(
            classify("SELECT * FROM t WHERE id = 1"),
            CqlQueryClass::Read
        );
        assert_eq!(
            classify("insert into t (a) values (1)"),
            CqlQueryClass::Mutation
        );
        assert_eq!(
            classify("UPDATE t SET a = 1 WHERE id = 1"),
            CqlQueryClass::Mutation
        );
        assert_eq!(
            classify("DELETE FROM t WHERE id = 1"),
            CqlQueryClass::Mutation
        );
        assert_eq!(classify("TRUNCATE t"), CqlQueryClass::Dangerous);
        assert_eq!(classify("DROP KEYSPACE ks"), CqlQueryClass::Dangerous);
        assert_eq!(
            classify("CREATE TABLE t (id int PRIMARY KEY)"),
            CqlQueryClass::Mutation
        );
        assert_eq!(
            classify("CREATE INDEX ON t (col)"),
            CqlQueryClass::Dangerous
        );
        assert_eq!(classify(""), CqlQueryClass::Unknown);
        assert_eq!(classify("NONSENSE foo"), CqlQueryClass::Unknown);
    }

    #[test]
    fn allow_filtering_is_found_whatever_the_spacing_or_case() {
        assert!(uses_allow_filtering(
            "SELECT * FROM t WHERE a = 1 ALLOW FILTERING"
        ));
        assert!(uses_allow_filtering("select * from t allow    filtering"));
        assert!(uses_allow_filtering(
            "SELECT * FROM t WHERE a = 1 ALLOW\n  FILTERING"
        ));
        assert!(!uses_allow_filtering("SELECT allowance FROM t WHERE a = 1"));
    }

    #[test]
    fn a_keyword_hidden_in_a_comment_is_still_found() {
        // The comment is stripped, so the real ALLOW FILTERING behind it counts
        // and a decoy inside a comment does not.
        assert!(uses_allow_filtering(
            "SELECT * FROM t -- nope\n ALLOW FILTERING"
        ));
        assert!(!uses_allow_filtering(
            "SELECT * FROM t /* ALLOW FILTERING */ WHERE a = 1"
        ));
        assert!(!uses_allow_filtering("SELECT * FROM t // ALLOW FILTERING"));
    }

    #[test]
    fn a_keyword_inside_a_string_literal_is_not_a_comment_marker() {
        assert_eq!(
            classify("INSERT INTO t (a) VALUES ('-- not a comment')"),
            CqlQueryClass::Mutation
        );
        // The scan does not look inside literals, so a value that spells the
        // keywords trips it. For a production guard, refusing a statement that
        // merely mentions them is the safe direction to be wrong in.
        assert!(uses_allow_filtering(
            "INSERT INTO t (a) VALUES ('allow filtering')"
        ));
    }

    #[test]
    fn an_unbounded_select_is_the_one_without_where_and_without_limit() {
        assert!(is_unbounded_select("SELECT * FROM t"));
        assert!(!is_unbounded_select("SELECT * FROM t WHERE id = 1"));
        assert!(!is_unbounded_select("SELECT * FROM t LIMIT 100"));
        assert!(!is_unbounded_select("INSERT INTO t (a) VALUES (1)"));
    }

    #[test]
    fn production_refuses_the_irreversible_and_the_ring_wide() {
        assert!(production_refusal("TRUNCATE t").is_some());
        assert!(production_refusal("DROP KEYSPACE ks").is_some());
        assert!(production_refusal("DROP TABLE t").is_some());
        assert!(production_refusal("SELECT * FROM t ALLOW FILTERING").is_some());
        assert!(production_refusal("SELECT * FROM t").is_some());

        assert!(production_refusal("SELECT * FROM t WHERE id = 1").is_none());
        assert!(production_refusal("SELECT * FROM t LIMIT 50").is_none());
        assert!(production_refusal("INSERT INTO t (a) VALUES (1)").is_none());
    }
}

// SPDX-License-Identifier: Apache-2.0

//! Statement splitter for migration scripts.
//!
//! Unlike [`crate::safety::split_sql_statements`], which round-trips through
//! sqlparser and therefore re-renders the SQL, this splitter returns borrowed
//! slices of the original script. Migrations are hand-written SQL that must
//! reach the database exactly as authored — comments, casing and formatting
//! included. Constructs that cannot be split safely are rejected rather than
//! guessed at.

use serde::{Deserialize, Serialize};
use std::ops::Range;

/// A statement carved out of the original script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStatement<'a> {
    /// Exact slice of the input. Invariant: `&input[span] == text`.
    pub text: &'a str,
    pub span: Range<usize>,
    /// 1-based position among the returned statements.
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitErrorCode {
    UnterminatedString,
    UnterminatedComment,
    UnterminatedDollarQuote,
    UnterminatedBracket,
    /// MySQL `DELIMITER`.
    UnsupportedDelimiter,
    /// T-SQL `GO <n>` repeat count.
    UnsupportedGoCount,
    /// Procedural body whose inner `;` cannot be told apart from separators.
    UnsupportedProceduralBlock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitError {
    pub code: SplitErrorCode,
    pub message: String,
    /// Byte offset where the problem was detected.
    pub offset: usize,
}

impl std::fmt::Display for SplitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SplitError {}

impl SplitError {
    fn new(code: SplitErrorCode, offset: usize, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            offset,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SplitDialect {
    dollar_quoting: bool,
    backtick_ident: bool,
    /// SQL Server only: elsewhere `a[1]` is an array index, not a quoted identifier.
    bracket_ident: bool,
    go_batch: bool,
    backslash_escape: bool,
    /// Postgres `E'…'`: backslash escapes, opt-in per literal.
    e_strings: bool,
    hash_comment: bool,
    /// MySQL needs whitespace after `--`; `SELECT 1--2` is arithmetic, not a comment.
    dash_comment_needs_space: bool,
    nested_block_comments: bool,
}

fn dialect_for(driver_id: &str) -> SplitDialect {
    let base = SplitDialect {
        dollar_quoting: false,
        backtick_ident: false,
        bracket_ident: false,
        go_batch: false,
        backslash_escape: false,
        e_strings: false,
        hash_comment: false,
        dash_comment_needs_space: false,
        nested_block_comments: false,
    };
    match driver_id.to_ascii_lowercase().as_str() {
        "postgres" | "cockroachdb" | "timescaledb" | "supabase" | "neon" => SplitDialect {
            dollar_quoting: true,
            e_strings: true,
            nested_block_comments: true,
            ..base
        },
        "mysql" | "mariadb" | "planetscale" => SplitDialect {
            backtick_ident: true,
            backslash_escape: true,
            hash_comment: true,
            dash_comment_needs_space: true,
            ..base
        },
        "sqlite" => SplitDialect {
            backtick_ident: true,
            bracket_ident: true,
            ..base
        },
        "sqlserver" | "mssql" => SplitDialect {
            bracket_ident: true,
            go_batch: true,
            ..base
        },
        _ => base,
    }
}

/// True when `--` at `i` opens a comment for this dialect.
fn is_dash_comment(bytes: &[u8], i: usize, d: SplitDialect) -> bool {
    if !(bytes[i] == b'-' && bytes.get(i + 1) == Some(&b'-')) {
        return false;
    }
    if !d.dash_comment_needs_space {
        return true;
    }
    bytes.get(i + 2).is_none_or(|c| c.is_ascii_whitespace())
}

/// True when the `'` at `i` opens a Postgres `E'…'` literal, where backslash
/// escapes apply. The `E` must not be the tail of a longer identifier.
fn is_e_string(bytes: &[u8], i: usize) -> bool {
    i > 0
        && (bytes[i - 1] | 0x20) == b'e'
        && (i < 2 || !(bytes[i - 2].is_ascii_alphanumeric() || bytes[i - 2] == b'_'))
}

/// Skips past an opaque region (string, comment, quoted identifier, dollar-quote)
/// starting at `i`. Returns the next offset, or `None` when `i` opens nothing.
fn skip_opaque(bytes: &[u8], i: usize, d: SplitDialect) -> Result<Option<usize>, SplitError> {
    let len = bytes.len();
    match bytes[i] {
        b'-' if is_dash_comment(bytes, i, d) => Ok(Some(skip_to_eol(bytes, i))),
        b'#' if d.hash_comment => Ok(Some(skip_to_eol(bytes, i))),
        b'/' if i + 1 < len && bytes[i + 1] == b'*' => skip_block_comment(bytes, i, d).map(Some),
        b'\'' => {
            let escapes = d.backslash_escape || (d.e_strings && is_e_string(bytes, i));
            skip_quoted(bytes, i, b'\'', escapes).map(Some)
        }
        b'"' => skip_quoted(bytes, i, b'"', false).map(Some),
        b'`' if d.backtick_ident => skip_quoted(bytes, i, b'`', false).map(Some),
        b'[' if d.bracket_ident => skip_bracket(bytes, i).map(Some),
        b'$' if d.dollar_quoting => skip_dollar_quote(bytes, i),
        _ => Ok(None),
    }
}

fn skip_to_eol(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

fn skip_block_comment(bytes: &[u8], start: usize, d: SplitDialect) -> Result<usize, SplitError> {
    let len = bytes.len();
    let mut i = start + 2;
    let mut depth = 1usize;
    while i < len {
        if d.nested_block_comments && i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            depth += 1;
            i += 2;
            continue;
        }
        if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'/' {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return Ok(i);
            }
            continue;
        }
        i += 1;
    }
    Err(SplitError::new(
        SplitErrorCode::UnterminatedComment,
        start,
        "Unterminated block comment: the script has a `/*` with no matching `*/`",
    ))
}

/// Skips a `quote`-delimited region. Doubling the quote escapes it; MySQL also
/// allows a backslash escape.
fn skip_quoted(
    bytes: &[u8],
    start: usize,
    quote: u8,
    backslash_escape: bool,
) -> Result<usize, SplitError> {
    let len = bytes.len();
    let mut i = start + 1;
    while i < len {
        let c = bytes[i];
        if backslash_escape && c == b'\\' && i + 1 < len {
            i += 2;
            continue;
        }
        if c == quote {
            if i + 1 < len && bytes[i + 1] == quote {
                i += 2;
                continue;
            }
            return Ok(i + 1);
        }
        i += 1;
    }
    Err(SplitError::new(
        SplitErrorCode::UnterminatedString,
        start,
        format!(
            "Unterminated quoted text: the script has a `{}` with no closing match",
            quote as char
        ),
    ))
}

fn skip_bracket(bytes: &[u8], start: usize) -> Result<usize, SplitError> {
    let len = bytes.len();
    let mut i = start + 1;
    while i < len {
        if bytes[i] == b']' {
            if i + 1 < len && bytes[i + 1] == b']' {
                i += 2;
                continue;
            }
            return Ok(i + 1);
        }
        i += 1;
    }
    Err(SplitError::new(
        SplitErrorCode::UnterminatedBracket,
        start,
        "Unterminated bracketed identifier: the script has a `[` with no closing `]`",
    ))
}

/// Reads a dollar-quote opener (`$$` or `$tag$`) at `i` and returns the tag's
/// byte length including both `$`. `None` when this `$` opens no quote — e.g.
/// the `$1` of a Postgres parameter, whose tag would start with a digit.
fn dollar_tag_len(bytes: &[u8], i: usize) -> Option<usize> {
    let len = bytes.len();
    let mut j = i + 1;
    if j < len && bytes[j] == b'$' {
        return Some(2);
    }
    if j >= len || !(bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
        return None;
    }
    while j < len && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
        j += 1;
    }
    (j < len && bytes[j] == b'$').then(|| j - i + 1)
}

fn skip_dollar_quote(bytes: &[u8], start: usize) -> Result<Option<usize>, SplitError> {
    let Some(tag_len) = dollar_tag_len(bytes, start) else {
        return Ok(None);
    };
    let tag = &bytes[start..start + tag_len];
    let len = bytes.len();
    let mut i = start + tag_len;
    while i + tag_len <= len {
        if &bytes[i..i + tag_len] == tag {
            return Ok(Some(i + tag_len));
        }
        i += 1;
    }
    Err(SplitError::new(
        SplitErrorCode::UnterminatedDollarQuote,
        start,
        format!(
            "Unterminated dollar-quoted block: `{}` has no closing match",
            String::from_utf8_lossy(tag)
        ),
    ))
}

/// True when only whitespace separates `i` from the previous newline.
fn at_line_start(bytes: &[u8], i: usize) -> bool {
    let mut j = i;
    while j > 0 {
        j -= 1;
        match bytes[j] {
            b'\n' => return true,
            c if c.is_ascii_whitespace() => continue,
            _ => return false,
        }
    }
    true
}

/// Matches a `GO` batch separator at `i`. Returns the offset just past it.
fn match_go(bytes: &[u8], i: usize) -> Result<Option<usize>, SplitError> {
    if bytes[i] | 0x20 != b'g' || !at_line_start(bytes, i) {
        return Ok(None);
    }
    let len = bytes.len();
    if i + 1 >= len || bytes[i + 1] | 0x20 != b'o' {
        return Ok(None);
    }
    let j = i + 2;
    // `GO` must stand alone: `GONE` is an identifier, not a separator.
    if j < len && !bytes[j].is_ascii_whitespace() {
        return Ok(None);
    }
    let mut k = j;
    let skip_horizontal = |mut at: usize| {
        while at < len && (bytes[at] == b' ' || bytes[at] == b'\t' || bytes[at] == b'\r') {
            at += 1;
        }
        at
    };
    k = skip_horizontal(k);
    if k < len && bytes[k].is_ascii_digit() {
        return Err(SplitError::new(
            SplitErrorCode::UnsupportedGoCount,
            i,
            "`GO <count>` is not supported in migrations: running the batch once would \
             silently differ from the script. Repeat the statements explicitly.",
        ));
    }
    // SQL Server tooling permits comments on a GO line. Consume comment-only
    // suffixes, but leave any other text in the statement so `GO garbage` is
    // rejected by the server rather than silently losing `garbage`.
    loop {
        if k >= len || bytes[k] == b'\n' {
            return Ok(Some(k));
        }
        if bytes[k] == b'-' && bytes.get(k + 1) == Some(&b'-') {
            return Ok(Some(skip_to_eol(bytes, k)));
        }
        if bytes[k] == b'/' && bytes.get(k + 1) == Some(&b'*') {
            k = skip_block_comment(bytes, k, dialect_for("sqlserver"))?;
            k = skip_horizontal(k);
            continue;
        }
        return Ok(None);
    }
}

fn scan_go_batches(sql: &str, d: SplitDialect) -> Result<Vec<Range<usize>>, SplitError> {
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < len {
        if let Some(next) = skip_opaque(bytes, i, d)? {
            i = next;
            continue;
        }
        if let Some(next) = match_go(bytes, i)? {
            out.push(start..i);
            start = next;
            i = next;
            continue;
        }
        i += 1;
    }
    out.push(start..len);
    Ok(out)
}

fn scan_semicolons(
    sql: &str,
    range: Range<usize>,
    d: SplitDialect,
) -> Result<Vec<Range<usize>>, SplitError> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut start = range.start;
    let mut i = range.start;
    while i < range.end {
        if let Some(next) = skip_opaque(bytes, i, d)? {
            i = next;
            continue;
        }
        if bytes[i] == b';' {
            out.push(start..i);
            i += 1;
            start = i;
            continue;
        }
        i += 1;
    }
    out.push(start..range.end);
    Ok(out)
}

/// Trims surrounding whitespace, shrinking the span so `&sql[span] == text`
/// still holds. Empty results are dropped.
fn push_trimmed<'a>(out: &mut Vec<MigrationStatement<'a>>, sql: &'a str, span: Range<usize>) {
    let raw = &sql[span.clone()];
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }
    let offset = span.start + (trimmed.as_ptr() as usize - raw.as_ptr() as usize);
    out.push(MigrationStatement {
        text: trimmed,
        span: offset..offset + trimmed.len(),
        index: 0,
    });
}

/// True when a slice holds nothing but comments and whitespace. Such a slice is
/// not a statement: sending it to the driver would be a no-op at best.
fn is_comment_only(sql: &str, d: SplitDialect) -> bool {
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    while i < len {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let is_comment_start = (bytes[i] == b'-' && i + 1 < len && bytes[i + 1] == b'-')
            || (bytes[i] == b'#' && d.hash_comment)
            || (bytes[i] == b'/' && i + 1 < len && bytes[i + 1] == b'*');
        if !is_comment_start {
            return false;
        }
        match skip_opaque(bytes, i, d) {
            Ok(Some(next)) => i = next,
            // Unterminated: the caller's scan already errored, so treat the rest
            // as comment and let that error surface.
            _ => return true,
        }
    }
    true
}

fn first_keywords(s: &str) -> String {
    s.split_whitespace()
        .take(6)
        .map(|w| w.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// True when the statement opens a procedural body whose inner `;` are part of
/// the body rather than separators.
fn is_procedural(sql: &str, d: SplitDialect) -> bool {
    let head = first_keywords(sql);
    if d.go_batch {
        return head.starts_with("CREATE PROC")
            || head.starts_with("CREATE OR ALTER PROC")
            || head.starts_with("ALTER PROC")
            || head.starts_with("CREATE FUNCTION")
            || head.starts_with("CREATE TRIGGER");
    }
    false
}

fn reject_unsupported(stmt: &str, d: SplitDialect, offset: usize) -> Result<(), SplitError> {
    let head = first_keywords(stmt);
    if d.hash_comment && head.starts_with("DELIMITER") {
        return Err(SplitError::new(
            SplitErrorCode::UnsupportedDelimiter,
            offset,
            "`DELIMITER` is not supported in migrations. Create routines with a dedicated \
             tool, or keep the migration to plain statements.",
        ));
    }
    // Without DELIMITER, a MySQL routine body's `;` are indistinguishable from
    // separators — splitting would silently truncate the body.
    if d.hash_comment || d.backtick_ident {
        let is_routine = head.contains("PROCEDURE") || head.contains("FUNCTION");
        let is_trigger = head.contains("TRIGGER");
        if head.starts_with("CREATE") && (is_routine || is_trigger) {
            return Err(SplitError::new(
                SplitErrorCode::UnsupportedProceduralBlock,
                offset,
                "Procedural bodies (PROCEDURE/FUNCTION/TRIGGER) are not supported in \
                 migrations for this driver: their inner `;` cannot be told apart from \
                 statement separators.",
            ));
        }
    }
    Ok(())
}

/// Splits a migration script into statements, preserving the original text.
pub fn split_migration_statements<'a>(
    driver_id: &str,
    sql: &'a str,
) -> Result<Vec<MigrationStatement<'a>>, SplitError> {
    let d = dialect_for(driver_id);
    let batches = if d.go_batch {
        scan_go_batches(sql, d)?
    } else {
        vec![0..sql.len()]
    };

    let mut out: Vec<MigrationStatement<'a>> = Vec::new();
    for batch in batches {
        // A T-SQL procedural batch is one statement: only GO ends it.
        if d.go_batch && is_procedural(sql[batch.clone()].trim(), d) {
            push_trimmed(&mut out, sql, batch);
            continue;
        }
        for span in scan_semicolons(sql, batch, d)? {
            let text = sql[span.clone()].trim();
            if text.is_empty() || is_comment_only(text, d) {
                continue;
            }
            reject_unsupported(text, d, span.start)?;
            push_trimmed(&mut out, sql, span);
        }
    }

    for (n, stmt) in out.iter_mut().enumerate() {
        stmt.index = n + 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split<'a>(driver: &str, sql: &'a str) -> Vec<&'a str> {
        split_migration_statements(driver, sql)
            .expect("split should succeed")
            .into_iter()
            .map(|s| s.text)
            .collect()
    }

    fn err(driver: &str, sql: &str) -> SplitError {
        split_migration_statements(driver, sql).expect_err("split should fail")
    }

    #[test]
    fn splits_plain_statements_preserving_exact_text() {
        let sql = "CREATE TABLE users (id serial);\nDROP TABLE old;";
        assert_eq!(
            split("postgres", sql),
            vec!["CREATE TABLE users (id serial)", "DROP TABLE old"]
        );
    }

    #[test]
    fn preserves_comments_and_whitespace_inside_statement() {
        let sql = "CREATE TABLE t (\n  -- keep me\n  id int\n);";
        assert_eq!(split("postgres", sql), vec![sql.trim_end_matches(';')]);
    }

    #[test]
    fn does_not_normalize_casing_or_quoting() {
        let sql = r#"sElEcT "Weird_Col" FROM t;"#;
        assert_eq!(split("postgres", sql), vec![r#"sElEcT "Weird_Col" FROM t"#]);
    }

    #[test]
    fn semicolon_inside_single_quoted_string_is_not_a_separator() {
        assert_eq!(
            split("postgres", "INSERT INTO t VALUES ('a;b');"),
            vec!["INSERT INTO t VALUES ('a;b')"]
        );
    }

    #[test]
    fn semicolon_inside_line_comment_is_not_a_separator() {
        let sql = "SELECT 1 -- a; b\n;SELECT 2;";
        assert_eq!(split("postgres", sql), vec!["SELECT 1 -- a; b", "SELECT 2"]);
    }

    #[test]
    fn semicolon_inside_block_comment_is_not_a_separator() {
        assert_eq!(
            split("postgres", "SELECT /* a; b */ 1;"),
            vec!["SELECT /* a; b */ 1"]
        );
    }

    #[test]
    fn escaped_single_quote_doubled_is_not_a_terminator() {
        assert_eq!(
            split("postgres", "SELECT 'it''s; fine';"),
            vec!["SELECT 'it''s; fine'"]
        );
    }

    #[test]
    fn mysql_backslash_escaped_quote_is_not_a_terminator() {
        assert_eq!(
            split("mysql", r"SELECT 'a\'; b';"),
            vec![r"SELECT 'a\'; b'"]
        );
    }

    #[test]
    fn postgres_backslash_is_literal_not_escape() {
        // Postgres ends the literal at the second quote, so the `;` separates.
        assert_eq!(
            split("postgres", r"SELECT 'a\'; SELECT 2;"),
            vec![r"SELECT 'a\'", "SELECT 2"]
        );
    }

    #[test]
    fn mysql_backtick_identifier_hides_semicolon() {
        assert_eq!(
            split("mysql", "SELECT `a;b` FROM t;"),
            vec!["SELECT `a;b` FROM t"]
        );
    }

    #[test]
    fn mysql_hash_comment_hides_semicolon() {
        let sql = "SELECT 1 # a; b\n;SELECT 2;";
        assert_eq!(split("mysql", sql), vec!["SELECT 1 # a; b", "SELECT 2"]);
    }

    #[test]
    fn sqlserver_bracket_identifier_hides_semicolon() {
        assert_eq!(
            split("sqlserver", "SELECT [a;b] FROM t;"),
            vec!["SELECT [a;b] FROM t"]
        );
    }

    #[test]
    fn postgres_array_index_is_not_a_bracket_quote() {
        assert_eq!(
            split("postgres", "SELECT a[1]; SELECT 2;"),
            vec!["SELECT a[1]", "SELECT 2"]
        );
    }

    #[test]
    fn postgres_dollar_quote_anonymous_hides_semicolons() {
        let sql = "DO $$ BEGIN PERFORM 1; PERFORM 2; END $$;";
        assert_eq!(
            split("postgres", sql),
            vec!["DO $$ BEGIN PERFORM 1; PERFORM 2; END $$"]
        );
    }

    #[test]
    fn postgres_dollar_quote_tagged_hides_semicolons() {
        let sql = "CREATE FUNCTION f() RETURNS int AS $body$ SELECT 1; $body$ LANGUAGE sql;";
        assert_eq!(
            split("postgres", sql),
            vec!["CREATE FUNCTION f() RETURNS int AS $body$ SELECT 1; $body$ LANGUAGE sql"]
        );
    }

    #[test]
    fn postgres_dollar_quote_nested_different_tags() {
        let sql = "DO $outer$ SELECT $inner$;$inner$; $outer$;";
        assert_eq!(
            split("postgres", sql),
            vec!["DO $outer$ SELECT $inner$;$inner$; $outer$"]
        );
    }

    #[test]
    fn postgres_dollar_param_is_not_a_dollar_quote() {
        assert_eq!(
            split("postgres", "SELECT $1; SELECT 2;"),
            vec!["SELECT $1", "SELECT 2"]
        );
    }

    #[test]
    fn postgres_nested_block_comment() {
        assert_eq!(
            split("postgres", "SELECT /* a /* b; */ c */ 1;"),
            vec!["SELECT /* a /* b; */ c */ 1"]
        );
    }

    #[test]
    fn sqlserver_go_separates_batches() {
        let sql = "CREATE TABLE a (id int)\nGO\nCREATE TABLE b (id int)\nGO\n";
        assert_eq!(
            split("sqlserver", sql),
            vec!["CREATE TABLE a (id int)", "CREATE TABLE b (id int)"]
        );
    }

    #[test]
    fn sqlserver_go_only_at_line_start() {
        assert_eq!(split("sqlserver", "SELECT 'GO';"), vec!["SELECT 'GO'"]);
    }

    #[test]
    fn sqlserver_go_inside_string_is_not_a_separator() {
        let sql = "SELECT '\nGO\n' AS s;";
        assert_eq!(split("sqlserver", sql), vec!["SELECT '\nGO\n' AS s"]);
    }

    #[test]
    fn sqlserver_go_with_repeat_count_is_rejected() {
        assert_eq!(
            err("sqlserver", "SELECT 1\nGO 5\n").code,
            SplitErrorCode::UnsupportedGoCount
        );
    }

    #[test]
    fn sqlserver_go_followed_by_junk_is_not_a_separator() {
        // Treating it as GO would silently delete `garbage` from the script;
        // leaving it in lets the server reject it with a real error.
        assert_eq!(
            split("sqlserver", "SELECT 1\nGO garbage\n"),
            vec!["SELECT 1\nGO garbage"]
        );
    }

    #[test]
    fn sqlserver_go_with_line_comment_is_a_separator() {
        assert_eq!(
            split(
                "sqlserver",
                "SELECT 1\nGO -- the first batch ends here\nSELECT 2;",
            ),
            vec!["SELECT 1", "SELECT 2"]
        );
    }

    #[test]
    fn sqlserver_go_with_block_comment_is_a_separator() {
        assert_eq!(
            split("sqlserver", "SELECT 1\nGO /* batch */\nSELECT 2;"),
            vec!["SELECT 1", "SELECT 2"]
        );
    }

    #[test]
    fn sqlserver_go_tolerates_crlf() {
        assert_eq!(
            split("sqlserver", "SELECT 1\r\nGO\r\nSELECT 2\r\n"),
            vec!["SELECT 1", "SELECT 2"]
        );
    }

    #[test]
    fn postgres_e_string_honours_backslash_escapes() {
        // In `E'…'` the `\'` is an escaped quote, so the `;` stays inside.
        assert_eq!(
            split("postgres", r"SELECT E'a\'; b';"),
            vec![r"SELECT E'a\'; b'"]
        );
    }

    #[test]
    fn postgres_plain_string_does_not_honour_backslash_escapes() {
        assert_eq!(
            split("postgres", r"SELECT 'a\'; SELECT 2;"),
            vec![r"SELECT 'a\'", "SELECT 2"]
        );
    }

    #[test]
    fn postgres_identifier_ending_in_e_does_not_open_an_e_string() {
        // `value` ends in `e`, but `value'…'` is not an E-string.
        assert_eq!(
            split("postgres", r"SELECT value'a\'; SELECT 2;"),
            vec![r"SELECT value'a\'", "SELECT 2"]
        );
    }

    #[test]
    fn mysql_dash_dash_without_space_is_not_a_comment() {
        // MySQL reads `1--2` as `1 - (-2)`; treating it as a comment would eat
        // the rest of the line, including the separator.
        assert_eq!(
            split("mysql", "SELECT 1--2;\nSELECT 3;"),
            vec!["SELECT 1--2", "SELECT 3"]
        );
    }

    #[test]
    fn mysql_dash_dash_with_space_is_a_comment() {
        assert_eq!(
            split("mysql", "SELECT 1 -- a; b\n;SELECT 2;"),
            vec!["SELECT 1 -- a; b", "SELECT 2"]
        );
    }

    #[test]
    fn postgres_dash_dash_without_space_is_a_comment() {
        // Postgres has no such rule; the dialects genuinely differ.
        assert_eq!(
            split("postgres", "SELECT 1--2;\nSELECT 3;"),
            vec!["SELECT 1--2;\nSELECT 3"]
        );
    }

    #[test]
    fn sqlserver_procedural_batch_is_kept_whole() {
        let sql = "CREATE PROCEDURE p AS BEGIN SELECT 1; SELECT 2; END\nGO\nSELECT 3;";
        assert_eq!(
            split("sqlserver", sql),
            vec![
                "CREATE PROCEDURE p AS BEGIN SELECT 1; SELECT 2; END",
                "SELECT 3"
            ]
        );
    }

    #[test]
    fn mysql_delimiter_is_rejected() {
        let e = err(
            "mysql",
            "DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; END //\n",
        );
        assert_eq!(e.code, SplitErrorCode::UnsupportedDelimiter);
        assert!(e.message.contains("DELIMITER"));
    }

    #[test]
    fn mysql_create_procedure_is_rejected() {
        assert_eq!(
            err("mysql", "CREATE PROCEDURE p() BEGIN SELECT 1; END;").code,
            SplitErrorCode::UnsupportedProceduralBlock
        );
    }

    #[test]
    fn sqlite_create_trigger_is_rejected() {
        assert_eq!(
            err(
                "sqlite",
                "CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE b SET x = 1; END;"
            )
            .code,
            SplitErrorCode::UnsupportedProceduralBlock
        );
    }

    #[test]
    fn postgres_do_block_is_not_rejected() {
        assert!(split_migration_statements("postgres", "DO $$ BEGIN PERFORM 1; END $$;").is_ok());
    }

    #[test]
    fn postgres_create_function_is_not_rejected() {
        let sql = "CREATE FUNCTION f() RETURNS int AS $$ SELECT 1; $$ LANGUAGE sql;";
        assert!(split_migration_statements("postgres", sql).is_ok());
    }

    #[test]
    fn unterminated_string_is_rejected() {
        let e = err("postgres", "SELECT 'abc");
        assert_eq!(e.code, SplitErrorCode::UnterminatedString);
        assert_eq!(e.offset, 7);
    }

    #[test]
    fn unterminated_block_comment_is_rejected() {
        assert_eq!(
            err("postgres", "SELECT 1 /* abc").code,
            SplitErrorCode::UnterminatedComment
        );
    }

    #[test]
    fn unterminated_dollar_quote_is_rejected() {
        assert_eq!(
            err("postgres", "DO $$ BEGIN END").code,
            SplitErrorCode::UnterminatedDollarQuote
        );
    }

    #[test]
    fn unterminated_bracket_is_rejected() {
        assert_eq!(
            err("sqlserver", "SELECT [abc").code,
            SplitErrorCode::UnterminatedBracket
        );
    }

    #[test]
    fn trailing_semicolon_yields_no_empty_statement() {
        assert_eq!(split("postgres", "SELECT 1;;\n;"), vec!["SELECT 1"]);
    }

    #[test]
    fn comment_only_script_yields_no_statements() {
        let out = split_migration_statements("postgres", "-- nothing here\n").expect("ok");
        assert!(out.is_empty());
    }

    #[test]
    fn statements_are_indexed_from_one() {
        let out = split_migration_statements("postgres", "SELECT 1; SELECT 2;").expect("ok");
        assert_eq!(out.iter().map(|s| s.index).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn spans_are_exact_slices_of_input() {
        let cases: &[(&str, &str)] = &[
            ("postgres", "SELECT 1; SELECT 'a;b'; DO $$ SELECT 1; $$;"),
            ("mysql", "SELECT `a;b`; # c\nSELECT 2;"),
            ("sqlserver", "SELECT [a;b]\nGO\nSELECT 2;"),
            (
                "sqlite",
                "CREATE TABLE t (id int); INSERT INTO t VALUES (1);",
            ),
            ("duckdb", "SELECT 1; /* x; */ SELECT 2;"),
        ];
        for (driver, sql) in cases {
            for stmt in split_migration_statements(driver, sql).expect("ok") {
                assert_eq!(&sql[stmt.span.clone()], stmt.text, "driver {driver}");
            }
        }
    }

    #[test]
    fn roundtrip_fidelity_preserves_every_non_separator_char() {
        // Concatenating the slices back must lose nothing but separators and
        // surrounding whitespace — this is what `to_string()` round-tripping breaks.
        let sql = "CREATE TABLE t (\n  id int -- pk\n);\n\nINSERT INTO t VALUES ('a;b');\n";
        let joined: String = split("postgres", sql).join("");
        let strip = |s: &str| -> String {
            s.chars()
                .filter(|c| !c.is_whitespace() && *c != ';')
                .collect()
        };
        assert_eq!(strip(&joined), strip(sql));
    }

    #[test]
    fn utf8_multibyte_is_preserved() {
        // The legacy `split_ch_statements` scanner casts bytes to char and
        // mangles this; slicing by byte offsets cannot.
        let sql = "INSERT INTO t VALUES ('créé; à göteborg');\nSELECT '日本語';";
        assert_eq!(
            split("postgres", sql),
            vec![
                "INSERT INTO t VALUES ('créé; à göteborg')",
                "SELECT '日本語'"
            ]
        );
    }

    #[test]
    fn utf8_spans_land_on_char_boundaries() {
        let sql = "SELECT 'éàü'; SELECT 'ß';";
        for stmt in split_migration_statements("postgres", sql).expect("ok") {
            assert!(sql.is_char_boundary(stmt.span.start));
            assert!(sql.is_char_boundary(stmt.span.end));
        }
    }
}

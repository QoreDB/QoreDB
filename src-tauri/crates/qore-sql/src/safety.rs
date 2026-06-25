// SPDX-License-Identifier: Apache-2.0

//! SQL safety classification for read-only and production enforcement.

use lru::LruCache;
use sqlparser::{
    ast::{Query, Select, SetExpr, Statement},
    dialect::{
        Dialect, DuckDbDialect, GenericDialect, MsSqlDialect, MySqlDialect, PostgreSqlDialect,
    },
    parser::Parser,
};
use std::num::NonZeroUsize;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqlSafetyAnalysis {
    pub is_mutation: bool,
    pub is_dangerous: bool,
}

type AnalyzeCache = Mutex<LruCache<(String, String), Result<SqlSafetyAnalysis, String>>>;
type ReturnsRowsCache = Mutex<LruCache<(String, String), Result<bool, String>>>;
type SplitCache = Mutex<LruCache<(String, String), Result<Vec<String>, String>>>;

/// Bounded cache of previously-analyzed (driver, trimmed SQL) pairs. sqlparser
/// is the dominant cost in `analyze_sql` (several ms for large queries) and
/// identical queries are re-run constantly during a session. 256 entries caps
/// memory at a few MB worst-case while covering typical editor/reuse patterns.
fn analyze_cache() -> &'static AnalyzeCache {
    static CACHE: OnceLock<AnalyzeCache> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(256).expect("non-zero capacity"),
        ))
    })
}

/// Cache for [`returns_rows`]. `query.rs` consults this on every streaming
/// command to decide whether to dispatch via the row-stream or the affected-
/// rows path; identical queries hit it repeatedly. Keyed identically to
/// `analyze_cache` so a query in the editor pays the parse cost only once
/// regardless of which entry-point the caller hits first.
fn returns_rows_cache() -> &'static ReturnsRowsCache {
    static CACHE: OnceLock<ReturnsRowsCache> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(256).expect("non-zero capacity"),
        ))
    })
}

/// Cache for [`split_sql_statements`]. Used when an editor pastes a multi-
/// statement script — the split result depends only on the dialect + SQL
/// string. Splits up to a few KB are common and re-runs (F5) frequent.
fn split_cache() -> &'static SplitCache {
    static CACHE: OnceLock<SplitCache> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(128).expect("non-zero capacity"),
        ))
    })
}

pub fn analyze_sql(driver_id: &str, sql: &str) -> Result<SqlSafetyAnalysis, String> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err("Empty SQL".to_string());
    }

    let cache_key = (driver_id.to_string(), trimmed.to_string());
    if let Ok(mut cache) = analyze_cache().lock() {
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
    }

    let result = analyze_sql_uncached(driver_id, trimmed);

    if let Ok(mut cache) = analyze_cache().lock() {
        cache.put(cache_key, result.clone());
    }

    result
}

fn analyze_sql_uncached(driver_id: &str, trimmed: &str) -> Result<SqlSafetyAnalysis, String> {
    // ClickHouse: sqlparser's GenericDialect fails to parse much of CH's
    // dialect (ENGINE clauses, ARRAY JOIN, FINAL, SETTINGS, FORMAT, etc.),
    // so we'd reject perfectly valid CH SQL as "parse error". Use our
    // keyword-based classifier instead — coarser but never wrongly blocks.
    if driver_id.eq_ignore_ascii_case("clickhouse") {
        return Ok(analyze_clickhouse(trimmed));
    }

    let dialect = dialect_for_driver(driver_id);
    let statements = Parser::parse_sql(&*dialect, trimmed).map_err(|err| err.to_string())?;

    let mut analysis = SqlSafetyAnalysis {
        is_mutation: false,
        is_dangerous: false,
    };

    for statement in statements {
        if is_mutation_statement(&statement) {
            analysis.is_mutation = true;
        }
        if is_dangerous_statement(&statement) {
            analysis.is_dangerous = true;
        }
    }

    Ok(analysis)
}

fn analyze_clickhouse(trimmed: &str) -> SqlSafetyAnalysis {
    use crate::clickhouse_safety::{classify, ClickHouseQueryClass};
    // Multi-statement scripts on the HTTP wire are rare for CH; classify
    // each segment split on `;` and OR the results so a `DROP TABLE; SELECT 1`
    // still flags as dangerous.
    let mut is_mutation = false;
    let mut is_dangerous = false;
    for stmt in trimmed.split(';') {
        if stmt.trim().is_empty() {
            continue;
        }
        match classify(stmt) {
            ClickHouseQueryClass::Read => {}
            ClickHouseQueryClass::Mutation => is_mutation = true,
            ClickHouseQueryClass::Dangerous => {
                is_mutation = true;
                is_dangerous = true;
            }
            ClickHouseQueryClass::Unknown => {
                // Treat unknown statements as potential mutations so the
                // read-only guard still trips; production confirmation
                // (`prod_require_confirmation`) covers ambiguous cases.
                is_mutation = true;
            }
        }
    }
    SqlSafetyAnalysis {
        is_mutation,
        is_dangerous,
    }
}

pub fn returns_rows(driver_id: &str, sql: &str) -> Result<bool, String> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err("Empty SQL".to_string());
    }

    let cache_key = (driver_id.to_string(), trimmed.to_string());
    if let Ok(mut cache) = returns_rows_cache().lock() {
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
    }

    let result = returns_rows_uncached(driver_id, trimmed);

    if let Ok(mut cache) = returns_rows_cache().lock() {
        cache.put(cache_key, result.clone());
    }

    result
}

fn returns_rows_uncached(driver_id: &str, trimmed: &str) -> Result<bool, String> {
    if driver_id.eq_ignore_ascii_case("clickhouse") {
        use crate::clickhouse_safety::{classify, ClickHouseQueryClass};
        return Ok(matches!(classify(trimmed), ClickHouseQueryClass::Read));
    }

    let dialect = dialect_for_driver(driver_id);
    let statements = Parser::parse_sql(&*dialect, trimmed).map_err(|err| err.to_string())?;

    let first = statements.first().ok_or_else(|| "Empty SQL".to_string())?;
    Ok(statement_returns_rows(first))
}

pub fn split_sql_statements(driver_id: &str, sql: &str) -> Result<Vec<String>, String> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err("Empty SQL".to_string());
    }

    let cache_key = (driver_id.to_string(), trimmed.to_string());
    if let Ok(mut cache) = split_cache().lock() {
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
    }

    let result = split_sql_statements_uncached(driver_id, trimmed);

    if let Ok(mut cache) = split_cache().lock() {
        cache.put(cache_key, result.clone());
    }

    result
}

fn split_sql_statements_uncached(driver_id: &str, trimmed: &str) -> Result<Vec<String>, String> {
    if driver_id.eq_ignore_ascii_case("clickhouse") {
        // sqlparser cannot reliably round-trip CH-specific syntax, so split
        // on top-level `;` outside string literals and trim each piece.
        return Ok(split_ch_statements(trimmed));
    }

    let dialect = dialect_for_driver(driver_id);
    let statements = Parser::parse_sql(&*dialect, trimmed).map_err(|err| err.to_string())?;

    let mut rendered = Vec::with_capacity(statements.len());
    for statement in statements {
        let statement_sql = statement.to_string();
        if !statement_sql.trim().is_empty() {
            rendered.push(statement_sql);
        }
    }

    Ok(rendered)
}

pub fn is_select_prefix(sql: &str) -> bool {
    let trimmed = sql.trim_start().to_ascii_uppercase();
    trimmed.starts_with("SELECT")
        || trimmed.starts_with("WITH")
        || trimmed.starts_with("SHOW")
        || trimmed.starts_with("EXPLAIN")
        || trimmed.starts_with("DESCRIBE")
}

/// Split a ClickHouse multi-statement script on top-level `;` while respecting
/// string literals (`'…'`, `"…"`) and bracketed comments (`-- …`, `/* … */`).
/// Cheaper and safer than running sqlparser on dialect-heavy CH SQL.
fn split_ch_statements(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    let len = bytes.len();
    let mut in_single = false;
    let mut in_double = false;
    while i < len {
        let c = bytes[i] as char;
        if !in_single && !in_double && i + 1 < len {
            if bytes[i] == b'-' && bytes[i + 1] == b'-' {
                while i < len && bytes[i] != b'\n' {
                    buf.push(bytes[i] as char);
                    i += 1;
                }
                continue;
            }
            if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                buf.push_str("/*");
                i += 2;
                while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    buf.push(bytes[i] as char);
                    i += 1;
                }
                if i + 1 < len {
                    buf.push_str("*/");
                    i += 2;
                }
                continue;
            }
        }
        match c {
            '\'' if !in_double => {
                // Treat `\'` as an escaped quote, not a string boundary.
                buf.push(c);
                if i > 0 && bytes[i - 1] == b'\\' {
                    i += 1;
                    continue;
                }
                in_single = !in_single;
                i += 1;
                continue;
            }
            '"' if !in_single => {
                buf.push(c);
                in_double = !in_double;
                i += 1;
                continue;
            }
            ';' if !in_single && !in_double => {
                let s = buf.trim().to_string();
                if !s.is_empty() {
                    out.push(s);
                }
                buf.clear();
                i += 1;
                continue;
            }
            _ => {
                buf.push(c);
                i += 1;
            }
        }
    }
    let s = buf.trim().to_string();
    if !s.is_empty() {
        out.push(s);
    }
    out
}

fn dialect_for_driver(driver_id: &str) -> Box<dyn Dialect> {
    if driver_id.eq_ignore_ascii_case("postgres") || driver_id.eq_ignore_ascii_case("cockroachdb") {
        Box::new(PostgreSqlDialect {})
    } else if driver_id.eq_ignore_ascii_case("mysql") {
        Box::new(MySqlDialect {})
    } else if driver_id.eq_ignore_ascii_case("duckdb") {
        Box::new(DuckDbDialect {})
    } else if driver_id.eq_ignore_ascii_case("sqlserver") || driver_id.eq_ignore_ascii_case("mssql")
    {
        Box::new(MsSqlDialect {})
    } else {
        Box::new(GenericDialect {})
    }
}

fn is_mutation_statement(statement: &Statement) -> bool {
    match statement {
        Statement::Query(query) => query_is_mutation(query),
        Statement::Explain {
            analyze, statement, ..
        } => {
            if *analyze {
                is_mutation_statement(statement)
            } else {
                false
            }
        }
        Statement::ExplainTable { .. }
        | Statement::ShowFunctions { .. }
        | Statement::ShowVariable { .. }
        | Statement::ShowStatus { .. }
        | Statement::ShowVariables { .. }
        | Statement::ShowCreate { .. }
        | Statement::ShowColumns { .. }
        | Statement::ShowDatabases { .. }
        | Statement::ShowSchemas { .. }
        | Statement::ShowCharset(_)
        | Statement::ShowObjects(_)
        | Statement::ShowTables { .. }
        | Statement::ShowViews { .. }
        | Statement::ShowCollation { .. }
        | Statement::Set(_)
        | Statement::Use(_)
        | Statement::StartTransaction { .. }
        | Statement::Commit { .. }
        | Statement::Rollback { .. }
        | Statement::Savepoint { .. }
        | Statement::ReleaseSavepoint { .. } => false,
        _ => true,
    }
}

fn statement_returns_rows(statement: &Statement) -> bool {
    matches!(
        statement,
        Statement::Query(_)
            | Statement::Explain { .. }
            | Statement::ExplainTable { .. }
            | Statement::ShowFunctions { .. }
            | Statement::ShowVariable { .. }
            | Statement::ShowStatus { .. }
            | Statement::ShowVariables { .. }
            | Statement::ShowCreate { .. }
            | Statement::ShowColumns { .. }
            | Statement::ShowDatabases { .. }
            | Statement::ShowSchemas { .. }
            | Statement::ShowCharset(_)
            | Statement::ShowObjects(_)
            | Statement::ShowTables { .. }
            | Statement::ShowViews { .. }
            | Statement::ShowCollation { .. }
    )
}

fn is_dangerous_statement(statement: &Statement) -> bool {
    match statement {
        Statement::Drop { .. }
        | Statement::DropFunction(_)
        | Statement::DropDomain(_)
        | Statement::DropProcedure { .. }
        | Statement::Truncate(_)
        | Statement::AlterTable(_)
        | Statement::AlterSchema(_)
        | Statement::AlterIndex { .. }
        | Statement::AlterView { .. }
        | Statement::AlterType(_)
        | Statement::AlterRole { .. }
        | Statement::AlterPolicy { .. }
        | Statement::AlterConnector { .. }
        | Statement::AlterSession { .. }
        | Statement::AlterUser(_) => true,
        Statement::Update(update) => update.selection.is_none(),
        Statement::Delete(delete) => delete.selection.is_none(),
        Statement::Explain {
            analyze, statement, ..
        } if *analyze => is_dangerous_statement(statement),
        _ => false,
    }
}

fn query_is_mutation(query: &Query) -> bool {
    // A data-modifying CTE (`WITH x AS (UPDATE … RETURNING *) SELECT * FROM x`)
    // keeps a SELECT on the surface but mutates rows inside `query.with`. The
    // body alone classifies it as read-only, so inspect each CTE body too.
    if let Some(with) = &query.with {
        if with
            .cte_tables
            .iter()
            .any(|cte| query_is_mutation(&cte.query))
        {
            return true;
        }
    }
    set_expr_is_mutation(&query.body)
}

fn set_expr_is_mutation(expr: &SetExpr) -> bool {
    match expr {
        SetExpr::Select(select) => select_has_into(select),
        SetExpr::Query(query) => query_is_mutation(query),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_is_mutation(left) || set_expr_is_mutation(right)
        }
        SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Delete(_) | SetExpr::Merge(_) => true,
        SetExpr::Values(_) | SetExpr::Table(_) => false,
    }
}

fn select_has_into(select: &Select) -> bool {
    select.into.is_some()
}

/// Why a DuckDB statement was flagged dangerous. Returned by
/// [`classify_duckdb_dangerous`] so callers can produce a precise error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuckDbDanger {
    /// `INSTALL <ext>` — installs an extension (httpfs, postgres_scanner, …).
    Install,
    /// `LOAD <ext>` — loads an extension into the session.
    Load,
    /// `ATTACH '…'` — attaches an arbitrary database (file, HTTP, postgres).
    Attach,
    /// `COPY … TO '<path>'` — writes query results to an arbitrary path.
    CopyTo,
    /// `PRAGMA enable_external_access` — toggles network / file egress.
    EnableExternalAccess,
}

impl DuckDbDanger {
    pub fn reason(self) -> &'static str {
        match self {
            DuckDbDanger::Install => "INSTALL is blocked (extensions can fetch remote code)",
            DuckDbDanger::Load => {
                "LOAD is blocked (loading extensions enables network/file egress)"
            }
            DuckDbDanger::Attach => {
                "ATTACH is blocked (can mount arbitrary databases, including HTTP)"
            }
            DuckDbDanger::CopyTo => "COPY ... TO is blocked (writes to arbitrary filesystem paths)",
            DuckDbDanger::EnableExternalAccess => {
                "PRAGMA enable_external_access is blocked (toggles network/file egress)"
            }
        }
    }
}

/// Classifies a single DuckDB statement and returns the reason if it would
/// give the user filesystem or network egress beyond the open database.
///
/// Designed to be cheap and conservative: it inspects the leading keyword
/// after stripping comments and whitespace, so an editor pasting a benign
/// `SELECT 1` is unaffected.
pub fn classify_duckdb_dangerous(sql: &str) -> Option<DuckDbDanger> {
    let trimmed = strip_leading_sql_noise(sql);
    let upper = trimmed.to_ascii_uppercase();

    if upper.starts_with("INSTALL") && next_char_is_separator(&trimmed, "INSTALL".len()) {
        return Some(DuckDbDanger::Install);
    }
    if upper.starts_with("LOAD") && next_char_is_separator(&trimmed, "LOAD".len()) {
        return Some(DuckDbDanger::Load);
    }
    if upper.starts_with("ATTACH") && next_char_is_separator(&trimmed, "ATTACH".len()) {
        return Some(DuckDbDanger::Attach);
    }
    if upper.starts_with("PRAGMA") {
        let rest = &upper["PRAGMA".len()..];
        if rest.trim_start().starts_with("ENABLE_EXTERNAL_ACCESS") {
            return Some(DuckDbDanger::EnableExternalAccess);
        }
    }
    if upper.starts_with("COPY") && next_char_is_separator(&trimmed, "COPY".len()) {
        // Block only `COPY … TO '<path>'`. `COPY <table> FROM '…'` imports
        // a local file the user already controls — the normal way to load
        // CSV/Parquet into DuckDB.
        if has_copy_to_clause(&upper) {
            return Some(DuckDbDanger::CopyTo);
        }
    }

    None
}

fn strip_leading_sql_noise(sql: &str) -> &str {
    let mut s = sql.trim_start();
    loop {
        if let Some(rest) = s.strip_prefix("--") {
            if let Some(nl) = rest.find('\n') {
                s = rest[nl + 1..].trim_start();
                continue;
            } else {
                return "";
            }
        }
        if let Some(rest) = s.strip_prefix("/*") {
            if let Some(end) = rest.find("*/") {
                s = rest[end + 2..].trim_start();
                continue;
            } else {
                return "";
            }
        }
        return s;
    }
}

fn next_char_is_separator(s: &str, idx: usize) -> bool {
    match s.as_bytes().get(idx) {
        None => true,
        Some(b) => matches!(*b, b' ' | b'\t' | b'\n' | b'\r' | b'(' | b';'),
    }
}

/// Why a SQLite statement was flagged dangerous. Returned by
/// [`classify_sqlite_dangerous`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteDanger {
    /// `ATTACH DATABASE '/path'` — mounts an arbitrary file/URI into the
    /// session, bypassing the read-only flag on the main database.
    Attach,
    /// `PRAGMA writable_schema = …` — toggles direct edits to `sqlite_master`,
    /// can corrupt the schema.
    WritableSchema,
    /// `PRAGMA journal_mode = OFF` — disables durability (crash = corruption).
    JournalModeOff,
    /// `PRAGMA foreign_keys = OFF` — disables referential integrity.
    ForeignKeysOff,
}

impl SqliteDanger {
    pub fn reason(self) -> &'static str {
        match self {
            SqliteDanger::Attach => {
                "ATTACH DATABASE is blocked (can mount arbitrary files outside the session policy)"
            }
            SqliteDanger::WritableSchema => {
                "PRAGMA writable_schema is blocked (allows direct edits to sqlite_master)"
            }
            SqliteDanger::JournalModeOff => {
                "PRAGMA journal_mode = OFF is blocked (disables durability)"
            }
            SqliteDanger::ForeignKeysOff => {
                "PRAGMA foreign_keys = OFF is blocked (disables referential integrity)"
            }
        }
    }
}

/// Classifies a single SQLite statement and returns the reason if it would
/// silently weaken the database (corrupt schema, disable durability, or
/// mount an arbitrary file).
///
/// Read-only PRAGMA inspections (`PRAGMA table_info`, `PRAGMA foreign_key_list`,
/// `PRAGMA database_list`, …) are not affected — only the specific assignments
/// that flip safety guarantees are blocked.
pub fn classify_sqlite_dangerous(sql: &str) -> Option<SqliteDanger> {
    let trimmed = strip_leading_sql_noise(sql);
    let upper = trimmed.to_ascii_uppercase();

    if upper.starts_with("ATTACH") && next_char_is_separator(&trimmed, "ATTACH".len()) {
        return Some(SqliteDanger::Attach);
    }
    if upper.starts_with("PRAGMA") {
        let rest = upper["PRAGMA".len()..].trim_start();
        // Strip a leading `<dbname>.` qualifier (e.g. `PRAGMA main.writable_schema`).
        let rest = match rest.find('.') {
            Some(idx) => {
                let before_dot = &rest[..idx];
                if !before_dot.is_empty()
                    && before_dot
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    rest[idx + 1..].trim_start()
                } else {
                    rest
                }
            }
            None => rest,
        };
        if rest.starts_with("WRITABLE_SCHEMA") && has_assignment(rest) {
            return Some(SqliteDanger::WritableSchema);
        }
        if rest.starts_with("JOURNAL_MODE")
            && has_assignment(rest)
            && pragma_argument_is(rest, "OFF")
        {
            return Some(SqliteDanger::JournalModeOff);
        }
        if rest.starts_with("FOREIGN_KEYS")
            && has_assignment(rest)
            && pragma_argument_is(rest, "OFF")
        {
            return Some(SqliteDanger::ForeignKeysOff);
        }
    }
    None
}

/// True iff `s` (an already-uppercased PRAGMA tail) contains an `=` or `(` —
/// i.e. it's an assignment / call form rather than a bare read.
fn has_assignment(s: &str) -> bool {
    s.contains('=') || s.contains('(')
}

/// Best-effort check that a PRAGMA's argument matches `expected` (already
/// uppercase). Accepts both `PRAGMA x = OFF` and `PRAGMA x(OFF)`. We deliberately
/// only block the *specific* dangerous values so that swapping `journal_mode`
/// to `WAL` or `MEMORY` from the editor remains possible.
fn pragma_argument_is(s: &str, expected: &str) -> bool {
    let after = s.find(['=', '(']).map(|i| &s[i + 1..]).unwrap_or("");
    let arg = after
        .trim()
        .trim_end_matches(';')
        .trim()
        .trim_end_matches(')')
        .trim();
    arg == expected
}

fn has_copy_to_clause(upper: &str) -> bool {
    // Looks for a ` TO ` keyword followed by a quoted/identifier target.
    // We intentionally search anywhere after the leading `COPY` since DuckDB
    // accepts forms like `COPY (subquery) TO 'path'` and `COPY tbl TO 'p'`.
    let after_copy = &upper["COPY".len()..];
    // Crude but effective: require ` TO ` separated by whitespace and then a
    // quote-like character ('"`) before a newline / end.
    let mut idx = 0;
    while let Some(pos) = after_copy[idx..].find(" TO") {
        let abs = idx + pos;
        let after = abs + " TO".len();
        let next = after_copy.as_bytes().get(after).copied().unwrap_or(b' ');
        if matches!(next, b' ' | b'\t' | b'\n' | b'\r') {
            // Look ahead for a quote on the rest of the line.
            let tail = &after_copy[after..];
            if tail.contains('\'') || tail.contains('"') {
                return true;
            }
        }
        idx = after;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_cte_select_is_read_only() {
        let analysis = analyze_sql(
            "postgres",
            "WITH cte AS (SELECT * FROM users) SELECT * FROM cte",
        )
        .expect("should parse");

        assert!(!analysis.is_mutation);
        assert!(!analysis.is_dangerous);
    }

    #[test]
    fn postgres_write_cte_is_mutation() {
        // A data-modifying CTE keeps a SELECT on the surface; the classifier
        // must still flag it as a mutation so read-only mode blocks it.
        let analysis = analyze_sql(
            "postgres",
            "WITH x AS (UPDATE users SET name = 'x' WHERE id = 1 RETURNING *) SELECT * FROM x",
        )
        .expect("should parse");

        assert!(analysis.is_mutation);
    }

    #[test]
    fn postgres_delete_cte_is_mutation() {
        let analysis = analyze_sql(
            "postgres",
            "WITH d AS (DELETE FROM users WHERE id = 1 RETURNING *) SELECT * FROM d",
        )
        .expect("should parse");

        assert!(analysis.is_mutation);
    }

    #[test]
    fn postgres_multi_statement_flags_mutation() {
        let analysis = analyze_sql(
            "postgres",
            "SELECT 1; UPDATE users SET name = 'x' WHERE id = 1;",
        )
        .expect("should parse");

        assert!(analysis.is_mutation);
        assert!(!analysis.is_dangerous);
    }

    #[test]
    fn postgres_update_without_where_is_dangerous() {
        let analysis =
            analyze_sql("postgres", "UPDATE users SET name = 'x'").expect("should parse");

        assert!(analysis.is_mutation);
        assert!(analysis.is_dangerous);
    }

    #[test]
    fn mysql_delete_without_where_is_dangerous() {
        let analysis = analyze_sql("mysql", "DELETE FROM users").expect("should parse");

        assert!(analysis.is_mutation);
        assert!(analysis.is_dangerous);
    }

    #[test]
    fn select_into_is_mutation() {
        let analysis = analyze_sql("postgres", "SELECT * INTO new_table FROM old_table")
            .expect("should parse");

        assert!(analysis.is_mutation);
        assert!(!analysis.is_dangerous);
    }

    #[test]
    fn alter_table_is_dangerous() {
        let analysis =
            analyze_sql("postgres", "ALTER TABLE users ADD COLUMN age INT").expect("should parse");

        assert!(analysis.is_mutation);
        assert!(analysis.is_dangerous);
    }

    #[test]
    fn mysql_show_tables_is_read_only() {
        let analysis = analyze_sql("mysql", "SHOW TABLES").expect("should parse");

        assert!(!analysis.is_mutation);
        assert!(!analysis.is_dangerous);
    }

    #[test]
    fn splits_postgres_multi_statement() {
        let statements = split_sql_statements(
            "postgres",
            "CREATE TABLE a (id INT); CREATE TABLE b (id INT);",
        )
        .expect("should parse");

        assert_eq!(statements.len(), 2);
        assert!(statements[0]
            .to_ascii_uppercase()
            .starts_with("CREATE TABLE"));
        assert!(statements[1]
            .to_ascii_uppercase()
            .starts_with("CREATE TABLE"));
    }

    #[test]
    fn clickhouse_engine_clause_classifies_without_parse_error() {
        // sqlparser GenericDialect chokes on ENGINE = MergeTree(); the bypass
        // must still classify this as a mutation, not bubble a parse error.
        let analysis = analyze_sql(
            "clickhouse",
            "CREATE TABLE events (id UInt64, ts DateTime) ENGINE = MergeTree() ORDER BY (ts, id)",
        )
        .expect("ch should not parse-error");
        assert!(analysis.is_mutation);
        assert!(!analysis.is_dangerous);
    }

    #[test]
    fn clickhouse_select_is_read_only() {
        let analysis = analyze_sql(
            "clickhouse",
            "SELECT count() FROM events WHERE ts >= now() - INTERVAL 1 DAY",
        )
        .expect("ok");
        assert!(!analysis.is_mutation);
        assert!(!analysis.is_dangerous);
    }

    #[test]
    fn clickhouse_drop_table_is_dangerous() {
        let analysis = analyze_sql("clickhouse", "DROP TABLE events").expect("ok");
        assert!(analysis.is_mutation);
        assert!(analysis.is_dangerous);
    }

    #[test]
    fn clickhouse_alter_update_is_mutation() {
        let analysis = analyze_sql(
            "clickhouse",
            "ALTER TABLE events UPDATE name = 'x' WHERE id = 1",
        )
        .expect("ok");
        assert!(analysis.is_mutation);
        assert!(!analysis.is_dangerous);
    }

    #[test]
    fn clickhouse_optimize_final_is_dangerous() {
        let analysis = analyze_sql("clickhouse", "OPTIMIZE TABLE events FINAL").expect("ok");
        assert!(analysis.is_dangerous);
    }

    #[test]
    fn clickhouse_returns_rows_only_for_reads() {
        assert_eq!(returns_rows("clickhouse", "SELECT 1"), Ok(true));
        assert_eq!(
            returns_rows("clickhouse", "INSERT INTO t VALUES (1)"),
            Ok(false)
        );
        assert_eq!(returns_rows("clickhouse", "EXPLAIN SELECT 1"), Ok(true));
    }

    #[test]
    fn clickhouse_split_respects_string_literals() {
        let stmts =
            split_sql_statements("clickhouse", "INSERT INTO t VALUES ('a;b', 'c'); SELECT 1;")
                .expect("ok");
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].contains("'a;b'"));
        assert!(stmts[1].to_ascii_uppercase().starts_with("SELECT"));
    }

    #[test]
    fn clickhouse_unknown_first_keyword_is_treated_as_mutation() {
        // `USE db` doesn't fit either Read or Mutation lists; classified as
        // Unknown → bypass treats it as a mutation so read-only mode blocks.
        let analysis = analyze_sql("clickhouse", "USE metrics").expect("ok");
        assert!(analysis.is_mutation);
    }

    #[test]
    fn duckdb_install_is_flagged() {
        assert_eq!(
            classify_duckdb_dangerous("INSTALL httpfs"),
            Some(DuckDbDanger::Install)
        );
        assert_eq!(
            classify_duckdb_dangerous("  install postgres_scanner"),
            Some(DuckDbDanger::Install)
        );
    }

    #[test]
    fn duckdb_load_is_flagged() {
        assert_eq!(
            classify_duckdb_dangerous("LOAD httpfs"),
            Some(DuckDbDanger::Load)
        );
    }

    #[test]
    fn duckdb_attach_is_flagged() {
        assert_eq!(
            classify_duckdb_dangerous("ATTACH 'http://x.com/db.duckdb' AS evil"),
            Some(DuckDbDanger::Attach)
        );
    }

    #[test]
    fn duckdb_copy_to_is_flagged() {
        assert_eq!(
            classify_duckdb_dangerous("COPY (SELECT * FROM t) TO '/tmp/leak.csv'"),
            Some(DuckDbDanger::CopyTo)
        );
        assert_eq!(
            classify_duckdb_dangerous("COPY tbl TO '/tmp/x.parquet' (FORMAT PARQUET)"),
            Some(DuckDbDanger::CopyTo)
        );
    }

    #[test]
    fn duckdb_pragma_external_access_is_flagged() {
        assert_eq!(
            classify_duckdb_dangerous("PRAGMA enable_external_access = true"),
            Some(DuckDbDanger::EnableExternalAccess)
        );
    }

    #[test]
    fn duckdb_safe_statements_pass() {
        assert_eq!(classify_duckdb_dangerous("SELECT 1"), None);
        assert_eq!(classify_duckdb_dangerous("INSERT INTO t VALUES (1)"), None);
        // COPY FROM is the normal data import path — should not be blocked.
        assert_eq!(classify_duckdb_dangerous("COPY t FROM '/data/x.csv'"), None);
        // PRAGMA other than enable_external_access is allowed (table_info,
        // database_size, etc. are common metadata helpers).
        assert_eq!(classify_duckdb_dangerous("PRAGMA database_size"), None);
        // Comment-only or empty input must not crash.
        assert_eq!(classify_duckdb_dangerous(""), None);
        assert_eq!(classify_duckdb_dangerous("-- INSTALL httpfs"), None);
    }

    #[test]
    fn duckdb_keyword_prefix_is_not_enough() {
        // `INSTALLED` is not `INSTALL`. The classifier must require a word
        // boundary so identifiers starting with the keyword don't trip it.
        assert_eq!(classify_duckdb_dangerous("INSTALLED_VIEW"), None);
        assert_eq!(classify_duckdb_dangerous("LOADER"), None);
    }

    #[test]
    fn sqlite_attach_is_flagged() {
        assert_eq!(
            classify_sqlite_dangerous("ATTACH DATABASE '/etc/passwd' AS x"),
            Some(SqliteDanger::Attach)
        );
        assert_eq!(
            classify_sqlite_dangerous("attach 'remote.db' as r"),
            Some(SqliteDanger::Attach)
        );
    }

    #[test]
    fn sqlite_pragma_writable_schema_assignment_is_flagged() {
        assert_eq!(
            classify_sqlite_dangerous("PRAGMA writable_schema = 1"),
            Some(SqliteDanger::WritableSchema)
        );
        assert_eq!(
            classify_sqlite_dangerous("PRAGMA writable_schema(ON)"),
            Some(SqliteDanger::WritableSchema)
        );
        // Bare read is informational and stays allowed.
        assert_eq!(classify_sqlite_dangerous("PRAGMA writable_schema"), None);
    }

    #[test]
    fn sqlite_pragma_journal_mode_off_is_flagged() {
        assert_eq!(
            classify_sqlite_dangerous("PRAGMA journal_mode = OFF"),
            Some(SqliteDanger::JournalModeOff)
        );
        // Other journal modes stay allowed.
        assert_eq!(classify_sqlite_dangerous("PRAGMA journal_mode = WAL"), None);
        assert_eq!(
            classify_sqlite_dangerous("PRAGMA journal_mode = MEMORY"),
            None
        );
    }

    #[test]
    fn sqlite_pragma_foreign_keys_off_is_flagged() {
        assert_eq!(
            classify_sqlite_dangerous("PRAGMA foreign_keys = OFF"),
            Some(SqliteDanger::ForeignKeysOff)
        );
        assert_eq!(classify_sqlite_dangerous("PRAGMA foreign_keys = ON"), None);
    }

    #[test]
    fn sqlite_pragma_with_db_qualifier_is_still_caught() {
        assert_eq!(
            classify_sqlite_dangerous("PRAGMA main.foreign_keys = OFF"),
            Some(SqliteDanger::ForeignKeysOff)
        );
    }

    #[test]
    fn sqlite_safe_pragmas_pass() {
        assert_eq!(classify_sqlite_dangerous("PRAGMA table_info(users)"), None);
        assert_eq!(
            classify_sqlite_dangerous("PRAGMA foreign_key_list(orders)"),
            None
        );
        assert_eq!(classify_sqlite_dangerous("PRAGMA index_list(users)"), None);
        assert_eq!(classify_sqlite_dangerous("SELECT 1"), None);
    }
}

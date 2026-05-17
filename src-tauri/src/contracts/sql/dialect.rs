// SPDX-License-Identifier: BUSL-1.1

//! Per-dialect SQL helpers (identifier quoting, function names, regex
//! predicates). Centralizes everything that differs between drivers so
//! the rule builders stay dialect-agnostic.

use super::literal::escape_string_literal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Postgres,
    MySql,
    Sqlite,
    DuckDb,
    SqlServer,
    ClickHouse,
}

impl Dialect {
    pub fn from_driver_id(id: &str) -> Option<Dialect> {
        match id.to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" | "cockroachdb" | "neon" | "supabase" | "timescaledb" => {
                Some(Dialect::Postgres)
            }
            "mysql" | "mariadb" => Some(Dialect::MySql),
            "sqlite" => Some(Dialect::Sqlite),
            "duckdb" => Some(Dialect::DuckDb),
            "sqlserver" | "mssql" => Some(Dialect::SqlServer),
            "clickhouse" => Some(Dialect::ClickHouse),
            _ => None,
        }
    }

    pub fn quote_ident(&self, ident: &str) -> String {
        match self {
            Dialect::MySql | Dialect::ClickHouse => format!("`{}`", ident.replace('`', "``")),
            Dialect::SqlServer => format!("[{}]", ident.replace(']', "]]")),
            Dialect::Postgres | Dialect::Sqlite | Dialect::DuckDb => {
                format!("\"{}\"", ident.replace('"', "\"\""))
            }
        }
    }

    pub fn quote_string(&self, value: &str) -> String {
        escape_string_literal(*self, value)
    }

    /// `length(x)` everywhere except SQL Server (`LEN(x)`) and MySQL
    /// (where `CHAR_LENGTH` is unicode-safe; `LENGTH` returns bytes).
    pub fn length_fn(&self) -> &'static str {
        match self {
            Dialect::SqlServer => "LEN",
            Dialect::MySql => "CHAR_LENGTH",
            _ => "length",
        }
    }

    pub fn now_expr(&self) -> &'static str {
        match self {
            Dialect::Postgres => "NOW()",
            Dialect::MySql => "NOW()",
            Dialect::Sqlite => "CURRENT_TIMESTAMP",
            Dialect::DuckDb => "NOW()",
            Dialect::SqlServer => "GETDATE()",
            Dialect::ClickHouse => "now()",
        }
    }

    /// Builds a regex predicate `<expr> [NOT] MATCHES <pattern>` portable
    /// across supported drivers. Returns `None` if the dialect lacks a
    /// native regex operator — the rule should then be reported as
    /// unsupported on this driver.
    pub fn regex_predicate(&self, col_sql: &str, pattern: &str, negate: bool) -> Option<String> {
        let pat = self.quote_string(pattern);
        Some(match self {
            Dialect::Postgres => {
                let op = if negate { "!~" } else { "~" };
                format!("({col_sql} {op} {pat})")
            }
            Dialect::MySql => {
                let op = if negate { "NOT REGEXP" } else { "REGEXP" };
                format!("({col_sql} {op} {pat})")
            }
            Dialect::DuckDb => {
                let body = format!("regexp_matches({col_sql}, {pat})");
                if negate { format!("(NOT {body})") } else { format!("({body})") }
            }
            Dialect::ClickHouse => {
                let body = format!("match({col_sql}, {pat})");
                if negate { format!("({body} = 0)") } else { format!("({body} = 1)") }
            }
            // SQLite REGEXP requires loading an extension; SQL Server has no
            // built-in regex. The runner surfaces this as 'skipped' with a
            // clear error message.
            Dialect::Sqlite | Dialect::SqlServer => return None,
        })
    }

    /// Timestamp comparison expression: `<now> - INTERVAL <amount> <unit>`.
    /// `amount` is a positive integer, `unit_short` is `ms|s|m|h|d`.
    pub fn now_minus_duration(&self, amount: u64, unit_short: &str) -> Option<String> {
        let (unit_word, factor): (&str, u64) = match unit_short {
            "ms" => return self.interval_milliseconds(amount),
            "s" => ("second", 1),
            "m" => ("minute", 1),
            "h" => ("hour", 1),
            "d" => ("day", 1),
            _ => return None,
        };
        let n = amount.checked_mul(factor)?;
        let now = self.now_expr();
        Some(match self {
            Dialect::Postgres | Dialect::DuckDb => {
                format!("({now} - INTERVAL '{n} {unit_word}')")
            }
            Dialect::MySql => format!("DATE_SUB({now}, INTERVAL {n} {})", unit_word.to_uppercase()),
            Dialect::Sqlite => {
                format!("datetime('now', '-{n} {unit_word}s')")
            }
            Dialect::SqlServer => {
                let mssql_unit = match unit_short {
                    "s" => "SECOND",
                    "m" => "MINUTE",
                    "h" => "HOUR",
                    "d" => "DAY",
                    _ => return None,
                };
                format!("DATEADD({mssql_unit}, -{n}, {now})")
            }
            Dialect::ClickHouse => {
                let ch_fn = match unit_short {
                    "s" => "toIntervalSecond",
                    "m" => "toIntervalMinute",
                    "h" => "toIntervalHour",
                    "d" => "toIntervalDay",
                    _ => return None,
                };
                format!("({now} - {ch_fn}({n}))")
            }
        })
    }

    fn interval_milliseconds(&self, ms: u64) -> Option<String> {
        let now = self.now_expr();
        Some(match self {
            Dialect::Postgres | Dialect::DuckDb => {
                format!("({now} - INTERVAL '{ms} milliseconds')")
            }
            Dialect::MySql => format!("DATE_SUB({now}, INTERVAL {ms} MICROSECOND * 1000)"),
            Dialect::Sqlite => format!("datetime('now', '-{ms} seconds' || '/1000.0')"),
            Dialect::SqlServer => format!("DATEADD(MILLISECOND, -{ms}, {now})"),
            Dialect::ClickHouse => format!("({now} - toIntervalMillisecond({ms}))"),
        })
    }

    /// Qualified table reference using the dialect's quoting rules.
    pub fn qualified_table(&self, schema: Option<&str>, table: &str) -> String {
        match (self, schema) {
            (Dialect::Sqlite, _) => self.quote_ident(table),
            (_, Some(s)) if !s.is_empty() => {
                format!("{}.{}", self.quote_ident(s), self.quote_ident(table))
            }
            _ => self.quote_ident(table),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_driver_id_handles_aliases() {
        assert_eq!(Dialect::from_driver_id("postgres"), Some(Dialect::Postgres));
        assert_eq!(Dialect::from_driver_id("PostgreSQL"), Some(Dialect::Postgres));
        assert_eq!(Dialect::from_driver_id("cockroachdb"), Some(Dialect::Postgres));
        assert_eq!(Dialect::from_driver_id("mariadb"), Some(Dialect::MySql));
        assert_eq!(Dialect::from_driver_id("clickhouse"), Some(Dialect::ClickHouse));
        assert_eq!(Dialect::from_driver_id("mongodb"), None);
    }

    #[test]
    fn quote_ident_per_dialect() {
        assert_eq!(Dialect::Postgres.quote_ident("col"), "\"col\"");
        assert_eq!(Dialect::MySql.quote_ident("col"), "`col`");
        assert_eq!(Dialect::SqlServer.quote_ident("col"), "[col]");
        assert_eq!(Dialect::ClickHouse.quote_ident("col"), "`col`");
        // injection-style escapes
        assert_eq!(Dialect::Postgres.quote_ident("a\"b"), "\"a\"\"b\"");
        assert_eq!(Dialect::MySql.quote_ident("a`b"), "`a``b`");
        assert_eq!(Dialect::SqlServer.quote_ident("a]b"), "[a]]b]");
    }

    #[test]
    fn qualified_table_handles_schemas() {
        assert_eq!(
            Dialect::Postgres.qualified_table(Some("public"), "users"),
            "\"public\".\"users\""
        );
        assert_eq!(
            Dialect::Sqlite.qualified_table(Some("ignored"), "users"),
            "\"users\""
        );
        assert_eq!(
            Dialect::MySql.qualified_table(Some("app"), "users"),
            "`app`.`users`"
        );
        assert_eq!(
            Dialect::SqlServer.qualified_table(Some("dbo"), "users"),
            "[dbo].[users]"
        );
        assert_eq!(Dialect::Postgres.qualified_table(None, "users"), "\"users\"");
    }

    #[test]
    fn regex_predicate_unsupported_on_sqlite_and_mssql() {
        assert!(Dialect::Sqlite.regex_predicate("c", "^a$", false).is_none());
        assert!(Dialect::SqlServer.regex_predicate("c", "^a$", false).is_none());
    }

    #[test]
    fn regex_predicate_postgres() {
        let p = Dialect::Postgres.regex_predicate("\"name\"", "^[A-Z]", false).unwrap();
        assert_eq!(p, "(\"name\" ~ '^[A-Z]')");
        let n = Dialect::Postgres.regex_predicate("\"name\"", "^[A-Z]", true).unwrap();
        assert_eq!(n, "(\"name\" !~ '^[A-Z]')");
    }

    #[test]
    fn regex_predicate_clickhouse() {
        let p = Dialect::ClickHouse.regex_predicate("`x`", "abc", false).unwrap();
        assert!(p.contains("match(`x`, 'abc')"));
        assert!(p.ends_with("= 1)"));
    }

    #[test]
    fn duration_postgres_days() {
        assert_eq!(
            Dialect::Postgres.now_minus_duration(7, "d").unwrap(),
            "(NOW() - INTERVAL '7 day')"
        );
    }

    #[test]
    fn duration_mysql_hours() {
        assert_eq!(
            Dialect::MySql.now_minus_duration(24, "h").unwrap(),
            "DATE_SUB(NOW(), INTERVAL 24 HOUR)"
        );
    }

    #[test]
    fn duration_clickhouse_minutes() {
        assert_eq!(
            Dialect::ClickHouse.now_minus_duration(30, "m").unwrap(),
            "(now() - toIntervalMinute(30))"
        );
    }

    #[test]
    fn duration_invalid_unit() {
        assert!(Dialect::Postgres.now_minus_duration(1, "y").is_none());
    }
}

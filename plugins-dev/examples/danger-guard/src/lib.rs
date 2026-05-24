// SPDX-License-Identifier: Apache-2.0

//! Danger Guard — pre-execute gate that catches the SQL patterns most likely
//! to ruin your day. `DROP`, `TRUNCATE`, `DELETE`/`UPDATE` without a `WHERE`
//! clause are blocked; broad mutations get a warning toast.

use qoredb_plugin_sdk::{export_pre_execute, log, Decision, HookContext, LogLevel};

fn check(ctx: HookContext) -> Decision {
    let normalised = normalise(&ctx.query);

    if let Some(reason) = block_reason(&normalised) {
        log(
            LogLevel::Warn,
            &format!("danger-guard blocked: {reason} (query: {})", preview(&ctx.query)),
        );
        return Decision::block(reason);
    }

    if let Some(message) = warn_message(&normalised, &ctx) {
        return Decision::warn(message);
    }

    Decision::allow()
}

/// Uppercases and collapses whitespace so the matchers don't have to think
/// about `\n`, `\t` or odd casing the user typed.
fn normalise(query: &str) -> String {
    let upper = query.to_uppercase();
    upper.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn block_reason(normalised: &str) -> Option<&'static str> {
    if normalised.starts_with("DROP DATABASE") || normalised.starts_with("DROP SCHEMA") {
        return Some("DROP DATABASE/SCHEMA is irreversible — run it directly in your DB client if you really mean it");
    }
    if normalised.starts_with("DROP TABLE") {
        return Some("DROP TABLE is irreversible — consider RENAME or a soft drop instead");
    }
    if normalised.starts_with("TRUNCATE") {
        return Some("TRUNCATE wipes the whole table without a way back");
    }
    if is_unscoped_mutation(normalised, "DELETE FROM") {
        return Some("DELETE without WHERE wipes the whole table");
    }
    if is_unscoped_mutation(normalised, "UPDATE ") && !normalised.contains(" WHERE ") {
        return Some("UPDATE without WHERE rewrites every row");
    }
    None
}

fn warn_message(normalised: &str, ctx: &HookContext) -> Option<String> {
    if normalised.starts_with("ALTER TABLE") && ctx.environment.eq_ignore_ascii_case("production")
    {
        return Some(format!(
            "ALTER TABLE on a production environment ({}) — double-check the migration plan",
            ctx.environment
        ));
    }
    if ctx.is_dangerous {
        return Some("Query flagged as dangerous by the safety analyser".into());
    }
    None
}

fn is_unscoped_mutation(normalised: &str, prefix: &str) -> bool {
    normalised.starts_with(prefix) && !normalised.contains(" WHERE ")
}

/// Trims the query for the log line. Long queries would otherwise drown the
/// tracing output and we only need a hint of what was blocked.
fn preview(query: &str) -> String {
    const MAX: usize = 120;
    if query.len() <= MAX {
        query.replace('\n', " ")
    } else {
        format!("{}…", &query[..MAX].replace('\n', " "))
    }
}

export_pre_execute!(check);

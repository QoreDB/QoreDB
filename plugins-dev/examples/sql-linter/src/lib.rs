// SPDX-License-Identifier: Apache-2.0

//! Example QoreDB plugin: blocks `UPDATE` / `DELETE` statements that carry no
//! `WHERE` clause — a classic foot-gun that rewrites or wipes a whole table.

use qoredb_plugin_sdk::{export_pre_execute, Decision, HookContext};

fn check(ctx: HookContext) -> Decision {
    let upper = ctx.query.to_uppercase();
    let trimmed = upper.trim_start();
    let is_update_or_delete =
        trimmed.starts_with("UPDATE ") || trimmed.starts_with("DELETE ");

    // Simple heuristic — a production linter would parse the SQL properly.
    if is_update_or_delete && !upper.contains("WHERE") {
        return Decision::block(
            "UPDATE/DELETE without a WHERE clause affects every row — add a filter.",
        );
    }
    Decision::allow()
}

export_pre_execute!(check);

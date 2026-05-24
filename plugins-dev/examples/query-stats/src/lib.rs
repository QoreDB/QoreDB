// SPDX-License-Identifier: Apache-2.0

//! Query Stats — counts how many queries you run, per driver and per
//! operation type. Totals live in the plugin's KV store so they survive
//! restarts; the `show-stats` command returns a JSON snapshot.

use qoredb_plugin_sdk::{
    export_command, export_post_execute, log, storage_delete, storage_get, storage_set,
    CommandEnvelope, LogLevel, PostExecuteEnvelope,
};
use serde_json::{json, Value};

const TOTALS_KEY: &str = "totals";

fn observe(envelope: PostExecuteEnvelope) {
    if !envelope.result.success {
        return;
    }
    let driver = envelope.context.driver_id;
    let op = envelope.context.operation_type.to_lowercase();

    let mut totals = load_totals();
    bump(&mut totals, "all");
    bump(&mut totals, &format!("driver:{driver}"));
    bump(&mut totals, &format!("op:{op}"));

    if !storage_set(TOTALS_KEY, &totals.to_string()) {
        log(LogLevel::Warn, "query-stats: storage write rejected");
    }
}

fn run(envelope: CommandEnvelope) -> Value {
    match envelope.id.as_str() {
        "show-stats" => load_totals(),
        "reset-stats" => {
            storage_delete(TOTALS_KEY);
            json!({ "reset": true })
        }
        _ => Value::Null,
    }
}

fn load_totals() -> Value {
    storage_get(TOTALS_KEY)
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({}))
}

fn bump(totals: &mut Value, key: &str) {
    let obj = totals.as_object_mut().expect("totals is an object");
    let entry = obj.entry(key.to_string()).or_insert_with(|| json!(0));
    let next = entry.as_u64().unwrap_or(0) + 1;
    *entry = json!(next);
}

export_post_execute!(observe);
export_command!(run);

// SPDX-License-Identifier: Apache-2.0

//! Query Stats — counts queries per driver and per operation type.
//!
//! `postExecute` increments the counters (persisted via the `storage`
//! capability, since each invocation runs in a fresh store); the `show-stats`
//! and `reset-stats` commands read and clear them.

use qoredb_plugin_sdk as sdk;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

sdk::export_alloc!();

const STORAGE_KEY: &str = "stats";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Context {
    driver_id: String,
    operation_type: String,
}

#[derive(Deserialize)]
struct Envelope {
    context: Context,
}

#[derive(Deserialize)]
struct Command {
    id: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stats {
    total: u64,
    by_driver: BTreeMap<String, u64>,
    by_operation: BTreeMap<String, u64>,
}

fn load() -> Stats {
    sdk::kv_get(STORAGE_KEY)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(stats: &Stats) {
    if let Ok(json) = serde_json::to_string(stats) {
        sdk::kv_set(STORAGE_KEY, &json);
    }
}

#[no_mangle]
pub extern "C" fn post_execute(ptr: i32, len: i32) {
    let bytes = sdk::input(ptr, len);
    let Ok(env) = serde_json::from_slice::<Envelope>(&bytes) else {
        return;
    };
    let mut stats = load();
    stats.total += 1;
    *stats.by_driver.entry(env.context.driver_id).or_default() += 1;
    *stats.by_operation.entry(env.context.operation_type).or_default() += 1;
    save(&stats);
}

#[no_mangle]
pub extern "C" fn command(ptr: i32, len: i32) -> i64 {
    let bytes = sdk::input(ptr, len);
    let Ok(cmd) = serde_json::from_slice::<Command>(&bytes) else {
        return 0;
    };
    match cmd.id.as_str() {
        "show-stats" => sdk::respond(&load()),
        "reset-stats" => {
            sdk::kv_del(STORAGE_KEY);
            sdk::respond(&serde_json::json!({ "message": "Query stats reset" }))
        }
        _ => 0,
    }
}

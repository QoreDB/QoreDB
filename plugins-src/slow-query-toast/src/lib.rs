// SPDX-License-Identifier: Apache-2.0

//! Slow Query Toast — notifies when a query exceeds a configurable threshold.
//!
//! `postExecute` compares the execution time against the stored threshold
//! (default 1000 ms) and raises a `notify` toast. The `show-threshold` command
//! reports the current value; `set-threshold` cycles through presets (the
//! command UI passes no arguments).

use qoredb_plugin_sdk as sdk;
use serde::Deserialize;

sdk::export_alloc!();

const STORAGE_KEY: &str = "threshold";
const DEFAULT_THRESHOLD_MS: u64 = 1000;
const PRESETS_MS: [u64; 5] = [250, 500, 1000, 2000, 5000];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Context {
    query: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecResult {
    execution_time_ms: u64,
}

#[derive(Deserialize)]
struct Envelope {
    context: Context,
    result: ExecResult,
}

#[derive(Deserialize)]
struct Command {
    id: String,
}

fn threshold() -> u64 {
    sdk::kv_get(STORAGE_KEY)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_THRESHOLD_MS)
}

#[no_mangle]
pub extern "C" fn post_execute(ptr: i32, len: i32) {
    let bytes = sdk::input(ptr, len);
    let Ok(env) = serde_json::from_slice::<Envelope>(&bytes) else {
        return;
    };
    let limit = threshold();
    if env.result.execution_time_ms > limit {
        let preview: String = env.context.query.chars().take(60).collect();
        let msg = format!(
            "Slow query: {} ms (limit {} ms) — {}",
            env.result.execution_time_ms, limit, preview
        );
        sdk::notify(sdk::Level::Warning, &msg);
    }
}

#[no_mangle]
pub extern "C" fn command(ptr: i32, len: i32) -> i64 {
    let bytes = sdk::input(ptr, len);
    let Ok(cmd) = serde_json::from_slice::<Command>(&bytes) else {
        return 0;
    };
    match cmd.id.as_str() {
        "show-threshold" => sdk::respond(&serde_json::json!({ "thresholdMs": threshold() })),
        "set-threshold" => {
            let current = threshold();
            let next = PRESETS_MS
                .iter()
                .copied()
                .find(|&p| p > current)
                .unwrap_or(PRESETS_MS[0]);
            sdk::kv_set(STORAGE_KEY, &next.to_string());
            sdk::respond(&serde_json::json!({
                "thresholdMs": next,
                "message": format!("Slow-query threshold set to {} ms", next)
            }))
        }
        _ => 0,
    }
}

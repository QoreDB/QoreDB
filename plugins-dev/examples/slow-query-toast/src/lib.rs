// SPDX-License-Identifier: Apache-2.0

//! Slow Query Toast — emits a Warning toast when a query takes longer than
//! the configured threshold. Threshold is stored in the plugin KV and can
//! be set through the `set-threshold` command (arg: `{"ms": 500}`).

use qoredb_plugin_sdk::{
    export_command, export_post_execute, notify, storage_get, storage_set, CommandEnvelope,
    NotifyLevel, PostExecuteEnvelope,
};
use serde_json::{json, Value};

const THRESHOLD_KEY: &str = "threshold_ms";
const DEFAULT_THRESHOLD_MS: u64 = 1000;

fn observe(envelope: PostExecuteEnvelope) {
    let threshold = current_threshold();
    let elapsed = envelope.result.execution_time_ms;
    if elapsed < threshold {
        return;
    }
    let op = envelope.context.operation_type.to_lowercase();
    let message = format!(
        "Slow query: {op} on {driver} took {elapsed} ms (threshold {threshold} ms)",
        driver = envelope.context.driver_id
    );
    notify(NotifyLevel::Warning, &message);
}

fn run(envelope: CommandEnvelope) -> Value {
    match envelope.id.as_str() {
        "show-threshold" => json!({ "thresholdMs": current_threshold() }),
        "set-threshold" => match envelope.args.get("ms").and_then(Value::as_u64) {
            Some(ms) if ms > 0 => {
                storage_set(THRESHOLD_KEY, &ms.to_string());
                json!({ "thresholdMs": ms })
            }
            _ => json!({ "error": "expected { \"ms\": <positive integer> }" }),
        },
        _ => Value::Null,
    }
}

fn current_threshold() -> u64 {
    storage_get(THRESHOLD_KEY)
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(DEFAULT_THRESHOLD_MS)
}

export_post_execute!(observe);
export_command!(run);

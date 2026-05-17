// SPDX-License-Identifier: BUSL-1.1

//! Streaming events emitted by `run_contract`.
//!
//! The runner is decoupled from Tauri so the orchestration logic stays
//! unit-testable: it takes a [`ContractEventSink`] and pushes events into
//! it. Production uses [`TauriContractSink`] which fans the event out via
//! `AppHandle::emit`. Tests can plug a recording sink and assert ordering.

use serde::{Deserialize, Serialize};

use super::{ContractRun, RuleResult};

/// Tauri event topic for every contract run notification. The payload carries
/// a `type` discriminant (`started` | `progress` | `completed` | `failed`) so a
/// single listener can fan out client-side.
pub const CONTRACT_RUN_EVENT: &str = "contract.run";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContractRunEvent {
    /// Emitted once when the runner starts.
    Started {
        run_id: String,
        contract_id: String,
        contract_name: String,
        rules_total: u32,
    },
    /// Emitted before each rule starts evaluating.
    RuleStarted {
        run_id: String,
        contract_id: String,
        rule_id: String,
        rule_type: String,
        index: u32,
        total: u32,
    },
    /// Emitted as soon as a rule completes (pass / fail / skipped / error).
    Progress {
        run_id: String,
        contract_id: String,
        result: RuleResult,
        index: u32,
        total: u32,
    },
    /// Emitted at the very end with the aggregated run.
    Completed { run_id: String, run: ContractRun },
    /// Emitted when the runner aborts before producing any rule result
    /// (e.g. unknown driver dialect, session lookup failed).
    Failed {
        run_id: String,
        contract_id: String,
        error: String,
    },
}

/// Side-effect sink the runner uses to surface events. Implementations may
/// emit to Tauri, record to a Vec for tests, or drop everything.
pub trait ContractEventSink: Send + Sync {
    fn emit(&self, event: ContractRunEvent);
}

/// Sink that discards every event. Useful when the caller is happy with the
/// final `ContractRun` returned by `run_contract` and doesn't need streaming.
pub struct NoopSink;

impl ContractEventSink for NoopSink {
    fn emit(&self, _event: ContractRunEvent) {}
}

#[cfg(test)]
pub mod testing {
    use std::sync::Mutex;

    use super::*;

    /// Recording sink for unit tests. Stores every emitted event in order.
    #[derive(Default)]
    pub struct RecordingSink {
        events: Mutex<Vec<ContractRunEvent>>,
    }

    impl RecordingSink {
        pub fn events(&self) -> Vec<ContractRunEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    impl ContractEventSink for RecordingSink {
        fn emit(&self, event: ContractRunEvent) {
            self.events.lock().unwrap().push(event);
        }
    }
}

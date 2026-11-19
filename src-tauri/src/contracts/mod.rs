// SPDX-License-Identifier: BUSL-1.1

//! Data Contracts (Pro) — desktop surface wrapper over `qore_service::contracts`.

#![cfg(feature = "pro")]

pub mod alert;
pub mod runner;

pub use qore_service::contracts::{
    AllowedValue, Contract, ContractMeta, ContractRun, ContractTarget, ForeignKeyReference, Rule,
    RuleResult, RuleStatus, Severity, events, parser, sql, storage,
};

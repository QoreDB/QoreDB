// SPDX-License-Identifier: Apache-2.0

//! QoreService — Tauri-free service layer shared by every QorePlatform surface
//! (desktop, CLI, MCP, server).

pub mod cache;
pub mod metrics;
pub mod paths;
pub mod policy;
pub mod ratelimit;
pub mod sensitive;
pub mod vault;

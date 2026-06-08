// SPDX-License-Identifier: Apache-2.0

//! QoreService — Tauri-free service layer shared by every QorePlatform surface
//! (desktop, CLI, MCP, server).

pub mod cache;
pub mod connection;
pub mod context;
pub mod error;
pub mod governance;
pub mod interceptor;
pub mod license;
pub mod metrics;
pub mod mutation;
pub mod paths;
pub mod policy;
pub mod query;
pub mod ratelimit;
pub mod sensitive;
pub mod vault;
pub mod virtual_relations;

pub use context::ServiceContext;
pub use error::ServiceError;

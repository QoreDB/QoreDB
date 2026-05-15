// SPDX-License-Identifier: BUSL-1.1

//! Instant Data API (Pro) — local HTTP server exposing read-only queries.
//!
//! The server binds **strictly** to `127.0.0.1` (never `0.0.0.0`) and serves
//! a small number of curated endpoints. Every endpoint:
//!   - executes a SQL query classified `Read` (no mutations);
//!   - requires `Authorization: Bearer <token>` (Argon2-hashed at rest);
//!   - is rate-limited via an in-memory token bucket (10 req/s by default).
//!
//! Lifecycle (see [`server::ApiServer`]):
//!   - explicit start via Tauri command;
//!   - shutdown on explicit stop, app lock, or workspace switch.

#![cfg(feature = "pro")]

pub mod auth;
pub mod endpoints;
pub mod handlers;
pub mod openapi;
pub mod rate_limit;
pub mod server;
pub mod types;

pub use endpoints::EndpointStore;
pub use server::ApiServer;
pub use types::{
    Endpoint, EndpointMeta, EndpointParam, EndpointParamType, InstantApiStatus,
    QueryShape,
};

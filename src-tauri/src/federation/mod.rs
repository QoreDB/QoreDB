// SPDX-License-Identifier: BUSL-1.1

//! Cross-Database Federation Engine
//!
//! Enables SQL queries that JOIN tables across multiple database connections.
//! Uses DuckDB as an ephemeral in-memory engine to execute federated queries.

pub mod duckdb_engine;
pub mod manager;
pub mod parser;
pub mod planner;
pub mod types;

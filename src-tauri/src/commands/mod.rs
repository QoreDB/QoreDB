// SPDX-License-Identifier: Apache-2.0

// Tauri Commands Module

pub mod agent;
pub mod agents;
pub mod ai;
pub mod backup;
pub mod cache;
pub mod chat;
pub mod confirmation;
pub mod connection;
pub mod connection_url;
#[cfg(feature = "pro")]
pub mod contracts;
pub mod data_generator;
pub mod driver;
pub mod export;
pub mod federation;
pub mod fulltext_search;
pub mod import;
#[cfg(feature = "pro")]
pub mod instant_api;
pub mod interceptor;
pub mod license;
pub mod logs;
pub mod maintenance;
pub mod metrics;
pub mod migrations;
pub mod mutation;
pub mod plugins;
pub mod policy;
pub mod query;
#[cfg(feature = "pro")]
pub mod replay;
pub mod routines;
pub mod sandbox;
pub mod schema_export;
pub mod sequences;
pub mod share;
pub mod snapshots;
pub mod stream_msg;
pub mod time_travel;
pub mod triggers;
pub mod vault;
pub mod virtual_relations;
pub mod workspace;
pub mod workspace_baselines;
pub mod workspace_migrations;
pub mod workspace_queries;

use std::sync::Arc;

use tauri::State;

use crate::engine::SessionManager;
use crate::engine::types::SessionId;

pub(crate) fn parse_session_id(id: &str) -> Result<SessionId, String> {
    let uuid = uuid::Uuid::parse_str(id).map_err(|e| format!("Invalid session ID: {}", e))?;
    Ok(SessionId(uuid))
}

pub(crate) trait SharedStateExt {
    async fn session_manager(&self) -> Arc<SessionManager>;
}

impl SharedStateExt for State<'_, crate::SharedState> {
    async fn session_manager(&self) -> Arc<SessionManager> {
        Arc::clone(&self.lock().await.session_manager)
    }
}

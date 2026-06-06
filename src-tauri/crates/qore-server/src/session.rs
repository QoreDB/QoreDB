// SPDX-License-Identifier: BUSL-1.1

use uuid::Uuid;

use qore_core::SessionId;
use qore_service::vault::backend::KeyringProvider;
use qore_service::vault::VaultStorage;

use crate::config::{ServerConfig, PROJECT_ID};
use crate::state::AppState;

pub fn storage(config: &ServerConfig) -> VaultStorage {
    VaultStorage::new(
        PROJECT_ID,
        config.config_dir.clone(),
        Box::new(KeyringProvider::new()),
    )
}

pub fn parse_session(session_id: &str) -> Result<SessionId, String> {
    Uuid::parse_str(session_id)
        .map(SessionId)
        .map_err(|_| "invalid session id".to_string())
}

pub async fn connect_saved(state: &AppState, connection_id: &str) -> Result<SessionId, String> {
    let store = storage(&state.config);
    let saved = store
        .get_connection(connection_id)
        .map_err(|_| "connection not found".to_string())?;
    let creds = store
        .get_credentials(connection_id)
        .map_err(|e| e.sanitized_message())?;
    let config = saved
        .to_connection_config(&creds)
        .map_err(|e| e.to_string())?;
    qore_service::connection::connect(&state.ctx.session_manager, config)
        .await
        .map_err(|e| e.sanitized())
}

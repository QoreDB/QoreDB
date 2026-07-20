// SPDX-License-Identifier: BUSL-1.1

//! Commands for agent chat conversation persistence. Pro feature.

use tauri::State;

use crate::SharedState;

#[cfg(not(feature = "pro"))]
const PRO_REQUIRED: &str = "AI Assistant requires a Pro license.";

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn chat_list_conversations(
    _state: State<'_, SharedState>,
) -> Result<Vec<serde_json::Value>, String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn chat_load_conversation(
    _state: State<'_, SharedState>,
    _id: String,
) -> Result<serde_json::Value, String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn chat_save_conversation(
    _state: State<'_, SharedState>,
    _conversation: serde_json::Value,
) -> Result<serde_json::Value, String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn chat_rename_conversation(
    _state: State<'_, SharedState>,
    _id: String,
    _title: String,
) -> Result<serde_json::Value, String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn chat_delete_conversation(
    _state: State<'_, SharedState>,
    _id: String,
) -> Result<(), String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(not(feature = "pro"))]
#[tauri::command]
pub async fn chat_generate_title(
    _state: State<'_, SharedState>,
    _first_message: String,
    _config: serde_json::Value,
) -> Result<String, String> {
    Err(PRO_REQUIRED.to_string())
}

#[cfg(feature = "pro")]
use std::sync::Arc;

#[cfg(feature = "pro")]
use crate::ai::agent::store::{Conversation, ConversationMeta, ConversationStore};
#[cfg(feature = "pro")]
use crate::ai::types::AiConfig;

#[cfg(feature = "pro")]
fn store() -> ConversationStore {
    ConversationStore::new(ConversationStore::default_dir())
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn chat_list_conversations(
    _state: State<'_, SharedState>,
) -> Result<Vec<ConversationMeta>, String> {
    store().list()
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn chat_load_conversation(
    _state: State<'_, SharedState>,
    id: String,
) -> Result<Conversation, String> {
    store().load(&id)
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn chat_save_conversation(
    _state: State<'_, SharedState>,
    conversation: Conversation,
) -> Result<Conversation, String> {
    store().save(conversation)
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn chat_rename_conversation(
    _state: State<'_, SharedState>,
    id: String,
    title: String,
) -> Result<Conversation, String> {
    store().rename(&id, title)
}

#[cfg(feature = "pro")]
#[tauri::command]
pub async fn chat_delete_conversation(
    _state: State<'_, SharedState>,
    id: String,
) -> Result<(), String> {
    store().delete(&id)
}

/// Short model-generated title from the conversation's first message.
#[cfg(feature = "pro")]
#[tauri::command]
pub async fn chat_generate_title(
    state: State<'_, SharedState>,
    first_message: String,
    config: AiConfig,
) -> Result<String, String> {
    crate::ai::context::validate_user_prompt(&first_message)?;
    let ai_manager = {
        let s = state.lock().await;
        Arc::clone(&s.ai_manager)
    };

    // Ten words of output: use the provider's cheapest model, unless a
    // custom base_url exposes an unknown model catalog.
    let mut config = config;
    if config.base_url.is_none() {
        config.model = Some(config.provider.utility_model().to_string());
    }
    config.max_tokens = Some(64);

    let system = "You name conversations. Reply with a short title (max 6 words) in the \
                  language of the message. Title only — no quotes, no punctuation around it.";
    let (title, _) =
        super::ai::collect_streamed_response(&ai_manager, &config, system, &first_message).await?;

    let title = title.trim().trim_matches(['"', '\'', '«', '»']).trim();
    let clamped: String = title.chars().take(80).collect();
    if clamped.is_empty() {
        return Err("Empty title generated".to_string());
    }
    Ok(clamped)
}

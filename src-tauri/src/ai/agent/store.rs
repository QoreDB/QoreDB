// SPDX-License-Identifier: BUSL-1.1

//! Conversation persistence for the agent chat. Option C: only messages and
//! short tool-step summaries are stored — never query results. Files live in
//! `<app data>/chat/`, one JSON per conversation.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ai::types::AiRole;

/// Hard guard for the no-raw-results invariant: a legitimate step summary
/// ("Executed SELECT … — 42 rows") never needs more than this.
pub const MAX_SUMMARY_CHARS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStepSummary {
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub role: AiRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_steps: Vec<ToolStepSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<StoredMessage>,
    /// Display labels of the connections the conversation touched.
    #[serde(default)]
    pub scope: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
}

pub struct ConversationStore {
    dir: PathBuf,
}

impl ConversationStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn default_dir() -> PathBuf {
        qore_service::paths::app_data_dir().join("chat")
    }

    fn path_for(&self, id: &str) -> Result<PathBuf, String> {
        // Ids are UUIDs; parsing rules out path traversal.
        Uuid::parse_str(id).map_err(|_| format!("Invalid conversation id: {id}"))?;
        Ok(self.dir.join(format!("{id}.json")))
    }

    pub fn list(&self) -> Result<Vec<ConversationMeta>, String> {
        let mut out = Vec::new();
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(format!("Failed to read conversations: {e}")),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(conversation) = read_conversation(&path) {
                out.push(ConversationMeta {
                    id: conversation.id,
                    title: conversation.title,
                    created_at: conversation.created_at,
                    updated_at: conversation.updated_at,
                    message_count: conversation.messages.len(),
                });
            }
        }
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(out)
    }

    pub fn load(&self, id: &str) -> Result<Conversation, String> {
        read_conversation(&self.path_for(id)?)
    }

    /// Upserts a conversation. `created_at` is preserved on update,
    /// `updated_at` always refreshed; summaries are clamped so raw query
    /// results can never sneak onto disk.
    pub fn save(&self, mut conversation: Conversation) -> Result<Conversation, String> {
        let path = self.path_for(&conversation.id)?;
        if let Ok(existing) = read_conversation(&path) {
            conversation.created_at = existing.created_at;
        }
        conversation.updated_at = Utc::now();
        for message in &mut conversation.messages {
            for step in &mut message.tool_steps {
                clamp_in_place(&mut step.summary, MAX_SUMMARY_CHARS);
            }
        }

        fs::create_dir_all(&self.dir).map_err(|e| format!("Failed to create chat dir: {e}"))?;
        let bytes = serde_json::to_vec_pretty(&conversation).map_err(|e| e.to_string())?;
        crate::atomic_write::write_atomic(&path, &bytes)
            .map_err(|e| format!("Failed to write conversation: {e}"))?;
        Ok(conversation)
    }

    pub fn rename(&self, id: &str, title: String) -> Result<Conversation, String> {
        let mut conversation = self.load(id)?;
        conversation.title = title;
        self.save(conversation)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let path = self.path_for(id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("Failed to delete conversation: {e}")),
        }
    }
}

fn read_conversation(path: &Path) -> Result<Conversation, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read conversation: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Corrupted conversation file: {e}"))
}

fn clamp_in_place(text: &mut String, max_chars: usize) {
    if text.chars().count() > max_chars {
        let clamped: String = text.chars().take(max_chars).collect();
        *text = format!("{clamped}…");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> Conversation {
        Conversation {
            id: id.to_string(),
            title: "Commandes par client".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            messages: vec![
                StoredMessage {
                    role: AiRole::User,
                    content: "combien de commandes par client ?".to_string(),
                    tool_steps: vec![],
                },
                StoredMessage {
                    role: AiRole::Assistant,
                    content: "Voici la répartition…".to_string(),
                    tool_steps: vec![ToolStepSummary {
                        name: "run_query".to_string(),
                        summary: "SELECT customer_id, COUNT(*) … — 42 lignes".to_string(),
                        is_error: false,
                    }],
                },
            ],
            scope: vec!["local-pg".to_string()],
        }
    }

    #[test]
    fn crud_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConversationStore::new(dir.path().to_path_buf());
        let id = Uuid::new_v4().to_string();

        let saved = store.save(sample(&id)).unwrap();
        assert_eq!(saved.id, id);

        let loaded = store.load(&id).unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[1].tool_steps[0].name, "run_query");

        let renamed = store.rename(&id, "Autre titre".to_string()).unwrap();
        assert_eq!(renamed.title, "Autre titre");
        assert_eq!(renamed.created_at, saved.created_at);

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].message_count, 2);

        store.delete(&id).unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn oversized_tool_summaries_are_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConversationStore::new(dir.path().to_path_buf());
        let id = Uuid::new_v4().to_string();

        let mut conversation = sample(&id);
        conversation.messages[1].tool_steps[0].summary = "x".repeat(10_000);
        store.save(conversation).unwrap();

        let loaded = store.load(&id).unwrap();
        assert!(loaded.messages[1].tool_steps[0].summary.chars().count() <= MAX_SUMMARY_CHARS + 1);
    }

    #[test]
    fn invalid_id_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConversationStore::new(dir.path().to_path_buf());
        assert!(store.load("../../etc/passwd").is_err());
    }
}

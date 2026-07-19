// SPDX-License-Identifier: BUSL-1.1

//! Provider-agnostic tool-calling types for the agent loop.

use serde::{Deserialize, Serialize};

use crate::ai::types::{AiRole, AiUsage};

/// Tool definition advertised to the model, translated per provider
/// (Anthropic `tools`, OpenAI functions, Gemini `functionDeclarations`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A tool invocation requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
    /// Gemini 3 attaches an opaque signature to functionCall parts and
    /// rejects replayed turns that don't echo it. Absent elsewhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
}

/// Result of a tool execution, fed back to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub id: String,
    pub content: String,
    pub is_error: bool,
}

/// One turn of the agent conversation. Assistant messages may carry
/// tool_calls; the following user-side message carries their tool_results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: AiRole,
    pub content: String,
    /// DeepSeek thinking models require the assistant's reasoning content to
    /// be replayed verbatim after a tool call. Absent for other providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_results: Vec<ToolResult>,
}

/// The model's complete answer for one agent iteration: streamed text plus
/// any tool calls to execute before the next iteration.
#[derive(Debug, Clone, Default)]
pub struct AgentTurn {
    pub text: String,
    pub reasoning_content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: AiUsage,
}

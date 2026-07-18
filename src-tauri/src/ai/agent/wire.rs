// SPDX-License-Identifier: BUSL-1.1

//! Translation between the provider-agnostic agent types and each vendor's
//! wire format, plus accumulators for streamed tool-call fragments.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Value, json};

use super::types::{AgentMessage, AgentTool, ToolCall};
use crate::ai::types::AiRole;

pub fn openai_tools(tools: &[AgentTool]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect(),
    )
}

/// OpenAI-style message list: assistant tool_calls carry their arguments as
/// a JSON *string*, and each tool result becomes a separate `tool` message.
pub fn openai_agent_messages(messages: &[AgentMessage]) -> Value {
    let mut out = Vec::new();
    for m in messages {
        match m.role {
            AiRole::System => out.push(json!({ "role": "system", "content": m.content })),
            AiRole::Assistant => {
                let content = if m.content.is_empty() {
                    Value::Null
                } else {
                    Value::String(m.content.clone())
                };
                let mut msg = json!({ "role": "assistant", "content": content });
                if !m.tool_calls.is_empty() {
                    msg["tool_calls"] = Value::Array(
                        m.tool_calls
                            .iter()
                            .map(|c| {
                                json!({
                                    "id": c.id,
                                    "type": "function",
                                    "function": {
                                        "name": c.name,
                                        "arguments": c.input.to_string(),
                                    }
                                })
                            })
                            .collect(),
                    );
                }
                out.push(msg);
            }
            AiRole::User => {
                for r in &m.tool_results {
                    out.push(json!({
                        "role": "tool",
                        "tool_call_id": r.id,
                        "content": r.content,
                    }));
                }
                if !m.content.is_empty() {
                    out.push(json!({ "role": "user", "content": m.content }));
                }
            }
        }
    }
    Value::Array(out)
}

pub fn anthropic_tools(tools: &[AgentTool]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .collect(),
    )
}

/// Returns `(system, messages)`; tool calls become `tool_use` content
/// blocks, tool results `tool_result` blocks on the following user message.
pub fn anthropic_agent_messages(messages: &[AgentMessage]) -> (String, Value) {
    let system = join_system(messages);
    let mut out = Vec::new();
    for m in messages.iter().filter(|m| m.role != AiRole::System) {
        match m.role {
            AiRole::Assistant => {
                let mut content = Vec::new();
                if !m.content.is_empty() {
                    content.push(json!({ "type": "text", "text": m.content }));
                }
                for c in &m.tool_calls {
                    content.push(json!({
                        "type": "tool_use",
                        "id": c.id,
                        "name": c.name,
                        "input": c.input,
                    }));
                }
                out.push(json!({ "role": "assistant", "content": content }));
            }
            _ => {
                let mut content = Vec::new();
                for r in &m.tool_results {
                    content.push(json!({
                        "type": "tool_result",
                        "tool_use_id": r.id,
                        "content": r.content,
                        "is_error": r.is_error,
                    }));
                }
                if !m.content.is_empty() {
                    content.push(json!({ "type": "text", "text": m.content }));
                }
                out.push(json!({ "role": "user", "content": content }));
            }
        }
    }
    (system, Value::Array(out))
}

pub fn gemini_tools(tools: &[AgentTool]) -> Value {
    json!([{
        "functionDeclarations": tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": gemini_schema(t.input_schema.clone()),
                })
            })
            .collect::<Vec<_>>()
    }])
}

/// Gemini's `parameters` field is an OpenAPI subset that rejects unknown
/// keys — `additionalProperties` in particular makes the whole request fail
/// with "Invalid JSON payload received".
fn gemini_schema(mut schema: Value) -> Value {
    strip_unsupported_schema_keys(&mut schema);
    schema
}

fn strip_unsupported_schema_keys(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("additionalProperties");
            map.remove("$schema");
            for child in map.values_mut() {
                strip_unsupported_schema_keys(child);
            }
        }
        Value::Array(items) => {
            for child in items {
                strip_unsupported_schema_keys(child);
            }
        }
        _ => {}
    }
}

/// Returns `(system, contents)`. Gemini matches tool responses by function
/// *name*, so the id→name mapping is rebuilt from prior assistant turns.
pub fn gemini_agent_contents(messages: &[AgentMessage]) -> (String, Value) {
    let system = join_system(messages);
    let mut call_names: HashMap<&str, &str> = HashMap::new();
    let mut out = Vec::new();
    for m in messages.iter().filter(|m| m.role != AiRole::System) {
        match m.role {
            AiRole::Assistant => {
                let mut parts = Vec::new();
                if !m.content.is_empty() {
                    parts.push(json!({ "text": m.content }));
                }
                for c in &m.tool_calls {
                    call_names.insert(c.id.as_str(), c.name.as_str());
                    let mut part = json!({
                        "functionCall": { "name": c.name, "args": c.input }
                    });
                    if let Some(signature) = &c.thought_signature {
                        part["thoughtSignature"] = json!(signature);
                    }
                    parts.push(part);
                }
                out.push(json!({ "role": "model", "parts": parts }));
            }
            _ => {
                let mut parts = Vec::new();
                for r in &m.tool_results {
                    let name = call_names.get(r.id.as_str()).copied().unwrap_or(&r.id);
                    // functionResponse.response must be a JSON object.
                    let response = match serde_json::from_str::<Value>(&r.content) {
                        Ok(v) if v.is_object() => v,
                        Ok(v) => json!({ "result": v }),
                        Err(_) => json!({ "result": r.content }),
                    };
                    parts.push(json!({
                        "functionResponse": { "name": name, "response": response }
                    }));
                }
                if !m.content.is_empty() {
                    parts.push(json!({ "text": m.content }));
                }
                out.push(json!({ "role": "user", "parts": parts }));
            }
        }
    }
    (system, Value::Array(out))
}

fn join_system(messages: &[AgentMessage]) -> String {
    messages
        .iter()
        .filter(|m| m.role == AiRole::System)
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn parse_arguments(raw: &str) -> Value {
    if raw.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
    }
}

/// Accumulates OpenAI-style streamed tool_call fragments: indexed deltas
/// whose `arguments` arrive as partial JSON strings.
#[derive(Default)]
pub struct OpenAiToolCallAccumulator {
    calls: BTreeMap<u64, PartialToolCall>,
}

#[derive(Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl OpenAiToolCallAccumulator {
    /// Feed a `choices[0].delta` object.
    pub fn feed(&mut self, delta: &Value) {
        let Some(items) = delta["tool_calls"].as_array() else {
            return;
        };
        for item in items {
            let index = item["index"].as_u64().unwrap_or(0);
            let entry = self.calls.entry(index).or_default();
            if let Some(id) = item["id"].as_str() {
                entry.id.push_str(id);
            }
            if let Some(name) = item["function"]["name"].as_str() {
                entry.name.push_str(name);
            }
            if let Some(args) = item["function"]["arguments"].as_str() {
                entry.arguments.push_str(args);
            }
        }
    }

    pub fn finish(self) -> Vec<ToolCall> {
        self.calls
            .into_iter()
            .filter(|(_, c)| !c.name.is_empty())
            .map(|(index, c)| ToolCall {
                id: if c.id.is_empty() {
                    format!("call_{index}")
                } else {
                    c.id
                },
                name: c.name,
                input: parse_arguments(&c.arguments),
                thought_signature: None,
            })
            .collect()
    }
}

/// Accumulates Anthropic `tool_use` content blocks streamed as
/// `content_block_start` + `input_json_delta` fragments.
#[derive(Default)]
pub struct AnthropicToolUseAccumulator {
    blocks: BTreeMap<u64, PartialToolUse>,
}

struct PartialToolUse {
    id: String,
    name: String,
    partial_json: String,
}

impl AnthropicToolUseAccumulator {
    /// Feed a `content_block_start` event's `content_block` object.
    pub fn start_block(&mut self, index: u64, content_block: &Value) {
        if content_block["type"].as_str() == Some("tool_use") {
            self.blocks.insert(
                index,
                PartialToolUse {
                    id: content_block["id"].as_str().unwrap_or_default().to_string(),
                    name: content_block["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    partial_json: String::new(),
                },
            );
        }
    }

    /// Feed a `content_block_delta` event's `delta` object.
    pub fn feed_delta(&mut self, index: u64, delta: &Value) {
        if delta["type"].as_str() == Some("input_json_delta") {
            if let (Some(block), Some(part)) =
                (self.blocks.get_mut(&index), delta["partial_json"].as_str())
            {
                block.partial_json.push_str(part);
            }
        }
    }

    pub fn finish(self) -> Vec<ToolCall> {
        self.blocks
            .into_values()
            .filter(|b| !b.name.is_empty())
            .map(|b| ToolCall {
                id: b.id,
                name: b.name,
                input: parse_arguments(&b.partial_json),
                thought_signature: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::ToolResult;
    use super::*;

    fn call(id: &str, name: &str, input: Value) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            input,
            thought_signature: None,
        }
    }

    #[test]
    fn openai_messages_split_tool_results_into_tool_role() {
        let messages = vec![
            AgentMessage {
                role: AiRole::Assistant,
                content: String::new(),
                tool_calls: vec![call("c1", "list_tables", json!({"database": "shop"}))],
                tool_results: vec![],
            },
            AgentMessage {
                role: AiRole::User,
                content: String::new(),
                tool_calls: vec![],
                tool_results: vec![ToolResult {
                    id: "c1".to_string(),
                    content: "[\"users\"]".to_string(),
                    is_error: false,
                }],
            },
        ];
        let out = openai_agent_messages(&messages);
        assert_eq!(out[0]["role"], "assistant");
        assert!(out[0]["content"].is_null());
        assert_eq!(
            out[0]["tool_calls"][0]["function"]["arguments"],
            "{\"database\":\"shop\"}"
        );
        assert_eq!(out[1]["role"], "tool");
        assert_eq!(out[1]["tool_call_id"], "c1");
    }

    #[test]
    fn gemini_tools_strip_additional_properties_recursively() {
        let tools = vec![AgentTool {
            name: "list_tables".to_string(),
            description: "List tables".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "filter": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": { "name": { "type": "string" } }
                    }
                },
                "additionalProperties": false
            }),
        }];
        let out = gemini_tools(&tools);
        let params = &out[0]["functionDeclarations"][0]["parameters"];
        assert!(params.get("additionalProperties").is_none());
        assert!(
            params["properties"]["filter"]
                .get("additionalProperties")
                .is_none()
        );
        assert_eq!(params["properties"]["filter"]["properties"]["name"]["type"], "string");
    }

    #[test]
    fn gemini_echoes_thought_signature_on_function_call_parts() {
        let messages = vec![AgentMessage {
            role: AiRole::Assistant,
            content: String::new(),
            tool_calls: vec![ToolCall {
                id: "call_0".to_string(),
                name: "list_connections".to_string(),
                input: json!({}),
                thought_signature: Some("sig-abc".to_string()),
            }],
            tool_results: vec![],
        }];
        let (_, contents) = gemini_agent_contents(&messages);
        assert_eq!(contents[0]["parts"][0]["thoughtSignature"], "sig-abc");
        assert_eq!(
            contents[0]["parts"][0]["functionCall"]["name"],
            "list_connections"
        );
    }

    #[test]
    fn gemini_resolves_tool_result_names_from_prior_calls() {
        let messages = vec![
            AgentMessage {
                role: AiRole::Assistant,
                content: String::new(),
                tool_calls: vec![call("c1", "describe_table", json!({"table": "users"}))],
                tool_results: vec![],
            },
            AgentMessage {
                role: AiRole::User,
                content: String::new(),
                tool_calls: vec![],
                tool_results: vec![ToolResult {
                    id: "c1".to_string(),
                    content: "not json".to_string(),
                    is_error: false,
                }],
            },
        ];
        let (_, contents) = gemini_agent_contents(&messages);
        let response_part = &contents[1]["parts"][0]["functionResponse"];
        assert_eq!(response_part["name"], "describe_table");
        assert_eq!(response_part["response"]["result"], "not json");
    }

    #[test]
    fn openai_accumulator_reassembles_split_arguments() {
        let mut acc = OpenAiToolCallAccumulator::default();
        acc.feed(&json!({
            "tool_calls": [{"index": 0, "id": "call_1", "function": {"name": "run_query", "arguments": ""}}]
        }));
        acc.feed(&json!({
            "tool_calls": [{"index": 0, "function": {"arguments": "{\"query\":"}}]
        }));
        acc.feed(&json!({
            "tool_calls": [{"index": 0, "function": {"arguments": "\"SELECT 1\"}"}}]
        }));
        let calls = acc.finish();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].name, "run_query");
        assert_eq!(calls[0].input, json!({"query": "SELECT 1"}));
    }

    #[test]
    fn anthropic_accumulator_reassembles_tool_use() {
        let mut acc = AnthropicToolUseAccumulator::default();
        acc.start_block(
            1,
            &json!({"type": "tool_use", "id": "toolu_1", "name": "list_tables"}),
        );
        acc.feed_delta(1, &json!({"type": "input_json_delta", "partial_json": "{\"data"}));
        acc.feed_delta(
            1,
            &json!({"type": "input_json_delta", "partial_json": "base\":\"shop\"}"}),
        );
        let calls = acc.finish();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "toolu_1");
        assert_eq!(calls[0].input, json!({"database": "shop"}));
    }

    #[test]
    fn accumulator_empty_arguments_default_to_object() {
        let mut acc = OpenAiToolCallAccumulator::default();
        acc.feed(&json!({
            "tool_calls": [{"index": 0, "id": "c", "function": {"name": "list_connections"}}]
        }));
        let calls = acc.finish();
        assert_eq!(calls[0].input, json!({}));
    }
}

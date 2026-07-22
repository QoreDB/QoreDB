// SPDX-License-Identifier: BUSL-1.1

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tracing::debug;

use super::agent::types::{AgentMessage, AgentTool, AgentTurn, ToolCall};
use super::agent::wire;
use super::local_runtime::LocalAiRuntime;
use super::types::{
    AiConfig, AiError, AiErrorKind, AiMessage, AiModelInfoOwned, AiProvider, AiRole, AiStreamChunk,
    AiUsage,
};

/// Per-request timeout applied to every LLM HTTP client. Streaming SSE
/// completions can legitimately take ~60 s for long answers, so we pick
/// 120 s as a generous ceiling — beyond that the user has likely lost
/// interest and the request would hold the abort handle / connection
/// indefinitely (cf. audit B7-A1).
const PROVIDER_HTTP_TIMEOUT: Duration = Duration::from_secs(120);
const PROVIDER_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Backoff before each of the two retries on transient failures
/// (429/5xx/transport). Retries never apply once a stream has started.
#[cfg(not(test))]
const RETRY_BACKOFFS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(3)];
#[cfg(test)]
const RETRY_BACKOFFS: [Duration; 2] = [Duration::from_millis(10), Duration::from_millis(20)];

fn build_provider_client() -> Client {
    Client::builder()
        .connect_timeout(PROVIDER_CONNECT_TIMEOUT)
        .timeout(PROVIDER_HTTP_TIMEOUT)
        .build()
        .unwrap_or_else(|err| {
            // Fall back to the default client rather than panic at startup; the
            // request will still surface a transport error on send if the env
            // is truly broken.
            tracing::warn!(?err, "AI provider client builder failed; using default");
            Client::new()
        })
}

#[async_trait]
pub trait AIProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;

    /// Streaming completion — sends chunks via channel.
    /// Returns the token usage reported by the provider, when available.
    async fn stream(
        &self,
        api_key: &str,
        messages: &[AiMessage],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AiUsage, AiError>;

    /// Agent-mode streaming completion with tool calling. Text deltas are
    /// sent through `sender`; completed tool calls come back in the turn.
    /// The default refuses, so a provider without tool support degrades
    /// gracefully instead of silently dropping tools.
    async fn stream_agent(
        &self,
        _api_key: &str,
        _messages: &[AgentMessage],
        _tools: &[AgentTool],
        _config: &AiConfig,
        _sender: mpsc::Sender<AiStreamChunk>,
        _request_id: String,
    ) -> Result<AgentTurn, AiError> {
        Err(AiError::provider(format!(
            "{} does not support tool calling",
            self.provider_id()
        )))
    }
}

/// Send the initial request, retrying up to twice on transient failures.
/// Wraps only the send + status check: once a response body is being
/// streamed, an error is terminal.
async fn send_with_retry(
    builder: reqwest::RequestBuilder,
    provider_name: &str,
) -> Result<reqwest::Response, AiError> {
    let mut current = builder;
    let mut attempt = 0;
    loop {
        let next = current.try_clone();
        let error = match current.send().await {
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) => map_error_response(response, provider_name).await,
            Err(e) => AiError::network(format!("{provider_name} request failed: {e}")),
        };
        // OpenAI can sporadically answer with a 401 invalid_request_error for
        // an otherwise working key in the middle of a tool loop. Retrying a
        // real invalid key is pointless, but bounded replays are safe here because
        // no tool has been executed for a rejected model request.
        let retry_permission_anomaly =
            is_permission_anomaly(&error) && attempt < RETRY_BACKOFFS.len();
        match next {
            Some(retry)
                if (error.is_retryable() && attempt < RETRY_BACKOFFS.len())
                    || retry_permission_anomaly =>
            {
                if retry_permission_anomaly {
                    tracing::warn!(
                        provider = provider_name,
                        request_id = ?error.request_id,
                        "Retrying anomalous provider permission rejection"
                    );
                }
                // Honor the provider's retry-after (bounded) instead of
                // burning both retries inside a still-closed window.
                let retry_after = error.retry_after_secs.unwrap_or(0).min(30);
                let delay = RETRY_BACKOFFS[attempt].max(Duration::from_secs(retry_after));
                tokio::time::sleep(delay).await;
                attempt += 1;
                current = retry;
            }
            _ => return Err(error),
        }
    }
}

async fn map_error_response(response: reqwest::Response, provider_name: &str) -> AiError {
    let status = response.status();
    let request_id = response
        .headers()
        .get("x-request-id")
        .or_else(|| response.headers().get("request-id"))
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok());
    let body = response.text().await.unwrap_or_default();
    let details = extract_api_error_details(&body);
    let message = details
        .message
        .unwrap_or_else(|| format!("{provider_name} HTTP {status}: {body}"));

    let permission_rejection = looks_insufficient_permissions(&message);
    let error = match status.as_u16() {
        401 if permission_rejection => AiError::provider(message),
        401 => AiError::invalid_key(message),
        // A 403 means the key was authenticated but is not authorized for the
        // requested project/model/operation. Calling it an invalid key hides
        // the actual remediation and confused the agent UI.
        403 => AiError::provider(message),
        429 => AiError::rate_limited(message, retry_after),
        400 | 413 if looks_context_too_large(&message) => AiError::context_too_large(message),
        code if code >= 500 => AiError::network(message),
        _ => AiError::provider(message),
    }
    .with_provider_details(
        provider_name,
        status.as_u16(),
        details.code,
        details.error_type,
        request_id,
    );

    tracing::warn!(
        provider = provider_name,
        http_status = status.as_u16(),
        provider_code = ?error.provider_code,
        provider_error_type = ?error.provider_error_type,
        request_id = ?error.request_id,
        "AI provider request rejected"
    );
    error
}

fn looks_insufficient_permissions(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("insufficient permission") || message.contains("missing scope")
}

fn is_permission_anomaly(error: &AiError) -> bool {
    error.http_status == Some(401)
        && error.provider_error_type.as_deref() == Some("invalid_request_error")
        && looks_insufficient_permissions(&error.message)
}

#[derive(Default)]
struct ApiErrorDetails {
    message: Option<String>,
    code: Option<String>,
    error_type: Option<String>,
}

fn json_scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn extract_api_error_details(body: &str) -> ApiErrorDetails {
    let Ok(parsed) = serde_json::from_str::<Value>(body) else {
        return ApiErrorDetails::default();
    };
    let error = &parsed["error"];
    ApiErrorDetails {
        message: error["message"].as_str().map(str::to_owned),
        code: json_scalar_string(&error["code"]),
        error_type: error["type"].as_str().map(str::to_owned),
    }
}

/// Providers report an oversized prompt as a 400 with wording of their own;
/// there is no standard error code across vendors.
fn looks_context_too_large(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    [
        "context length",
        "context_length",
        "too long",
        "maximum context",
        "token limit",
        "tokens exceed",
    ]
    .iter()
    .any(|needle| m.contains(needle))
}

fn stream_error(e: impl std::fmt::Display) -> AiError {
    AiError::network(format!("Stream error: {e}"))
}

/// Splits a byte stream into trimmed, non-empty lines (SSE or NDJSON).
/// Buffering *bytes* keeps multi-byte UTF-8 characters intact when the
/// network splits them across chunks — decoding each chunk separately turned
/// them into U+FFFD replacement characters in streamed text.
#[derive(Default)]
struct StreamLineBuffer {
    bytes: Vec<u8>,
}

impl StreamLineBuffer {
    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.bytes.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(pos) = self.bytes.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = self.bytes.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&raw);
            let line = line.trim();
            if !line.is_empty() {
                lines.push(line.to_string());
            }
        }
        lines
    }
}

/// OpenAI-style `usage` object, present on the final chunk when
/// `stream_options.include_usage` is set (native on Mistral/DeepSeek).
/// `prompt_tokens` *includes* cached tokens on these APIs; normalize so
/// `input_tokens` is the uncached part (AiUsage contract).
fn merge_openai_usage(usage: &mut AiUsage, parsed: &Value) {
    let u = &parsed["usage"];
    let cached = u["prompt_tokens_details"]["cached_tokens"]
        .as_u64()
        .or_else(|| u["prompt_cache_hit_tokens"].as_u64());
    if let Some(v) = u["prompt_tokens"].as_u64() {
        let cached = cached.unwrap_or(0).min(v);
        usage.input_tokens = Some((v - cached) as u32);
        if cached > 0 {
            usage.cache_read_tokens = Some(cached as u32);
        }
    }
    if let Some(v) = u["completion_tokens"].as_u64() {
        usage.output_tokens = Some(v as u32);
    }
}

/// Anthropic `usage` object (from `message_start`/`message_delta`):
/// `input_tokens` already excludes the cache_* counts.
fn merge_anthropic_usage(usage: &mut AiUsage, u: &Value) {
    if let Some(v) = u["input_tokens"].as_u64() {
        usage.input_tokens = Some(v as u32);
    }
    if let Some(v) = u["output_tokens"].as_u64() {
        usage.output_tokens = Some(v as u32);
    }
    if let Some(v) = u["cache_read_input_tokens"].as_u64() {
        usage.cache_read_tokens = Some(v as u32);
    }
    if let Some(v) = u["cache_creation_input_tokens"].as_u64() {
        usage.cache_creation_tokens = Some(v as u32);
    }
}

/// Gemini `usageMetadata`: `promptTokenCount` includes the implicitly cached
/// tokens reported by `cachedContentTokenCount`.
fn merge_gemini_usage(usage: &mut AiUsage, meta: &Value) {
    let cached = meta["cachedContentTokenCount"].as_u64();
    if let Some(v) = meta["promptTokenCount"].as_u64() {
        let cached = cached.unwrap_or(0).min(v);
        usage.input_tokens = Some((v - cached) as u32);
        if cached > 0 {
            usage.cache_read_tokens = Some(cached as u32);
        }
    }
    if let Some(v) = meta["candidatesTokenCount"].as_u64() {
        usage.output_tokens = Some(v as u32);
    }
}

fn role_str(role: AiRole) -> &'static str {
    match role {
        AiRole::System => "system",
        AiRole::User => "user",
        AiRole::Assistant => "assistant",
    }
}

/// Map messages to the OpenAI-style `messages` array (also used by Ollama).
fn openai_style_messages(messages: &[AiMessage]) -> Value {
    Value::Array(
        messages
            .iter()
            .map(|m| json!({ "role": role_str(m.role), "content": m.content }))
            .collect(),
    )
}

/// Text deltas are usually strings, but Mistral may stream typed content
/// chunks. Accept both representations so no visible text is dropped.
fn openai_delta_texts(delta: &Value) -> Vec<&str> {
    match &delta["content"] {
        Value::String(text) => vec![text.as_str()],
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part["text"].as_str().or_else(|| part["content"].as_str()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Native OpenAI Chat Completions has diverged from the older
/// "OpenAI-compatible" dialect used by Mistral and DeepSeek. Keep the native
/// request policy in one place so regular completions and agent turns cannot
/// drift apart as model families evolve.
fn native_openai_chat_body(
    model: &str,
    messages: Value,
    max_tokens: u32,
    temperature: f32,
) -> Value {
    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_completion_tokens": max_tokens,
        "stream": true,
    });

    // Agent tool calling uses the Responses API below. This Chat Completions
    // path is text-only, where recent GPT-5 models support `none` and reject
    // sampling parameters such as temperature.
    if openai_supports_reasoning_none(model) {
        body["reasoning_effort"] = json!("none");
    } else if !openai_is_reasoning_model(model) {
        body["temperature"] = json!(temperature);
    }

    body
}

fn openai_is_reasoning_model(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model == "gpt-5"
        || model.starts_with("gpt-5-")
        || model.starts_with("gpt-5.")
        || model
            .strip_prefix('o')
            .and_then(|suffix| suffix.chars().next())
            .is_some_and(|c| c.is_ascii_digit())
}

fn openai_supports_reasoning_none(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    let Some(version) = model.strip_prefix("gpt-5.") else {
        return false;
    };
    let minor = version
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    minor.parse::<u32>().is_ok_and(|minor| minor >= 4)
}

/// Split messages into a combined system prompt and the user/assistant turns,
/// for APIs where the system prompt travels outside the message list
/// (Anthropic, Gemini).
fn split_system(messages: &[AiMessage]) -> (String, Vec<&AiMessage>) {
    let system = messages
        .iter()
        .filter(|m| m.role == AiRole::System)
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let turns = messages
        .iter()
        .filter(|m| m.role != AiRole::System)
        .collect();
    (system, turns)
}

pub struct OpenAiProvider {
    client: Client,
}

impl OpenAiProvider {
    pub fn new() -> Self {
        Self {
            client: build_provider_client(),
        }
    }
}

impl Default for OpenAiProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AIProvider for OpenAiProvider {
    fn provider_id(&self) -> &'static str {
        "openai"
    }

    async fn stream(
        &self,
        api_key: &str,
        messages: &[AiMessage],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AiUsage, AiError> {
        let model = config.effective_model();
        let max_tokens = config.effective_max_tokens();
        let temperature = config.effective_temperature();
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let mut body = native_openai_chat_body(
            &model,
            openai_style_messages(messages),
            max_tokens,
            temperature,
        );
        body["stream_options"] = json!({ "include_usage": true });

        debug!(
            "OpenAI request: model={}, max_completion_tokens={}",
            model, max_tokens
        );

        let response = send_with_retry(
            self.client
                .post(format!(
                    "{}/chat/completions",
                    base_url.trim_end_matches('/')
                ))
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&body),
            "OpenAI",
        )
        .await?;

        // Parse SSE stream
        let mut usage = AiUsage::default();
        let mut stream = response.bytes_stream();
        let mut buffer = StreamLineBuffer::default();

        use futures::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result.map_err(stream_error)?;
            for line in buffer.push(&bytes) {
                if line.starts_with(':') {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if data.trim() == "[DONE]" {
                        return Ok(usage);
                    }

                    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                        merge_openai_usage(&mut usage, &parsed);
                        for delta in openai_delta_texts(&parsed["choices"][0]["delta"]) {
                            if sender
                                .send(AiStreamChunk::delta(&request_id, delta))
                                .await
                                .is_err()
                            {
                                return Ok(usage); // Receiver dropped (cancelled)
                            }
                        }
                    }
                }
            }
        }

        Ok(usage)
    }

    async fn stream_agent(
        &self,
        api_key: &str,
        messages: &[AgentMessage],
        tools: &[AgentTool],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AgentTurn, AiError> {
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        stream_openai_responses_agent(
            &self.client,
            &format!("{}/responses", base_url.trim_end_matches('/')),
            api_key,
            messages,
            tools,
            config,
            sender,
            request_id,
        )
        .await
    }
}

pub struct AnthropicProvider {
    client: Client,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        Self {
            client: build_provider_client(),
        }
    }
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AIProvider for AnthropicProvider {
    fn provider_id(&self) -> &'static str {
        "anthropic"
    }

    async fn stream(
        &self,
        api_key: &str,
        messages: &[AiMessage],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AiUsage, AiError> {
        let model = config.effective_model();
        let max_tokens = config.effective_max_tokens();
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "https://api.anthropic.com/v1".to_string());

        let (system, turns) = split_system(messages);
        let api_messages: Vec<Value> = turns
            .iter()
            .map(|m| json!({ "role": role_str(m.role), "content": m.content }))
            .collect();

        // No sampling params: recent Anthropic models (Opus 4.7+, Sonnet 5)
        // reject temperature/top_p/top_k with a 400.
        let body = json!({
            "model": model,
            "system": wire::anthropic_system_blocks(&system),
            "messages": api_messages,
            "max_tokens": max_tokens,
            "stream": true
        });

        debug!(
            "Anthropic request: model={}, max_tokens={}",
            model, max_tokens
        );

        let response = send_with_retry(
            self.client
                .post(format!("{}/messages", base_url.trim_end_matches('/')))
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .json(&body),
            "Anthropic",
        )
        .await?;

        // Parse SSE stream (Anthropic format)
        let mut usage = AiUsage::default();
        let mut stream = response.bytes_stream();
        let mut buffer = StreamLineBuffer::default();

        use futures::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result.map_err(stream_error)?;
            for line in buffer.push(&bytes) {
                if line.starts_with(':') {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                        let event_type = parsed["type"].as_str().unwrap_or("");

                        match event_type {
                            "message_start" => {
                                merge_anthropic_usage(&mut usage, &parsed["message"]["usage"]);
                            }
                            "content_block_delta" => {
                                if let Some(text) = parsed["delta"]["text"].as_str() {
                                    if sender
                                        .send(AiStreamChunk::delta(&request_id, text))
                                        .await
                                        .is_err()
                                    {
                                        return Ok(usage);
                                    }
                                }
                            }
                            "message_delta" => {
                                merge_anthropic_usage(&mut usage, &parsed["usage"]);
                            }
                            "message_stop" => {
                                return Ok(usage);
                            }
                            "error" => {
                                let msg = parsed["error"]["message"]
                                    .as_str()
                                    .unwrap_or("Unknown Anthropic error");
                                // Overload is transient: report it as network
                                // so the agent loop may replay the turn.
                                return Err(
                                    if parsed["error"]["type"].as_str() == Some("overloaded_error")
                                    {
                                        AiError::network(msg)
                                    } else {
                                        AiError::provider(msg)
                                    },
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(usage)
    }

    async fn stream_agent(
        &self,
        api_key: &str,
        messages: &[AgentMessage],
        tools: &[AgentTool],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AgentTurn, AiError> {
        let model = config.effective_model();
        let max_tokens = config.effective_max_tokens();
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "https://api.anthropic.com/v1".to_string());

        let (system, mut api_messages) = wire::anthropic_agent_messages(messages);
        wire::anthropic_mark_cache_tail(&mut api_messages);
        // No sampling params: recent Anthropic models reject them with a 400.
        let mut body = json!({
            "model": model,
            "system": wire::anthropic_system_blocks(&system),
            "messages": api_messages,
            "max_tokens": max_tokens,
            "stream": true
        });
        if !tools.is_empty() {
            body["tools"] = wire::anthropic_tools(tools);
        }

        debug!(
            "Anthropic agent request: model={}, tools={}",
            model,
            tools.len()
        );

        let response = send_with_retry(
            self.client
                .post(format!("{}/messages", base_url.trim_end_matches('/')))
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .json(&body),
            "Anthropic",
        )
        .await?;

        let mut turn = AgentTurn::default();
        let mut acc = wire::AnthropicToolUseAccumulator::default();
        let mut stream = response.bytes_stream();
        let mut buffer = StreamLineBuffer::default();

        use futures::StreamExt;
        'outer: while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result.map_err(stream_error)?;
            for line in buffer.push(&bytes) {
                if line.starts_with(':') {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                        let index = parsed["index"].as_u64().unwrap_or(0);
                        match parsed["type"].as_str().unwrap_or("") {
                            "message_start" => {
                                merge_anthropic_usage(&mut turn.usage, &parsed["message"]["usage"]);
                            }
                            "content_block_start" => {
                                acc.start_block(index, &parsed["content_block"]);
                            }
                            "content_block_delta" => {
                                let delta = &parsed["delta"];
                                acc.feed_delta(index, delta);
                                if let Some(text) = delta["text"].as_str() {
                                    turn.text.push_str(text);
                                    if sender
                                        .send(AiStreamChunk::delta(&request_id, text))
                                        .await
                                        .is_err()
                                    {
                                        break 'outer;
                                    }
                                }
                            }
                            "message_delta" => {
                                merge_anthropic_usage(&mut turn.usage, &parsed["usage"]);
                            }
                            "message_stop" => {
                                break 'outer;
                            }
                            "error" => {
                                let msg = parsed["error"]["message"]
                                    .as_str()
                                    .unwrap_or("Unknown Anthropic error");
                                // Overload is transient: report it as network
                                // so the agent loop may replay the turn.
                                return Err(
                                    if parsed["error"]["type"].as_str() == Some("overloaded_error")
                                    {
                                        AiError::network(msg)
                                    } else {
                                        AiError::provider(msg)
                                    },
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        turn.tool_calls = acc.finish();
        Ok(turn)
    }
}

pub struct QoreLocalProvider {
    client: Client,
    runtime: Arc<LocalAiRuntime>,
}

impl QoreLocalProvider {
    pub fn new(runtime: Arc<LocalAiRuntime>) -> Self {
        Self {
            client: build_provider_client(),
            runtime,
        }
    }

    async fn base_url(&self, config: &AiConfig) -> Result<String, AiError> {
        match config.base_url.as_deref().map(str::trim) {
            Some(base_url) if !base_url.is_empty() => {
                let parsed = reqwest::Url::parse(base_url)
                    .map_err(|_| AiError::provider("Invalid Qore AI Local endpoint"))?;
                let local_host = parsed.host_str().is_some_and(|host| {
                    host.eq_ignore_ascii_case("localhost")
                        || host
                            .parse::<std::net::IpAddr>()
                            .is_ok_and(|address| address.is_loopback())
                });
                if parsed.scheme() != "http" || !local_host {
                    return Err(AiError::provider(
                        "Qore AI Local only accepts loopback HTTP endpoints",
                    ));
                }
                Ok(base_url.trim_end_matches('/').to_string())
            }
            _ => self
                .runtime
                .ensure_running()
                .await
                .map_err(AiError::provider),
        }
    }
}

#[async_trait]
impl AIProvider for QoreLocalProvider {
    fn provider_id(&self) -> &'static str {
        "qore_local"
    }

    async fn stream(
        &self,
        _api_key: &str,
        messages: &[AiMessage],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AiUsage, AiError> {
        let base_url = self.base_url(config).await?;
        stream_openai_compatible(
            &self.client,
            &format!("{base_url}/chat/completions"),
            "qore-local",
            messages,
            config,
            sender,
            request_id,
            "Qore AI Local",
        )
        .await
    }

    async fn stream_agent(
        &self,
        _api_key: &str,
        messages: &[AgentMessage],
        tools: &[AgentTool],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AgentTurn, AiError> {
        let base_url = self.base_url(config).await?;
        stream_agent_openai_compatible(
            &self.client,
            &format!("{base_url}/chat/completions"),
            "qore-local",
            messages,
            tools,
            config,
            sender,
            request_id,
            "Qore AI Local",
            false,
            false,
        )
        .await
    }
}

pub struct OllamaProvider {
    client: Client,
}

impl OllamaProvider {
    pub fn new() -> Self {
        Self {
            client: build_provider_client(),
        }
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AIProvider for OllamaProvider {
    fn provider_id(&self) -> &'static str {
        "ollama"
    }

    async fn stream(
        &self,
        _api_key: &str,
        messages: &[AiMessage],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AiUsage, AiError> {
        let model = config.effective_model();
        let max_tokens = config.effective_max_tokens();
        let temperature = config.effective_temperature();
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "http://localhost:11434".to_string());

        let body = json!({
            "model": model,
            "messages": openai_style_messages(messages),
            "options": {
                "num_predict": max_tokens,
                "temperature": temperature
            },
            "stream": true
        });

        debug!("Ollama request: model={}, base_url={}", model, base_url);

        let response = send_with_retry(
            self.client
                .post(format!("{}/api/chat", base_url.trim_end_matches('/')))
                .header("Content-Type", "application/json")
                .json(&body),
            "Ollama",
        )
        .await
        .map_err(|mut e| {
            if e.kind == AiErrorKind::Network {
                e.message.push_str(". Is Ollama running?");
            }
            e
        })?;

        // Parse NDJSON stream
        let mut usage = AiUsage::default();
        let mut stream = response.bytes_stream();
        let mut buffer = StreamLineBuffer::default();

        use futures::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result.map_err(stream_error)?;
            for line in buffer.push(&bytes) {
                if let Ok(parsed) = serde_json::from_str::<Value>(&line) {
                    let done = parsed["done"].as_bool().unwrap_or(false);

                    if let Some(content) = parsed["message"]["content"].as_str() {
                        if !content.is_empty() {
                            if sender
                                .send(AiStreamChunk::delta(&request_id, content))
                                .await
                                .is_err()
                            {
                                return Ok(usage);
                            }
                        }
                    }

                    if done {
                        if let Some(v) = parsed["prompt_eval_count"].as_u64() {
                            usage.input_tokens = Some(v as u32);
                        }
                        if let Some(v) = parsed["eval_count"].as_u64() {
                            usage.output_tokens = Some(v as u32);
                        }
                        return Ok(usage);
                    }
                }
            }
        }

        Ok(usage)
    }

    async fn stream_agent(
        &self,
        _api_key: &str,
        messages: &[AgentMessage],
        tools: &[AgentTool],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AgentTurn, AiError> {
        let model = config.effective_model();
        let max_tokens = config.effective_max_tokens();
        let temperature = config.effective_temperature();
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "http://localhost:11434".to_string());

        let mut body = json!({
            "model": model,
            "messages": wire::openai_agent_messages(messages),
            "options": {
                "num_predict": max_tokens,
                "temperature": temperature
            },
            "stream": true
        });
        if !tools.is_empty() {
            body["tools"] = wire::openai_tools(tools);
        }

        debug!(
            "Ollama agent request: model={}, tools={}",
            model,
            tools.len()
        );

        // A model without tool support answers 400 with an explicit message;
        // the orchestrator uses that Provider error to fall back to text mode.
        let response = send_with_retry(
            self.client
                .post(format!("{}/api/chat", base_url.trim_end_matches('/')))
                .header("Content-Type", "application/json")
                .json(&body),
            "Ollama",
        )
        .await
        .map_err(|mut e| {
            if e.kind == AiErrorKind::Network {
                e.message.push_str(". Is Ollama running?");
            }
            e
        })?;

        let mut turn = AgentTurn::default();
        let mut stream = response.bytes_stream();
        let mut buffer = StreamLineBuffer::default();

        use futures::StreamExt;
        'outer: while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result.map_err(stream_error)?;
            for line in buffer.push(&bytes) {
                if let Ok(parsed) = serde_json::from_str::<Value>(&line) {
                    let done = parsed["done"].as_bool().unwrap_or(false);

                    // Ollama sends tool calls complete, arguments as object.
                    if let Some(calls) = parsed["message"]["tool_calls"].as_array() {
                        for call in calls {
                            if let Some(name) = call["function"]["name"].as_str() {
                                turn.tool_calls.push(ToolCall {
                                    id: format!("call_{}", turn.tool_calls.len()),
                                    name: name.to_string(),
                                    input: call["function"]["arguments"].clone(),
                                    thought_signature: None,
                                });
                            }
                        }
                    }

                    if let Some(content) = parsed["message"]["content"].as_str() {
                        if !content.is_empty() {
                            turn.text.push_str(content);
                            if sender
                                .send(AiStreamChunk::delta(&request_id, content))
                                .await
                                .is_err()
                            {
                                break 'outer;
                            }
                        }
                    }

                    if done {
                        if let Some(v) = parsed["prompt_eval_count"].as_u64() {
                            turn.usage.input_tokens = Some(v as u32);
                        }
                        if let Some(v) = parsed["eval_count"].as_u64() {
                            turn.usage.output_tokens = Some(v as u32);
                        }
                        break 'outer;
                    }
                }
            }
        }

        Ok(turn)
    }
}

pub struct MistralAiProvider {
    client: Client,
}

impl MistralAiProvider {
    pub fn new() -> Self {
        Self {
            client: build_provider_client(),
        }
    }
}

impl Default for MistralAiProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AIProvider for MistralAiProvider {
    fn provider_id(&self) -> &'static str {
        "mistral_ai"
    }

    async fn stream(
        &self,
        api_key: &str,
        messages: &[AiMessage],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AiUsage, AiError> {
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "https://api.mistral.ai/v1".to_string());
        stream_openai_compatible(
            &self.client,
            &format!("{}/chat/completions", base_url.trim_end_matches('/')),
            api_key,
            messages,
            config,
            sender,
            request_id,
            "Mistral",
        )
        .await
    }

    async fn stream_agent(
        &self,
        api_key: &str,
        messages: &[AgentMessage],
        tools: &[AgentTool],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AgentTurn, AiError> {
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "https://api.mistral.ai/v1".to_string());
        stream_agent_openai_compatible(
            &self.client,
            &format!("{}/chat/completions", base_url.trim_end_matches('/')),
            api_key,
            messages,
            tools,
            config,
            sender,
            request_id,
            "Mistral",
            false,
            false,
        )
        .await
    }
}

pub struct GoogleGeminiProvider {
    client: Client,
}

impl GoogleGeminiProvider {
    pub fn new() -> Self {
        Self {
            client: build_provider_client(),
        }
    }
}

impl Default for GoogleGeminiProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AIProvider for GoogleGeminiProvider {
    fn provider_id(&self) -> &'static str {
        "google_gemini"
    }

    async fn stream(
        &self,
        api_key: &str,
        messages: &[AiMessage],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AiUsage, AiError> {
        let model = config.effective_model();
        let max_tokens = config.effective_max_tokens();
        let temperature = config.effective_temperature();
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string());

        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse",
            base_url.trim_end_matches('/'),
            model
        );

        let (system, turns) = split_system(messages);
        let contents: Vec<Value> = turns
            .iter()
            .map(|m| {
                let role = match m.role {
                    AiRole::Assistant => "model",
                    _ => "user",
                };
                json!({ "role": role, "parts": [{ "text": m.content }] })
            })
            .collect();

        let body = json!({
            "systemInstruction": {
                "parts": [{ "text": system }]
            },
            "contents": contents,
            "generationConfig": {
                "maxOutputTokens": max_tokens,
                "temperature": temperature
            }
        });

        debug!("Gemini request: model={}, max_tokens={}", model, max_tokens);

        let response = send_with_retry(
            self.client
                .post(&url)
                .header("x-goog-api-key", api_key)
                .header("Content-Type", "application/json")
                .json(&body),
            "Gemini",
        )
        .await?;

        // Parse SSE stream (Gemini format)
        let mut usage = AiUsage::default();
        let mut stream = response.bytes_stream();
        let mut buffer = StreamLineBuffer::default();

        use futures::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result.map_err(stream_error)?;
            for line in buffer.push(&bytes) {
                if line.starts_with(':') {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                        merge_gemini_usage(&mut usage, &parsed["usageMetadata"]);
                        if let Some(parts) = parsed["candidates"][0]["content"]["parts"].as_array()
                        {
                            for part in parts {
                                if let Some(text) = part["text"].as_str() {
                                    if sender
                                        .send(AiStreamChunk::delta(&request_id, text))
                                        .await
                                        .is_err()
                                    {
                                        return Ok(usage);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(usage)
    }

    async fn stream_agent(
        &self,
        api_key: &str,
        messages: &[AgentMessage],
        tools: &[AgentTool],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AgentTurn, AiError> {
        let model = config.effective_model();
        let max_tokens = config.effective_max_tokens();
        let temperature = config.effective_temperature();
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string());

        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse",
            base_url.trim_end_matches('/'),
            model
        );

        let (system, contents) = wire::gemini_agent_contents(messages);
        let mut body = json!({
            "systemInstruction": { "parts": [{ "text": system }] },
            "contents": contents,
            "generationConfig": {
                "maxOutputTokens": max_tokens,
                "temperature": temperature
            }
        });
        if !tools.is_empty() {
            body["tools"] = wire::gemini_tools(tools);
        }

        debug!(
            "Gemini agent request: model={}, tools={}",
            model,
            tools.len()
        );

        let response = send_with_retry(
            self.client
                .post(&url)
                .header("x-goog-api-key", api_key)
                .header("Content-Type", "application/json")
                .json(&body),
            "Gemini",
        )
        .await?;

        let mut turn = AgentTurn::default();
        let mut stream = response.bytes_stream();
        let mut buffer = StreamLineBuffer::default();

        use futures::StreamExt;
        'outer: while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result.map_err(stream_error)?;
            for line in buffer.push(&bytes) {
                if line.starts_with(':') {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                        merge_gemini_usage(&mut turn.usage, &parsed["usageMetadata"]);
                        if let Some(parts) = parsed["candidates"][0]["content"]["parts"].as_array()
                        {
                            for part in parts {
                                if let Some(text) = part["text"].as_str() {
                                    turn.text.push_str(text);
                                    if sender
                                        .send(AiStreamChunk::delta(&request_id, text))
                                        .await
                                        .is_err()
                                    {
                                        break 'outer;
                                    }
                                }
                                // Gemini sends functionCall parts complete,
                                // never fragmented.
                                if let Some(name) = part["functionCall"]["name"].as_str() {
                                    turn.tool_calls.push(ToolCall {
                                        id: part["functionCall"]["id"]
                                            .as_str()
                                            .map(String::from)
                                            .unwrap_or_else(|| {
                                                format!("call_{}", turn.tool_calls.len())
                                            }),
                                        name: name.to_string(),
                                        input: part["functionCall"]["args"].clone(),
                                        thought_signature: part["thoughtSignature"]
                                            .as_str()
                                            .map(String::from),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(turn)
    }
}

pub struct DeepSeekProvider {
    client: Client,
}

impl DeepSeekProvider {
    pub fn new() -> Self {
        Self {
            client: build_provider_client(),
        }
    }
}

impl Default for DeepSeekProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AIProvider for DeepSeekProvider {
    fn provider_id(&self) -> &'static str {
        "deepseek"
    }

    async fn stream(
        &self,
        api_key: &str,
        messages: &[AiMessage],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AiUsage, AiError> {
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "https://api.deepseek.com".to_string());
        stream_openai_compatible(
            &self.client,
            &format!("{}/chat/completions", base_url.trim_end_matches('/')),
            api_key,
            messages,
            config,
            sender,
            request_id,
            "DeepSeek",
        )
        .await
    }

    async fn stream_agent(
        &self,
        api_key: &str,
        messages: &[AgentMessage],
        tools: &[AgentTool],
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<AgentTurn, AiError> {
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "https://api.deepseek.com".to_string());
        stream_agent_openai_compatible(
            &self.client,
            &format!("{}/chat/completions", base_url.trim_end_matches('/')),
            api_key,
            messages,
            tools,
            config,
            sender,
            request_id,
            "DeepSeek",
            false,
            true,
        )
        .await
    }
}

/// Shared streaming implementation for OpenAI-compatible APIs (Mistral, DeepSeek, etc.)
/// The `usage` object arrives on the final chunk (sent natively by these APIs).
#[allow(clippy::too_many_arguments)]
async fn stream_openai_compatible(
    client: &Client,
    url: &str,
    api_key: &str,
    messages: &[AiMessage],
    config: &AiConfig,
    sender: mpsc::Sender<AiStreamChunk>,
    request_id: String,
    provider_name: &str,
) -> Result<AiUsage, AiError> {
    let model = config.effective_model();
    let max_tokens = config.effective_max_tokens();
    let temperature = config.effective_temperature();

    let body = json!({
        "model": model,
        "messages": openai_style_messages(messages),
        "max_tokens": max_tokens,
        "temperature": temperature,
        "stream": true
    });

    debug!(
        "{} request: model={}, max_tokens={}",
        provider_name, model, max_tokens
    );

    let response = send_with_retry(
        client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body),
        provider_name,
    )
    .await?;

    // Parse SSE stream (OpenAI-compatible format)
    let mut usage = AiUsage::default();
    let mut stream = response.bytes_stream();
    let mut buffer = StreamLineBuffer::default();

    use futures::StreamExt;
    while let Some(chunk_result) = stream.next().await {
        let bytes = chunk_result.map_err(stream_error)?;
        for line in buffer.push(&bytes) {
            if line.starts_with(':') {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if data.trim() == "[DONE]" {
                    return Ok(usage);
                }

                if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                    merge_openai_usage(&mut usage, &parsed);
                    for delta in openai_delta_texts(&parsed["choices"][0]["delta"]) {
                        if sender
                            .send(AiStreamChunk::delta(&request_id, delta))
                            .await
                            .is_err()
                        {
                            return Ok(usage);
                        }
                    }
                }
            }
        }
    }

    Ok(usage)
}

/// Agent-mode counterpart of `stream_openai_compatible`: advertises tools,
/// streams text deltas and reassembles tool_call fragments.
async fn stream_openai_responses_agent(
    client: &Client,
    url: &str,
    api_key: &str,
    messages: &[AgentMessage],
    tools: &[AgentTool],
    config: &AiConfig,
    sender: mpsc::Sender<AiStreamChunk>,
    request_id: String,
) -> Result<AgentTurn, AiError> {
    let model = config.effective_model();
    // Reasoning tokens count against max_output_tokens: with the default
    // 2048 the model could think, then have no room left for the answer.
    let max_tokens = if openai_is_reasoning_model(&model) {
        config.effective_max_tokens().max(4_096)
    } else {
        config.effective_max_tokens()
    };
    let temperature = config.effective_temperature();
    let replaying_tool_turn = messages
        .iter()
        .any(|message| !message.provider_output_items.is_empty());
    let mut body = json!({
        "model": model,
        "instructions": wire::agent_system_instructions(messages),
        "input": wire::openai_responses_input(messages),
        "tools": wire::openai_responses_tools(tools),
        "max_output_tokens": max_tokens,
        "stream": true,
        "store": false,
    });
    if openai_is_reasoning_model(&model) {
        body["reasoning"] = json!({ "effort": "low" });
        body["include"] = json!(["reasoning.encrypted_content"]);
    } else {
        body["temperature"] = json!(temperature);
    }

    debug!(
        "OpenAI Responses agent request: model={}, tools={}, continuation={}",
        model,
        tools.len(),
        replaying_tool_turn
    );

    let response = send_with_retry(
        client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body),
        "OpenAI",
    )
    .await?;

    let mut turn = AgentTurn::default();
    let mut stream = response.bytes_stream();
    let mut buffer = StreamLineBuffer::default();

    use futures::StreamExt;
    'outer: while let Some(chunk_result) = stream.next().await {
        let bytes = chunk_result.map_err(stream_error)?;
        for line in buffer.push(&bytes) {
            if line.starts_with(':') {
                continue;
            }
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data.trim() == "[DONE]" {
                break 'outer;
            }
            let Ok(parsed) = serde_json::from_str::<Value>(data) else {
                continue;
            };

            match parsed["type"].as_str().unwrap_or("") {
                "response.created" => {}
                "response.output_text.delta" => {
                    if let Some(text) = parsed["delta"].as_str() {
                        turn.text.push_str(text);
                        if sender
                            .send(AiStreamChunk::delta(&request_id, text))
                            .await
                            .is_err()
                        {
                            break 'outer;
                        }
                    }
                }
                "response.output_item.done" => {
                    let item = &parsed["item"];
                    if item["type"].as_str() == Some("function_call") {
                        let arguments = item["arguments"]
                            .as_str()
                            .and_then(|raw| serde_json::from_str(raw).ok())
                            .unwrap_or_else(|| json!({}));
                        turn.tool_calls.push(ToolCall {
                            id: item["call_id"]
                                .as_str()
                                .or_else(|| item["id"].as_str())
                                .unwrap_or_default()
                                .to_string(),
                            name: item["name"].as_str().unwrap_or_default().to_string(),
                            input: arguments,
                            thought_signature: None,
                        });
                    }
                }
                "response.completed" => {
                    let completed = &parsed["response"];
                    if let Some(items) = completed["output"].as_array() {
                        turn.provider_output_items = items.clone();
                    }
                    let usage = &completed["usage"];
                    let cached = usage["input_tokens_details"]["cached_tokens"]
                        .as_u64()
                        .unwrap_or(0);
                    if let Some(value) = usage["input_tokens"].as_u64() {
                        let cached = cached.min(value);
                        turn.usage.input_tokens = Some((value - cached) as u32);
                        if cached > 0 {
                            turn.usage.cache_read_tokens = Some(cached as u32);
                        }
                    }
                    if let Some(value) = usage["output_tokens"].as_u64() {
                        turn.usage.output_tokens = Some(value as u32);
                    }
                    break 'outer;
                }
                "response.failed" | "error" => {
                    let message = parsed["response"]["error"]["message"]
                        .as_str()
                        .or_else(|| parsed["error"]["message"].as_str())
                        .or_else(|| parsed["message"].as_str())
                        .unwrap_or("OpenAI Responses stream failed");
                    return Err(AiError::provider(message));
                }
                _ => {}
            }
        }
    }

    Ok(turn)
}

/// Agent-mode counterpart of `stream_openai_compatible`: advertises tools,
/// streams text deltas and reassembles tool_call fragments.
#[allow(clippy::too_many_arguments)]
async fn stream_agent_openai_compatible(
    client: &Client,
    url: &str,
    api_key: &str,
    messages: &[AgentMessage],
    tools: &[AgentTool],
    config: &AiConfig,
    sender: mpsc::Sender<AiStreamChunk>,
    request_id: String,
    provider_name: &str,
    native_openai: bool,
    deepseek_reasoning: bool,
) -> Result<AgentTurn, AiError> {
    let model = config.effective_model();
    let max_tokens = config.effective_max_tokens();
    let temperature = config.effective_temperature();

    let mut body = if native_openai {
        native_openai_chat_body(
            &model,
            wire::openai_agent_messages(messages),
            max_tokens,
            temperature,
        )
    } else {
        json!({
            "model": model,
            "messages": if deepseek_reasoning {
                wire::deepseek_agent_messages(messages)
            } else {
                wire::openai_agent_messages(messages)
            },
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": true
        })
    };
    if !tools.is_empty() {
        body["tools"] = wire::openai_tools(tools);
    }
    if native_openai {
        body["stream_options"] = json!({ "include_usage": true });
    }

    debug!(
        "{} agent request: model={}, tools={}",
        provider_name,
        model,
        tools.len()
    );

    let response = send_with_retry(
        client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body),
        provider_name,
    )
    .await?;

    let mut turn = AgentTurn::default();
    let mut acc = wire::OpenAiToolCallAccumulator::default();
    let mut stream = response.bytes_stream();
    let mut buffer = StreamLineBuffer::default();

    use futures::StreamExt;
    'outer: while let Some(chunk_result) = stream.next().await {
        let bytes = chunk_result.map_err(stream_error)?;
        for line in buffer.push(&bytes) {
            if line.starts_with(':') {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if data.trim() == "[DONE]" {
                    break 'outer;
                }

                if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                    merge_openai_usage(&mut turn.usage, &parsed);
                    let delta = &parsed["choices"][0]["delta"];
                    acc.feed(delta);
                    if deepseek_reasoning {
                        if let Some(reasoning) = delta["reasoning_content"].as_str() {
                            turn.reasoning_content
                                .get_or_insert_with(String::new)
                                .push_str(reasoning);
                        }
                    }
                    for text in openai_delta_texts(delta) {
                        turn.text.push_str(text);
                        if sender
                            .send(AiStreamChunk::delta(&request_id, text))
                            .await
                            .is_err()
                        {
                            break 'outer; // Receiver dropped (cancelled)
                        }
                    }
                }
            }
        }
    }

    turn.tool_calls = acc.finish();
    Ok(turn)
}

/// Extract a user-friendly error message from an API error response body
fn extract_api_error(body: &str) -> Option<String> {
    extract_api_error_details(body).message
}

/// Queries the provider's model inventory, then reduces it to Qore AI's
/// curated agent-ready catalog.
pub async fn fetch_models(
    provider: &AiProvider,
    api_key: &str,
    base_url: Option<String>,
) -> Result<Vec<AiModelInfoOwned>, String> {
    let client = build_provider_client();
    let discovered = match provider {
        AiProvider::QoreLocal => return Ok(provider.available_models_owned()),
        AiProvider::OpenAi => {
            let base = base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            let parsed = get_json(
                client
                    .get(format!("{}/models", base.trim_end_matches('/')))
                    .header("Authorization", format!("Bearer {api_key}")),
            )
            .await?;
            openai_style_models(&parsed, |id| {
                (id.starts_with("gpt-")
                    || id.starts_with("chatgpt")
                    || (id.starts_with('o')
                        && id.chars().nth(1).is_some_and(|c| c.is_ascii_digit())))
                    && !contains_any(id, OPENAI_MODEL_EXCLUSIONS)
            })
        }
        AiProvider::MistralAi => {
            let base = base_url.unwrap_or_else(|| "https://api.mistral.ai/v1".to_string());
            let parsed = get_json(
                client
                    .get(format!("{}/models", base.trim_end_matches('/')))
                    .header("Authorization", format!("Bearer {api_key}")),
            )
            .await?;
            openai_style_models(&parsed, |id| !contains_any(id, MISTRAL_MODEL_EXCLUSIONS))
        }
        AiProvider::DeepSeek => {
            let base = base_url.unwrap_or_else(|| "https://api.deepseek.com".to_string());
            let parsed = get_json(
                client
                    .get(format!("{}/models", base.trim_end_matches('/')))
                    .header("Authorization", format!("Bearer {api_key}")),
            )
            .await?;
            openai_style_models(&parsed, |_| true)
        }
        AiProvider::Anthropic => {
            let base = base_url.unwrap_or_else(|| "https://api.anthropic.com/v1".to_string());
            let parsed = get_json(
                client
                    .get(format!("{}/models?limit=100", base.trim_end_matches('/')))
                    .header("x-api-key", api_key)
                    .header("anthropic-version", "2023-06-01"),
            )
            .await?;
            let mut out = Vec::new();
            if let Some(items) = parsed["data"].as_array() {
                for item in items {
                    if let Some(id) = item["id"].as_str() {
                        out.push(AiModelInfoOwned {
                            id: id.to_string(),
                            label: item["display_name"].as_str().unwrap_or(id).to_string(),
                        });
                    }
                }
            }
            out
        }
        AiProvider::GoogleGemini => {
            let base = base_url
                .unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string());
            let parsed = get_json(
                client
                    .get(format!(
                        "{}/models?pageSize=200",
                        base.trim_end_matches('/')
                    ))
                    .header("x-goog-api-key", api_key),
            )
            .await?;
            let mut out = Vec::new();
            if let Some(items) = parsed["models"].as_array() {
                for item in items {
                    let supports_generate = item["supportedGenerationMethods"]
                        .as_array()
                        .is_some_and(|m| m.iter().any(|v| v.as_str() == Some("generateContent")));
                    if !supports_generate {
                        continue;
                    }
                    let Some(name) = item["name"].as_str() else {
                        continue;
                    };
                    let id = name.strip_prefix("models/").unwrap_or(name);
                    if contains_any(id, GEMINI_MODEL_EXCLUSIONS) {
                        continue;
                    }
                    out.push(AiModelInfoOwned {
                        id: id.to_string(),
                        label: item["displayName"].as_str().unwrap_or(id).to_string(),
                    });
                }
            }
            out
        }
        AiProvider::Ollama => {
            let base = base_url.unwrap_or_else(|| "http://localhost:11434".to_string());
            let parsed = get_json(client.get(format!("{}/api/tags", base))).await?;
            let mut out = Vec::new();
            if let Some(items) = parsed["models"].as_array() {
                for item in items {
                    if let Some(name) = item["name"].as_str() {
                        out.push(AiModelInfoOwned {
                            id: name.to_string(),
                            label: name.to_string(),
                        });
                    }
                }
            }
            out
        }
    };

    Ok(curate_discovered_models(provider, discovered))
}

const MAX_UNRECOGNIZED_MODELS: usize = 6;

/// The provider endpoints are capability inventories, not product pickers:
/// they also contain dated snapshots, legacy families and specialist models.
/// Prefer Qore AI's small role-based catalog when those aliases are available.
/// A bounded live fallback keeps custom/early-access accounts usable without
/// turning the selector back into an unfiltered API dump.
fn curate_discovered_models(
    provider: &AiProvider,
    discovered: Vec<AiModelInfoOwned>,
) -> Vec<AiModelInfoOwned> {
    if matches!(provider, AiProvider::QoreLocal | AiProvider::Ollama) {
        return discovered;
    }

    let recommended = provider
        .available_models()
        .iter()
        .filter(|candidate| discovered.iter().any(|model| model.id == candidate.id))
        .map(|candidate| AiModelInfoOwned {
            id: candidate.id.to_string(),
            label: candidate.label.to_string(),
        })
        .collect::<Vec<_>>();

    if !recommended.is_empty() {
        return recommended;
    }

    discovered
        .into_iter()
        .take(MAX_UNRECOGNIZED_MODELS)
        .collect()
}

const OPENAI_MODEL_EXCLUSIONS: &[&str] = &[
    "embed",
    "whisper",
    "tts",
    "audio",
    "realtime",
    "image",
    "dall-e",
    "moderation",
    "transcribe",
    "search",
    "computer-use",
    "instruct",
];

const MISTRAL_MODEL_EXCLUSIONS: &[&str] = &["embed", "moderation", "ocr", "transcribe", "voxtral"];

const GEMINI_MODEL_EXCLUSIONS: &[&str] = &[
    "embedding",
    "tts",
    "image",
    "audio",
    "live",
    "veo",
    "imagen",
    "aqa",
];

fn contains_any(id: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| id.contains(needle))
}

/// Parses an OpenAI-style `{"data": [{"id": ...}]}` model list, newest ids
/// first (descending lexicographic order works for versioned names).
fn openai_style_models(parsed: &Value, keep: impl Fn(&str) -> bool) -> Vec<AiModelInfoOwned> {
    let mut out: Vec<AiModelInfoOwned> = parsed["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["id"].as_str())
                .filter(|id| keep(id))
                .map(|id| AiModelInfoOwned {
                    id: id.to_string(),
                    label: id.to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.id.cmp(&a.id));
    out.dedup_by(|a, b| a.id == b.id);
    out
}

async fn get_json(builder: reqwest::RequestBuilder) -> Result<Value, String> {
    let response = builder
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(extract_api_error(&body).unwrap_or_else(|| format!("HTTP {status}: {body}")));
    }
    response.json::<Value>().await.map_err(|e| e.to_string())
}

/// Extract a SQL/MQL code block from LLM response text.
///
/// All fenced blocks are collected and scanned from the last one backwards —
/// when a model corrects itself mid-answer, the final block is the one it
/// stands behind. For SQL drivers a block that actually parses (via the same
/// sqlparser chain as `sql_safety`) wins over one that merely looks like a
/// query, so prose wrapped in a fence ("SELECT is a keyword that…") doesn't
/// get promoted. We also sanity-check the first non-empty token: if it
/// doesn't look like a query/statement (SELECT, INSERT, db., {...}, etc.),
/// the candidate is rejected so an LLM that escaped the code-block contract —
/// "Sure! Here is the password: 12345" — doesn't get forwarded verbatim to
/// the user (cf. audit B7-A5).
pub fn extract_query_from_response(response: &str, driver_id: &str) -> Option<String> {
    let blocks = collect_code_blocks(response);

    let is_sql = !matches!(driver_id, "mongodb" | "redis");
    if is_sql {
        if let Some(parsed) = blocks.iter().rev().find(|b| {
            looks_like_query(b) && crate::engine::sql_safety::analyze_sql(driver_id, b).is_ok()
        }) {
            return Some(parsed.clone());
        }
    }

    blocks.iter().rev().find(|b| looks_like_query(b)).cloned()
}

/// Collect the contents of every fenced code block, in document order.
/// A language tag on the opening fence line is dropped.
fn collect_code_blocks(response: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut rest = response;

    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let Some(end) = after.find("```") else { break };
        let raw = &after[..end];

        let content = match raw.find('\n') {
            Some(nl) => {
                let first_line = raw[..nl].trim();
                let is_lang_tag = !first_line.is_empty()
                    && first_line.len() <= 16
                    && first_line.chars().all(|c| c.is_ascii_alphanumeric());
                if is_lang_tag { &raw[nl + 1..] } else { raw }
            }
            None => raw,
        };

        let content = content.trim();
        if !content.is_empty() {
            blocks.push(content.to_string());
        }
        rest = &after[end + 3..];
    }

    blocks
}

/// Heuristic check that the extracted block resembles a SQL / MQL / Redis
/// statement. Intentionally permissive — we don't want to reject a valid
/// query just because it starts with a comment — but explicit enough to
/// catch obvious natural-language leakage.
fn looks_like_query(candidate: &str) -> bool {
    // Strip leading SQL/Mongo line comments + whitespace so `-- header\nSELECT…`
    // still classifies correctly.
    let mut text = candidate.trim_start();
    while text.starts_with("--") {
        match text.find('\n') {
            Some(idx) => text = text[idx + 1..].trim_start(),
            None => return false,
        }
    }
    if text.is_empty() {
        return false;
    }

    // JSON / Mongo-shell payload.
    if text.starts_with('{') || text.starts_with('[') || text.starts_with("db.") {
        return true;
    }

    // SQL / Redis keyword prefix.
    const ALLOWED_PREFIXES: &[&str] = &[
        "SELECT", "WITH", "INSERT", "UPDATE", "DELETE", "MERGE", "CREATE", "DROP", "ALTER",
        "TRUNCATE", "EXPLAIN", "SHOW", "DESCRIBE", "DESC", "VALUES", "CALL", "PRAGMA",
        // Mongo shell verbs that don't start with `db.` (rare but legal).
        "USE", // Redis commands.
        "GET", "SET", "HGET", "HSET", "LPUSH", "RPUSH", "LRANGE", "SADD", "ZADD", "KEYS", "SCAN",
        "DEL", "EXPIRE", "INCR", "DECR", "PING", "INFO",
    ];
    let upper_head: String = text
        .chars()
        .take(16)
        .collect::<String>()
        .to_ascii_uppercase();
    ALLOWED_PREFIXES.iter().any(|p| upper_head.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_buffer_survives_utf8_split_across_chunks() {
        let payload = "data: {\"delta\":\"réponse détaillée\"}\n".as_bytes();
        // Coupe au milieu du "é" multi-octets.
        let split = payload.iter().position(|&b| b == 0xC3).unwrap() + 1;
        let mut buffer = StreamLineBuffer::default();
        assert!(buffer.push(&payload[..split]).is_empty());
        let lines = buffer.push(&payload[split..]);
        assert_eq!(lines, vec!["data: {\"delta\":\"réponse détaillée\"}"]);
    }

    #[test]
    fn line_buffer_drops_blank_lines_and_keeps_partial_tail() {
        let mut buffer = StreamLineBuffer::default();
        let lines = buffer.push(b"a\n\n  \nb\npartial");
        assert_eq!(lines, vec!["a", "b"]);
        assert_eq!(buffer.push(b"-tail\n"), vec!["partial-tail"]);
    }

    #[test]
    fn cost_weighted_discounts_cache_reads() {
        let usage = AiUsage {
            input_tokens: Some(1_000),
            output_tokens: Some(500),
            cache_read_tokens: Some(10_000),
            cache_creation_tokens: Some(2_000),
        };
        assert_eq!(usage.total(), Some(13_500));
        // 1000 + 500 + 10000/10 + 2000*1.25
        assert_eq!(usage.cost_weighted(), Some(5_000));
    }

    #[test]
    fn openai_usage_normalizes_cached_prompt_tokens() {
        let mut usage = AiUsage::default();
        merge_openai_usage(
            &mut usage,
            &json!({
                "usage": {
                    "prompt_tokens": 1_200,
                    "completion_tokens": 80,
                    "prompt_tokens_details": { "cached_tokens": 1_000 }
                }
            }),
        );
        assert_eq!(usage.input_tokens, Some(200));
        assert_eq!(usage.cache_read_tokens, Some(1_000));
        assert_eq!(usage.output_tokens, Some(80));
    }

    #[test]
    fn test_extract_query_sql_block() {
        let response = "Here's your query:\n\n```sql\nSELECT * FROM users WHERE id = 1;\n```\n\nThis selects...";
        assert_eq!(
            extract_query_from_response(response, "postgres"),
            Some("SELECT * FROM users WHERE id = 1;".to_string())
        );
    }

    #[test]
    fn test_extract_query_generic_block() {
        let response = "```\ndb.users.find({age: {$gt: 25}})\n```";
        assert_eq!(
            extract_query_from_response(response, "mongodb"),
            Some("db.users.find({age: {$gt: 25}})".to_string())
        );
    }

    #[test]
    fn test_extract_query_no_block() {
        let response = "Just a plain text response without any code blocks.";
        assert_eq!(extract_query_from_response(response, "postgres"), None);
    }

    #[test]
    fn test_extract_query_prefers_last_valid_block() {
        let response = "First attempt:\n```sql\nSELECT * FROM userz;\n```\nActually, the table is `users`:\n```sql\nSELECT * FROM users;\n```";
        assert_eq!(
            extract_query_from_response(response, "postgres"),
            Some("SELECT * FROM users;".to_string())
        );
    }

    #[test]
    fn test_extract_query_skips_prose_block_when_valid_sql_exists() {
        let response = "```sql\nSELECT id, name FROM users;\n```\nNote:\n```\nSELECT is the keyword that reads rows from a table\n```";
        assert_eq!(
            extract_query_from_response(response, "postgres"),
            Some("SELECT id, name FROM users;".to_string())
        );
    }

    #[test]
    fn test_extract_query_rejects_non_query_block() {
        let response = "```\nSure! Here is the password: 12345\n```";
        assert_eq!(extract_query_from_response(response, "postgres"), None);
    }

    #[test]
    fn test_collect_code_blocks_multiple() {
        let response = "```sql\nSELECT 1;\n```\ntext\n```json\n{\"a\": 1}\n```";
        assert_eq!(
            collect_code_blocks(response),
            vec!["SELECT 1;".to_string(), "{\"a\": 1}".to_string()]
        );
    }

    #[test]
    fn test_extract_api_error() {
        let body = r#"{"error":{"message":"Invalid API key","type":"invalid_request_error"}}"#;
        assert_eq!(extract_api_error(body), Some("Invalid API key".to_string()));
    }

    #[test]
    fn test_looks_context_too_large() {
        assert!(looks_context_too_large(
            "This model's maximum context length is 8192 tokens"
        ));
        assert!(looks_context_too_large(
            "prompt is too long: 250000 tokens > 200000 maximum"
        ));
        assert!(!looks_context_too_large("Invalid request"));
    }

    #[test]
    fn native_openai_reasoning_request_uses_current_chat_parameters() {
        let body = native_openai_chat_body("gpt-5.6-terra", serde_json::json!([]), 2048, 0.3);

        assert_eq!(body["max_completion_tokens"], 2048);
        assert_eq!(body["reasoning_effort"], "none");
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn native_openai_legacy_request_keeps_sampling_parameter() {
        let body = native_openai_chat_body("gpt-4.1-mini", serde_json::json!([]), 1024, 0.2);

        assert_eq!(body["max_completion_tokens"], 1024);
        let temperature = body["temperature"].as_f64().unwrap();
        assert!((temperature - 0.2).abs() < 0.000_001);
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn native_openai_older_reasoning_request_omits_sampling_parameter() {
        let body = native_openai_chat_body("o4-mini", serde_json::json!([]), 1024, 0.2);

        assert_eq!(body["max_completion_tokens"], 1024);
        assert!(body.get("temperature").is_none());
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn openai_compatible_delta_accepts_mistral_content_chunks() {
        let delta = serde_json::json!({
            "content": [
                {"type": "text", "text": "Hello"},
                {"type": "text", "text": " world"}
            ]
        });
        assert_eq!(openai_delta_texts(&delta), vec!["Hello", " world"]);
    }

    #[test]
    fn model_picker_prefers_curated_aliases_in_product_order() {
        let discovered = vec![
            AiModelInfoOwned {
                id: "gpt-5.6-sol".to_string(),
                label: "raw sol".to_string(),
            },
            AiModelInfoOwned {
                id: "gpt-5.6-terra".to_string(),
                label: "raw terra".to_string(),
            },
            AiModelInfoOwned {
                id: "gpt-5.5-pro-2026-04-23".to_string(),
                label: "legacy snapshot".to_string(),
            },
        ];

        let curated = curate_discovered_models(&AiProvider::OpenAi, discovered);
        let ids = curated
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["gpt-5.6-terra", "gpt-5.6-sol"]);
        assert_eq!(curated[0].label, "GPT-5.6 Terra · Balanced");
    }

    #[test]
    fn model_picker_bounds_unknown_cloud_catalogs_but_keeps_ollama() {
        let discovered = (0..9)
            .map(|index| AiModelInfoOwned {
                id: format!("custom-model-{index}"),
                label: format!("Custom model {index}"),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            curate_discovered_models(&AiProvider::OpenAi, discovered.clone()).len(),
            MAX_UNRECOGNIZED_MODELS
        );
        assert_eq!(
            curate_discovered_models(&AiProvider::Ollama, discovered).len(),
            9
        );
    }

    mod http {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        use super::super::*;
        use crate::ai::types::AiProvider;

        fn config(provider: AiProvider, base_url: &str) -> AiConfig {
            AiConfig {
                provider,
                model: Some("test-model".to_string()),
                base_url: Some(base_url.to_string()),
                max_tokens: Some(64),
                temperature: Some(0.0),
                allow_sensitive_data: false,
            }
        }

        #[tokio::test]
        async fn openai_401_maps_to_invalid_key_without_retry() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(
                    ResponseTemplate::new(401)
                        .set_body_string(r#"{"error":{"message":"Invalid API key"}}"#),
                )
                .expect(1)
                .mount(&server)
                .await;

            let provider = OpenAiProvider::new();
            let (tx, _rx) = mpsc::channel(8);
            let err = provider
                .stream(
                    "sk-test",
                    &[AiMessage::user("hi")],
                    &config(AiProvider::OpenAi, &server.uri()),
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap_err();
            assert_eq!(err.kind, AiErrorKind::InvalidKey);
            assert_eq!(err.message, "Invalid API key");
        }

        #[tokio::test]
        async fn openai_permission_401_uses_transient_retries_and_is_not_an_invalid_key() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(
                    ResponseTemplate::new(401)
                        .insert_header("x-request-id", "req_permission_401")
                        .set_body_string(
                            r#"{"error":{"message":"You have insufficient permissions for this operation.","type":"invalid_request_error"}}"#,
                        ),
                )
                .expect(3)
                .mount(&server)
                .await;

            let provider = OpenAiProvider::new();
            let (tx, _rx) = mpsc::channel(8);
            let err = provider
                .stream(
                    "sk-test",
                    &[AiMessage::user("hi")],
                    &config(AiProvider::OpenAi, &server.uri()),
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap_err();

            assert_eq!(err.kind, AiErrorKind::Provider);
            assert_eq!(err.http_status, Some(401));
            assert_eq!(
                err.provider_error_type.as_deref(),
                Some("invalid_request_error")
            );
            assert_eq!(err.request_id.as_deref(), Some("req_permission_401"));
        }

        #[tokio::test]
        async fn openai_403_preserves_provider_diagnostics() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(
                    ResponseTemplate::new(403)
                        .insert_header("x-request-id", "req_permission_test")
                        .set_body_string(
                            r#"{"error":{"message":"You have insufficient permissions for this operation.","type":"insufficient_permissions_error","code":"model_not_allowed"}}"#,
                        ),
                )
                .expect(1)
                .mount(&server)
                .await;

            let provider = OpenAiProvider::new();
            let (tx, _rx) = mpsc::channel(8);
            let err = provider
                .stream(
                    "sk-test",
                    &[AiMessage::user("hi")],
                    &config(AiProvider::OpenAi, &server.uri()),
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap_err();

            assert_eq!(err.kind, AiErrorKind::Provider);
            assert_eq!(err.provider.as_deref(), Some("OpenAI"));
            assert_eq!(err.http_status, Some(403));
            assert_eq!(err.provider_code.as_deref(), Some("model_not_allowed"));
            assert_eq!(
                err.provider_error_type.as_deref(),
                Some("insufficient_permissions_error")
            );
            assert_eq!(err.request_id.as_deref(), Some("req_permission_test"));
        }

        #[tokio::test]
        async fn openai_429_retries_twice_then_reports_rate_limited() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(
                    ResponseTemplate::new(429)
                        .insert_header("retry-after", "7")
                        .set_body_string(r#"{"error":{"message":"Rate limit exceeded"}}"#),
                )
                .expect(3)
                .mount(&server)
                .await;

            let provider = OpenAiProvider::new();
            let (tx, _rx) = mpsc::channel(8);
            let err = provider
                .stream(
                    "sk-test",
                    &[AiMessage::user("hi")],
                    &config(AiProvider::OpenAi, &server.uri()),
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap_err();
            assert_eq!(err.kind, AiErrorKind::RateLimited);
            assert_eq!(err.retry_after_secs, Some(7));
        }

        #[tokio::test]
        async fn openai_500_then_success_streams_and_reports_usage() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
                .up_to_n_times(1)
                .expect(1)
                .mount(&server)
                .await;
            let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n\
                       data: {\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}}\n\n\
                       data: [DONE]\n\n";
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string(sse),
                )
                .expect(1)
                .mount(&server)
                .await;

            let provider = OpenAiProvider::new();
            let (tx, mut rx) = mpsc::channel(8);
            let usage = provider
                .stream(
                    "sk-test",
                    &[AiMessage::user("hi")],
                    &config(AiProvider::OpenAi, &server.uri()),
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap();
            assert_eq!(usage.total(), Some(15));
            let chunk = rx.recv().await.unwrap();
            assert_eq!(chunk.delta, "Hello");
        }

        #[tokio::test]
        async fn openai_400_context_error_maps_to_context_too_large() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(ResponseTemplate::new(400).set_body_string(
                    r#"{"error":{"message":"This model's maximum context length is 8192 tokens"}}"#,
                ))
                .expect(1)
                .mount(&server)
                .await;

            let provider = OpenAiProvider::new();
            let (tx, _rx) = mpsc::channel(8);
            let err = provider
                .stream(
                    "sk-test",
                    &[AiMessage::user("hi")],
                    &config(AiProvider::OpenAi, &server.uri()),
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap_err();
            assert_eq!(err.kind, AiErrorKind::ContextTooLarge);
        }

        #[tokio::test]
        async fn anthropic_stream_reports_usage() {
            let server = MockServer::start().await;
            let sse = "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":12}}}\n\n\
                       data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"Hi\"}}\n\n\
                       data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":4}}\n\n\
                       data: {\"type\":\"message_stop\"}\n\n";
            Mock::given(method("POST"))
                .and(path("/messages"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string(sse),
                )
                .expect(1)
                .mount(&server)
                .await;

            let provider = AnthropicProvider::new();
            let (tx, mut rx) = mpsc::channel(8);
            let usage = provider
                .stream(
                    "sk-ant-test",
                    &[AiMessage::user("hi")],
                    &config(AiProvider::Anthropic, &server.uri()),
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap();
            assert_eq!(usage.input_tokens, Some(12));
            assert_eq!(usage.output_tokens, Some(4));
            assert_eq!(rx.recv().await.unwrap().delta, "Hi");
        }

        #[tokio::test]
        async fn openai_agent_round_trip_reassembles_tool_calls() {
            let server = MockServer::start().await;
            let sse = "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n\
                       data: {\"type\":\"response.output_text.delta\",\"delta\":\"Let me check.\"}\n\n\
                       data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"list_tables\",\"arguments\":\"{\\\"database\\\":\\\"shop\\\"}\"}}\n\n\
                       data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"reasoning\",\"id\":\"rs_1\",\"encrypted_content\":\"encrypted-state\"},{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"list_tables\",\"arguments\":\"{\\\"database\\\":\\\"shop\\\"}\"}],\"usage\":{\"input_tokens\":12,\"output_tokens\":4}}}\n\n\
                       data: [DONE]\n\n";
            Mock::given(method("POST"))
                .and(path("/responses"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string(sse),
                )
                .expect(1)
                .mount(&server)
                .await;

            let provider = OpenAiProvider::new();
            let (tx, mut rx) = mpsc::channel(8);
            let tools = vec![AgentTool {
                name: "list_tables".to_string(),
                description: "List tables".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
            }];
            let messages = vec![AgentMessage {
                role: AiRole::User,
                content: "which tables?".to_string(),
                reasoning_content: None,
                tool_calls: vec![],
                tool_results: vec![],
                provider_output_items: vec![],
            }];
            let mut openai_config = config(AiProvider::OpenAi, &server.uri());
            openai_config.model = Some("gpt-5.6-terra".to_string());
            let turn = provider
                .stream_agent(
                    "sk-test",
                    &messages,
                    &tools,
                    &openai_config,
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap();
            assert_eq!(turn.text, "Let me check.");
            assert_eq!(turn.tool_calls.len(), 1);
            assert_eq!(turn.tool_calls[0].id, "call_1");
            assert_eq!(turn.tool_calls[0].name, "list_tables");
            assert_eq!(turn.provider_output_items.len(), 2);
            assert_eq!(turn.usage.input_tokens, Some(12));
            assert_eq!(turn.usage.output_tokens, Some(4));
            assert_eq!(
                turn.tool_calls[0].input,
                serde_json::json!({"database": "shop"})
            );
            assert_eq!(rx.recv().await.unwrap().delta, "Let me check.");

            // The advertised tools travelled in the request body.
            let requests = server.received_requests().await.unwrap();
            let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
            assert_eq!(body["tools"][0]["name"], "list_tables");
            assert_eq!(body["max_output_tokens"], 64);
            assert_eq!(body["reasoning"]["effort"], "low");
            assert_eq!(body["input"][0]["content"], "which tables?");
            assert_eq!(body["store"], false);
            assert_eq!(body["include"][0], "reasoning.encrypted_content");
            assert!(body.get("messages").is_none());
            assert!(body.get("reasoning_effort").is_none());
            assert!(body.get("max_tokens").is_none());
            assert!(body.get("temperature").is_none());
        }

        #[tokio::test]
        async fn openai_agent_continues_with_function_outputs() {
            let server = MockServer::start().await;
            let sse = "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_2\"}}\n\n\
                       data: {\"type\":\"response.output_text.delta\",\"delta\":\"There are 3 tables.\"}\n\n\
                       data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_2\",\"usage\":{\"input_tokens\":8,\"output_tokens\":5}}}\n\n";
            Mock::given(method("POST"))
                .and(path("/responses"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string(sse),
                )
                .expect(1)
                .mount(&server)
                .await;

            let messages = vec![
                AgentMessage {
                    role: AiRole::System,
                    content: "Answer from database evidence.".to_string(),
                    reasoning_content: None,
                    tool_calls: vec![],
                    tool_results: vec![],
                    provider_output_items: vec![],
                },
                AgentMessage {
                    role: AiRole::User,
                    content: "How many users belong to organization 51?".to_string(),
                    reasoning_content: None,
                    tool_calls: vec![],
                    tool_results: vec![],
                    provider_output_items: vec![],
                },
                AgentMessage {
                    role: AiRole::Assistant,
                    content: String::new(),
                    reasoning_content: None,
                    tool_calls: vec![],
                    tool_results: vec![],
                    provider_output_items: vec![
                        serde_json::json!({
                            "type": "reasoning",
                            "id": "rs_1",
                            "encrypted_content": "encrypted-state"
                        }),
                        serde_json::json!({
                            "type": "function_call",
                            "id": "fc_1",
                            "call_id": "call_1",
                            "name": "list_tables",
                            "arguments": "{}"
                        }),
                    ],
                },
                AgentMessage {
                    role: AiRole::User,
                    content: String::new(),
                    reasoning_content: None,
                    tool_calls: vec![],
                    tool_results: vec![crate::ai::agent::types::ToolResult {
                        id: "call_1".to_string(),
                        content: "[\"users\",\"orders\",\"products\"]".to_string(),
                        is_error: false,
                    }],
                    provider_output_items: vec![],
                },
            ];
            let mut openai_config = config(AiProvider::OpenAi, &server.uri());
            openai_config.model = Some("gpt-5.6-terra".to_string());
            let provider = OpenAiProvider::new();
            let (tx, mut rx) = mpsc::channel(8);
            let turn = provider
                .stream_agent(
                    "sk-test",
                    &messages,
                    &[],
                    &openai_config,
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap();

            assert_eq!(turn.text, "There are 3 tables.");
            assert_eq!(rx.recv().await.unwrap().delta, "There are 3 tables.");
            let requests = server.received_requests().await.unwrap();
            let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
            assert!(body.get("previous_response_id").is_none());
            assert_eq!(body["instructions"], "Answer from database evidence.");
            assert_eq!(
                body["input"][0]["content"],
                "How many users belong to organization 51?"
            );
            assert_eq!(body["input"][1]["type"], "reasoning");
            assert_eq!(body["input"][1]["encrypted_content"], "encrypted-state");
            assert_eq!(body["input"][2]["type"], "function_call");
            assert_eq!(body["input"][3]["type"], "function_call_output");
            assert_eq!(body["input"][3]["call_id"], "call_1");
            assert!(
                body["input"][3]["output"]
                    .as_str()
                    .unwrap()
                    .contains("orders")
            );
        }

        #[tokio::test]
        async fn anthropic_agent_round_trip_reassembles_tool_use() {
            let server = MockServer::start().await;
            let sse = "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":9}}}\n\n\
                       data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\"}}\n\n\
                       data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Checking.\"}}\n\n\
                       data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"describe_table\"}}\n\n\
                       data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"table\\\":\"}}\n\n\
                       data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"users\\\"}\"}}\n\n\
                       data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":3}}\n\n\
                       data: {\"type\":\"message_stop\"}\n\n";
            Mock::given(method("POST"))
                .and(path("/messages"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string(sse),
                )
                .expect(1)
                .mount(&server)
                .await;

            let provider = AnthropicProvider::new();
            let (tx, mut rx) = mpsc::channel(8);
            let tools = vec![AgentTool {
                name: "describe_table".to_string(),
                description: "Describe a table".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
            }];
            let messages = vec![AgentMessage {
                role: AiRole::User,
                content: "describe users".to_string(),
                reasoning_content: None,
                tool_calls: vec![],
                tool_results: vec![],
                provider_output_items: vec![],
            }];
            let turn = provider
                .stream_agent(
                    "sk-ant-test",
                    &messages,
                    &tools,
                    &config(AiProvider::Anthropic, &server.uri()),
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap();
            assert_eq!(turn.text, "Checking.");
            assert_eq!(turn.tool_calls.len(), 1);
            assert_eq!(turn.tool_calls[0].id, "toolu_1");
            assert_eq!(turn.tool_calls[0].name, "describe_table");
            assert_eq!(
                turn.tool_calls[0].input,
                serde_json::json!({"table": "users"})
            );
            assert_eq!(turn.usage.input_tokens, Some(9));
            assert_eq!(turn.usage.output_tokens, Some(3));
            assert_eq!(rx.recv().await.unwrap().delta, "Checking.");
        }

        #[tokio::test]
        async fn ollama_sends_generation_options_and_trims_base_url() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(
                    "{\"message\":{\"content\":\"Hi\"},\"done\":false}\n{\"done\":true,\"prompt_eval_count\":2,\"eval_count\":1}\n",
                ))
                .expect(1)
                .mount(&server)
                .await;

            let provider = OllamaProvider::new();
            let (tx, mut rx) = mpsc::channel(8);
            let mut cfg = config(AiProvider::Ollama, &format!("{}/", server.uri()));
            cfg.max_tokens = Some(321);
            cfg.temperature = Some(0.25);
            let usage = provider
                .stream("", &[AiMessage::user("hi")], &cfg, tx, "r1".to_string())
                .await
                .unwrap();

            assert_eq!(usage.total(), Some(3));
            assert_eq!(rx.recv().await.unwrap().delta, "Hi");
            let requests = server.received_requests().await.unwrap();
            let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
            assert_eq!(body["options"]["num_predict"], 321);
            assert_eq!(body["options"]["temperature"], 0.25);
        }

        #[tokio::test]
        async fn qore_local_uses_openai_compatible_streaming() {
            let server = MockServer::start().await;
            let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Local\"}}]}\n\n\
                       data: {\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2},\"choices\":[]}\n\n\
                       data: [DONE]\n\n";
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string(sse),
                )
                .expect(1)
                .mount(&server)
                .await;

            let temp = tempfile::tempdir().unwrap();
            let provider =
                QoreLocalProvider::new(Arc::new(LocalAiRuntime::new(temp.path().to_path_buf())));
            let (tx, mut rx) = mpsc::channel(8);
            let usage = provider
                .stream(
                    "",
                    &[AiMessage::user("hi")],
                    &config(AiProvider::QoreLocal, &server.uri()),
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap();

            assert_eq!(usage.total(), Some(5));
            assert_eq!(rx.recv().await.unwrap().delta, "Local");
        }

        #[tokio::test]
        async fn qore_local_rejects_remote_endpoint_overrides() {
            let temp = tempfile::tempdir().unwrap();
            let provider =
                QoreLocalProvider::new(Arc::new(LocalAiRuntime::new(temp.path().to_path_buf())));
            let error = provider
                .base_url(&config(AiProvider::QoreLocal, "https://example.com/v1"))
                .await
                .unwrap_err();

            assert!(error.message.contains("loopback"));
        }

        #[tokio::test]
        async fn deepseek_agent_preserves_streamed_reasoning_for_next_step() {
            let server = MockServer::start().await;
            let sse = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"Need schema. \"}}]}\n\n\
                       data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"I'll inspect.\",\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"list_tables\",\"arguments\":\"{}\"}}]}}]}\n\n\
                       data: [DONE]\n\n";
            Mock::given(method("POST"))
                .and(path("/chat/completions"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string(sse),
                )
                .expect(1)
                .mount(&server)
                .await;

            let provider = DeepSeekProvider::new();
            let (tx, _rx) = mpsc::channel(8);
            let messages = vec![AgentMessage {
                role: AiRole::User,
                content: "inspect".to_string(),
                reasoning_content: None,
                tool_calls: vec![],
                tool_results: vec![],
                provider_output_items: vec![],
            }];
            let turn = provider
                .stream_agent(
                    "sk-test",
                    &messages,
                    &[],
                    &config(AiProvider::DeepSeek, &server.uri()),
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap();

            assert_eq!(
                turn.reasoning_content.as_deref(),
                Some("Need schema. I'll inspect.")
            );
            assert_eq!(turn.tool_calls[0].id, "call_1");
        }

        #[tokio::test]
        async fn gemini_agent_preserves_function_id_and_thought_signature() {
            let server = MockServer::start().await;
            let sse = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"id\":\"fc-123\",\"name\":\"list_tables\",\"args\":{}},\"thoughtSignature\":\"sig-abc\"}]}}]}\n\n";
            Mock::given(method("POST"))
                .and(path("/models/test-model:streamGenerateContent"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "text/event-stream")
                        .set_body_string(sse),
                )
                .expect(1)
                .mount(&server)
                .await;

            let provider = GoogleGeminiProvider::new();
            let (tx, _rx) = mpsc::channel(8);
            let messages = vec![AgentMessage {
                role: AiRole::User,
                content: "inspect".to_string(),
                reasoning_content: None,
                tool_calls: vec![],
                tool_results: vec![],
                provider_output_items: vec![],
            }];
            let turn = provider
                .stream_agent(
                    "gemini-key",
                    &messages,
                    &[],
                    &config(AiProvider::GoogleGemini, &server.uri()),
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap();

            assert_eq!(turn.tool_calls[0].id, "fc-123");
            assert_eq!(
                turn.tool_calls[0].thought_signature.as_deref(),
                Some("sig-abc")
            );
        }

        #[tokio::test]
        async fn fetch_models_openai_filters_non_chat_models() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/models"))
                .respond_with(ResponseTemplate::new(200).set_body_string(
                    r#"{"data":[
                        {"id":"gpt-5.6-terra"},
                        {"id":"gpt-5.6-sol"},
                        {"id":"text-embedding-3-large"},
                        {"id":"whisper-1"},
                        {"id":"o4-mini"},
                        {"id":"dall-e-3"}
                    ]}"#,
                ))
                .expect(1)
                .mount(&server)
                .await;

            let models = fetch_models(&AiProvider::OpenAi, "sk-test", Some(server.uri()))
                .await
                .unwrap();
            let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
            assert_eq!(ids, vec!["gpt-5.6-terra", "gpt-5.6-sol"]);
            assert_eq!(models[0].label, "GPT-5.6 Terra · Balanced");
        }

        #[tokio::test]
        async fn fetch_models_gemini_keeps_generate_content_only() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/models"))
                .respond_with(ResponseTemplate::new(200).set_body_string(
                    r#"{"models":[
                        {"name":"models/gemini-3.5-flash","displayName":"Gemini 3.5 Flash","supportedGenerationMethods":["generateContent","countTokens"]},
                        {"name":"models/gemini-embedding-001","displayName":"Embedding","supportedGenerationMethods":["embedContent"]},
                        {"name":"models/gemini-3.1-pro-preview","displayName":"Gemini 3.1 Pro","supportedGenerationMethods":["generateContent"]},
                        {"name":"models/gemini-2.5-flash-preview-tts","displayName":"TTS","supportedGenerationMethods":["generateContent"]}
                    ]}"#,
                ))
                .expect(1)
                .mount(&server)
                .await;

            let models = fetch_models(&AiProvider::GoogleGemini, "key", Some(server.uri()))
                .await
                .unwrap();
            let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
            assert_eq!(ids, vec!["gemini-3.5-flash", "gemini-3.1-pro-preview"]);
            assert_eq!(models[0].label, "Gemini 3.5 Flash");
        }

        #[tokio::test]
        async fn transport_error_maps_to_network() {
            // Port 1 refuses connections instantly; retries then Network.
            let provider = OpenAiProvider::new();
            let (tx, _rx) = mpsc::channel(8);
            let err = provider
                .stream(
                    "sk-test",
                    &[AiMessage::user("hi")],
                    &config(AiProvider::OpenAi, "http://127.0.0.1:1"),
                    tx,
                    "r1".to_string(),
                )
                .await
                .unwrap_err();
            assert_eq!(err.kind, AiErrorKind::Network);
        }
    }
}

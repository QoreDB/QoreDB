// SPDX-License-Identifier: BUSL-1.1

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::debug;

use super::types::{AiConfig, AiStreamChunk};

/// Per-request timeout applied to every LLM HTTP client. Streaming SSE
/// completions can legitimately take ~60 s for long answers, so we pick
/// 120 s as a generous ceiling — beyond that the user has likely lost
/// interest and the request would hold the abort handle / connection
/// indefinitely (cf. audit B7-A1).
const PROVIDER_HTTP_TIMEOUT: Duration = Duration::from_secs(120);
const PROVIDER_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

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

// ─── Trait ───────────────────────────────────────────────────

#[async_trait]
pub trait AIProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;

    /// Streaming completion — sends chunks via channel
    async fn stream(
        &self,
        api_key: &str,
        system_prompt: &str,
        user_prompt: &str,
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<(), String>;
}

// ─── OpenAI ──────────────────────────────────────────────────

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
        system_prompt: &str,
        user_prompt: &str,
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<(), String> {
        let model = config.effective_model();
        let max_tokens = config.effective_max_tokens();
        let temperature = config.effective_temperature();

        let body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": true
        });

        debug!("OpenAI request: model={}, max_tokens={}", model, max_tokens);

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("OpenAI request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let msg =
                extract_api_error(&body).unwrap_or_else(|| format!("HTTP {}: {}", status, body));
            return Err(msg);
        }

        // Parse SSE stream
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        use futures::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            // Process complete SSE lines
            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].trim().to_string();
                buffer = buffer[pos + 1..].to_string();

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if data.trim() == "[DONE]" {
                        return Ok(());
                    }

                    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                        if let Some(delta) = parsed["choices"][0]["delta"]["content"].as_str() {
                            let chunk = AiStreamChunk {
                                request_id: request_id.clone(),
                                delta: delta.to_string(),
                                done: false,
                                error: None,
                                generated_query: None,
                                safety_analysis: None,
                            };
                            if sender.send(chunk).await.is_err() {
                                return Ok(()); // Receiver dropped (cancelled)
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

// ─── Anthropic ───────────────────────────────────────────────

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
        system_prompt: &str,
        user_prompt: &str,
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<(), String> {
        let model = config.effective_model();
        let max_tokens = config.effective_max_tokens();
        let temperature = config.effective_temperature();

        let body = json!({
            "model": model,
            "system": system_prompt,
            "messages": [
                { "role": "user", "content": user_prompt }
            ],
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": true
        });

        debug!(
            "Anthropic request: model={}, max_tokens={}",
            model, max_tokens
        );

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Anthropic request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let msg =
                extract_api_error(&body).unwrap_or_else(|| format!("HTTP {}: {}", status, body));
            return Err(msg);
        }

        // Parse SSE stream (Anthropic format)
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        use futures::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].trim().to_string();
                buffer = buffer[pos + 1..].to_string();

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                        let event_type = parsed["type"].as_str().unwrap_or("");

                        match event_type {
                            "content_block_delta" => {
                                if let Some(text) = parsed["delta"]["text"].as_str() {
                                    let chunk = AiStreamChunk {
                                        request_id: request_id.clone(),
                                        delta: text.to_string(),
                                        done: false,
                                        error: None,
                                        generated_query: None,
                                        safety_analysis: None,
                                    };
                                    if sender.send(chunk).await.is_err() {
                                        return Ok(());
                                    }
                                }
                            }
                            "message_stop" => {
                                return Ok(());
                            }
                            "error" => {
                                let msg = parsed["error"]["message"]
                                    .as_str()
                                    .unwrap_or("Unknown Anthropic error");
                                return Err(msg.to_string());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

// ─── Ollama ──────────────────────────────────────────────────

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
        system_prompt: &str,
        user_prompt: &str,
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<(), String> {
        let model = config.effective_model();
        let base_url = config
            .effective_base_url()
            .unwrap_or_else(|| "http://localhost:11434".to_string());

        let body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "stream": true
        });

        debug!("Ollama request: model={}, base_url={}", model, base_url);

        let response = self
            .client
            .post(format!("{}/api/chat", base_url))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Ollama request failed: {}. Is Ollama running?", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Ollama HTTP {}: {}", status, body));
        }

        // Parse NDJSON stream
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        use futures::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].trim().to_string();
                buffer = buffer[pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Ok(parsed) = serde_json::from_str::<Value>(&line) {
                    let done = parsed["done"].as_bool().unwrap_or(false);

                    if let Some(content) = parsed["message"]["content"].as_str() {
                        if !content.is_empty() {
                            let chunk = AiStreamChunk {
                                request_id: request_id.clone(),
                                delta: content.to_string(),
                                done: false,
                                error: None,
                                generated_query: None,
                                safety_analysis: None,
                            };
                            if sender.send(chunk).await.is_err() {
                                return Ok(());
                            }
                        }
                    }

                    if done {
                        return Ok(());
                    }
                }
            }
        }

        Ok(())
    }
}

// ─── Mistral AI (OpenAI-compatible) ─────────────────────────

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
        system_prompt: &str,
        user_prompt: &str,
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<(), String> {
        stream_openai_compatible(
            &self.client,
            "https://api.mistral.ai/v1/chat/completions",
            api_key,
            system_prompt,
            user_prompt,
            config,
            sender,
            request_id,
            "Mistral",
        )
        .await
    }
}

// ─── Google Gemini ──────────────────────────────────────────

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
        system_prompt: &str,
        user_prompt: &str,
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<(), String> {
        let model = config.effective_model();
        let max_tokens = config.effective_max_tokens();
        let temperature = config.effective_temperature();

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse",
            model
        );

        let body = json!({
            "systemInstruction": {
                "parts": [{ "text": system_prompt }]
            },
            "contents": [{
                "role": "user",
                "parts": [{ "text": user_prompt }]
            }],
            "generationConfig": {
                "maxOutputTokens": max_tokens,
                "temperature": temperature
            }
        });

        debug!("Gemini request: model={}, max_tokens={}", model, max_tokens);

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let msg =
                extract_api_error(&body).unwrap_or_else(|| format!("HTTP {}: {}", status, body));
            return Err(msg);
        }

        // Parse SSE stream (Gemini format)
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        use futures::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].trim().to_string();
                buffer = buffer[pos + 1..].to_string();

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                        if let Some(text) =
                            parsed["candidates"][0]["content"]["parts"][0]["text"].as_str()
                        {
                            let chunk = AiStreamChunk {
                                request_id: request_id.clone(),
                                delta: text.to_string(),
                                done: false,
                                error: None,
                                generated_query: None,
                                safety_analysis: None,
                            };
                            if sender.send(chunk).await.is_err() {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

// ─── DeepSeek (OpenAI-compatible) ───────────────────────────

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
        system_prompt: &str,
        user_prompt: &str,
        config: &AiConfig,
        sender: mpsc::Sender<AiStreamChunk>,
        request_id: String,
    ) -> Result<(), String> {
        stream_openai_compatible(
            &self.client,
            "https://api.deepseek.com/chat/completions",
            api_key,
            system_prompt,
            user_prompt,
            config,
            sender,
            request_id,
            "DeepSeek",
        )
        .await
    }
}

// ─── Helpers ─────────────────────────────────────────────────

/// Shared streaming implementation for OpenAI-compatible APIs (Mistral, DeepSeek, etc.)
async fn stream_openai_compatible(
    client: &Client,
    url: &str,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
    config: &AiConfig,
    sender: mpsc::Sender<AiStreamChunk>,
    request_id: String,
    provider_name: &str,
) -> Result<(), String> {
    let model = config.effective_model();
    let max_tokens = config.effective_max_tokens();
    let temperature = config.effective_temperature();

    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "max_tokens": max_tokens,
        "temperature": temperature,
        "stream": true
    });

    debug!(
        "{} request: model={}, max_tokens={}",
        provider_name, model, max_tokens
    );

    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("{} request failed: {}", provider_name, e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let msg = extract_api_error(&body).unwrap_or_else(|| format!("HTTP {}: {}", status, body));
        return Err(msg);
    }

    // Parse SSE stream (OpenAI-compatible format)
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    use futures::StreamExt;
    while let Some(chunk_result) = stream.next().await {
        let bytes = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if data.trim() == "[DONE]" {
                    return Ok(());
                }

                if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                    if let Some(delta) = parsed["choices"][0]["delta"]["content"].as_str() {
                        let chunk = AiStreamChunk {
                            request_id: request_id.clone(),
                            delta: delta.to_string(),
                            done: false,
                            error: None,
                            generated_query: None,
                            safety_analysis: None,
                        };
                        if sender.send(chunk).await.is_err() {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Extract a user-friendly error message from an API error response body
fn extract_api_error(body: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(body).ok()?;
    // OpenAI format: { "error": { "message": "..." } }
    // Anthropic format: { "error": { "message": "..." } }
    parsed["error"]["message"].as_str().map(|s| s.to_string())
}

/// Extract a SQL/MQL code block from LLM response text.
///
/// We also sanity-check the first non-empty token: if it doesn't look like
/// a query/statement (SELECT, INSERT, db., {...}, etc.), the candidate is
/// rejected so an LLM that escaped the code-block contract — "Sure! Here is
/// the password: 12345" — doesn't get forwarded verbatim to the user
/// (cf. audit B7-A5).
pub fn extract_query_from_response(response: &str) -> Option<String> {
    // Try to find a fenced code block (```sql ... ``` or ```json ... ``` or ``` ... ```)
    let code_block_patterns = [
        "```sql",
        "```mysql",
        "```postgresql",
        "```mongo",
        "```json",
        "```js",
        "```javascript",
        "```redis",
        "```",
    ];

    for pattern in &code_block_patterns {
        if let Some(start_idx) = response.find(pattern) {
            let content_start = start_idx + pattern.len();
            // Skip to the next line after the opening fence
            let content_start = response[content_start..]
                .find('\n')
                .map(|i| content_start + i + 1)
                .unwrap_or(content_start);

            if let Some(end_idx) = response[content_start..].find("```") {
                let query = response[content_start..content_start + end_idx].trim();
                if !query.is_empty() && looks_like_query(query) {
                    return Some(query.to_string());
                }
            }
        }
    }

    None
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
    fn test_extract_query_sql_block() {
        let response = "Here's your query:\n\n```sql\nSELECT * FROM users WHERE id = 1;\n```\n\nThis selects...";
        assert_eq!(
            extract_query_from_response(response),
            Some("SELECT * FROM users WHERE id = 1;".to_string())
        );
    }

    #[test]
    fn test_extract_query_generic_block() {
        let response = "```\ndb.users.find({age: {$gt: 25}})\n```";
        assert_eq!(
            extract_query_from_response(response),
            Some("db.users.find({age: {$gt: 25}})".to_string())
        );
    }

    #[test]
    fn test_extract_query_no_block() {
        let response = "Just a plain text response without any code blocks.";
        assert_eq!(extract_query_from_response(response), None);
    }

    #[test]
    fn test_extract_api_error() {
        let body = r#"{"error":{"message":"Invalid API key","type":"invalid_request_error"}}"#;
        assert_eq!(extract_api_error(body), Some("Invalid API key".to_string()));
    }
}

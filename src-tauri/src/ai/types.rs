// SPDX-License-Identifier: BUSL-1.1

use serde::{Deserialize, Serialize};

use crate::engine::types::Namespace;

/// A model available for a given provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelInfo {
    /// Model identifier sent to the API (e.g. "gpt-4.1")
    pub id: &'static str,
    /// Human-readable label (e.g. "GPT-4.1")
    pub label: &'static str,
}

/// Supported AI providers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    OpenAi,
    Anthropic,
    MistralAi,
    GoogleGemini,
    DeepSeek,
    Ollama,
}

impl AiProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiProvider::OpenAi => "openai",
            AiProvider::Anthropic => "anthropic",
            AiProvider::MistralAi => "mistral_ai",
            AiProvider::GoogleGemini => "google_gemini",
            AiProvider::DeepSeek => "deepseek",
            AiProvider::Ollama => "ollama",
        }
    }

    /// Curated Qore AI catalog, intersected with live provider availability;
    /// also used as the fallback when model discovery is unavailable.
    /// First entry is the default. Verified against provider docs 2026-07-18.
    pub fn available_models(&self) -> &'static [AiModelInfo] {
        match self {
            AiProvider::OpenAi => &[
                AiModelInfo {
                    id: "gpt-5.6-terra",
                    label: "GPT-5.6 Terra · Balanced",
                },
                AiModelInfo {
                    id: "gpt-5.6-sol",
                    label: "GPT-5.6 Sol · Best quality",
                },
                AiModelInfo {
                    id: "gpt-5.6-luna",
                    label: "GPT-5.6 Luna · Fast",
                },
            ],
            AiProvider::Anthropic => &[
                AiModelInfo {
                    id: "claude-sonnet-5",
                    label: "Claude Sonnet 5",
                },
                AiModelInfo {
                    id: "claude-opus-4-8",
                    label: "Claude Opus 4.8",
                },
                AiModelInfo {
                    id: "claude-haiku-4-5",
                    label: "Claude Haiku 4.5",
                },
            ],
            AiProvider::MistralAi => &[
                AiModelInfo {
                    id: "mistral-medium-latest",
                    label: "Mistral Medium",
                },
                AiModelInfo {
                    id: "mistral-large-latest",
                    label: "Mistral Large",
                },
                AiModelInfo {
                    id: "mistral-small-latest",
                    label: "Mistral Small",
                },
            ],
            AiProvider::GoogleGemini => &[
                AiModelInfo {
                    id: "gemini-3.5-flash",
                    label: "Gemini 3.5 Flash",
                },
                AiModelInfo {
                    id: "gemini-3.1-pro-preview",
                    label: "Gemini 3.1 Pro",
                },
                AiModelInfo {
                    id: "gemini-3.1-flash-lite",
                    label: "Gemini 3.1 Flash Lite",
                },
            ],
            AiProvider::DeepSeek => &[
                AiModelInfo {
                    id: "deepseek-v4-flash",
                    label: "DeepSeek V4 Flash",
                },
                AiModelInfo {
                    id: "deepseek-v4-pro",
                    label: "DeepSeek V4 Pro",
                },
            ],
            AiProvider::Ollama => &[
                AiModelInfo {
                    id: "qwen3",
                    label: "Qwen 3",
                },
                AiModelInfo {
                    id: "llama3.3",
                    label: "Llama 3.3",
                },
                AiModelInfo {
                    id: "deepseek-r1",
                    label: "DeepSeek R1",
                },
                AiModelInfo {
                    id: "mistral",
                    label: "Mistral",
                },
            ],
        }
    }

    pub fn available_models_owned(&self) -> Vec<AiModelInfoOwned> {
        self.available_models()
            .iter()
            .map(|m| AiModelInfoOwned {
                id: m.id.to_string(),
                label: m.label.to_string(),
            })
            .collect()
    }

    pub fn default_model(&self) -> &'static str {
        self.available_models()[0].id
    }

    /// Cheapest curated model, for utility calls (conversation titles) where
    /// quality doesn't matter.
    pub fn utility_model(&self) -> &'static str {
        match self {
            AiProvider::OpenAi => "gpt-5.6-luna",
            AiProvider::Anthropic => "claude-haiku-4-5",
            AiProvider::MistralAi => "mistral-small-latest",
            AiProvider::GoogleGemini => "gemini-3.1-flash-lite",
            AiProvider::DeepSeek => "deepseek-v4-flash",
            AiProvider::Ollama => self.default_model(),
        }
    }

    pub fn default_base_url(&self) -> Option<&'static str> {
        match self {
            AiProvider::Ollama => Some("http://localhost:11434"),
            _ => None,
        }
    }

    pub fn requires_api_key(&self) -> bool {
        !matches!(self, AiProvider::Ollama)
    }
}

/// Configuration for an AI request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub provider: AiProvider,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

impl AiConfig {
    pub fn effective_model(&self) -> String {
        self.model
            .clone()
            .unwrap_or_else(|| self.provider.default_model().to_string())
    }

    pub fn effective_base_url(&self) -> Option<String> {
        self.base_url
            .clone()
            .or_else(|| self.provider.default_base_url().map(String::from))
    }

    /// Upper bound applied to `max_tokens` regardless of what the user
    /// configured. Picked at 8000 — large enough for a multi-table EXPLAIN
    /// answer, small enough that a single mistuned request can't burn $30
    /// of Claude Opus by accident (cf. audit B7-A2).
    pub const MAX_TOKENS_CEILING: u32 = 8_000;

    pub fn effective_max_tokens(&self) -> u32 {
        self.max_tokens
            .unwrap_or(2048)
            .min(Self::MAX_TOKENS_CEILING)
    }

    pub fn effective_temperature(&self) -> f32 {
        self.temperature.unwrap_or(0.3)
    }
}

/// Type of AI action to perform
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiAction {
    GenerateQuery,
    ExplainResult,
    SummarizeSchema,
    FixError,
}

/// Role of a chat message
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiRole {
    System,
    User,
    Assistant,
}

/// A single chat message exchanged with the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: AiRole,
    pub content: String,
}

impl AiMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: AiRole::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: AiRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: AiRole::Assistant,
            content: content.into(),
        }
    }
}

/// Editor state sent alongside a request so the assistant sees what the user sees
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EditorContext {
    pub current_query: Option<String>,
    pub active_table: Option<String>,
    pub last_error: Option<String>,
    pub result_shape: Option<String>,
}

/// Request sent from frontend to backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    pub request_id: String,
    pub action: AiAction,
    pub prompt: String,
    pub session_id: String,
    pub namespace: Option<Namespace>,
    pub connection_id: Option<String>,
    pub config: AiConfig,
    /// Prior conversation turns (user/assistant), oldest first
    #[serde(default)]
    pub history: Vec<AiMessage>,
    /// What the user currently sees in the query tab
    #[serde(default)]
    pub editor_context: Option<EditorContext>,
    /// Opt-in: include redacted sample rows in the schema context
    #[serde(default)]
    pub include_sample_rows: bool,
    /// For FixError: the original query that failed
    pub original_query: Option<String>,
    /// For FixError: the error message
    pub error_context: Option<String>,
    /// For ExplainResult: serialized result summary
    pub result_context: Option<String>,
}

/// Safety information about a generated query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyInfo {
    pub is_mutation: bool,
    pub is_dangerous: bool,
    pub warnings: Vec<String>,
}

/// Stable error category, sent to the frontend so messages can be localized
/// and retried appropriately.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiErrorKind {
    InvalidKey,
    RateLimited,
    ContextTooLarge,
    Network,
    Provider,
}

/// Typed provider error. `message` keeps the raw provider detail as fallback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiError {
    pub kind: AiErrorKind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_error_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl AiError {
    fn new(kind: AiErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retry_after_secs: None,
            provider: None,
            http_status: None,
            provider_code: None,
            provider_error_type: None,
            request_id: None,
        }
    }

    pub fn invalid_key(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::InvalidKey, message)
    }

    pub fn rate_limited(message: impl Into<String>, retry_after_secs: Option<u64>) -> Self {
        Self {
            retry_after_secs,
            ..Self::new(AiErrorKind::RateLimited, message)
        }
    }

    pub fn context_too_large(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::ContextTooLarge, message)
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::Network, message)
    }

    pub fn provider(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::Provider, message)
    }

    pub fn with_provider_details(
        mut self,
        provider: &str,
        http_status: u16,
        provider_code: Option<String>,
        provider_error_type: Option<String>,
        request_id: Option<String>,
    ) -> Self {
        self.provider = Some(provider.to_string());
        self.http_status = Some(http_status);
        self.provider_code = provider_code;
        self.provider_error_type = provider_error_type;
        self.request_id = request_id;
        self
    }

    /// Only transient failures (rate limit, 5xx, transport) deserve a retry.
    pub fn is_retryable(&self) -> bool {
        matches!(self.kind, AiErrorKind::RateLimited | AiErrorKind::Network)
    }
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<AiError> for String {
    fn from(e: AiError) -> Self {
        e.message
    }
}

/// Token counts reported by the provider at the end of a stream.
/// `input_tokens` is normalized to the *uncached* input across providers
/// (some report cached tokens inside their prompt count, some outside).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct AiUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cache_read_tokens: Option<u32>,
    pub cache_creation_tokens: Option<u32>,
}

impl AiUsage {
    pub fn total(&self) -> Option<u32> {
        if self.input_tokens.is_none()
            && self.output_tokens.is_none()
            && self.cache_read_tokens.is_none()
            && self.cache_creation_tokens.is_none()
        {
            return None;
        }
        Some(
            self.input_tokens.unwrap_or(0)
                + self.output_tokens.unwrap_or(0)
                + self.cache_read_tokens.unwrap_or(0)
                + self.cache_creation_tokens.unwrap_or(0),
        )
    }

    /// Billing-representative weight: cache reads cost ~10% of fresh input
    /// and cache writes ~125% (Anthropic pricing, close enough elsewhere).
    /// Used for the per-run budget so cached agent iterations don't burn it
    /// quadratically.
    pub fn cost_weighted(&self) -> Option<u32> {
        self.total()?;
        Some(
            self.input_tokens.unwrap_or(0)
                + self.output_tokens.unwrap_or(0)
                + self.cache_read_tokens.unwrap_or(0).div_ceil(10)
                + {
                    let creation = self.cache_creation_tokens.unwrap_or(0);
                    creation + creation / 4
                },
        )
    }
}

/// A streaming chunk emitted via window.emit()
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiStreamChunk {
    pub request_id: String,
    /// Incremental text delta
    pub delta: String,
    /// True when streaming is complete
    pub done: bool,
    /// Typed error if the request failed
    pub error: Option<AiError>,
    /// The extracted SQL/MQL query (populated only when done=true)
    pub generated_query: Option<String>,
    /// Safety analysis of the generated query (populated only when done=true)
    pub safety_analysis: Option<SafetyInfo>,
    /// Total tokens consumed (populated only when done=true, if reported)
    pub tokens_used: Option<u32>,
}

impl AiStreamChunk {
    pub fn delta(request_id: &str, delta: impl Into<String>) -> Self {
        Self {
            request_id: request_id.to_string(),
            delta: delta.into(),
            done: false,
            error: None,
            generated_query: None,
            safety_analysis: None,
            tokens_used: None,
        }
    }
}

/// Non-streaming response for sync commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    pub request_id: String,
    pub content: String,
    pub generated_query: Option<String>,
    pub safety_analysis: Option<SafetyInfo>,
    pub provider_used: AiProvider,
    pub tokens_used: Option<u32>,
}

/// Status of a configured provider (returned by ai_get_provider_status)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderStatus {
    pub provider: AiProvider,
    pub has_key: bool,
    pub default_model: String,
    pub models: Vec<AiModelInfoOwned>,
    pub base_url: Option<String>,
}

/// Owned variant of AiModelInfo for serialization in status responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelInfoOwned {
    pub id: String,
    pub label: String,
}

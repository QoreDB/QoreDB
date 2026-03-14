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

    /// Curated list of models for this provider. First entry is the default.
    pub fn available_models(&self) -> &'static [AiModelInfo] {
        match self {
            AiProvider::OpenAi => &[
                AiModelInfo { id: "gpt-4.1", label: "GPT-4.1" },
                AiModelInfo { id: "gpt-4.1-mini", label: "GPT-4.1 Mini" },
                AiModelInfo { id: "gpt-4.1-nano", label: "GPT-4.1 Nano" },
                AiModelInfo { id: "o4-mini", label: "o4-mini" },
                AiModelInfo { id: "o3-mini", label: "o3-mini" },
            ],
            AiProvider::Anthropic => &[
                AiModelInfo { id: "claude-sonnet-4-6", label: "Claude Sonnet 4.6" },
                AiModelInfo { id: "claude-sonnet-4-20250514", label: "Claude Sonnet 4" },
                AiModelInfo { id: "claude-haiku-4-5-20251001", label: "Claude Haiku 4.5" },
            ],
            AiProvider::MistralAi => &[
                AiModelInfo { id: "mistral-large-latest", label: "Mistral Large" },
                AiModelInfo { id: "mistral-medium-latest", label: "Mistral Medium" },
                AiModelInfo { id: "mistral-small-latest", label: "Mistral Small" },
                AiModelInfo { id: "codestral-latest", label: "Codestral" },
                AiModelInfo { id: "pixtral-large-latest", label: "Pixtral Large" },
            ],
            AiProvider::GoogleGemini => &[
                AiModelInfo { id: "gemini-2.5-pro-preview-05-06", label: "Gemini 2.5 Pro" },
                AiModelInfo { id: "gemini-2.5-flash-preview-05-20", label: "Gemini 2.5 Flash" },
                AiModelInfo { id: "gemini-2.0-flash", label: "Gemini 2.0 Flash" },
            ],
            AiProvider::DeepSeek => &[
                AiModelInfo { id: "deepseek-chat", label: "DeepSeek V3" },
                AiModelInfo { id: "deepseek-reasoner", label: "DeepSeek R1" },
            ],
            AiProvider::Ollama => &[
                AiModelInfo { id: "llama3.3", label: "Llama 3.3" },
                AiModelInfo { id: "llama3.1", label: "Llama 3.1" },
                AiModelInfo { id: "qwen2.5-coder", label: "Qwen 2.5 Coder" },
                AiModelInfo { id: "deepseek-r1", label: "DeepSeek R1" },
                AiModelInfo { id: "codellama", label: "Code Llama" },
                AiModelInfo { id: "mistral", label: "Mistral" },
            ],
        }
    }

    pub fn default_model(&self) -> &'static str {
        self.available_models()[0].id
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

    pub fn effective_max_tokens(&self) -> u32 {
        self.max_tokens.unwrap_or(2048)
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

/// A streaming chunk emitted via window.emit()
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiStreamChunk {
    pub request_id: String,
    /// Incremental text delta
    pub delta: String,
    /// True when streaming is complete
    pub done: bool,
    /// Error message if the request failed
    pub error: Option<String>,
    /// The extracted SQL/MQL query (populated only when done=true)
    pub generated_query: Option<String>,
    /// Safety analysis of the generated query (populated only when done=true)
    pub safety_analysis: Option<SafetyInfo>,
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

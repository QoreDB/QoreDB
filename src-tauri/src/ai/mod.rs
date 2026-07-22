// SPDX-License-Identifier: BUSL-1.1

//! AI BYOK (Bring Your Own Key) module — Pro feature
//!
//! Provides LLM-powered query generation, explanation, and schema summarization
//! using the user's own API keys (OpenAI, Anthropic, Mistral, Gemini, DeepSeek, Ollama).

pub mod agent;
pub mod context;
pub mod local_installer;
pub mod local_manifest;
pub mod local_runtime;
pub mod manager;
pub mod provider;
pub mod safety;
pub mod types;

// SPDX-License-Identifier: BUSL-1.1

//! AI BYOK (Bring Your Own Key) module — Pro feature
//!
//! Provides LLM-powered query generation, explanation, and schema summarization
//! using the user's own API keys (OpenAI, Anthropic, Ollama).

pub mod context;
pub mod manager;
pub mod provider;
pub mod safety;
pub mod types;

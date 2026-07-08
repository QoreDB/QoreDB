// SPDX-License-Identifier: Apache-2.0

//! Ollama embedding client. The chat-oriented `OllamaProvider` only speaks
//! `/api/chat`; this client targets `/api/embed` and `/api/tags`.

use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::SemanticConfig;

const EMBED_BATCH_SIZE: usize = 32;
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize)]
pub struct OllamaStatus {
    pub running: bool,
    pub model_available: bool,
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
}

pub struct OllamaEmbedder {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaEmbedder {
    pub fn new(config: &SemanticConfig) -> Self {
        let client = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(HTTP_TIMEOUT)
            .build()
            .unwrap_or_default();
        Self {
            client,
            base_url: config.effective_base_url().trim_end_matches('/').to_string(),
            model: config.model.clone(),
        }
    }

    pub async fn detect(&self) -> OllamaStatus {
        let url = format!("{}/api/tags", self.base_url);
        let response = match self.client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => {
                return OllamaStatus {
                    running: false,
                    model_available: false,
                }
            }
        };
        let tags: TagsResponse = match response.json().await {
            Ok(t) => t,
            Err(_) => {
                return OllamaStatus {
                    running: true,
                    model_available: false,
                }
            }
        };
        let model_available = tags
            .models
            .iter()
            .any(|m| m.name == self.model || m.name.starts_with(&format!("{}:", self.model)));
        OllamaStatus {
            running: true,
            model_available,
        }
    }

    pub async fn embed_documents(&self, documents: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let mut all = Vec::with_capacity(documents.len());
        for chunk in documents.chunks(EMBED_BATCH_SIZE) {
            let input = chunk
                .iter()
                .map(|d| format!("search_document: {d}"))
                .collect();
            let mut embeddings = self.embed(input).await?;
            if embeddings.len() != chunk.len() {
                return Err(format!(
                    "Ollama returned {} embeddings for {} inputs",
                    embeddings.len(),
                    chunk.len()
                ));
            }
            all.append(&mut embeddings);
        }
        Ok(all)
    }

    pub async fn embed_query(&self, query: &str) -> Result<Vec<f32>, String> {
        let mut embeddings = self.embed(vec![format!("search_query: {query}")]).await?;
        embeddings
            .pop()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| "Ollama returned no embedding for the query".to_string())
    }

    async fn embed(&self, input: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        let url = format!("{}/api/embed", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&EmbedRequest {
                model: &self.model,
                input,
            })
            .send()
            .await
            .map_err(|e| format!("Ollama request failed: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let detail = body.chars().take(200).collect::<String>();
            return Err(format!("Ollama embed failed (HTTP {status}): {detail}"));
        }

        let parsed: EmbedResponse = response
            .json()
            .await
            .map_err(|e| format!("Invalid Ollama embed response: {e}"))?;
        Ok(parsed.embeddings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_embed_response() {
        let json = r#"{"model":"nomic-embed-text","embeddings":[[0.1,-0.2,0.3],[0.4,0.5,0.6]],"total_duration":1}"#;
        let parsed: EmbedResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.embeddings.len(), 2);
        assert_eq!(parsed.embeddings[0].len(), 3);
    }

    #[test]
    fn parses_tags_response_and_matches_model_variants() {
        let json = r#"{"models":[{"name":"nomic-embed-text:latest","size":1},{"name":"qwen2.5-coder:7b"}]}"#;
        let parsed: TagsResponse = serde_json::from_str(json).unwrap();
        let names: Vec<&str> = parsed.models.iter().map(|m| m.name.as_str()).collect();
        let model = "nomic-embed-text";
        assert!(names
            .iter()
            .any(|n| *n == model || n.starts_with(&format!("{model}:"))));
    }

    #[tokio::test]
    #[ignore = "requires a local Ollama with nomic-embed-text pulled"]
    async fn embeds_against_local_ollama() {
        let embedder = OllamaEmbedder::new(&SemanticConfig::default());
        let status = embedder.detect().await;
        assert!(status.running && status.model_available);
        let vec = embedder.embed_query("where is the customer email stored?").await.unwrap();
        assert_eq!(vec.len(), 768);
        let docs = embedder
            .embed_documents(&["table customers: columns id, email".to_string()])
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].len(), 768);
    }
}

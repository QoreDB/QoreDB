// SPDX-License-Identifier: BUSL-1.1

//! AI Manager: orchestrates providers and stores API keys in the OS keyring.

use std::collections::HashMap;
use std::sync::Arc;

use super::provider::{AIProvider, AnthropicProvider, OllamaProvider, OpenAiProvider};
use super::types::{AiProvider, AiProviderStatus};
use crate::vault::backend::CredentialProvider;

const KEYRING_SERVICE: &str = "qoredb_ai";

pub struct AiManager {
    credential_provider: Box<dyn CredentialProvider>,
    providers: HashMap<String, Arc<dyn AIProvider>>,
}

impl AiManager {
    pub fn new(credential_provider: Box<dyn CredentialProvider>) -> Self {
        let mut providers: HashMap<String, Arc<dyn AIProvider>> = HashMap::new();
        providers.insert("openai".to_string(), Arc::new(OpenAiProvider::new()));
        providers.insert("anthropic".to_string(), Arc::new(AnthropicProvider::new()));
        providers.insert("ollama".to_string(), Arc::new(OllamaProvider::new()));

        Self {
            credential_provider,
            providers,
        }
    }

    /// Store an API key for a provider in the OS keyring
    pub fn save_api_key(&self, provider: &AiProvider, key: &str) -> Result<(), String> {
        self.credential_provider
            .set_password(KEYRING_SERVICE, provider.as_str(), key)
            .map_err(|e| format!("Failed to save API key: {}", e))
    }

    /// Retrieve an API key for a provider from the OS keyring
    pub fn get_api_key(&self, provider: &AiProvider) -> Result<String, String> {
        self.credential_provider
            .get_password(KEYRING_SERVICE, provider.as_str())
            .map_err(|e| format!("No API key found for {}: {}", provider.as_str(), e))
    }

    /// Delete an API key for a provider
    pub fn delete_api_key(&self, provider: &AiProvider) -> Result<(), String> {
        self.credential_provider
            .delete_password(KEYRING_SERVICE, provider.as_str())
            .map_err(|e| format!("Failed to delete API key: {}", e))
    }

    /// Check whether an API key is stored for a provider
    pub fn has_api_key(&self, provider: &AiProvider) -> bool {
        // Ollama doesn't require an API key
        if !provider.requires_api_key() {
            return true;
        }
        self.credential_provider
            .get_password(KEYRING_SERVICE, provider.as_str())
            .is_ok()
    }

    /// Get a provider implementation by enum variant (returns Arc for 'static lifetime)
    pub fn get_provider(&self, provider: &AiProvider) -> Option<Arc<dyn AIProvider>> {
        self.providers.get(provider.as_str()).cloned()
    }

    /// List all providers with their configuration status
    pub fn list_configured_providers(&self) -> Vec<AiProviderStatus> {
        vec![
            AiProviderStatus {
                provider: AiProvider::OpenAi,
                has_key: self.has_api_key(&AiProvider::OpenAi),
                model: Some(AiProvider::OpenAi.default_model().to_string()),
                base_url: None,
            },
            AiProviderStatus {
                provider: AiProvider::Anthropic,
                has_key: self.has_api_key(&AiProvider::Anthropic),
                model: Some(AiProvider::Anthropic.default_model().to_string()),
                base_url: None,
            },
            AiProviderStatus {
                provider: AiProvider::Ollama,
                has_key: true, // Ollama never needs a key
                model: Some(AiProvider::Ollama.default_model().to_string()),
                base_url: AiProvider::Ollama.default_base_url().map(String::from),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::backend::MockProvider;

    #[test]
    fn test_save_and_retrieve_api_key() {
        let manager = AiManager::new(Box::new(MockProvider::new()));

        manager
            .save_api_key(&AiProvider::OpenAi, "sk-test-key-123")
            .unwrap();
        let key = manager.get_api_key(&AiProvider::OpenAi).unwrap();
        assert_eq!(key, "sk-test-key-123");
    }

    #[test]
    fn test_has_api_key() {
        let manager = AiManager::new(Box::new(MockProvider::new()));

        assert!(!manager.has_api_key(&AiProvider::OpenAi));
        assert!(manager.has_api_key(&AiProvider::Ollama)); // Ollama never needs key

        manager
            .save_api_key(&AiProvider::OpenAi, "sk-test")
            .unwrap();
        assert!(manager.has_api_key(&AiProvider::OpenAi));
    }

    #[test]
    fn test_delete_api_key() {
        let manager = AiManager::new(Box::new(MockProvider::new()));

        manager.save_api_key(&AiProvider::Anthropic, "key").unwrap();
        assert!(manager.has_api_key(&AiProvider::Anthropic));

        manager.delete_api_key(&AiProvider::Anthropic).unwrap();
        assert!(!manager.has_api_key(&AiProvider::Anthropic));
    }

    #[test]
    fn test_list_configured_providers() {
        let manager = AiManager::new(Box::new(MockProvider::new()));
        let list = manager.list_configured_providers();

        assert_eq!(list.len(), 3);
        assert!(!list[0].has_key); // OpenAI — no key set
        assert!(!list[1].has_key); // Anthropic — no key set
        assert!(list[2].has_key); // Ollama — always true
    }

    #[test]
    fn test_get_provider() {
        let manager = AiManager::new(Box::new(MockProvider::new()));

        assert!(manager.get_provider(&AiProvider::OpenAi).is_some());
        assert!(manager.get_provider(&AiProvider::Anthropic).is_some());
        assert!(manager.get_provider(&AiProvider::Ollama).is_some());
    }
}

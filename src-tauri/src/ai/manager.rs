// SPDX-License-Identifier: BUSL-1.1

//! AI Manager: orchestrates providers and stores API keys in the OS keyring.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

use super::local_runtime::LocalAiRuntime;
use super::provider::{
    AIProvider, AnthropicProvider, DeepSeekProvider, GoogleGeminiProvider, MistralAiProvider,
    OllamaProvider, OpenAiProvider, QoreLocalProvider,
};
use super::types::{AiModelInfoOwned, AiProvider, AiProviderStatus};
use crate::vault::backend::CredentialProvider;

const KEYRING_SERVICE: &str = "qoredb_ai";

/// Non-sensitive index used to render provider status without opening every
/// Keychain item. It stores only provider ids, never API keys.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct ProviderKeyIndex {
    configured: HashSet<AiProvider>,
    checked: HashSet<AiProvider>,
}

pub struct AiManager {
    credential_provider: Box<dyn CredentialProvider>,
    providers: HashMap<String, Arc<dyn AIProvider>>,
    /// Keys already authorized during this process. Besides saving work, this
    /// prevents macOS from displaying another prompt for every model-list or
    /// completion request made with the same provider.
    api_key_cache: Mutex<HashMap<AiProvider, String>>,
    key_index: Mutex<ProviderKeyIndex>,
    key_index_path: Option<PathBuf>,
    /// Per-provider model lists fetched from the provider API (session TTL).
    models_cache: Mutex<HashMap<(AiProvider, String), Vec<AiModelInfoOwned>>>,
    local_runtime: Arc<LocalAiRuntime>,
}

impl AiManager {
    pub fn new(credential_provider: Box<dyn CredentialProvider>) -> Self {
        Self::with_key_index(
            credential_provider,
            None,
            crate::paths::app_data_dir().join("ai-local"),
        )
    }

    pub fn new_persistent(
        credential_provider: Box<dyn CredentialProvider>,
        key_index_path: PathBuf,
    ) -> Self {
        let local_root = key_index_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("ai-local");
        Self::with_key_index(credential_provider, Some(key_index_path), local_root)
    }

    fn with_key_index(
        credential_provider: Box<dyn CredentialProvider>,
        key_index_path: Option<PathBuf>,
        local_root: PathBuf,
    ) -> Self {
        let local_runtime = Arc::new(LocalAiRuntime::new(local_root));
        let mut providers: HashMap<String, Arc<dyn AIProvider>> = HashMap::new();
        providers.insert(
            "qore_local".to_string(),
            Arc::new(QoreLocalProvider::new(Arc::clone(&local_runtime))),
        );
        providers.insert("openai".to_string(), Arc::new(OpenAiProvider::new()));
        providers.insert("anthropic".to_string(), Arc::new(AnthropicProvider::new()));
        providers.insert("mistral_ai".to_string(), Arc::new(MistralAiProvider::new()));
        providers.insert(
            "google_gemini".to_string(),
            Arc::new(GoogleGeminiProvider::new()),
        );
        providers.insert("deepseek".to_string(), Arc::new(DeepSeekProvider::new()));
        providers.insert("ollama".to_string(), Arc::new(OllamaProvider::new()));

        let key_index = key_index_path
            .as_deref()
            .map(Self::load_key_index)
            .unwrap_or_default();

        Self {
            credential_provider,
            providers,
            api_key_cache: Mutex::new(HashMap::new()),
            key_index: Mutex::new(key_index),
            key_index_path,
            models_cache: Mutex::new(HashMap::new()),
            local_runtime,
        }
    }

    fn load_key_index(path: &Path) -> ProviderKeyIndex {
        let Ok(content) = std::fs::read_to_string(path) else {
            return ProviderKeyIndex::default();
        };
        serde_json::from_str(&content).unwrap_or_else(|error| {
            tracing::warn!(?error, path = %path.display(), "Ignoring invalid AI key index");
            ProviderKeyIndex::default()
        })
    }

    fn persist_key_index(&self, index: &ProviderKeyIndex) {
        let Some(path) = &self.key_index_path else {
            return;
        };
        let result = serde_json::to_vec(index)
            .map_err(std::io::Error::other)
            .and_then(|bytes| crate::atomic_write::write_atomic(path, &bytes));
        if let Err(error) = result {
            tracing::warn!(?error, path = %path.display(), "Could not persist AI key index");
        }
    }

    fn record_key_status(&self, provider: &AiProvider, configured: bool) {
        let mut index = self.key_index.lock();
        index.checked.insert(provider.clone());
        if configured {
            index.configured.insert(provider.clone());
        } else {
            index.configured.remove(provider);
        }
        self.persist_key_index(&index);
    }

    pub fn cached_models(
        &self,
        provider: &AiProvider,
        base_url: Option<&str>,
    ) -> Option<Vec<AiModelInfoOwned>> {
        self.models_cache
            .lock()
            .get(&model_cache_key(provider, base_url))
            .cloned()
    }

    pub fn local_runtime(&self) -> Arc<LocalAiRuntime> {
        Arc::clone(&self.local_runtime)
    }

    pub fn cache_models(
        &self,
        provider: AiProvider,
        base_url: Option<&str>,
        models: Vec<AiModelInfoOwned>,
    ) {
        self.models_cache
            .lock()
            .insert(model_cache_key(&provider, base_url), models);
    }

    /// Store an API key for a provider in the OS keyring
    pub fn save_api_key(&self, provider: &AiProvider, key: &str) -> Result<(), String> {
        self.credential_provider
            .set_password(KEYRING_SERVICE, provider.as_str(), key)
            .map_err(|e| format!("Failed to save API key: {}", e))?;
        self.api_key_cache
            .lock()
            .insert(provider.clone(), key.to_string());
        self.record_key_status(provider, true);
        Ok(())
    }

    /// Retrieve an API key for a provider from the OS keyring
    pub fn get_api_key(&self, provider: &AiProvider) -> Result<String, String> {
        if let Some(key) = self.api_key_cache.lock().get(provider).cloned() {
            return Ok(key);
        }
        let key = self
            .credential_provider
            .get_password(KEYRING_SERVICE, provider.as_str())
            .map_err(|e| format!("No API key found for {}: {}", provider.as_str(), e))?;
        self.api_key_cache
            .lock()
            .insert(provider.clone(), key.clone());
        self.record_key_status(provider, true);
        Ok(key)
    }

    /// Delete an API key for a provider
    pub fn delete_api_key(&self, provider: &AiProvider) -> Result<(), String> {
        self.credential_provider
            .delete_password(KEYRING_SERVICE, provider.as_str())
            .map_err(|e| format!("Failed to delete API key: {}", e))?;
        self.api_key_cache.lock().remove(provider);
        self.record_key_status(provider, false);
        Ok(())
    }

    /// Check whether an API key is stored for a provider
    pub fn has_api_key(&self, provider: &AiProvider) -> bool {
        // Ollama doesn't require an API key
        if !provider.requires_api_key() {
            return true;
        }
        self.key_index.lock().configured.contains(provider)
    }

    /// One-provider legacy migration. Older releases had no non-sensitive
    /// status index, so inspect only the provider the user selected—not every
    /// Keychain entry at application startup.
    pub fn probe_api_key_once(&self, provider: &AiProvider) {
        if !provider.requires_api_key() || self.key_index.lock().checked.contains(provider) {
            return;
        }
        match self
            .credential_provider
            .get_password(KEYRING_SERVICE, provider.as_str())
        {
            Ok(key) => {
                self.api_key_cache.lock().insert(provider.clone(), key);
                self.record_key_status(provider, true);
            }
            Err(_) => self.record_key_status(provider, false),
        }
    }

    /// Get a provider implementation by enum variant (returns Arc for 'static lifetime)
    pub fn get_provider(&self, provider: &AiProvider) -> Option<Arc<dyn AIProvider>> {
        self.providers.get(provider.as_str()).cloned()
    }

    /// List all providers with their configuration status
    pub fn list_configured_providers(&self) -> Vec<AiProviderStatus> {
        let all = [
            AiProvider::QoreLocal,
            AiProvider::OpenAi,
            AiProvider::Anthropic,
            AiProvider::MistralAi,
            AiProvider::GoogleGemini,
            AiProvider::DeepSeek,
            AiProvider::Ollama,
        ];

        all.into_iter()
            .map(|p| {
                let models = p
                    .available_models()
                    .iter()
                    .map(|m| AiModelInfoOwned {
                        id: m.id.to_string(),
                        label: m.label.to_string(),
                    })
                    .collect();
                AiProviderStatus {
                    has_key: self.has_api_key(&p),
                    default_model: p.default_model().to_string(),
                    models,
                    base_url: p.default_base_url().map(String::from),
                    provider: p,
                }
            })
            .collect()
    }
}

fn model_cache_key(provider: &AiProvider, base_url: Option<&str>) -> (AiProvider, String) {
    let endpoint = base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches('/').to_string())
        .or_else(|| provider.default_base_url().map(str::to_string))
        .unwrap_or_default();
    (provider.clone(), endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::error::{EngineError, EngineResult};
    use crate::vault::backend::{CredentialError, MockProvider};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingProvider {
        get_count: Arc<AtomicUsize>,
        values: Mutex<HashMap<String, String>>,
    }

    impl CountingProvider {
        fn with_key(get_count: Arc<AtomicUsize>, provider: &AiProvider, key: &str) -> Self {
            Self {
                get_count,
                values: Mutex::new(HashMap::from([(
                    provider.as_str().to_string(),
                    key.to_string(),
                )])),
            }
        }
    }

    impl CredentialProvider for CountingProvider {
        fn set_password(&self, _service: &str, username: &str, password: &str) -> EngineResult<()> {
            self.values
                .lock()
                .insert(username.to_string(), password.to_string());
            Ok(())
        }

        fn get_password(&self, _service: &str, username: &str) -> EngineResult<String> {
            self.get_count.fetch_add(1, Ordering::Relaxed);
            self.values
                .lock()
                .get(username)
                .cloned()
                .ok_or_else(|| EngineError::internal("Credentials not found"))
        }

        fn delete_password(&self, _service: &str, username: &str) -> EngineResult<()> {
            self.values.lock().remove(username);
            Ok(())
        }

        fn has_credential(&self, _service: &str, username: &str) -> Result<bool, CredentialError> {
            Ok(self.values.lock().contains_key(username))
        }

        fn delete_credential(&self, _service: &str, username: &str) -> Result<(), CredentialError> {
            self.values.lock().remove(username);
            Ok(())
        }
    }

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
        assert!(manager.has_api_key(&AiProvider::QoreLocal));
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

        assert_eq!(list.len(), 7);
        assert!(list[0].has_key); // Qore Local — never needs a key
        assert!(!list[1].has_key); // OpenAI — no key set
        assert!(!list[2].has_key); // Anthropic — no key set
        assert!(!list[3].has_key); // Mistral — no key set
        assert!(!list[4].has_key); // Gemini — no key set
        assert!(!list[5].has_key); // DeepSeek — no key set
        assert!(list[6].has_key); // Ollama — always true
    }

    #[test]
    fn test_get_provider() {
        let manager = AiManager::new(Box::new(MockProvider::new()));

        assert!(manager.get_provider(&AiProvider::QoreLocal).is_some());
        assert!(manager.get_provider(&AiProvider::OpenAi).is_some());
        assert!(manager.get_provider(&AiProvider::Anthropic).is_some());
        assert!(manager.get_provider(&AiProvider::MistralAi).is_some());
        assert!(manager.get_provider(&AiProvider::GoogleGemini).is_some());
        assert!(manager.get_provider(&AiProvider::DeepSeek).is_some());
        assert!(manager.get_provider(&AiProvider::Ollama).is_some());
    }

    #[test]
    fn model_cache_is_scoped_by_endpoint() {
        let manager = AiManager::new(Box::new(MockProvider::new()));
        let models = vec![AiModelInfoOwned {
            id: "local-model".to_string(),
            label: "Local model".to_string(),
        }];
        manager.cache_models(
            AiProvider::Ollama,
            Some("http://localhost:11434/"),
            models.clone(),
        );

        assert_eq!(
            manager
                .cached_models(&AiProvider::Ollama, Some("http://localhost:11434"))
                .unwrap()[0]
                .id,
            "local-model"
        );
        assert!(
            manager
                .cached_models(&AiProvider::Ollama, Some("http://localhost:22434"))
                .is_none()
        );
    }

    #[test]
    fn provider_status_does_not_scan_keychain_and_legacy_probe_runs_once() {
        let get_count = Arc::new(AtomicUsize::new(0));
        let manager = AiManager::new(Box::new(CountingProvider::with_key(
            Arc::clone(&get_count),
            &AiProvider::OpenAi,
            "sk-test",
        )));

        let openai_has_key = |manager: &AiManager| {
            manager
                .list_configured_providers()
                .into_iter()
                .find(|status| status.provider == AiProvider::OpenAi)
                .unwrap()
                .has_key
        };

        assert!(!openai_has_key(&manager));
        assert_eq!(get_count.load(Ordering::Relaxed), 0);

        manager.probe_api_key_once(&AiProvider::OpenAi);
        manager.probe_api_key_once(&AiProvider::OpenAi);
        assert_eq!(get_count.load(Ordering::Relaxed), 1);
        assert!(openai_has_key(&manager));

        assert_eq!(manager.get_api_key(&AiProvider::OpenAi).unwrap(), "sk-test");
        assert_eq!(get_count.load(Ordering::Relaxed), 1);
    }
}

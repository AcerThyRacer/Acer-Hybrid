//! Provider trait definitions

use acer_core::{AcerError, Model, ModelRequest, ModelResponse, ProviderType, Result};
use async_trait::async_trait;
use std::collections::HashMap;

/// Provider trait for model backends
#[async_trait]
pub trait Provider: Send + Sync {
    /// Get the provider type
    fn provider_type(&self) -> ProviderType;

    /// Check if the provider is available
    async fn is_available(&self) -> bool;

    /// List available models
    async fn list_models(&self) -> Result<Vec<Model>>;

    /// Send a chat completion request
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse>;

    /// Get provider name
    fn name(&self) -> &str;

    /// Check if this is a local provider
    fn is_local(&self) -> bool;
}

/// Provider factory for creating providers
pub struct ProviderFactory {
    configs: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Clone)]
pub enum ProviderConfig {
    Ollama {
        base_url: String,
    },
    OpenAI {
        api_key: String,
    },
    Anthropic {
        api_key: String,
    },
    Gemini {
        api_key: String,
    },
    Custom {
        name: String,
        base_url: String,
        api_key: Option<String>,
    },
}

impl ProviderFactory {
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: String, config: ProviderConfig) {
        self.configs.insert(name, config);
    }

    pub fn create(&self, name: &str) -> Result<Box<dyn Provider>> {
        let config = self
            .configs
            .get(name)
            .ok_or_else(|| AcerError::Provider(format!("Unknown provider: {}", name)))?;

        match config {
            ProviderConfig::Ollama { base_url } => {
                Ok(Box::new(super::OllamaProvider::new(base_url.clone())))
            }
            ProviderConfig::OpenAI { api_key } => {
                Ok(Box::new(super::OpenAIProvider::new(api_key.clone())))
            }
            ProviderConfig::Anthropic { api_key } => {
                Ok(Box::new(super::AnthropicProvider::new(api_key.clone())))
            }
            ProviderConfig::Gemini { api_key } => {
                Ok(Box::new(super::GeminiProvider::new(api_key.clone())))
            }
            ProviderConfig::Custom {
                name,
                base_url,
                api_key,
            } => Ok(Box::new(super::CustomProvider::new(
                name.clone(),
                base_url.clone(),
                api_key.clone(),
            ))),
        }
    }
}

impl Default for ProviderFactory {
    fn default() -> Self {
        Self::new()
    }
}

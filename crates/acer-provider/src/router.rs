//! Model router for intelligent request routing

use crate::Provider;
use acer_core::{
    AcerError, Model, ModelRequest, ModelResponse, PolicyDecision, ProviderType, Result,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Router for directing requests to appropriate providers
pub struct ModelRouter {
    providers: Arc<RwLock<HashMap<String, Box<dyn Provider>>>>,
    default_provider: String,
}

impl ModelRouter {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            default_provider: "ollama".to_string(),
        }
    }

    /// Register a provider
    pub async fn register_provider(&self, name: String, provider: Box<dyn Provider>) {
        let mut providers = self.providers.write().await;
        providers.insert(name, provider);
    }

    /// Set the default provider
    pub fn set_default(&mut self, name: &str) {
        self.default_provider = name.to_string();
    }

    /// Resolve the registered provider name for an identifier or prefixed model.
    pub async fn resolve_provider_name(&self, name: &str) -> Option<String> {
        let providers = self.providers.read().await;

        if providers.contains_key(name) {
            return Some(name.to_string());
        }

        // Try to find by model prefix
        for provider_name in providers.keys() {
            if name.starts_with(provider_name) {
                return Some(provider_name.clone());
            }
        }

        None
    }

    /// Route a request to the appropriate provider
    pub async fn route(
        &self,
        request: ModelRequest,
        policy: Option<&PolicyDecision>,
    ) -> Result<ModelResponse> {
        if self.providers.read().await.is_empty() {
            return Err(AcerError::Provider(
                "No providers are registered. Configure at least one provider before sending requests."
                    .to_string(),
            ));
        }

        let provider_name = self.determine_provider(&request.model, policy).await?;
        let mut request = request;

        if let Some(policy) = policy {
            if let Some(override_model) = &policy.model_override {
                request.model = override_model.clone();
            }
        }

        let prefix = format!("{}:", provider_name);
        if request.model.starts_with(&prefix) {
            request.model = request.model[prefix.len()..].to_string();
        }

        let providers = self.providers.read().await;
        let provider = providers
            .get(&provider_name)
            .ok_or_else(|| AcerError::Provider(format!("Provider not found: {}", provider_name)))?;

        provider.complete(request).await
    }

    /// Determine which provider to use for a model
    async fn determine_provider(
        &self,
        model: &str,
        policy: Option<&PolicyDecision>,
    ) -> Result<String> {
        let providers = self.providers.read().await;

        // Check for explicit model override from policy
        if let Some(policy) = policy {
            if let Some(ref override_model) = policy.model_override {
                // Find provider for the override model
                for (name, provider) in providers.iter() {
                    if !provider.is_available().await {
                        continue;
                    }
                    let models = provider.list_models().await?;
                    if models.iter().any(|m| m.id == *override_model) {
                        return Ok(name.clone());
                    }
                }
            }
        }

        // Check for model prefix (e.g., "openai:gpt-4" -> use openai provider)
        if let Some(colon_pos) = model.find(':') {
            let prefix = &model[..colon_pos];
            if providers.contains_key(prefix) {
                return Ok(prefix.to_string());
            }
        }

        // Try to find provider that has this model
        for (name, provider) in providers.iter() {
            if !provider.is_available().await {
                continue;
            }
            let models = provider.list_models().await?;
            if models.iter().any(|m| m.id == model || m.name == model) {
                return Ok(name.clone());
            }
        }

        // Fall back to default
        if let Some(default_provider) = providers.get(&self.default_provider) {
            if default_provider.is_available().await {
                return Ok(self.default_provider.clone());
            }
        }

        for (name, provider) in providers.iter() {
            if provider.is_available().await {
                return Ok(name.clone());
            }
        }

        Err(AcerError::Provider(
            "No available providers matched the request.".to_string(),
        ))
    }

    /// List all available models across all providers
    pub async fn list_all_models(&self) -> Result<Vec<Model>> {
        let providers = self.providers.read().await;
        if providers.is_empty() {
            return Err(AcerError::Provider(
                "No providers are registered, so no models are available.".to_string(),
            ));
        }
        let mut all_models = Vec::new();

        for provider in providers.values() {
            match provider.list_models().await {
                Ok(models) => all_models.extend(models),
                Err(e) => tracing::warn!("Failed to list models for provider: {}", e),
            }
        }

        if all_models.is_empty() {
            return Err(AcerError::ModelNotFound(
                "No models were returned by the configured providers.".to_string(),
            ));
        }

        Ok(all_models)
    }

    /// Check which providers are available
    pub async fn check_availability(&self) -> HashMap<String, bool> {
        let providers = self.providers.read().await;
        let mut availability = HashMap::new();

        for (name, provider) in providers.iter() {
            let available = provider.is_available().await;
            availability.insert(name.clone(), available);
        }

        availability
    }

    pub async fn provider_count(&self) -> usize {
        self.providers.read().await.len()
    }

    pub async fn estimate_cost(&self, response: &ModelResponse) -> Option<f64> {
        let providers = self.providers.read().await;
        for provider in providers.values() {
            if provider.provider_type() != response.provider {
                continue;
            }

            if let Ok(models) = provider.list_models().await {
                if let Some(model) = models
                    .iter()
                    .find(|model| model.id == response.model || model.name == response.model)
                {
                    if let Some(cost_per_1k) = model.cost_per_1k_tokens {
                        return Some((response.usage.total_tokens as f64 / 1000.0) * cost_per_1k);
                    }
                }
            }
        }

        if response.provider == ProviderType::Ollama {
            return Some(0.0);
        }

        None
    }
}

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Routing rules for model selection
#[derive(Debug, Clone)]
pub struct RoutingRules {
    /// Prefer local models when available
    pub prefer_local: bool,
    /// Maximum cost per request in USD
    pub max_cost_usd: Option<f64>,
    /// Maximum latency in milliseconds
    pub max_latency_ms: Option<u64>,
    /// Required capabilities
    pub required_capabilities: Vec<String>,
    /// Fallback order
    pub fallback_order: Vec<String>,
}

impl Default for RoutingRules {
    fn default() -> Self {
        Self {
            prefer_local: true,
            max_cost_usd: None,
            max_latency_ms: None,
            required_capabilities: Vec::new(),
            fallback_order: vec!["ollama".to_string(), "openai".to_string()],
        }
    }
}

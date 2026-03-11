//! Anthropic provider implementation

use crate::Provider;
use acer_core::{AcerError, Model, ModelRequest, ModelResponse, ProviderType, Result, TokenUsage};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    api_key: String,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Anthropic
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn list_models(&self) -> Result<Vec<Model>> {
        // Anthropic doesn't have a models endpoint, return known models
        Ok(vec![
            Model {
                id: "claude-3-opus-20240229".to_string(),
                name: "Claude 3 Opus".to_string(),
                provider: ProviderType::Anthropic,
                is_local: false,
                context_window: Some(200000),
                cost_per_1k_tokens: Some(0.015),
            },
            Model {
                id: "claude-3-sonnet-20240229".to_string(),
                name: "Claude 3 Sonnet".to_string(),
                provider: ProviderType::Anthropic,
                is_local: false,
                context_window: Some(200000),
                cost_per_1k_tokens: Some(0.003),
            },
            Model {
                id: "claude-3-haiku-20240307".to_string(),
                name: "Claude 3 Haiku".to_string(),
                provider: ProviderType::Anthropic,
                is_local: false,
                context_window: Some(200000),
                cost_per_1k_tokens: Some(0.00025),
            },
        ])
    }

    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse> {
        let start = Instant::now();

        // Extract system message if present
        let (system, messages): (Vec<_>, Vec<_>) = request.messages.into_iter()
            .partition(|m| matches!(m.role, acer_core::MessageRole::System));

        let system_text = system.first()
            .map(|m| m.content.as_str())
            .unwrap_or("");

        #[derive(Serialize)]
        struct AnthropicRequest {
            model: String,
            messages: Vec<AnthropicMessage>,
            #[serde(skip_serializing_if = "String::is_empty")]
            system: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_tokens: Option<usize>,
        }

        #[derive(Serialize)]
        struct AnthropicMessage {
            role: String,
            content: String,
        }

        let anthropic_request = AnthropicRequest {
            model: request.model.clone(),
            messages: messages.into_iter().map(|m| AnthropicMessage {
                role: match m.role {
                    acer_core::MessageRole::User => "user".to_string(),
                    acer_core::MessageRole::Assistant => "assistant".to_string(),
                    _ => "user".to_string(),
                },
                content: m.content,
            }).collect(),
            system: system_text.to_string(),
            max_tokens: request.max_tokens.or(Some(4096)),
        };

        let response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&anthropic_request)
            .send()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        if response.status() == 401 {
            return Err(AcerError::Auth("Invalid Anthropic API key".to_string()));
        }

        if response.status() == 429 {
            return Err(AcerError::RateLimited("Anthropic rate limit exceeded".to_string()));
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AcerError::Provider(format!("Anthropic error: {}", error_text)));
        }

        #[derive(Deserialize)]
        struct AnthropicResponse {
            id: String,
            model: String,
            content: Vec<AnthropicContent>,
            usage: AnthropicUsage,
            stop_reason: Option<String>,
        }

        #[derive(Deserialize)]
        struct AnthropicContent {
            text: Option<String>,
        }

        #[derive(Deserialize)]
        struct AnthropicUsage {
            input_tokens: usize,
            output_tokens: usize,
        }

        let data: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        let content = data.content
            .first()
            .and_then(|c| c.text.clone())
            .unwrap_or_default();

        Ok(ModelResponse {
            id: data.id,
            model: data.model,
            content,
            usage: TokenUsage {
                prompt_tokens: data.usage.input_tokens,
                completion_tokens: data.usage.output_tokens,
                total_tokens: data.usage.input_tokens + data.usage.output_tokens,
            },
            latency_ms: start.elapsed().as_millis() as u64,
            provider: ProviderType::Anthropic,
            finish_reason: data.stop_reason,
        })
    }

    fn name(&self) -> &str {
        "anthropic"
    }

    fn is_local(&self) -> bool {
        false
    }
}
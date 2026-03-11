//! Custom provider implementation for user-defined endpoints

use crate::http::{build_http_client, send_with_retries};
use crate::Provider;
use acer_core::{
    AcerError, Model, ModelRequest, ModelResponse, ProviderHttpConfig, ProviderType, Result,
    TokenUsage,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct CustomProvider {
    name: String,
    base_url: String,
    api_key: Option<String>,
    client: Client,
    retry_attempts: u32,
}

impl CustomProvider {
    pub fn new(name: String, base_url: String, api_key: Option<String>) -> Self {
        Self::with_http_config(name, base_url, api_key, ProviderHttpConfig::default())
    }

    pub fn with_http_config(
        name: String,
        base_url: String,
        api_key: Option<String>,
        http_config: ProviderHttpConfig,
    ) -> Self {
        Self {
            name,
            base_url,
            api_key,
            client: build_http_client(&http_config),
            retry_attempts: http_config.retry_attempts,
        }
    }
}

#[async_trait]
impl Provider for CustomProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Custom
    }

    async fn is_available(&self) -> bool {
        self.client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<Model>> {
        // Try OpenAI-compatible endpoint
        let response = send_with_retries(
            || {
                let request = self.client.get(format!("{}/models", self.base_url));
                if let Some(ref api_key) = self.api_key {
                    request.header("Authorization", format!("Bearer {}", api_key))
                } else {
                    request
                }
            },
            self.retry_attempts,
            "Custom provider list models",
        )
        .await?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        #[derive(Deserialize)]
        struct ModelsResponse {
            data: Vec<ModelInfo>,
        }

        #[derive(Deserialize)]
        struct ModelInfo {
            id: String,
        }

        let data: ModelsResponse = response
            .json()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        Ok(data
            .data
            .into_iter()
            .map(|m| Model {
                id: m.id.clone(),
                name: m.id,
                provider: ProviderType::Custom,
                is_local: false,
                context_window: None,
                cost_per_1k_tokens: None,
            })
            .collect())
    }

    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse> {
        let start = Instant::now();

        // Use OpenAI-compatible format
        #[derive(Serialize)]
        struct ChatRequest {
            model: String,
            messages: Vec<ChatMessage>,
            #[serde(skip_serializing_if = "Option::is_none")]
            temperature: Option<f32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_tokens: Option<usize>,
        }

        #[derive(Serialize)]
        struct ChatMessage {
            role: String,
            content: String,
        }

        let chat_request = ChatRequest {
            model: request.model.clone(),
            messages: request
                .messages
                .into_iter()
                .map(|m| ChatMessage {
                    role: match m.role {
                        acer_core::MessageRole::System => "system".to_string(),
                        acer_core::MessageRole::User => "user".to_string(),
                        acer_core::MessageRole::Assistant => "assistant".to_string(),
                        acer_core::MessageRole::Tool => "tool".to_string(),
                    },
                    content: m.content,
                })
                .collect(),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
        };

        let response = send_with_retries(
            || {
                let request = self
                    .client
                    .post(format!("{}/chat/completions", self.base_url))
                    .header("Content-Type", "application/json")
                    .json(&chat_request);

                if let Some(ref api_key) = self.api_key {
                    request.header("Authorization", format!("Bearer {}", api_key))
                } else {
                    request
                }
            },
            self.retry_attempts,
            "Custom provider completion",
        )
        .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AcerError::Provider(format!(
                "Custom provider completion failed with status {}: {}",
                status, error_text
            )));
        }

        #[derive(Deserialize)]
        struct ChatResponse {
            id: String,
            model: String,
            choices: Vec<Choice>,
            usage: Usage,
        }

        #[derive(Deserialize)]
        struct Choice {
            message: ResponseMessage,
            finish_reason: Option<String>,
        }

        #[derive(Deserialize)]
        struct ResponseMessage {
            content: Option<String>,
        }

        #[derive(Deserialize)]
        struct Usage {
            prompt_tokens: usize,
            completion_tokens: usize,
            total_tokens: usize,
        }

        let data: ChatResponse = response
            .json()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        let content = data
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(ModelResponse {
            id: data.id,
            model: data.model,
            content,
            usage: TokenUsage {
                prompt_tokens: data.usage.prompt_tokens,
                completion_tokens: data.usage.completion_tokens,
                total_tokens: data.usage.total_tokens,
            },
            latency_ms: start.elapsed().as_millis() as u64,
            provider: ProviderType::Custom,
            finish_reason: data.choices.first().and_then(|c| c.finish_reason.clone()),
        })
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_local(&self) -> bool {
        false
    }
}

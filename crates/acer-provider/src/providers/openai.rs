//! OpenAI provider implementation

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
pub struct OpenAIProvider {
    api_key: String,
    client: Client,
    base_url: String,
    retry_attempts: u32,
}

impl OpenAIProvider {
    pub fn new(api_key: String) -> Self {
        Self::with_http_config(api_key, ProviderHttpConfig::default())
    }

    pub fn with_http_config(api_key: String, http_config: ProviderHttpConfig) -> Self {
        Self {
            api_key,
            client: build_http_client(&http_config),
            base_url: "https://api.openai.com/v1".to_string(),
            retry_attempts: http_config.retry_attempts,
        }
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self::with_base_url_and_config(api_key, base_url, ProviderHttpConfig::default())
    }

    pub fn with_base_url_and_config(
        api_key: String,
        base_url: String,
        http_config: ProviderHttpConfig,
    ) -> Self {
        Self {
            api_key,
            client: build_http_client(&http_config),
            base_url,
            retry_attempts: http_config.retry_attempts,
        }
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::OpenAI
    }

    async fn is_available(&self) -> bool {
        if self.api_key.trim().is_empty() {
            return false;
        }

        send_with_retries(
            || {
                self.client
                    .get(format!("{}/models", self.base_url))
                    .header("Authorization", format!("Bearer {}", self.api_key))
            },
            0,
            "OpenAI availability check",
        )
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<Model>> {
        #[derive(Deserialize)]
        struct OpenAIModelsResponse {
            data: Vec<OpenAIModel>,
        }

        #[derive(Deserialize)]
        struct OpenAIModel {
            id: String,
        }

        let response = send_with_retries(
            || {
                self.client
                    .get(format!("{}/models", self.base_url))
                    .header("Authorization", format!("Bearer {}", self.api_key))
            },
            self.retry_attempts,
            "OpenAI list models",
        )
        .await?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let data: OpenAIModelsResponse = response
            .json()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        Ok(data
            .data
            .into_iter()
            .map(|m| {
                let cost = get_model_cost(&m.id);
                Model {
                    id: m.id.clone(),
                    name: m.id,
                    provider: ProviderType::OpenAI,
                    is_local: false,
                    context_window: None,
                    cost_per_1k_tokens: cost,
                }
            })
            .collect())
    }

    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse> {
        let start = Instant::now();

        #[derive(Serialize)]
        struct OpenAIRequest {
            model: String,
            messages: Vec<OpenAIMessage>,
            #[serde(skip_serializing_if = "Option::is_none")]
            temperature: Option<f32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_tokens: Option<usize>,
        }

        #[derive(Serialize)]
        struct OpenAIMessage {
            role: String,
            content: String,
        }

        let openai_request = OpenAIRequest {
            model: request.model.clone(),
            messages: request
                .messages
                .into_iter()
                .map(|m| OpenAIMessage {
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
                self.client
                    .post(format!("{}/chat/completions", self.base_url))
                    .header("Authorization", format!("Bearer {}", self.api_key))
                    .header("Content-Type", "application/json")
                    .json(&openai_request)
            },
            self.retry_attempts,
            "OpenAI completion",
        )
        .await?;

        if response.status() == 401 {
            return Err(AcerError::Auth("Invalid OpenAI API key".to_string()));
        }

        if response.status() == 429 {
            return Err(AcerError::RateLimited(
                "OpenAI rate limit exceeded".to_string(),
            ));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AcerError::Provider(format!(
                "OpenAI completion failed with status {}: {}",
                status, error_text
            )));
        }

        #[derive(Deserialize)]
        struct OpenAIResponse {
            id: String,
            model: String,
            choices: Vec<OpenAIChoice>,
            usage: OpenAIUsage,
        }

        #[derive(Deserialize)]
        struct OpenAIChoice {
            message: OpenAIMessageResponse,
            finish_reason: Option<String>,
        }

        #[derive(Deserialize)]
        struct OpenAIMessageResponse {
            content: Option<String>,
        }

        #[derive(Deserialize)]
        struct OpenAIUsage {
            prompt_tokens: usize,
            completion_tokens: usize,
            total_tokens: usize,
        }

        let data: OpenAIResponse = response
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
            provider: ProviderType::OpenAI,
            finish_reason: data.choices.first().and_then(|c| c.finish_reason.clone()),
        })
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn is_local(&self) -> bool {
        false
    }
}

fn get_model_cost(model_id: &str) -> Option<f64> {
    // Approximate costs per 1K tokens (as of 2024)
    match model_id {
        m if m.starts_with("gpt-4-turbo") => Some(0.01),
        m if m.starts_with("gpt-4") => Some(0.03),
        m if m.starts_with("gpt-3.5-turbo") => Some(0.001),
        _ => None,
    }
}

//! Ollama provider implementation

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
pub struct OllamaProvider {
    base_url: String,
    client: Client,
    retry_attempts: u32,
}

impl OllamaProvider {
    pub fn new(base_url: String) -> Self {
        Self::with_http_config(base_url, ProviderHttpConfig::default())
    }

    pub fn with_http_config(base_url: String, http_config: ProviderHttpConfig) -> Self {
        Self {
            base_url,
            client: build_http_client(&http_config),
            retry_attempts: http_config.retry_attempts,
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Ollama
    }

    async fn is_available(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<Model>> {
        #[derive(Deserialize)]
        struct OllamaResponse {
            models: Vec<OllamaModel>,
        }

        #[derive(Deserialize)]
        struct OllamaModel {
            name: String,
        }

        let response = send_with_retries(
            || self.client.get(format!("{}/api/tags", self.base_url)),
            self.retry_attempts,
            "Ollama list models",
        )
        .await?;

        let data: OllamaResponse = response
            .json()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        Ok(data
            .models
            .into_iter()
            .map(|m| Model {
                id: m.name.clone(),
                name: m.name,
                provider: ProviderType::Ollama,
                is_local: true,
                context_window: None,
                cost_per_1k_tokens: Some(0.0), // Local models are free
            })
            .collect())
    }

    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse> {
        let start = Instant::now();

        #[derive(Serialize)]
        struct OllamaRequest {
            model: String,
            messages: Vec<OllamaMessage>,
            #[serde(skip_serializing_if = "Option::is_none")]
            stream: Option<bool>,
        }

        #[derive(Serialize)]
        struct OllamaMessage {
            role: String,
            content: String,
        }

        let ollama_request = OllamaRequest {
            model: request.model.clone(),
            messages: request
                .messages
                .into_iter()
                .map(|m| OllamaMessage {
                    role: match m.role {
                        acer_core::MessageRole::System => "system".to_string(),
                        acer_core::MessageRole::User => "user".to_string(),
                        acer_core::MessageRole::Assistant => "assistant".to_string(),
                        acer_core::MessageRole::Tool => "tool".to_string(),
                    },
                    content: m.content,
                })
                .collect(),
            stream: request.stream.or(Some(false)),
        };

        let response = send_with_retries(
            || {
                self.client
                    .post(format!("{}/api/chat", self.base_url))
                    .json(&ollama_request)
            },
            self.retry_attempts,
            "Ollama completion",
        )
        .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AcerError::Provider(format!(
                "Ollama completion failed with status {}: {}",
                status, error_text
            )));
        }

        #[derive(Deserialize)]
        struct OllamaChatResponse {
            model: String,
            message: OllamaResponseMessage,
            #[serde(default)]
            done: bool,
            #[serde(default)]
            prompt_eval_count: Option<usize>,
            #[serde(default)]
            eval_count: Option<usize>,
        }

        #[derive(Deserialize)]
        struct OllamaResponseMessage {
            content: String,
        }

        let data: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        let prompt_tokens = data.prompt_eval_count.unwrap_or_default();
        let completion_tokens = data.eval_count.unwrap_or_default();

        Ok(ModelResponse {
            id: format!("ollama-{}", uuid::Uuid::new_v4().simple()),
            model: data.model,
            content: data.message.content,
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens.saturating_add(completion_tokens),
            },
            latency_ms: start.elapsed().as_millis() as u64,
            provider: ProviderType::Ollama,
            finish_reason: if data.done {
                Some("stop".to_string())
            } else {
                None
            },
        })
    }

    fn name(&self) -> &str {
        "ollama"
    }

    fn is_local(&self) -> bool {
        true
    }
}

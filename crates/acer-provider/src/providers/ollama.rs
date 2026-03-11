//! Ollama provider implementation

use crate::{Provider, ProviderConfig};
use acer_core::{AcerError, Model, ModelRequest, ModelResponse, ProviderType, Result, TokenUsage};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct OllamaProvider {
    base_url: String,
    client: Client,
}

impl OllamaProvider {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: Client::new(),
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
            #[serde(default)]
            size: Option<u64>,
        }

        let response = self.client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        let data: OllamaResponse = response
            .json()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        Ok(data.models.into_iter().map(|m| Model {
            id: m.name.clone(),
            name: m.name,
            provider: ProviderType::Ollama,
            is_local: true,
            context_window: None,
            cost_per_1k_tokens: Some(0.0), // Local models are free
        }).collect())
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
            messages: request.messages.into_iter().map(|m| OllamaMessage {
                role: match m.role {
                    acer_core::MessageRole::System => "system".to_string(),
                    acer_core::MessageRole::User => "user".to_string(),
                    acer_core::MessageRole::Assistant => "assistant".to_string(),
                    acer_core::MessageRole::Tool => "tool".to_string(),
                },
                content: m.content,
            }).collect(),
            stream: Some(false),
        };

        let response = self.client
            .post(format!("{}/api/chat", self.base_url))
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AcerError::Provider(format!("Ollama error: {}", error_text)));
        }

        #[derive(Deserialize)]
        struct OllamaChatResponse {
            model: String,
            message: OllamaResponseMessage,
            #[serde(default)]
            done: bool,
            #[serde(default)]
            total_duration: Option<u64>,
            #[serde(default)]
            prompt_eval_count: Option<usize>,
            #[serde(default)]
            eval_count: Option<usize>,
        }

        #[derive(Deserialize)]
        struct OllamaResponseMessage {
            role: String,
            content: String,
        }

        let data: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        Ok(ModelResponse {
            id: format!("ollama-{}", uuid::Uuid::new_v4().simple()),
            model: data.model,
            content: data.message.content,
            usage: TokenUsage {
                prompt_tokens: data.prompt_eval_count.unwrap_or(0),
                completion_tokens: data.eval_count.unwrap_or(0),
                total_tokens: data.prompt_eval_count.unwrap_or(0) + data.eval_count.unwrap_or(0),
            },
            latency_ms: start.elapsed().as_millis() as u64,
            provider: ProviderType::Ollama,
            finish_reason: if data.done { Some("stop".to_string()) } else { None },
        })
    }

    fn name(&self) -> &str {
        "ollama"
    }

    fn is_local(&self) -> bool {
        true
    }
}
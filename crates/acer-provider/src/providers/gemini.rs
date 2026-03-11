//! Google Gemini provider implementation

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
pub struct GeminiProvider {
    api_key: String,
    client: Client,
    retry_attempts: u32,
}

impl GeminiProvider {
    pub fn new(api_key: String) -> Self {
        Self::with_http_config(api_key, ProviderHttpConfig::default())
    }

    pub fn with_http_config(api_key: String, http_config: ProviderHttpConfig) -> Self {
        Self {
            api_key,
            client: build_http_client(&http_config),
            retry_attempts: http_config.retry_attempts,
        }
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Gemini
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn list_models(&self) -> Result<Vec<Model>> {
        Ok(vec![
            Model {
                id: "gemini-1.5-pro".to_string(),
                name: "Gemini 1.5 Pro".to_string(),
                provider: ProviderType::Gemini,
                is_local: false,
                context_window: Some(1000000),
                cost_per_1k_tokens: Some(0.0035),
            },
            Model {
                id: "gemini-1.5-flash".to_string(),
                name: "Gemini 1.5 Flash".to_string(),
                provider: ProviderType::Gemini,
                is_local: false,
                context_window: Some(1000000),
                cost_per_1k_tokens: Some(0.00035),
            },
            Model {
                id: "gemini-pro".to_string(),
                name: "Gemini Pro".to_string(),
                provider: ProviderType::Gemini,
                is_local: false,
                context_window: Some(32000),
                cost_per_1k_tokens: Some(0.0005),
            },
        ])
    }

    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse> {
        let start = Instant::now();

        #[derive(Serialize)]
        struct GeminiRequest {
            contents: Vec<GeminiContent>,
            #[serde(skip_serializing_if = "Option::is_none")]
            #[serde(rename = "generationConfig")]
            generation_config: Option<GeminiConfig>,
        }

        #[derive(Serialize)]
        struct GeminiContent {
            parts: Vec<GeminiPart>,
            role: String,
        }

        #[derive(Serialize)]
        struct GeminiPart {
            text: String,
        }

        #[derive(Serialize)]
        struct GeminiConfig {
            #[serde(skip_serializing_if = "Option::is_none")]
            temperature: Option<f32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            #[serde(rename = "maxOutputTokens")]
            max_output_tokens: Option<usize>,
        }

        let contents: Vec<GeminiContent> = request
            .messages
            .into_iter()
            .map(|m| GeminiContent {
                parts: vec![GeminiPart { text: m.content }],
                role: match m.role {
                    acer_core::MessageRole::User => "user".to_string(),
                    acer_core::MessageRole::Assistant => "model".to_string(),
                    _ => "user".to_string(),
                },
            })
            .collect();

        let gemini_request = GeminiRequest {
            contents,
            generation_config: Some(GeminiConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
        };

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            request.model, self.api_key
        );

        let response = send_with_retries(
            || {
                self.client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .json(&gemini_request)
            },
            self.retry_attempts,
            "Gemini completion",
        )
        .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AcerError::Provider(format!(
                "Gemini completion failed with status {}: {}",
                status, error_text
            )));
        }

        #[derive(Deserialize)]
        struct GeminiResponse {
            candidates: Vec<GeminiCandidate>,
            #[serde(rename = "usageMetadata")]
            usage_metadata: Option<GeminiUsage>,
        }

        #[derive(Deserialize)]
        struct GeminiCandidate {
            content: GeminiResponseContent,
            #[serde(rename = "finishReason")]
            finish_reason: Option<String>,
        }

        #[derive(Deserialize)]
        struct GeminiResponseContent {
            parts: Vec<GeminiResponsePart>,
        }

        #[derive(Deserialize)]
        struct GeminiResponsePart {
            text: Option<String>,
        }

        #[derive(Deserialize)]
        struct GeminiUsage {
            #[serde(rename = "promptTokenCount")]
            prompt_token_count: usize,
            #[serde(rename = "candidatesTokenCount")]
            candidates_token_count: usize,
            #[serde(rename = "totalTokenCount")]
            total_token_count: usize,
        }

        let data: GeminiResponse = response
            .json()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        let content = data
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .and_then(|p| p.text.clone())
            .unwrap_or_default();

        let usage = data.usage_metadata.unwrap_or(GeminiUsage {
            prompt_token_count: 0,
            candidates_token_count: 0,
            total_token_count: 0,
        });

        Ok(ModelResponse {
            id: format!("gemini-{}", uuid::Uuid::new_v4().simple()),
            model: request.model,
            content,
            usage: TokenUsage {
                prompt_tokens: usage.prompt_token_count,
                completion_tokens: usage.candidates_token_count,
                total_tokens: usage.total_token_count,
            },
            latency_ms: start.elapsed().as_millis() as u64,
            provider: ProviderType::Gemini,
            finish_reason: data
                .candidates
                .first()
                .and_then(|c| c.finish_reason.clone()),
        })
    }

    fn name(&self) -> &str {
        "gemini"
    }

    fn is_local(&self) -> bool {
        false
    }
}

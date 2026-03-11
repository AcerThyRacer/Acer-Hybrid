//! Google Gemini provider implementation

use crate::Provider;
use acer_core::{AcerError, Model, ModelRequest, ModelResponse, ProviderType, Result, TokenUsage};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct GeminiProvider {
    api_key: String,
    client: Client,
}

impl GeminiProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
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
            generationConfig: Option<GeminiConfig>,
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
            maxOutputTokens: Option<usize>,
        }

        let contents: Vec<GeminiContent> = request.messages.into_iter()
            .map(|m| GeminiContent {
                parts: vec![GeminiPart { text: m.content }],
                role: match m.role {
                    acer_core::MessageRole::User => "user".to_string(),
                    acer_core::MessageRole::Assistant => "model".to_string(),
                    _ => "user".to_string(),
                },
            }).collect();

        let gemini_request = GeminiRequest {
            contents,
            generationConfig: Some(GeminiConfig {
                temperature: request.temperature,
                maxOutputTokens: request.max_tokens,
            }),
        };

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            request.model, self.api_key
        );

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&gemini_request)
            .send()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AcerError::Provider(format!("Gemini error: {}", error_text)));
        }

        #[derive(Deserialize)]
        struct GeminiResponse {
            candidates: Vec<GeminiCandidate>,
            usageMetadata: Option<GeminiUsage>,
        }

        #[derive(Deserialize)]
        struct GeminiCandidate {
            content: GeminiResponseContent,
            finishReason: Option<String>,
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
            promptTokenCount: usize,
            candidatesTokenCount: usize,
            totalTokenCount: usize,
        }

        let data: GeminiResponse = response
            .json()
            .await
            .map_err(|e| AcerError::Http(e.to_string()))?;

        let content = data.candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .and_then(|p| p.text.clone())
            .unwrap_or_default();

        let usage = data.usageMetadata.unwrap_or(GeminiUsage {
            promptTokenCount: 0,
            candidatesTokenCount: 0,
            totalTokenCount: 0,
        });

        Ok(ModelResponse {
            id: format!("gemini-{}", uuid::Uuid::new_v4().simple()),
            model: request.model,
            content,
            usage: TokenUsage {
                prompt_tokens: usage.promptTokenCount,
                completion_tokens: usage.candidatesTokenCount,
                total_tokens: usage.totalTokenCount,
            },
            latency_ms: start.elapsed().as_millis() as u64,
            provider: ProviderType::Gemini,
            finish_reason: data.candidates.first().and_then(|c| c.finishReason.clone()),
        })
    }

    fn name(&self) -> &str {
        "gemini"
    }

    fn is_local(&self) -> bool {
        false
    }
}
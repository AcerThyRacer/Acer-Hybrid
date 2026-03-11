//! HTTP handlers for the gateway

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use acer_core::{Message, MessageRole, ModelRequest};
use crate::{ChatCompletionRequest, ChatCompletionResponse, ChatChoice, ChatMessageResponse, ChatUsage, ModelsResponse, ModelInfo, ErrorResponse};
use acer_provider::ModelRouter;
use acer_policy::PolicyEngine;
use acer_trace::TraceStore;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Gateway state
#[derive(Clone)]
pub struct GatewayState {
    pub router: Arc<RwLock<ModelRouter>>,
    pub policy: Arc<RwLock<PolicyEngine>>,
    pub trace_store: Arc<RwLock<Option<TraceStore>>>,
}

/// API error
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let error = ErrorResponse::new(&self.message, "api_error");
        (self.status, Json(error)).into_response()
    }
}

/// Health check handler
pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "acer-hybrid-gateway"
    }))
}

/// List models handler
pub async fn list_models(
    State(state): State<GatewayState>,
) -> Result<Json<ModelsResponse>, ApiError> {
    let router = state.router.read().await;
    let models = router.list_all_models().await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let model_infos: Vec<ModelInfo> = models.into_iter().map(|m| ModelInfo {
        id: m.id,
        object: "model".to_string(),
        created: 0,
        owned_by: m.provider.to_string(),
    }).collect();

    Ok(Json(ModelsResponse {
        object: "list".to_string(),
        data: model_infos,
    }))
}

/// Chat completions handler
pub async fn chat_completions(
    State(state): State<GatewayState>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Json<ChatCompletionResponse>, ApiError> {
    // Convert to internal request
    let messages: Vec<Message> = request.messages.into_iter().map(|m| Message {
        role: match m.role.as_str() {
            "system" => MessageRole::System,
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "tool" => MessageRole::Tool,
            _ => MessageRole::User,
        },
        content: m.content,
        name: m.name,
    }).collect();

    let model_request = ModelRequest {
        model: request.model.clone(),
        messages,
        temperature: request.temperature,
        max_tokens: request.max_tokens,
        stream: request.stream,
    };

    // Check policy
    let policy = state.policy.read().await;
    let decision = policy.validate(&model_request)
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !decision.allowed {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            decision.reason.unwrap_or_else(|| "Policy violation".to_string()),
        ));
    }

    drop(policy);

    // Route to provider
    let router = state.router.read().await;
    let response = router.route(model_request, None).await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Build response
    let chat_response = ChatCompletionResponse {
        id: response.id,
        object: "chat.completion".to_string(),
        created: chrono::Utc::now().timestamp(),
        model: response.model,
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessageResponse {
                role: "assistant".to_string(),
                content: response.content,
            },
            finish_reason: response.finish_reason,
        }],
        usage: ChatUsage {
            prompt_tokens: response.usage.prompt_tokens,
            completion_tokens: response.usage.completion_tokens,
            total_tokens: response.usage.total_tokens,
        },
    };

    Ok(Json(chat_response))
}

/// Get model info handler
pub async fn get_model(
    State(state): State<GatewayState>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelInfo>, ApiError> {
    let router = state.router.read().await;
    let models = router.list_all_models().await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let model = models.into_iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("Model not found: {}", model_id)))?;

    Ok(Json(ModelInfo {
        id: model.id,
        object: "model".to_string(),
        created: 0,
        owned_by: model.provider.to_string(),
    }))
}
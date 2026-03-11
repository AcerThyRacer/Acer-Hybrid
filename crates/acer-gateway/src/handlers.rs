//! HTTP handlers for the gateway

use crate::{
    ChatChoice, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessageResponse, ChatUsage, ChunkChoice, ChunkDelta, ErrorResponse, ModelInfo,
    ModelsResponse,
};
use acer_core::{
    validate_identifier, validate_max_tokens, validate_messages, validate_temperature, CostEntry,
    Message, MessageRole, ModelRequest, ModelResponse, RunRecord,
};
use acer_policy::PolicyEngine;
use acer_provider::ModelRouter;
use acer_trace::TraceStore;
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use chrono::Utc;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, RwLock};

/// Gateway metrics exposed in Prometheus text format.
#[derive(Default)]
pub struct GatewayMetrics {
    pub requests_total: AtomicU64,
    pub active_requests: AtomicU64,
    pub error_responses_total: AtomicU64,
    pub auth_failures_total: AtomicU64,
    pub rate_limited_total: AtomicU64,
}

pub struct GatewayRateLimiter {
    max_requests: u64,
    window: Duration,
    requests: VecDeque<Instant>,
}

impl GatewayRateLimiter {
    pub fn new(max_requests: u64, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            requests: VecDeque::new(),
        }
    }

    pub fn check(&mut self) -> bool {
        let now = Instant::now();
        while self
            .requests
            .front()
            .is_some_and(|timestamp| now.duration_since(*timestamp) > self.window)
        {
            self.requests.pop_front();
        }

        if self.requests.len() as u64 >= self.max_requests {
            return false;
        }

        self.requests.push_back(now);
        true
    }
}

/// Gateway state
#[derive(Clone)]
pub struct GatewayState {
    pub router: Arc<RwLock<ModelRouter>>,
    pub policy: Arc<RwLock<PolicyEngine>>,
    pub trace_store: Arc<RwLock<Option<TraceStore>>>,
    pub auth_token: Arc<Option<String>>,
    pub metrics: Arc<GatewayMetrics>,
    pub rate_limiter: Arc<Mutex<GatewayRateLimiter>>,
    pub max_messages_per_request: usize,
    pub max_message_chars: usize,
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

pub fn is_authorized(state: &GatewayState, headers: &HeaderMap) -> bool {
    let Some(expected) = state.auth_token.as_ref() else {
        return true;
    };

    let bearer_ok = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|token| token == expected)
        .unwrap_or(false);

    let api_key_ok = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(|token| token == expected)
        .unwrap_or(false);

    bearer_ok || api_key_ok
}

/// Health check handler
pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "acer-hybrid-gateway"
    }))
}

/// Prometheus metrics endpoint.
pub async fn metrics(State(state): State<GatewayState>) -> impl IntoResponse {
    let body = format!(
        "# TYPE acer_gateway_requests_total counter\nacer_gateway_requests_total {}\n# TYPE acer_gateway_active_requests gauge\nacer_gateway_active_requests {}\n# TYPE acer_gateway_error_responses_total counter\nacer_gateway_error_responses_total {}\n# TYPE acer_gateway_auth_failures_total counter\nacer_gateway_auth_failures_total {}\n# TYPE acer_gateway_rate_limited_total counter\nacer_gateway_rate_limited_total {}\n",
        state.metrics.requests_total.load(Ordering::Relaxed),
        state.metrics.active_requests.load(Ordering::Relaxed),
        state.metrics.error_responses_total.load(Ordering::Relaxed),
        state.metrics.auth_failures_total.load(Ordering::Relaxed),
        state.metrics.rate_limited_total.load(Ordering::Relaxed),
    );

    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

/// Browser dashboard
pub async fn dashboard() -> impl IntoResponse {
    Html(
        r#"<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Acer-Hybrid Dashboard</title>
  <style>
    body { font-family: ui-monospace, monospace; margin: 24px; background: #111827; color: #e5e7eb; }
    h1 { margin-bottom: 8px; }
    .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 16px; }
    .card { background: #1f2937; border: 1px solid #374151; border-radius: 12px; padding: 16px; }
    pre { white-space: pre-wrap; word-break: break-word; }
  </style>
</head>
<body>
  <h1>Acer-Hybrid Dashboard</h1>
  <div class="grid">
    <div class="card"><h2>Stats</h2><pre id="stats">loading...</pre></div>
    <div class="card"><h2>Recent Runs</h2><pre id="runs">loading...</pre></div>
  </div>
  <script>
    async function refresh() {
      const stats = await fetch('/api/stats').then(r => r.json());
      const runs = await fetch('/api/runs').then(r => r.json());
      document.getElementById('stats').textContent = JSON.stringify(stats, null, 2);
      document.getElementById('runs').textContent = JSON.stringify(runs, null, 2);
    }
    refresh();
    setInterval(refresh, 2000);
  </script>
</body>
</html>"#,
    )
}

/// List models handler
pub async fn list_models(
    State(state): State<GatewayState>,
) -> Result<Json<ModelsResponse>, ApiError> {
    let router = state.router.read().await;
    let models = router
        .list_all_models()
        .await
        .map_err(|e| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;

    let model_infos: Vec<ModelInfo> = models
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id,
            object: "model".to_string(),
            created: 0,
            owned_by: m.provider.to_string(),
        })
        .collect();

    Ok(Json(ModelsResponse {
        object: "list".to_string(),
        data: model_infos,
    }))
}

/// Stats API handler
pub async fn api_stats(
    State(state): State<GatewayState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let trace_store = state.trace_store.read().await;
    let store = trace_store
        .as_ref()
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "Trace store unavailable"))?;
    let stats = store
        .get_stats(Utc::now() - chrono::Duration::hours(24))
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch gateway stats: {}", e),
            )
        })?;
    Ok(Json(
        serde_json::to_value(stats).unwrap_or_else(|_| serde_json::json!({})),
    ))
}

/// Recent runs API handler
pub async fn api_runs(
    State(state): State<GatewayState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let trace_store = state.trace_store.read().await;
    let store = trace_store
        .as_ref()
        .ok_or_else(|| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "Trace store unavailable"))?;
    let runs = store.list_runs(20).await.map_err(|e| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to fetch recent runs: {}", e),
        )
    })?;
    Ok(Json(
        serde_json::to_value(runs).unwrap_or_else(|_| serde_json::json!([])),
    ))
}

/// Chat completions handler
pub async fn chat_completions(
    State(state): State<GatewayState>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, ApiError> {
    let stream = request.stream.unwrap_or(false);
    let user = request.user.clone();

    let messages: Vec<Message> = request
        .messages
        .into_iter()
        .map(|m| Message {
            role: match m.role.as_str() {
                "system" => MessageRole::System,
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                "tool" => MessageRole::Tool,
                _ => MessageRole::User,
            },
            content: m.content,
            name: m.name,
        })
        .collect();

    let model_request = ModelRequest {
        model: request.model.clone(),
        messages,
        temperature: request.temperature,
        max_tokens: request.max_tokens,
        stream: request.stream,
    };

    validate_request(&model_request, &state)?;

    let policy = state.policy.read().await;
    let (prepared_request, decision) = policy.prepare_request(&model_request).map_err(|e| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Policy validation failed: {}", e),
        )
    })?;

    if !decision.allowed {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            decision
                .reason
                .unwrap_or_else(|| "Policy violation".to_string()),
        ));
    }

    drop(policy);

    let router = state.router.read().await;
    let response = router
        .route(prepared_request.clone(), Some(&decision))
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                format!("Provider request failed: {}", e),
            )
        })?;
    let estimated_cost = router.estimate_cost(&response).await;
    drop(router);

    store_run(
        &state,
        &prepared_request,
        &response,
        estimated_cost,
        decision,
        user,
    )
    .await;

    if stream {
        return Ok(streaming_response(&response));
    }

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

    Ok(Json(chat_response).into_response())
}

/// Get model info handler
pub async fn get_model(
    State(state): State<GatewayState>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelInfo>, ApiError> {
    let router = state.router.read().await;
    let models = router
        .list_all_models()
        .await
        .map_err(|e| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;

    let model = models
        .into_iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                format!("Model not found: {}", model_id),
            )
        })?;

    Ok(Json(ModelInfo {
        id: model.id,
        object: "model".to_string(),
        created: 0,
        owned_by: model.provider.to_string(),
    }))
}

async fn store_run(
    state: &GatewayState,
    trace_request: &ModelRequest,
    response: &ModelResponse,
    estimated_cost: Option<f64>,
    decision: acer_core::PolicyDecision,
    user: Option<String>,
) {
    let mut run = RunRecord::new(trace_request.clone());
    run.provider = response.provider;
    run.response = Some(response.clone());
    run.success = true;
    run.latency_ms = response.latency_ms;
    run.cost_usd = estimated_cost;
    run.policy_decision = Some(decision);
    run.metadata = serde_json::json!({
        "gateway": true,
        "stream": trace_request.stream.unwrap_or(false),
        "user": user
    });

    let trace_store = state.trace_store.read().await;
    if let Some(store) = trace_store.as_ref() {
        let _ = store.store_run(&run).await;
        if let Some(cost_usd) = run.cost_usd {
            let _ = store
                .store_cost(&CostEntry {
                    timestamp: Utc::now(),
                    provider: response.provider,
                    model: response.model.clone(),
                    tokens: response.usage.clone(),
                    cost_usd,
                    run_id: run.id.clone(),
                })
                .await;
        }
    }
}

fn streaming_response(response: &ModelResponse) -> Response {
    let created = chrono::Utc::now().timestamp();
    let chunk = ChatCompletionChunk {
        id: response.id.clone(),
        object: "chat.completion.chunk".to_string(),
        created,
        model: response.model.clone(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                role: Some("assistant".to_string()),
                content: Some(response.content.clone()),
            },
            finish_reason: None,
        }],
    };

    let final_chunk = ChatCompletionChunk {
        id: response.id.clone(),
        object: "chat.completion.chunk".to_string(),
        created,
        model: response.model.clone(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                role: None,
                content: None,
            },
            finish_reason: response.finish_reason.clone(),
        }],
    };

    let body = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        serde_json::to_string(&chunk).unwrap_or_else(|_| "{}".to_string()),
        serde_json::to_string(&final_chunk).unwrap_or_else(|_| "{}".to_string()),
    );

    (
        [(header::CONTENT_TYPE, "text/event-stream; charset=utf-8")],
        body,
    )
        .into_response()
}

fn validate_request(request: &ModelRequest, state: &GatewayState) -> Result<(), ApiError> {
    validate_identifier("model", &request.model).map_err(api_bad_request)?;
    validate_temperature(request.temperature).map_err(api_bad_request)?;
    validate_max_tokens(request.max_tokens, None).map_err(api_bad_request)?;
    validate_messages(
        &request.messages,
        state.max_messages_per_request,
        state.max_message_chars,
    )
    .map_err(api_bad_request)?;
    Ok(())
}

fn api_bad_request(error: acer_core::AcerError) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, error.to_string())
}

//! Gateway server implementation

use axum::{
    extract::DefaultBodyLimit,
    extract::{Request, State},
    http::{
        header::{self, HeaderName},
        HeaderValue, Method, StatusCode,
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tower_http::{
    cors::CorsLayer,
    trace::{DefaultOnResponse, TraceLayer},
};

use crate::handlers::{
    api_runs, api_stats, chat_completions, dashboard, get_model, health, is_authorized,
    list_models, metrics, GatewayMetrics, GatewayRateLimiter, GatewayState,
};
use acer_core::{AcerConfig, GatewayConfig};
use acer_policy::PolicyEngine;
use acer_provider::ModelRouter;
use acer_trace::TraceStore;

/// Gateway server
pub struct GatewayServer {
    addr: SocketAddr,
    config: GatewayConfig,
    trace_db_path: PathBuf,
    trace_max_connections: u32,
    state: GatewayState,
}

impl GatewayServer {
    /// Create a new gateway server
    pub fn new(host: &str, port: u16) -> acer_core::Result<Self> {
        let mut config = GatewayConfig::default();
        config.host = host.to_string();
        config.port = port;
        Self::with_gateway_config(config, AcerConfig::data_dir().join("traces.db"), 5)
    }

    pub fn from_config(config: &AcerConfig) -> acer_core::Result<Self> {
        let trace_db_path = config
            .tracing
            .database_path
            .clone()
            .unwrap_or_else(|| AcerConfig::data_dir().join("traces.db"));
        Self::with_gateway_config(
            config.gateway.clone(),
            trace_db_path,
            config.tracing.max_connections,
        )
    }

    fn with_gateway_config(
        config: GatewayConfig,
        trace_db_path: PathBuf,
        trace_max_connections: u32,
    ) -> acer_core::Result<Self> {
        let addr: SocketAddr = format!("{}:{}", config.host, config.port)
            .parse()
            .map_err(|e| {
                acer_core::AcerError::Gateway(format!("Invalid gateway address: {}", e))
            })?;
        let auth_token = resolve_auth_token(&config);
        let rate_limit_requests = config.rate_limit_requests;
        let rate_limit_window = Duration::from_secs(config.rate_limit_window_secs);
        let max_messages_per_request = config.max_messages_per_request;
        let max_message_chars = config.max_message_chars;

        Ok(Self {
            addr,
            config,
            trace_db_path,
            trace_max_connections,
            state: GatewayState {
                router: Arc::new(RwLock::new(ModelRouter::new())),
                policy: Arc::new(RwLock::new(PolicyEngine::new())),
                trace_store: Arc::new(RwLock::new(None)),
                auth_token: Arc::new(auth_token),
                metrics: Arc::new(GatewayMetrics::default()),
                rate_limiter: Arc::new(tokio::sync::Mutex::new(GatewayRateLimiter::new(
                    rate_limit_requests,
                    rate_limit_window,
                ))),
                max_messages_per_request,
                max_message_chars,
            },
        })
    }

    /// Create with existing components
    pub fn with_components(
        addr: SocketAddr,
        router: ModelRouter,
        policy: PolicyEngine,
        trace_store: Option<TraceStore>,
    ) -> Self {
        let mut config = GatewayConfig::default();
        config.host = addr.ip().to_string();
        config.port = addr.port();
        let trace_db_path = AcerConfig::data_dir().join("traces.db");
        let max_messages_per_request = config.max_messages_per_request;
        let max_message_chars = config.max_message_chars;

        Self {
            addr,
            trace_db_path,
            trace_max_connections: 5,
            state: GatewayState {
                router: Arc::new(RwLock::new(router)),
                policy: Arc::new(RwLock::new(policy)),
                trace_store: Arc::new(RwLock::new(trace_store)),
                auth_token: Arc::new(resolve_auth_token(&config)),
                metrics: Arc::new(GatewayMetrics::default()),
                rate_limiter: Arc::new(tokio::sync::Mutex::new(GatewayRateLimiter::new(
                    config.rate_limit_requests,
                    Duration::from_secs(config.rate_limit_window_secs),
                ))),
                max_messages_per_request,
                max_message_chars,
            },
            config,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn router(&self) -> Arc<RwLock<ModelRouter>> {
        self.state.router.clone()
    }

    pub fn policy(&self) -> Arc<RwLock<PolicyEngine>> {
        self.state.policy.clone()
    }

    pub fn trace_store(&self) -> Arc<RwLock<Option<TraceStore>>> {
        self.state.trace_store.clone()
    }

    fn build_router(&self) -> Router {
        let state = self.state.clone();
        let mut app = Router::new()
            .route("/v1/chat/completions", post(chat_completions))
            .route("/v1/models", get(list_models))
            .route("/v1/models/:model", get(get_model))
            .route("/dashboard", get(dashboard))
            .route("/api/stats", get(api_stats))
            .route("/api/runs", get(api_runs))
            .route("/metrics", get(metrics))
            .route("/health", get(health))
            .route("/", get(health))
            .layer(DefaultBodyLimit::max(self.config.max_request_body_bytes))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                track_requests_middleware,
            ))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            ))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                rate_limit_middleware,
            ))
            .layer(
                TraceLayer::new_for_http()
                    .on_response(DefaultOnResponse::new().level(tracing::Level::INFO)),
            )
            .with_state(state);

        if !self.config.cors_allowed_origins.is_empty() {
            let origins: Vec<HeaderValue> = self
                .config
                .cors_allowed_origins
                .iter()
                .filter_map(|origin| HeaderValue::from_str(origin).ok())
                .collect();

            if !origins.is_empty() {
                app = app.layer(
                    CorsLayer::new()
                        .allow_origin(origins)
                        .allow_methods([Method::GET, Method::POST])
                        .allow_headers([
                            header::AUTHORIZATION,
                            header::CONTENT_TYPE,
                            HeaderName::from_static("x-api-key"),
                        ]),
                );
            }
        }

        app
    }

    /// Start the server
    pub async fn serve(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.preflight().await?;

        let app = self.build_router();
        tracing::info!("Starting gateway server on {}", self.addr);

        let listener = tokio::net::TcpListener::bind(self.addr).await?;
        axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(shutdown_signal())
            .await?;

        Ok(())
    }

    /// Run the server (blocking)
    pub async fn run(self) {
        if let Err(error) = self.serve().await {
            tracing::error!("Gateway server failed: {}", error);
        }
    }

    async fn preflight(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.addr.ip().is_loopback() && self.state.auth_token.is_none() {
            return Err("Gateway authentication is required for non-loopback bindings. Set gateway.api_key_env or ACER_GATEWAY_API_KEY.".into());
        }

        {
            let mut trace_store = self.state.trace_store.write().await;
            if trace_store.is_none() {
                *trace_store = Some(
                    TraceStore::with_max_connections(
                        &self.trace_db_path,
                        self.trace_max_connections,
                    )
                    .await?,
                );
            }
        }

        if self.router().read().await.provider_count().await == 0 {
            return Err("Gateway has no registered providers. Initialize providers before serving requests.".into());
        }

        Ok(())
    }
}

async fn auth_middleware(
    State(state): State<GatewayState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if request.method() == Method::OPTIONS || path == "/" || path == "/health" {
        return next.run(request).await;
    }

    if is_authorized(&state, request.headers()) {
        return next.run(request).await;
    }

    state
        .metrics
        .auth_failures_total
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state
        .metrics
        .error_responses_total
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    (
        StatusCode::UNAUTHORIZED,
        axum::Json(crate::ErrorResponse::new(
            "Unauthorized. Provide a valid Bearer token or x-api-key.",
            "auth_error",
        )),
    )
        .into_response()
}

async fn track_requests_middleware(
    State(state): State<GatewayState>,
    request: Request,
    next: Next,
) -> Response {
    state
        .metrics
        .requests_total
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state
        .metrics
        .active_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let response = next.run(request).await;

    state
        .metrics
        .active_requests
        .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    if response.status().is_client_error() || response.status().is_server_error() {
        state
            .metrics
            .error_responses_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    response
}

async fn rate_limit_middleware(
    State(state): State<GatewayState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if path == "/" || path == "/health" {
        return next.run(request).await;
    }

    // Use authorized auth token or IP address as the rate limit key
    let key = if is_authorized(&state, request.headers()) {
        request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .or_else(|| request.headers().get("x-api-key").and_then(|h| h.to_str().ok()))
            .map(|s| s.to_string())
            .unwrap_or_else(|| addr.ip().to_string())
    } else {
        addr.ip().to_string()
    };

    let allowed = {
        let mut limiter = state.rate_limiter.lock().await;
        limiter.check(&key)
    };

    if allowed {
        return next.run(request).await;
    }

    state
        .metrics
        .rate_limited_total
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state
        .metrics
        .error_responses_total
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    (
        StatusCode::TOO_MANY_REQUESTS,
        axum::Json(crate::ErrorResponse::new(
            "Rate limit exceeded for the configured gateway window.",
            "rate_limit_error",
        )),
    )
        .into_response()
}

fn resolve_auth_token(config: &GatewayConfig) -> Option<String> {
    config
        .api_key_env
        .as_ref()
        .and_then(|name| std::env::var(name).ok())
        .or_else(|| std::env::var("ACER_GATEWAY_API_KEY").ok())
        .filter(|value| !value.trim().is_empty())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Gateway shutdown signal received");
}

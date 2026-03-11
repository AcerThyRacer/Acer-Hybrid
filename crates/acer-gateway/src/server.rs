//! Gateway server implementation

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::handlers::{GatewayState, health, list_models, chat_completions, get_model};
use acer_provider::ModelRouter;
use acer_policy::PolicyEngine;
use acer_trace::TraceStore;

/// Gateway server
pub struct GatewayServer {
    addr: SocketAddr,
    state: GatewayState,
}

impl GatewayServer {
    /// Create a new gateway server
    pub fn new(host: &str, port: u16) -> Self {
        let addr: SocketAddr = format!("{}:{}", host, port)
            .parse()
            .expect("Invalid address");

        Self {
            addr,
            state: GatewayState {
                router: Arc::new(RwLock::new(ModelRouter::new())),
                policy: Arc::new(RwLock::new(PolicyEngine::new())),
                trace_store: Arc::new(RwLock::new(None)),
            },
        }
    }

    /// Create with existing components
    pub fn with_components(
        addr: SocketAddr,
        router: ModelRouter,
        policy: PolicyEngine,
        trace_store: Option<TraceStore>,
    ) -> Self {
        Self {
            addr,
            state: GatewayState {
                router: Arc::new(RwLock::new(router)),
                policy: Arc::new(RwLock::new(policy)),
                trace_store: Arc::new(RwLock::new(trace_store)),
            },
        }
    }

    /// Get the server address
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Get a reference to the router
    pub fn router(&self) -> Arc<RwLock<ModelRouter>> {
        self.state.router.clone()
    }

    /// Get a reference to the policy engine
    pub fn policy(&self) -> Arc<RwLock<PolicyEngine>> {
        self.state.policy.clone()
    }

    /// Build the Axum router
    fn build_router(&self) -> Router {
        Router::new()
            // OpenAI-compatible endpoints
            .route("/v1/chat/completions", post(chat_completions))
            .route("/v1/models", get(list_models))
            .route("/v1/models/:model", get(get_model))
            
            // Health check
            .route("/health", get(health))
            .route("/", get(health))
            
            // Middleware
            .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any))
            .layer(TraceLayer::new_for_http())
            
            .with_state(self.state.clone())
    }

    /// Start the server
    pub async fn serve(&self) -> Result<(), Box<dyn std::error::Error>> {
        let app = self.build_router();
        
        tracing::info!("Starting gateway server on {}", self.addr);
        
        let listener = tokio::net::TcpListener::bind(self.addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    /// Run the server (blocking)
    pub async fn run(self) {
        let app = self.build_router();
        
        tracing::info!("Gateway server listening on {}", self.addr);
        
        let listener = tokio::net::TcpListener::bind(self.addr)
            .await
            .expect("Failed to bind");

        axum::serve(listener, app)
            .await
            .expect("Server failed");
    }
}
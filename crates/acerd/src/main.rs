//! Acer Hybrid Daemon - Background services

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use acer_core::AcerConfig;
use acer_gateway::GatewayServer;
use acer_provider::{ModelRouter, OllamaProvider, OpenAIProvider};
use acer_policy::PolicyEngine;
use acer_trace::TraceStore;
use acer_vault::SecretsVault;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
        )
        .init();

    tracing::info!("Starting Acer Hybrid daemon...");

    // Load configuration
    let config = AcerConfig::load()?;
    tracing::info!("Configuration loaded");

    // Initialize components
    let data_dir = AcerConfig::data_dir();
    std::fs::create_dir_all(&data_dir)?;

    // Write PID file
    let pid = std::process::id();
    let pid_file = data_dir.join("acerd.pid");
    std::fs::write(&pid_file, pid.to_string())?;
    tracing::info!("PID: {} (written to {:?})", pid, pid_file);

    // Ensure cleanup on exit
    let pid_file_cleanup = pid_file.clone();
    ctrlc::set_handler(move || {
        tracing::info!("Shutting down...");
        let _ = std::fs::remove_file(&pid_file_cleanup);
        std::process::exit(0);
    })?;

    // Initialize trace store
    let db_path = data_dir.join("traces.db");
    let trace_store = TraceStore::new(&db_path).await?;
    tracing::info!("Trace store initialized");

    // Initialize policy engine
    let policy = PolicyEngine::new();
    tracing::info!("Policy engine initialized");

    // Initialize model router
    let router = ModelRouter::new();
    tracing::info!("Model router initialized");

    // Register providers
    {
        // Ollama (local)
        let ollama = OllamaProvider::new(config.providers.ollama.base_url.clone());
        if ollama.is_available().await {
            tracing::info!("Ollama provider available");
        } else {
            tracing::warn!("Ollama provider not available");
        }

        // OpenAI (cloud)
        let vault_path = data_dir.join("vault.json");
        if vault_path.exists() {
            if let Ok(vault) = SecretsVault::load(vault_path, None) {
                if let Ok(Some(api_key)) = vault.get(acer_vault::keys::OPENAI_API_KEY) {
                    tracing::info!("OpenAI provider configured");
                }
            }
        }
    }

    // Start gateway server
    if config.gateway.enabled {
        let addr: std::net::SocketAddr = format!("{}:{}", config.gateway.host, config.gateway.port)
            .parse()
            .expect("Invalid gateway address");

        tracing::info!("Starting gateway on {}", addr);

        let server = GatewayServer::new(&config.gateway.host, config.gateway.port);
        
        // Run the server
        server.serve().await?;
    } else {
        tracing::info!("Gateway disabled in configuration");

        // Keep the daemon running
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&pid_file);

    Ok(())
}
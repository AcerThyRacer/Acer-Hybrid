//! Acer Hybrid Daemon - Background services

use acer_core::AcerConfig;
use acer_gateway::GatewayServer;
use acer_policy::PolicyEngine;
use acer_provider::{AnthropicProvider, GeminiProvider, OllamaProvider, OpenAIProvider, Provider};
use acer_trace::TraceStore;
use acer_vault::SecretsVault;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("Starting Acer Hybrid daemon...");

    // Load configuration
    let config = AcerConfig::load()?;
    let http_config = config.providers.http.clone();
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
    let _trace_store = TraceStore::new(&db_path).await?;
    tracing::info!("Trace store initialized");

    // Initialize policy engine
    tracing::info!("Policy engine initialized");
    tracing::info!("Model router initialized");

    // Start gateway server
    if config.gateway.enabled {
        let addr: std::net::SocketAddr = format!("{}:{}", config.gateway.host, config.gateway.port)
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid gateway address: {}", e))?;

        tracing::info!("Starting gateway on {}", addr);

        let server = GatewayServer::from_config(&config)?;
        {
            let policy = server.policy();
            let mut policy = policy.write().await;
            *policy = PolicyEngine::with_config(config.policy.clone().into());
        }
        {
            let trace_store = server.trace_store();
            let mut trace_store = trace_store.write().await;
            *trace_store = Some(TraceStore::from_config(&config).await?);
        }

        {
            let router = server.router();
            let router = router.read().await;
            let ollama = OllamaProvider::with_http_config(
                config.providers.ollama.base_url.clone(),
                http_config.clone(),
            );
            if ollama.is_available().await {
                tracing::info!("Ollama provider available");
            } else {
                tracing::warn!("Ollama provider not available");
            }
            router
                .register_provider("ollama".to_string(), Box::new(ollama))
                .await;
        }

        let vault_path = data_dir.join("vault.json");
        if vault_path.exists() {
            if let Ok(vault) = SecretsVault::load(vault_path, None) {
                if let Ok(password) = std::env::var("ACER_VAULT_PASSWORD") {
                    let mut vault = vault;
                    if vault.unlock(&password).is_ok() {
                        let router = server.router();
                        let router = router.read().await;
                        if let Ok(Some(api_key)) = vault.get(acer_vault::keys::OPENAI_API_KEY) {
                            router
                                .register_provider(
                                    "openai".to_string(),
                                    Box::new(OpenAIProvider::with_http_config(
                                        api_key,
                                        http_config.clone(),
                                    )),
                                )
                                .await;
                        }
                        if let Ok(Some(api_key)) = vault.get(acer_vault::keys::ANTHROPIC_API_KEY) {
                            router
                                .register_provider(
                                    "anthropic".to_string(),
                                    Box::new(AnthropicProvider::with_http_config(
                                        api_key,
                                        http_config.clone(),
                                    )),
                                )
                                .await;
                        }
                        if let Ok(Some(api_key)) = vault.get(acer_vault::keys::GEMINI_API_KEY) {
                            router
                                .register_provider(
                                    "gemini".to_string(),
                                    Box::new(GeminiProvider::with_http_config(
                                        api_key,
                                        http_config.clone(),
                                    )),
                                )
                                .await;
                        }
                    }
                }
            }
        }

        // Run the server
        server
            .serve()
            .await
            .map_err(|e| anyhow::anyhow!("Gateway server failed: {}", e))?;
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

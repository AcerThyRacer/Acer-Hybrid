//! Gateway command - start the local API gateway

use acer_core::AcerConfig;
use acer_gateway::GatewayServer;
use acer_policy::PolicyEngine;
use acer_provider::{AnthropicProvider, GeminiProvider, OllamaProvider, OpenAIProvider};
use acer_trace::TraceStore;
use anyhow::Result;

pub async fn execute(host: String, port: u16) -> Result<()> {
    let mut config = AcerConfig::load()?;
    config.gateway.host = host.clone();
    config.gateway.port = port;
    let http_config = config.providers.http.clone();

    println!("Starting Acer Hybrid Gateway...");
    println!("Listening on http://{}:{}", host, port);
    println!("\nOpenAI-compatible endpoints:");
    println!("  POST http://{}:{}/v1/chat/completions", host, port);
    println!("  GET  http://{}:{}/v1/models", host, port);
    println!("\nPress Ctrl+C to stop");

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

    // Register providers
    {
        let router = server.router();
        let router = router.read().await;
        router
            .register_provider(
                "ollama".to_string(),
                Box::new(OllamaProvider::with_http_config(
                    config.providers.ollama.base_url.clone(),
                    http_config.clone(),
                )),
            )
            .await;
    }

    let vault_path = AcerConfig::data_dir().join("vault.json");
    if vault_path.exists() {
        let mut vault = acer_vault::SecretsVault::load(vault_path, None)?;
        if !vault.list_keys().is_empty() {
            let password = rpassword::prompt_password("Vault password for gateway access: ")?;
            vault.unlock(&password)?;
            let router = server.router();
            let router = router.read().await;
            if let Some(api_key) = vault.get(acer_vault::keys::OPENAI_API_KEY)? {
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
            if let Some(api_key) = vault.get(acer_vault::keys::ANTHROPIC_API_KEY)? {
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
            if let Some(api_key) = vault.get(acer_vault::keys::GEMINI_API_KEY)? {
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

    server
        .serve()
        .await
        .map_err(|e| anyhow::anyhow!("Gateway server failed: {}", e))?;

    Ok(())
}

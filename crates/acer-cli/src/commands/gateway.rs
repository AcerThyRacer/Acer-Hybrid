//! Gateway command - start the local API gateway

use acer_core::AcerConfig;
use acer_gateway::GatewayServer;
use acer_provider::{ModelRouter, OllamaProvider, OpenAIProvider};
use acer_policy::PolicyEngine;
use anyhow::Result;

pub async fn execute(
    host: String,
    port: u16,
) -> Result<()> {
    let config = AcerConfig::load()?;
    
    println!("Starting Acer Hybrid Gateway...");
    println!("Listening on http://{}:{}", host, port);
    println!("\nOpenAI-compatible endpoints:");
    println!("  POST http://{}:{}/v1/chat/completions", host, port);
    println!("  GET  http://{}:{}/v1/models", host, port);
    println!("\nPress Ctrl+C to stop");
    
    let server = GatewayServer::new(&host, port);
    
    // Register providers
    {
        let router = server.router().write().await;
        
        // Register Ollama
        let ollama = OllamaProvider::new(config.providers.ollama.base_url.clone());
        // router.register_provider("ollama".to_string(), Box::new(ollama)).await;
    }
    
    server.serve().await?;
    
    Ok(())
}
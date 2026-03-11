//! Ask command - send a prompt to an AI model

use acer_core::{AcerConfig, Message, ModelRequest};
use acer_provider::{ModelRouter, OllamaProvider, OpenAIProvider};
use acer_policy::PolicyEngine;
use acer_trace::TraceStore;
use acer_vault::SecretsVault;
use anyhow::Result;

pub async fn execute(
    prompt: String,
    model: Option<String>,
    provider: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<usize>,
    attach: Option<String>,
    verbose: bool,
) -> Result<()> {
    let config = AcerConfig::load()?;
    
    // Initialize router
    let router = ModelRouter::new();
    
    // Register providers
    // TODO: Use Arc<RwLock<ModelRouter>> pattern
    
    // Build request
    let mut messages = vec![];
    
    // Add attached file content if provided
    if let Some(ref path) = attach {
        let content = std::fs::read_to_string(path)?;
        messages.push(Message::system(format!("Attached file content:\n\n{}", content)));
    }
    
    messages.push(Message::user(&prompt));
    
    let model_name = model
        .or(config.providers.default_provider.clone())
        .or(config.providers.ollama.default_model.clone())
        .unwrap_or_else(|| "llama2".to_string());
    
    let request = ModelRequest {
        model: model_name.clone(),
        messages,
        temperature,
        max_tokens,
        stream: None,
    };
    
    // Check policy
    let policy = PolicyEngine::new();
    let decision = policy.validate(&request)?;
    
    if !decision.allowed {
        eprintln!("Policy violation: {}", decision.reason.unwrap_or_default());
        std::process::exit(1);
    }
    
    if verbose {
        println!("Model: {}", model_name);
        println!("Policy: {}", if decision.allowed { "ALLOWED" } else { "BLOCKED" });
        if !decision.redactions.is_empty() {
            println!("Redactions: {} items", decision.redactions.len());
        }
        println!("---");
    }
    
    // Try Ollama first (local)
    let ollama = OllamaProvider::new(config.providers.ollama.base_url.clone());
    
    if ollama.is_available().await {
        if verbose {
            eprintln!("Using Ollama (local)");
        }
        
        match ollama.complete(request).await {
            Ok(response) => {
                println!("{}", response.content);
                
                if verbose {
                    eprintln!("\n---");
                    eprintln!("Tokens: {} prompt + {} completion = {} total",
                        response.usage.prompt_tokens,
                        response.usage.completion_tokens,
                        response.usage.total_tokens
                    );
                    eprintln!("Latency: {}ms", response.latency_ms);
                }
                
                return Ok(());
            }
            Err(e) => {
                eprintln!("Error from Ollama: {}", e);
            }
        }
    }
    
    // Try OpenAI if configured
    let vault_path = AcerConfig::data_dir().join("vault.json");
    if vault_path.exists() {
        let vault = SecretsVault::load(vault_path, None)?;
        
        if let Ok(Some(api_key)) = vault.get(acer_vault::keys::OPENAI_API_KEY) {
            if verbose {
                eprintln!("Using OpenAI (cloud)");
            }
            
            let openai = OpenAIProvider::new(api_key);
            let mut request = request;
            request.model = model.unwrap_or_else(|| "gpt-3.5-turbo".to_string());
            
            match openai.complete(request).await {
                Ok(response) => {
                    println!("{}", response.content);
                    
                    if verbose {
                        eprintln!("\n---");
                        eprintln!("Tokens: {} prompt + {} completion = {} total",
                            response.usage.prompt_tokens,
                            response.usage.completion_tokens,
                            response.usage.total_tokens
                        );
                        eprintln!("Latency: {}ms", response.latency_ms);
                        if let Some(cost) = response.usage.total_tokens.checked_mul(1) {
                            let cost = cost as f64 * 0.001 / 1000.0;
                            eprintln!("Est. cost: ${:.6}", cost);
                        }
                    }
                    
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("Error from OpenAI: {}", e);
                }
            }
        }
    }
    
    eprintln!("No available providers. Run 'acer doctor' for help.");
    std::process::exit(1);
}
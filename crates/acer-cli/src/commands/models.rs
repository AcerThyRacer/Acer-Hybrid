//! Models command - list available models

use acer_core::AcerConfig;
use acer_provider::{ModelRouter, OllamaProvider, OpenAIProvider};
use anyhow::Result;

pub async fn execute(
    provider: Option<String>,
    local: bool,
    json: bool,
) -> Result<()> {
    let config = AcerConfig::load()?;
    let mut all_models = vec![];
    
    // Check Ollama
    if provider.is_none() || provider.as_deref() == Some("ollama") {
        let ollama = OllamaProvider::new(config.providers.ollama.base_url.clone());
        
        if ollama.is_available().await {
            match ollama.list_models().await {
                Ok(models) => {
                    for mut m in models {
                        if local && !m.is_local {
                            continue;
                        }
                        m.provider = acer_core::ProviderType::Ollama;
                        all_models.push(m);
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Could not list Ollama models: {}", e);
                }
            }
        }
    }
    
    // Check OpenAI
    if !local && (provider.is_none() || provider.as_deref() == Some("openai")) {
        let vault_path = AcerConfig::data_dir().join("vault.json");
        if vault_path.exists() {
            let vault = acer_vault::SecretsVault::load(vault_path, None)?;
            if let Ok(Some(api_key)) = vault.get(acer_vault::keys::OPENAI_API_KEY) {
                let openai = OpenAIProvider::new(api_key);
                match openai.list_models().await {
                    Ok(models) => {
                        for mut m in models {
                            m.provider = acer_core::ProviderType::OpenAI;
                            all_models.push(m);
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: Could not list OpenAI models: {}", e);
                    }
                }
            }
        }
    }
    
    if json {
        println!("{}", serde_json::to_string_pretty(&all_models)?);
    } else {
        if all_models.is_empty() {
            println!("No models available.");
            println!("\nTo use local models, ensure Ollama is running: ollama serve");
            println!("To use cloud models, set your API key: acer secrets set openai_api_key");
            return Ok(());
        }
        
        println!("Available Models:");
        println!("{:-<60}", "");
        
        for model in all_models {
            let local_tag = if model.is_local { "[LOCAL]" } else { "[CLOUD]" };
            let cost = model.cost_per_1k_tokens
                .map(|c| format!("${:.4}/1K", c))
                .unwrap_or_else(|| "N/A".to_string());
            
            println!("  {} {}", model.id, local_tag);
            if let Some(ctx) = model.context_window {
                println!("    Context: {} tokens", ctx);
            }
            println!("    Cost: {}", cost);
            println!();
        }
    }
    
    Ok(())
}
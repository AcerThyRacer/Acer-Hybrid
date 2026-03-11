//! Models command - list available models

use crate::runtime::build_router;
use acer_core::AcerConfig;
use anyhow::Result;

pub async fn execute(provider: Option<String>, local: bool, json: bool) -> Result<()> {
    let config = AcerConfig::load()?;
    let router = build_router(&config, provider.as_deref(), true).await?;
    let mut models = router.list_all_models().await?;

    if let Some(provider) = provider.as_deref() {
        models.retain(|model| model.provider.to_string() == provider);
    }
    if local {
        models.retain(|model| model.is_local);
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&models)?);
        return Ok(());
    }

    if models.is_empty() {
        println!("No models available.");
        println!("\nTo use local models, ensure Ollama is running: ollama serve");
        println!("To use cloud models, configure keys with: acer secrets set <provider>_api_key");
        return Ok(());
    }

    println!("Available Models:");
    println!("{:-<60}", "");
    for model in models {
        let local_tag = if model.is_local { "[LOCAL]" } else { "[CLOUD]" };
        let cost = model
            .cost_per_1k_tokens
            .map(|c| format!("${:.4}/1K", c))
            .unwrap_or_else(|| "N/A".to_string());

        println!("  {} {}", model.id, local_tag);
        println!("    Provider: {}", model.provider);
        if let Some(ctx) = model.context_window {
            println!("    Context: {} tokens", ctx);
        }
        println!("    Cost: {}", cost);
        println!();
    }

    Ok(())
}

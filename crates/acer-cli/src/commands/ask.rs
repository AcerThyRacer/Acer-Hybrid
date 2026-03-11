//! Ask command - send a prompt to an AI model

use crate::runtime::{execute_request, model_for_provider, request_from_prompt};
use acer_core::AcerConfig;
use anyhow::Result;

pub async fn execute(
    prompt: String,
    model: Option<String>,
    provider: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<usize>,
    attach: Option<String>,
    cache: bool,
    project: Option<String>,
    verbose: bool,
) -> Result<()> {
    let config = AcerConfig::load()?;
    let selected_provider = provider.as_deref();
    let model_name = model_for_provider(&config, model, selected_provider);
    let request = request_from_prompt(model_name.clone(), prompt, attach, temperature, max_tokens)?;
    let result = execute_request(
        &config,
        request,
        selected_provider,
        cache,
        project.as_deref(),
    )
    .await?;

    let response = &result.response;
    println!("{}", response.content);

    if verbose {
        eprintln!("\n---");
        eprintln!("Model: {}", response.model);
        eprintln!("Provider: {}", response.provider);
        eprintln!(
            "Tokens: {} prompt + {} completion = {} total",
            response.usage.prompt_tokens,
            response.usage.completion_tokens,
            response.usage.total_tokens
        );
        eprintln!("Latency: {}ms", response.latency_ms);
        if let Some(cost) = result.run.cost_usd {
            eprintln!("Est. cost: ${:.6}", cost);
        }
        eprintln!("Run ID: {}", result.run.id);
        if result.cached {
            eprintln!("Cache: HIT");
        }
    }

    Ok(())
}

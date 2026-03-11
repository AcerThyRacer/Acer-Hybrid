//! Stats command - show usage statistics

use acer_core::AcerConfig;
use acer_trace::TraceStore;
use anyhow::Result;
use chrono::{Duration, Utc};

pub async fn execute(period: String, json: bool) -> Result<()> {
    let db_path = AcerConfig::data_dir().join("traces.db");

    if !db_path.exists() {
        println!("No usage data available yet.");
        println!("Run some commands to generate traces.");
        return Ok(());
    }

    let store = TraceStore::new(&db_path).await?;

    // Parse period
    let since = match period.as_str() {
        "1h" => Utc::now() - Duration::hours(1),
        "24h" => Utc::now() - Duration::hours(24),
        "7d" => Utc::now() - Duration::days(7),
        "30d" => Utc::now() - Duration::days(30),
        _ => {
            eprintln!("Invalid period. Use: 1h, 24h, 7d, 30d");
            std::process::exit(1);
        }
    };

    let stats = store.get_stats(since).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&stats)?);
        return Ok(());
    }

    println!("Usage Statistics (last {})", period);
    println!("{}", "=".repeat(40));
    println!();

    println!("Requests:");
    println!("  Total:     {}", stats.total_requests);
    println!("  Successful: {}", stats.successful_requests);
    println!("  Failed:    {}", stats.failed_requests);

    println!("\nTokens:");
    println!("  Total:     {}", stats.total_tokens);
    println!("  Prompt:    {}", stats.prompt_tokens);
    println!("  Completion: {}", stats.completion_tokens);

    println!("\nCost: ${:.4}", stats.total_cost_usd);
    println!("Avg Latency: {:.0}ms", stats.avg_latency_ms);

    if !stats.by_provider.is_empty() {
        println!("\nBy Provider:");
        for (provider, pstats) in stats.by_provider {
            println!("  {}:", provider);
            println!("    Requests: {}", pstats.requests);
            println!("    Tokens:   {}", pstats.tokens);
            println!("    Cost:     ${:.4}", pstats.cost_usd);
        }
    }

    if !stats.by_model.is_empty() {
        println!("\nBy Model:");
        for (model, mstats) in stats.by_model {
            println!("  {}:", model);
            println!("    Requests: {}", mstats.requests);
            println!("    Tokens:   {}", mstats.tokens);
            println!("    Cost:     ${:.4}", mstats.cost_usd);
            println!("    Avg Latency: {:.0}ms", mstats.avg_latency_ms);
        }
    }

    Ok(())
}

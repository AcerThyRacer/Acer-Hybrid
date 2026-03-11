//! Trace command - inspect a specific run

use acer_core::{AcerConfig, RunId};
use acer_trace::TraceStore;
use anyhow::Result;

pub async fn execute(
    run_id: String,
    full: bool,
) -> Result<()> {
    let db_path = AcerConfig::data_dir().join("traces.db");
    
    if !db_path.exists() {
        eprintln!("No traces found.");
        std::process::exit(1);
    }
    
    let store = TraceStore::new(&db_path).await?;
    
    // Parse run ID (allow partial match)
    let id = RunId::new(); // We need to search by partial ID
    
    let runs = store.list_runs(100).await?;
    let run = runs.into_iter()
        .find(|r| r.id.as_str().starts_with(&run_id))
        .ok_or_else(|| anyhow::anyhow!("Run not found: {}", run_id))?;
    
    println!("Run: {}", run.id);
    println!("Time: {}", run.timestamp.format("%Y-%m-%d %H:%M:%S UTC"));
    println!("Model: {} ({})", run.model, run.provider);
    println!("Status: {}", if run.success { "Success" } else { "Failed" });
    println!("Latency: {}ms", run.latency_ms);
    
    if let Some(cost) = run.cost_usd {
        println!("Cost: ${:.6}", cost);
    }
    
    if let Some(ref error) = run.error {
        println!("\nError: {}", error);
    }
    
    if !run.redactions.is_empty() {
        println!("\nRedactions:");
        for r in run.redactions {
            println!("  [{}] {} -> {}", r.pattern_type, r.original, r.replacement);
        }
    }
    
    if full {
        println!("\n--- Request ---");
        for msg in &run.request.messages {
            println!("[{}] {}", msg.role, msg.content);
        }
        
        if let Some(ref response) = run.response {
            println!("\n--- Response ---");
            println!("{}", response.content);
            
            println!("\n--- Usage ---");
            println!("Prompt tokens: {}", response.usage.prompt_tokens);
            println!("Completion tokens: {}", response.usage.completion_tokens);
            println!("Total tokens: {}", response.usage.total_tokens);
        }
    }
    
    Ok(())
}
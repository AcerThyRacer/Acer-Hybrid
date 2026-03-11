//! Trace command - inspect, replay, diff, and export runs

use crate::runtime::{execute_request, trace_store};
use acer_core::AcerConfig;
use anyhow::{anyhow, Result};
use std::path::Path;

pub async fn execute(
    run_id: String,
    full: bool,
    replay: bool,
    diff_with: Option<String>,
    export: Option<String>,
) -> Result<()> {
    let config = AcerConfig::load()?;
    let store = trace_store(&config).await?;
    let run = find_run(&store, &run_id).await?;

    if let Some(path) = export {
        export_trace(&config, &store, &run_id, &path).await?;
    }

    if let Some(other_id) = diff_with {
        let other = find_run(&store, &other_id).await?;
        print_diff(
            &run.response
                .as_ref()
                .map(|r| r.content.as_str())
                .unwrap_or(""),
            &other
                .response
                .as_ref()
                .map(|r| r.content.as_str())
                .unwrap_or(""),
        );
    }

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
        for r in &run.redactions {
            println!("  [{}] {} -> {}", r.pattern_type, r.original, r.replacement);
        }
    }

    if full {
        println!("\n--- Request ---");
        for msg in &run.request.messages {
            println!("[{:?}] {}", msg.role, msg.content);
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

    if replay {
        let replayed = execute_request(&config, run.request.clone(), None, false, None).await?;
        println!("\n--- Replay ---");
        println!("{}", replayed.response.content);
    }

    Ok(())
}

async fn find_run(store: &acer_trace::TraceStore, prefix: &str) -> Result<acer_core::RunRecord> {
    store
        .list_runs(1000)
        .await?
        .into_iter()
        .find(|run| run.id.as_str().starts_with(prefix))
        .ok_or_else(|| anyhow!("Run not found: {}", prefix))
}

async fn export_trace(
    config: &AcerConfig,
    store: &acer_trace::TraceStore,
    run_id: &str,
    path: &str,
) -> Result<()> {
    if path.ends_with(".json") {
        let run = find_run(store, run_id).await?;
        std::fs::write(path, serde_json::to_string_pretty(&run)?)?;
        println!("Exported run to {}", path);
    } else if path.ends_with(".db") {
        let source = config
            .tracing
            .database_path
            .clone()
            .unwrap_or_else(|| AcerConfig::data_dir().join("traces.db"));
        std::fs::copy(source, Path::new(path))?;
        println!("Copied trace database to {}", path);
    } else {
        return Err(anyhow!("Unsupported export path. Use .json or .db"));
    }
    Ok(())
}

fn print_diff(left: &str, right: &str) {
    println!("\n--- Diff ---");
    let left_lines = left.lines().collect::<Vec<_>>();
    let right_lines = right.lines().collect::<Vec<_>>();
    let max = left_lines.len().max(right_lines.len());
    for index in 0..max {
        match (left_lines.get(index), right_lines.get(index)) {
            (Some(a), Some(b)) if a == b => println!("  {}", a),
            (Some(a), Some(b)) => {
                println!("- {}", a);
                println!("+ {}", b);
            }
            (Some(a), None) => println!("- {}", a),
            (None, Some(b)) => println!("+ {}", b),
            (None, None) => {}
        }
    }
}

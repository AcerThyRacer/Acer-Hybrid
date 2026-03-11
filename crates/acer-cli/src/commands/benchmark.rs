//! Benchmark command - compare multiple models

use crate::runtime::execute_request;
use acer_core::{AcerConfig, Message, ModelRequest};
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
struct BenchmarkResult {
    model: String,
    run_id: String,
    latency_ms: u64,
    total_tokens: usize,
    cost_usd: Option<f64>,
    preview: String,
}

#[derive(Serialize)]
struct BenchmarkSummary {
    model: String,
    runs: usize,
    min_latency_ms: u64,
    avg_latency_ms: f64,
    max_latency_ms: u64,
    avg_tokens: f64,
    total_cost_usd: f64,
}

pub async fn execute(prompt: String, models: Vec<String>, repeat: usize, json: bool) -> Result<()> {
    let config = AcerConfig::load()?;
    let mut results = Vec::new();

    for model in models {
        for _ in 0..repeat {
            let request = ModelRequest {
                model: model.clone(),
                messages: vec![Message::user(prompt.clone())],
                temperature: None,
                max_tokens: None,
                stream: None,
            };
            let execution = execute_request(&config, request, None, false, None).await?;
            results.push(BenchmarkResult {
                model: execution.response.model.clone(),
                run_id: execution.run.id.to_string(),
                latency_ms: execution.response.latency_ms,
                total_tokens: execution.response.usage.total_tokens,
                cost_usd: execution.run.cost_usd,
                preview: execution.response.content.chars().take(120).collect(),
            });
        }
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "results": results,
                "summary": summarize(&results),
            }))?
        );
        return Ok(());
    }

    println!("Benchmark Results");
    println!("{:-<80}", "");
    for result in &results {
        println!(
            "{}  latency={}ms tokens={} cost={} run={}",
            result.model,
            result.latency_ms,
            result.total_tokens,
            result
                .cost_usd
                .map(|cost| format!("${:.6}", cost))
                .unwrap_or_else(|| "N/A".to_string()),
            result.run_id
        );
        println!("  {}", result.preview);
    }
    println!("\nSummary");
    println!("{:-<80}", "");
    for summary in summarize(&results) {
        println!(
            "{} runs={} latency(min/avg/max)={}/{:.1}/{}ms avg_tokens={:.1} total_cost=${:.6}",
            summary.model,
            summary.runs,
            summary.min_latency_ms,
            summary.avg_latency_ms,
            summary.max_latency_ms,
            summary.avg_tokens,
            summary.total_cost_usd
        );
    }

    Ok(())
}

fn summarize(results: &[BenchmarkResult]) -> Vec<BenchmarkSummary> {
    let mut grouped: BTreeMap<&str, Vec<&BenchmarkResult>> = BTreeMap::new();
    for result in results {
        grouped.entry(&result.model).or_default().push(result);
    }

    grouped
        .into_iter()
        .map(|(model, entries)| {
            let runs = entries.len();
            let min_latency_ms = entries
                .iter()
                .map(|entry| entry.latency_ms)
                .min()
                .unwrap_or(0);
            let max_latency_ms = entries
                .iter()
                .map(|entry| entry.latency_ms)
                .max()
                .unwrap_or(0);
            let avg_latency_ms = entries.iter().map(|entry| entry.latency_ms).sum::<u64>() as f64
                / runs.max(1) as f64;
            let avg_tokens = entries
                .iter()
                .map(|entry| entry.total_tokens)
                .sum::<usize>() as f64
                / runs.max(1) as f64;
            let total_cost_usd = entries
                .iter()
                .filter_map(|entry| entry.cost_usd)
                .sum::<f64>();

            BenchmarkSummary {
                model: model.to_string(),
                runs,
                min_latency_ms,
                avg_latency_ms,
                max_latency_ms,
                avg_tokens,
                total_cost_usd,
            }
        })
        .collect()
}

//! Logs command - view logs and traces

use acer_core::AcerConfig;
use acer_trace::TraceStore;
use anyhow::Result;

pub async fn execute(
    limit: usize,
    model: Option<String>,
    provider: Option<String>,
    errors: bool,
    json: bool,
) -> Result<()> {
    let db_path = AcerConfig::data_dir().join("traces.db");

    if !db_path.exists() {
        println!("No traces found. Run some commands first.");
        return Ok(());
    }

    let store = TraceStore::new(&db_path).await?;
    let runs = store.list_runs(limit as i64).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&runs)?);
        return Ok(());
    }

    if runs.is_empty() {
        println!("No traces found.");
        return Ok(());
    }

    println!("Recent Runs:");
    println!("{:-<80}", "");

    for run in runs {
        // Apply filters
        if let Some(ref m) = model {
            if !run.model.contains(m) {
                continue;
            }
        }

        if let Some(ref p) = provider {
            if run.provider.to_string() != *p {
                continue;
            }
        }

        if errors && run.success {
            continue;
        }

        let status = if run.success { "✓" } else { "✗" };
        println!(
            "{} {} [{}] {} ({:.0}ms)",
            status,
            run.id.as_str().chars().take(12).collect::<String>(),
            run.provider,
            run.model,
            run.latency_ms as f64
        );

        if let Some(ref error) = run.error {
            println!("  Error: {}", error);
        }

        if let Some(cost) = run.cost_usd {
            println!("  Cost: ${:.6}", cost);
        }

        // Show first message preview
        if let Some(first_msg) = run.request.messages.first() {
            let preview: String = first_msg.content.chars().take(60).collect();
            println!("  Prompt: {}...", preview);
        }

        println!();
    }

    Ok(())
}

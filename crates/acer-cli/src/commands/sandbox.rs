//! Sandbox command - run tools under policy control

use crate::runtime::{policy_engine, trace_store};
use acer_core::{AcerConfig, Message, ModelRequest, ProviderType, RunRecord};
use anyhow::{anyhow, Result};
use std::process::Command;

pub struct CommandOutcome {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

pub async fn execute(cmd: Vec<String>, isolated: bool, project: Option<String>) -> Result<()> {
    if cmd.is_empty() {
        return Err(anyhow!("No command provided"));
    }

    let config = AcerConfig::load()?;
    let outcome = run_command(&config, &cmd.join(" "), isolated, project.as_deref()).await?;

    if !outcome.stdout.is_empty() {
        print!("{}", outcome.stdout);
    }
    if !outcome.stderr.is_empty() {
        eprint!("{}", outcome.stderr);
    }

    if !outcome.success {
        std::process::exit(1);
    }

    Ok(())
}

pub async fn run_command(
    config: &AcerConfig,
    command: &str,
    isolated: bool,
    project: Option<&str>,
) -> Result<CommandOutcome> {
    let tool = command.split_whitespace().next().unwrap_or(command);
    let policy = policy_engine(config, project);
    let decision = policy.validate_tool(tool)?;
    if !decision.allowed {
        return Err(anyhow!(
            "{}",
            decision
                .reason
                .unwrap_or_else(|| "Command blocked by policy".to_string())
        ));
    }

    let temp_dir = if isolated {
        Some(tempfile::tempdir()?)
    } else {
        None
    };
    let workdir = temp_dir
        .as_ref()
        .map(|dir| dir.path().to_path_buf())
        .unwrap_or(std::env::current_dir()?);

    let output = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .current_dir(&workdir)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let store = trace_store(config).await?;
    let request = ModelRequest {
        model: "sandbox/tool".to_string(),
        messages: vec![Message::user(command)],
        temperature: None,
        max_tokens: None,
        stream: None,
    };
    let mut run = RunRecord::new(request);
    run.provider = ProviderType::Custom;
    run.success = output.status.success();
    run.error = if run.success {
        None
    } else {
        Some(format!(
            "Tool command failed with status {:?}",
            output.status.code()
        ))
    };
    run.metadata = serde_json::json!({
        "stdout": stdout,
        "stderr": stderr,
        "workdir": workdir,
        "isolated": isolated,
        "project": project,
        "tool": tool
    });
    store.store_run(&run).await?;

    Ok(CommandOutcome {
        stdout: run
            .metadata
            .get("stdout")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        stderr: run
            .metadata
            .get("stderr")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        success: run.success,
    })
}

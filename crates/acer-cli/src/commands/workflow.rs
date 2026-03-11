//! Workflow command - run AI workflows

use crate::commands::sandbox;
use crate::plugins::{find_plugin, PluginType};
use crate::runtime::{execute_request, model_for_provider};
use acer_core::AcerConfig;
use acer_policy::{RedactionEngine, RedactionPattern};
use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct Workflow {
    name: String,
    #[serde(default)]
    description: Option<String>,
    steps: Vec<WorkflowStep>,
}

#[derive(Debug, Deserialize)]
struct WorkflowStep {
    name: String,
    #[serde(rename = "type")]
    step_type: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    patterns: Vec<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    max_tokens: Option<usize>,
    #[serde(default)]
    rules: Vec<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    isolated: bool,
    #[serde(default)]
    cache: bool,
    #[serde(default)]
    plugin: Option<String>,
}

pub async fn execute(workflow: String, vars: Vec<String>, project: Option<String>) -> Result<()> {
    let path = Path::new(&workflow);
    if !path.exists() {
        return Err(anyhow!("Workflow file not found: {}", workflow));
    }

    let content = std::fs::read_to_string(path)?;
    let workflow = if workflow.ends_with(".yaml") || workflow.ends_with(".yml") {
        serde_yaml::from_str::<Workflow>(&content)?
    } else {
        toml::from_str::<Workflow>(&content)?
    };

    let config = AcerConfig::load()?;
    let mut context = parse_vars(vars);

    println!("Running workflow: {}", workflow.name);
    if let Some(description) = workflow.description.as_deref() {
        println!("{}", description);
    }

    for step in workflow.steps {
        println!("\n==> {}", step.name);
        match step.step_type.as_str() {
            "command" => {
                let command = interpolate(step.command.as_deref().unwrap_or_default(), &context);
                let result =
                    sandbox::run_command(&config, &command, step.isolated, project.as_deref())
                        .await?;
                let combined = if result.stderr.is_empty() {
                    result.stdout
                } else if result.stdout.is_empty() {
                    result.stderr
                } else {
                    format!("{}\n{}", result.stdout, result.stderr)
                };
                context.insert(step.name, combined);
            }
            "redact" => {
                let mut engine = RedactionEngine::new();
                for pattern in &step.patterns {
                    engine.add_pattern(RedactionPattern::new(
                        pattern,
                        pattern,
                        &format!("[REDACTED_{}]", pattern.to_uppercase()),
                    )?)?;
                }
                let source = context.values().last().cloned().unwrap_or_default();
                let (redacted, _) = engine.redact(&source);
                println!("{}", redacted);
                context.insert(step.name, redacted);
            }
            "llm" => {
                let provider = step.provider.as_deref();
                let model = model_for_provider(&config, step.model.clone(), provider);
                let prompt = interpolate(step.prompt.as_deref().unwrap_or_default(), &context);
                let request = acer_core::ModelRequest {
                    model,
                    messages: vec![acer_core::Message::user(prompt)],
                    temperature: step.temperature,
                    max_tokens: step.max_tokens,
                    stream: None,
                };
                let result =
                    execute_request(&config, request, provider, step.cache, project.as_deref())
                        .await?;
                println!("{}", result.response.content);
                context.insert(step.name, result.response.content);
            }
            "validate" => {
                let candidate = context.values().last().cloned().unwrap_or_default();
                validate_output(&candidate, &step.rules)?;
                context.insert(step.name, candidate);
            }
            "output" => {
                let path = interpolate(
                    step.path.as_deref().unwrap_or("workflow-output.txt"),
                    &context,
                );
                let body = context.values().last().cloned().unwrap_or_default();
                let rendered = match step.format.as_deref() {
                    Some("json") => serde_json::to_string_pretty(&context)?,
                    Some("markdown") => format!("# {}\n\n{}", workflow.name, body),
                    _ => body,
                };
                std::fs::write(&path, rendered)?;
                println!("Saved {}", path);
                context.insert(step.name, path);
            }
            "plugin" => {
                let plugin_name = step
                    .plugin
                    .as_deref()
                    .ok_or_else(|| anyhow!("Plugin step requires `plugin`"))?;
                let plugin = find_plugin(plugin_name)?
                    .ok_or_else(|| anyhow!("Plugin not found: {}", plugin_name))?;
                if plugin.plugin_type != PluginType::Workflow {
                    return Err(anyhow!("Plugin '{}' is not a workflow plugin", plugin_name));
                }
                let workflow_plugin = plugin.workflow.ok_or_else(|| {
                    anyhow!("Workflow plugin '{}' is missing config", plugin_name)
                })?;
                let command = interpolate(&workflow_plugin.command, &context);
                let result =
                    sandbox::run_command(&config, &command, step.isolated, project.as_deref())
                        .await?;
                let combined = if result.stderr.is_empty() {
                    result.stdout
                } else if result.stdout.is_empty() {
                    result.stderr
                } else {
                    format!("{}\n{}", result.stdout, result.stderr)
                };
                context.insert(step.name, combined);
            }
            other => return Err(anyhow!("Unsupported workflow step type: {}", other)),
        }
    }

    Ok(())
}

fn parse_vars(vars: Vec<String>) -> HashMap<String, String> {
    vars.into_iter()
        .filter_map(|var| {
            let (key, value) = var.split_once('=')?;
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

fn interpolate(template: &str, values: &HashMap<String, String>) -> String {
    let mut rendered = template.to_string();
    for (key, value) in values {
        rendered = rendered.replace(&format!("{{{}}}", key), value);
    }
    rendered
}

fn validate_output(candidate: &str, rules: &[String]) -> Result<()> {
    for rule in rules {
        match rule.as_str() {
            "non_empty_output" => {
                if candidate.trim().is_empty() {
                    return Err(anyhow!("Validation failed: output is empty"));
                }
            }
            "no_secrets_in_output" => {
                if RedactionEngine::new().contains_sensitive(candidate) {
                    return Err(anyhow!("Validation failed: output contains sensitive data"));
                }
            }
            _ if rule.starts_with("max_length:") => {
                let max_len: usize = rule
                    .split_once(':')
                    .and_then(|(_, value)| value.parse().ok())
                    .ok_or_else(|| anyhow!("Invalid max_length rule: {}", rule))?;
                if candidate.len() > max_len {
                    return Err(anyhow!("Validation failed: output exceeds {}", max_len));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

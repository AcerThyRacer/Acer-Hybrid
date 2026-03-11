//! Policy command - manage policies

use acer_core::AcerConfig;
use acer_policy::{PolicyEngine, PolicyRules};
use anyhow::Result;

use crate::PolicyCommands;

pub async fn execute(command: PolicyCommands) -> Result<()> {
    let config = AcerConfig::load()?;
    
    match command {
        PolicyCommands::Show { project } => {
            let engine = PolicyEngine::new();
            
            if let Some(proj) = project {
                println!("Policy for project '{}':", proj);
            } else {
                println!("Default policy:");
            }
            
            let rules = engine.current_rules();
            
            println!("\n  max_cost_usd: ${:.2}", rules.max_cost_usd);
            println!("  allow_remote: {}", rules.allow_remote);
            println!("  redact_pii: {}", rules.redact_pii);
            println!("  require_confirmation: {}", rules.require_confirmation);
            
            if !rules.allow_tools.is_empty() {
                println!("  allow_tools: {:?}", rules.allow_tools);
            }
            
            if !rules.block_patterns.is_empty() {
                println!("  block_patterns:");
                for p in &rules.block_patterns {
                    println!("    - {}", p);
                }
            }
            
            if !rules.allowed_models.is_empty() {
                println!("  allowed_models: {:?}", rules.allowed_models);
            }
            
            if !rules.blocked_models.is_empty() {
                println!("  blocked_models: {:?}", rules.blocked_models);
            }
        }
        
        PolicyCommands::Check { prompt } => {
            let engine = PolicyEngine::new();
            let request = acer_core::ModelRequest {
                model: "default".to_string(),
                messages: vec![acer_core::Message::user(&prompt)],
                temperature: None,
                max_tokens: None,
                stream: None,
            };
            
            let decision = engine.validate(&request)?;
            
            if decision.allowed {
                println!("✓ Prompt allowed");
            } else {
                println!("✗ Prompt blocked");
                if let Some(reason) = decision.reason {
                    println!("  Reason: {}", reason);
                }
            }
            
            if !decision.redactions.is_empty() {
                println!("\nRedactions:");
                for r in decision.redactions {
                    println!("  - {} ({}): '{}' -> '{}'", 
                        r.pattern_type, r.position, r.original, r.replacement);
                }
            }
        }
        
        PolicyCommands::Set { rule, value, project } => {
            let config_path = AcerConfig::config_path();
            let mut config = AcerConfig::load()?;
            
            let rules = if let Some(proj) = &project {
                config.policy.projects.entry(proj.clone()).or_default()
            } else {
                &mut config.policy.default
            };
            
            // Parse and set the rule
            match rule.as_str() {
                "max_cost_usd" => {
                    rules.max_cost_usd = value.parse()
                        .map_err(|e| anyhow::anyhow!("Invalid number: {}", e))?;
                }
                "allow_remote" => {
                    rules.allow_remote = value.parse()
                        .map_err(|e| anyhow::anyhow!("Invalid boolean: {}", e))?;
                }
                "redact_pii" => {
                    rules.redact_pii = value.parse()
                        .map_err(|e| anyhow::anyhow!("Invalid boolean: {}", e))?;
                }
                "require_confirmation" => {
                    rules.require_confirmation = value.parse()
                        .map_err(|e| anyhow::anyhow!("Invalid boolean: {}", e))?;
                }
                "default_model" => {
                    rules.default_model = Some(value);
                }
                _ => {
                    eprintln!("Unknown rule: {}", rule);
                    eprintln!("Available rules: max_cost_usd, allow_remote, redact_pii, require_confirmation, default_model");
                    std::process::exit(1);
                }
            }
            
            config.save()?;
            println!("Rule '{}' set to '{}' for {}", 
                rule, value, 
                project.map(|p| format!("project '{}'", p)).unwrap_or_else(|| "default".to_string())
            );
        }
        
        PolicyCommands::Test { prompt } => {
            let engine = PolicyEngine::new();
            let request = acer_core::ModelRequest {
                model: "default".to_string(),
                messages: vec![acer_core::Message::user(&prompt)],
                temperature: None,
                max_tokens: None,
                stream: None,
            };
            
            println!("Policy Simulation");
            println!("==================");
            println!("\nPrompt: \"{}\"", prompt);
            
            let decision = engine.simulate(&request);
            
            println!("\nDecision: {}", if decision.allowed { "ALLOW" } else { "DENY" });
            
            if let Some(ref reason) = decision.reason {
                println!("Reason: {}", reason);
            }
            
            if let Some(cost_limit) = decision.cost_limit {
                println!("Cost limit: ${:.2}", cost_limit);
            }
            
            if !decision.redactions.is_empty() {
                println!("\nRedactions applied:");
                for r in decision.redactions {
                    println!("  [{}] '{}' -> '{}'", r.pattern_type, r.original, r.replacement);
                }
            }
        }
    }
    
    Ok(())
}
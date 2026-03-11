//! Config command - manage configuration

use acer_core::AcerConfig;
use anyhow::Result;

use crate::ConfigCommands;

pub async fn execute(command: ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::Show => {
            let config = AcerConfig::load()?;
            println!("{}", toml::to_string_pretty(&config)?);
        }

        ConfigCommands::Set { key, value } => {
            let mut config = AcerConfig::load()?;

            // Parse key path and set value
            set_config_value(&mut config, &key, &value)?;

            config.save()?;
            println!("Set {} = {}", key, value);
        }

        ConfigCommands::Get { key } => {
            let config = AcerConfig::load()?;

            match get_config_value(&config, &key) {
                Some(value) => println!("{}", value),
                None => {
                    eprintln!("Key not found: {}", key);
                    std::process::exit(1);
                }
            }
        }

        ConfigCommands::Edit => {
            let config_path = AcerConfig::config_path();

            let editor = std::env::var("EDITOR")
                .or_else(|_| std::env::var("VISUAL"))
                .unwrap_or_else(|_| "nano".to_string());

            let status = std::process::Command::new(&editor)
                .arg(&config_path)
                .status()
                .map_err(|e| anyhow::anyhow!("Failed to open editor: {}", e))?;

            if status.success() {
                println!("Configuration updated.");
            } else {
                eprintln!("Editor exited with error.");
            }
        }

        ConfigCommands::Reset { force } => {
            if !force {
                println!("This will reset all configuration to defaults.");
                println!("Use --force to confirm.");
                return Ok(());
            }

            let config = AcerConfig::default();
            config.save()?;

            println!("Configuration reset to defaults.");
        }
    }

    Ok(())
}

fn set_config_value(config: &mut AcerConfig, key: &str, value: &str) -> Result<()> {
    let parts: Vec<&str> = key.split('.').collect();

    match parts.as_slice() {
        ["providers", "ollama", "base_url"] => {
            config.providers.ollama.base_url = value.to_string();
        }
        ["providers", "ollama", "enabled"] => {
            config.providers.ollama.enabled = value
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid boolean: {}", e))?;
        }
        ["providers", "ollama", "default_model"] => {
            config.providers.ollama.default_model = Some(value.to_string());
        }
        ["providers", "openai", "enabled"] => {
            config.providers.openai.enabled = value
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid boolean: {}", e))?;
        }
        ["providers", "openai", "default_model"] => {
            config.providers.openai.default_model = Some(value.to_string());
        }
        ["providers", "anthropic", "enabled"] => {
            config.providers.anthropic.enabled = value
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid boolean: {}", e))?;
        }
        ["providers", "anthropic", "default_model"] => {
            config.providers.anthropic.default_model = Some(value.to_string());
        }
        ["providers", "gemini", "enabled"] => {
            config.providers.gemini.enabled = value
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid boolean: {}", e))?;
        }
        ["providers", "gemini", "default_model"] => {
            config.providers.gemini.default_model = Some(value.to_string());
        }
        ["providers", "default_provider"] => {
            config.providers.default_provider = Some(value.to_string());
        }
        ["gateway", "host"] => {
            config.gateway.host = value.to_string();
        }
        ["gateway", "port"] => {
            config.gateway.port = value
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid port: {}", e))?;
        }
        ["policy", "default", "max_cost_usd"] => {
            config.policy.default.max_cost_usd = value
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid number: {}", e))?;
        }
        ["policy", "default", "allow_remote"] => {
            config.policy.default.allow_remote = value
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid boolean: {}", e))?;
        }
        ["policy", "default", "redact_pii"] => {
            config.policy.default.redact_pii = value
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid boolean: {}", e))?;
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown config key: {}", key));
        }
    }

    Ok(())
}

fn get_config_value(config: &AcerConfig, key: &str) -> Option<String> {
    let parts: Vec<&str> = key.split('.').collect();

    match parts.as_slice() {
        ["providers", "ollama", "base_url"] => Some(config.providers.ollama.base_url.clone()),
        ["providers", "ollama", "enabled"] => Some(config.providers.ollama.enabled.to_string()),
        ["providers", "ollama", "default_model"] => config.providers.ollama.default_model.clone(),
        ["providers", "openai", "enabled"] => Some(config.providers.openai.enabled.to_string()),
        ["providers", "openai", "default_model"] => config.providers.openai.default_model.clone(),
        ["providers", "anthropic", "enabled"] => {
            Some(config.providers.anthropic.enabled.to_string())
        }
        ["providers", "anthropic", "default_model"] => {
            config.providers.anthropic.default_model.clone()
        }
        ["providers", "gemini", "enabled"] => Some(config.providers.gemini.enabled.to_string()),
        ["providers", "gemini", "default_model"] => config.providers.gemini.default_model.clone(),
        ["providers", "default_provider"] => config.providers.default_provider.clone(),
        ["gateway", "host"] => Some(config.gateway.host.clone()),
        ["gateway", "port"] => Some(config.gateway.port.to_string()),
        ["policy", "default", "max_cost_usd"] => {
            Some(config.policy.default.max_cost_usd.to_string())
        }
        ["policy", "default", "allow_remote"] => {
            Some(config.policy.default.allow_remote.to_string())
        }
        ["policy", "default", "redact_pii"] => Some(config.policy.default.redact_pii.to_string()),
        _ => None,
    }
}

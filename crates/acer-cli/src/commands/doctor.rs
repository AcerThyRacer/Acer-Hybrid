//! Doctor command - check system health

use acer_core::AcerConfig;
use acer_provider::{OllamaProvider, Provider};
use anyhow::Result;

pub async fn execute(fix: bool) -> Result<()> {
    println!("Acer Hybrid Health Check");
    println!("========================\n");

    let mut issues = Vec::new();

    // Check config
    print!("Configuration... ");
    let config = match AcerConfig::load() {
        Ok(c) => {
            println!("✓ OK");
            c
        }
        Err(e) => {
            println!("✗ MISSING");
            issues.push(format!("Configuration error: {}", e));
            AcerConfig::default()
        }
    };

    // Check data directory
    print!("Data directory... ");
    let data_dir = AcerConfig::data_dir();
    if data_dir.exists() {
        println!("✓ OK ({:?})", data_dir);
    } else {
        println!("✗ MISSING");
        issues.push("Data directory does not exist".to_string());

        if fix {
            std::fs::create_dir_all(&data_dir)?;
            println!("  Created: {:?}", data_dir);
        }
    }

    print!("Plugins directory... ");
    let plugins_dir = AcerConfig::plugins_dir();
    if plugins_dir.exists() {
        println!("✓ OK ({:?})", plugins_dir);
    } else {
        println!("✗ MISSING");
        issues.push("Plugins directory does not exist".to_string());
        if fix {
            std::fs::create_dir_all(&plugins_dir)?;
            println!("  Created: {:?}", plugins_dir);
        }
    }

    print!("Policy packs... ");
    let policy_packs_dir = AcerConfig::policy_packs_dir();
    if policy_packs_dir.exists() {
        println!("✓ OK ({:?})", policy_packs_dir);
    } else {
        println!("✗ MISSING");
        issues.push("Policy packs directory does not exist".to_string());
        if fix {
            std::fs::create_dir_all(&policy_packs_dir)?;
            println!("  Created: {:?}", policy_packs_dir);
        }
    }

    // Check vault
    print!("Secrets vault... ");
    let vault_path = AcerConfig::data_dir().join("vault.json");
    if vault_path.exists() {
        println!("✓ OK");
    } else {
        println!("✗ NOT INITIALIZED");
        issues.push("Secrets vault not initialized".to_string());
    }

    // Check Ollama
    print!("Ollama (local)... ");
    let ollama = OllamaProvider::new(config.providers.ollama.base_url.clone());
    if ollama.is_available().await {
        println!("✓ OK");

        // List models
        match ollama.list_models().await {
            Ok(models) => {
                println!("  {} model(s) available", models.len());
            }
            Err(e) => {
                println!("  Warning: Could not list models: {}", e);
            }
        }
    } else {
        println!("✗ NOT RUNNING");
        issues.push("Ollama is not running. Start with: ollama serve".to_string());
    }

    // Check OpenAI
    print!("OpenAI (cloud)... ");
    if vault_path.exists() {
        let vault = acer_vault::SecretsVault::load(vault_path.clone(), None)?;
        if vault.exists(acer_vault::keys::OPENAI_API_KEY) {
            println!("✓ CONFIGURED");
        } else {
            println!("✗ NO API KEY");
            issues.push("OpenAI API key not set. Use: acer secrets set openai_api_key".to_string());
        }
    } else {
        println!("✗ NOT CONFIGURED");
    }

    // Check trace database
    print!("Trace database... ");
    let db_path = AcerConfig::data_dir().join("traces.db");
    if db_path.exists() {
        println!("✓ OK");
    } else {
        println!("✗ NOT CREATED");
        if fix {
            println!("  Will be created on first use");
        }
    }

    // Summary
    println!();
    if issues.is_empty() {
        println!("All checks passed! ✓");
    } else {
        println!("Issues found ({}):", issues.len());
        for issue in &issues {
            println!("  - {}", issue);
        }

        if !fix {
            println!("\nRun with --fix to attempt automatic fixes.");
        }
    }

    Ok(())
}

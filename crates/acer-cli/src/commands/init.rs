//! Init command - initialize configuration

use acer_core::AcerConfig;
use anyhow::Result;

pub async fn execute(force: bool) -> Result<()> {
    let config_path = AcerConfig::config_path();
    let data_dir = AcerConfig::data_dir();
    let plugins_dir = AcerConfig::plugins_dir();
    let policy_packs_dir = AcerConfig::policy_packs_dir();

    // Check if already initialized
    if config_path.exists() && !force {
        println!("Already initialized.");
        println!("Use --force to overwrite.");
        return Ok(());
    }

    // Create directories
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(&plugins_dir)?;
    std::fs::create_dir_all(&policy_packs_dir)?;

    // Create default config
    let config = AcerConfig::default();
    config.save()?;

    let starter_pack = policy_packs_dir.join("team-default.toml");
    if !starter_pack.exists() {
        std::fs::write(
            &starter_pack,
            "[default]\nmax_cost_usd = 0.05\nallow_remote = true\nredact_pii = true\nrequire_confirmation = false\n",
        )?;
    }

    println!("Acer Hybrid initialized!");
    println!();
    println!("Configuration: {:?}", config_path);
    println!("Data directory: {:?}", data_dir);
    println!("Plugins directory: {:?}", plugins_dir);
    println!("Policy packs: {:?}", policy_packs_dir);
    println!();
    println!("Next steps:");
    println!("  1. Set up local models: ollama pull llama2");
    println!("  2. Or configure cloud API: acer secrets set openai_api_key");
    println!("  3. Run health check: acer doctor");

    Ok(())
}

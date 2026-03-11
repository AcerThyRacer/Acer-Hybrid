//! Plugin command - list and scaffold plugin manifests

use crate::plugins::load_plugins;
use crate::PluginCommands;
use acer_core::AcerConfig;
use anyhow::Result;

pub async fn execute(command: PluginCommands) -> Result<()> {
    match command {
        PluginCommands::List => {
            let plugins = load_plugins()?;
            if plugins.is_empty() {
                println!("No plugins installed.");
                return Ok(());
            }
            for plugin in plugins {
                println!(
                    "{} [{}]{}",
                    plugin.name,
                    match plugin.plugin_type {
                        crate::plugins::PluginType::Provider => "provider",
                        crate::plugins::PluginType::Workflow => "workflow",
                    },
                    plugin
                        .description
                        .as_deref()
                        .map(|d| format!(" - {}", d))
                        .unwrap_or_default()
                );
            }
        }
        PluginCommands::Scaffold { name, plugin_type } => {
            let dir = AcerConfig::plugins_dir();
            std::fs::create_dir_all(&dir)?;
            let path = dir.join(format!("{}.toml", name));
            let content = match plugin_type.as_str() {
                "workflow" => format!(
                    "name = \"{name}\"\ntype = \"workflow\"\ndescription = \"Workflow plugin\"\n\n[workflow]\ncommand = \"python ./plugins/{name}.py\"\n"
                ),
                _ => format!(
                    "name = \"{name}\"\ntype = \"provider\"\ndescription = \"OpenAI-compatible provider\"\n\n[provider]\nbase_url = \"http://localhost:9000/v1\"\napi_key_env = \"{upper}_API_KEY\"\n",
                    upper = name.to_uppercase().replace('-', "_")
                ),
            };
            std::fs::write(&path, content)?;
            println!("Created {}", path.display());
        }
    }

    Ok(())
}

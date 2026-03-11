use acer_core::AcerConfig;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub plugin_type: PluginType,
    #[serde(default)]
    pub provider: Option<ProviderPlugin>,
    #[serde(default)]
    pub workflow: Option<WorkflowPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PluginType {
    Provider,
    Workflow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPlugin {
    pub base_url: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub vault_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPlugin {
    pub command: String,
}

impl PluginManifest {
    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(anyhow!("Plugin name cannot be empty"));
        }

        match self.plugin_type {
            PluginType::Provider => {
                let provider = self.provider.as_ref().ok_or_else(|| {
                    anyhow!("Provider plugin '{}' is missing [provider]", self.name)
                })?;
                if provider.base_url.trim().is_empty() {
                    return Err(anyhow!(
                        "Provider plugin '{}' has an empty base_url",
                        self.name
                    ));
                }
            }
            PluginType::Workflow => {
                let workflow = self.workflow.as_ref().ok_or_else(|| {
                    anyhow!("Workflow plugin '{}' is missing [workflow]", self.name)
                })?;
                if workflow.command.trim().is_empty() {
                    return Err(anyhow!(
                        "Workflow plugin '{}' has an empty command",
                        self.name
                    ));
                }
            }
        }

        Ok(())
    }
}

pub fn load_plugins() -> Result<Vec<PluginManifest>> {
    let dir = AcerConfig::plugins_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut plugins = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        let content = std::fs::read_to_string(&path)?;
        let manifest: PluginManifest = toml::from_str(&content)?;
        manifest.validate()?;
        plugins.push(manifest);
    }
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(plugins)
}

pub fn find_plugin(name: &str) -> Result<Option<PluginManifest>> {
    Ok(load_plugins()?
        .into_iter()
        .find(|plugin| plugin.name == name))
}

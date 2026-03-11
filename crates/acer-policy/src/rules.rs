//! Policy rules and configuration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Policy configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicyConfig {
    #[serde(default)]
    pub default: PolicyRules,
    #[serde(default)]
    pub projects: HashMap<String, PolicyRules>,
    #[serde(default)]
    pub profiles: HashMap<String, PolicyRules>,
}

/// Policy rules for a project or profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRules {
    /// Maximum cost per request in USD
    #[serde(default = "default_max_cost")]
    pub max_cost_usd: f64,
    
    /// Allow remote (cloud) providers
    #[serde(default = "default_true")]
    pub allow_remote: bool,
    
    /// Redact PII before sending to models
    #[serde(default = "default_true")]
    pub redact_pii: bool,
    
    /// Allowed tools for agent mode
    #[serde(default)]
    pub allow_tools: Vec<String>,
    
    /// Blocked patterns (regex)
    #[serde(default)]
    pub block_patterns: Vec<String>,
    
    /// Require confirmation for dangerous actions
    #[serde(default)]
    pub require_confirmation: bool,
    
    /// Allowed models (empty = all allowed)
    #[serde(default)]
    pub allowed_models: Vec<String>,
    
    /// Blocked models
    #[serde(default)]
    pub blocked_models: Vec<String>,
    
    /// Maximum tokens per request
    #[serde(default)]
    pub max_tokens: Option<usize>,
    
    /// Default model to use
    #[serde(default)]
    pub default_model: Option<String>,
    
    /// Enable audit logging
    #[serde(default = "default_true")]
    pub audit_logging: bool,
    
    /// Custom metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

fn default_max_cost() -> f64 { 0.10 }
fn default_true() -> bool { true }

impl Default for PolicyRules {
    fn default() -> Self {
        Self {
            max_cost_usd: default_max_cost(),
            allow_remote: true,
            redact_pii: true,
            allow_tools: Vec::new(),
            block_patterns: Vec::new(),
            require_confirmation: false,
            allowed_models: Vec::new(),
            blocked_models: Vec::new(),
            max_tokens: None,
            default_model: None,
            audit_logging: true,
            metadata: HashMap::new(),
        }
    }
}

impl PolicyRules {
    /// Load from a TOML file
    pub fn from_file(path: &std::path::Path) -> acer_core::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let rules: Self = toml::from_str(&content)?;
        Ok(rules)
    }

    /// Save to a TOML file
    pub fn to_file(&self, path: &std::path::Path) -> acer_core::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Merge with another set of rules (other takes precedence)
    pub fn merge(&self, other: &PolicyRules) -> Self {
        Self {
            max_cost_usd: if other.max_cost_usd != default_max_cost() {
                other.max_cost_usd
            } else {
                self.max_cost_usd
            },
            allow_remote: other.allow_remote,
            redact_pii: other.redact_pii,
            allow_tools: if !other.allow_tools.is_empty() {
                other.allow_tools.clone()
            } else {
                self.allow_tools.clone()
            },
            block_patterns: if !other.block_patterns.is_empty() {
                other.block_patterns.clone()
            } else {
                self.block_patterns.clone()
            },
            require_confirmation: other.require_confirmation,
            allowed_models: if !other.allowed_models.is_empty() {
                other.allowed_models.clone()
            } else {
                self.allowed_models.clone()
            },
            blocked_models: if !other.blocked_models.is_empty() {
                other.blocked_models.clone()
            } else {
                self.blocked_models.clone()
            },
            max_tokens: other.max_tokens.or(self.max_tokens),
            default_model: other.default_model.clone().or(self.default_model.clone()),
            audit_logging: other.audit_logging,
            metadata: {
                let mut m = self.metadata.clone();
                m.extend(other.metadata.clone());
                m
            },
        }
    }
}
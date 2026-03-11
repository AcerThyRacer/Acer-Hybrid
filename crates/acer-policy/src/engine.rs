//! Policy engine for request validation and enforcement

use crate::{PolicyRules, PolicyConfig, RedactionEngine};
use acer_core::{AcerError, ModelRequest, PolicyDecision, Redaction, Result};
use std::collections::HashMap;

/// Policy engine
pub struct PolicyEngine {
    config: PolicyConfig,
    redaction_engine: RedactionEngine,
    current_project: Option<String>,
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyEngine {
    pub fn new() -> Self {
        Self {
            config: PolicyConfig::default(),
            redaction_engine: RedactionEngine::new(),
            current_project: None,
        }
    }

    /// Create with custom config
    pub fn with_config(config: PolicyConfig) -> Self {
        Self {
            config,
            redaction_engine: RedactionEngine::new(),
            current_project: None,
        }
    }

    /// Load policy from file
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: PolicyConfig = toml::from_str(&content)?;
        Ok(Self::with_config(config))
    }

    /// Set current project
    pub fn set_project(&mut self, project: &str) {
        self.current_project = Some(project.to_string());
    }

    /// Get current rules (merged with project rules if applicable)
    pub fn current_rules(&self) -> PolicyRules {
        match &self.current_project {
            Some(project) => {
                let project_rules = self.config.projects.get(project)
                    .cloned()
                    .unwrap_or_default();
                self.config.default.merge(&project_rules)
            }
            None => self.config.default.clone(),
        }
    }

    /// Validate a request against policy
    pub fn validate(&self, request: &ModelRequest) -> Result<PolicyDecision> {
        let rules = self.current_rules();
        let mut redactions = Vec::new();
        let mut allowed = true;
        let mut reason = None;

        // Check model allowlist/blocklist
        if !rules.allowed_models.is_empty() {
            if !rules.allowed_models.contains(&request.model) {
                allowed = false;
                reason = Some(format!("Model '{}' is not in allowed list", request.model));
            }
        }

        if rules.blocked_models.contains(&request.model) {
            allowed = false;
            reason = Some(format!("Model '{}' is blocked", request.model));
        }

        // Check for blocked patterns in messages
        for message in &request.messages {
            for pattern in &rules.block_patterns {
                let re = regex::Regex::new(pattern)
                    .map_err(|e| AcerError::PolicyViolation(format!("Invalid pattern: {}", e)))?;
                
                if re.is_match(&message.content) {
                    allowed = false;
                    reason = Some("Content matches blocked pattern".to_string());
                    break;
                }
            }
        }

        // Apply redaction if enabled
        if rules.redact_pii && allowed {
            for message in &request.messages {
                let (_, msg_redactions) = self.redaction_engine.redact(&message.content);
                redactions.extend(msg_redactions);
            }
        }

        // Check max tokens
        if let Some(max_tokens) = rules.max_tokens {
            if request.max_tokens.map(|t| t > max_tokens).unwrap_or(false) {
                allowed = false;
                reason = Some(format!("Max tokens {} exceeds limit {}", 
                    request.max_tokens.unwrap(), max_tokens));
            }
        }

        Ok(PolicyDecision {
            allowed,
            reason,
            redactions,
            model_override: rules.default_model.clone(),
            cost_limit: Some(rules.max_cost_usd),
        })
    }

    /// Validate a tool/command
    pub fn validate_tool(&self, tool: &str) -> Result<PolicyDecision> {
        let rules = self.current_rules();

        if rules.allow_tools.is_empty() {
            // No restrictions
            return Ok(PolicyDecision {
                allowed: true,
                reason: None,
                redactions: Vec::new(),
                model_override: None,
                cost_limit: Some(rules.max_cost_usd),
            });
        }

        let allowed = rules.allow_tools.iter().any(|t| {
            tool == t || tool.starts_with(&format!("{} ", t))
        });

        Ok(PolicyDecision {
            allowed,
            reason: if !allowed {
                Some(format!("Tool '{}' is not allowed", tool))
            } else {
                None
            },
            redactions: Vec::new(),
            model_override: None,
            cost_limit: Some(rules.max_cost_usd),
        })
    }

    /// Check if remote providers are allowed
    pub fn allow_remote(&self) -> bool {
        self.current_rules().allow_remote
    }

    /// Get max cost limit
    pub fn max_cost(&self) -> f64 {
        self.current_rules().max_cost_usd
    }

    /// Check if confirmation is required
    pub fn requires_confirmation(&self) -> bool {
        self.current_rules().require_confirmation
    }

    /// Redact sensitive data from text
    pub fn redact(&self, text: &str) -> (String, Vec<Redaction>) {
        self.redaction_engine.redact(text)
    }

    /// Add custom redaction patterns
    pub fn add_redaction_patterns(&mut self, patterns: &[String]) -> Result<()> {
        self.redaction_engine.add_block_patterns(patterns)
    }

    /// Simulate policy check (dry run)
    pub fn simulate(&self, request: &ModelRequest) -> PolicyDecision {
        self.validate(request).unwrap_or(PolicyDecision {
            allowed: false,
            reason: Some("Policy validation failed".to_string()),
            redactions: Vec::new(),
            model_override: None,
            cost_limit: None,
        })
    }

    /// Update default rules
    pub fn update_default_rules(&mut self, rules: PolicyRules) {
        self.config.default = rules;
    }

    /// Add project rules
    pub fn add_project_rules(&mut self, project: &str, rules: PolicyRules) {
        self.config.projects.insert(project.to_string(), rules);
    }

    /// List all projects with custom rules
    pub fn list_projects(&self) -> Vec<&String> {
        self.config.projects.keys().collect()
    }
}
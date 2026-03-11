//! Policy engine for request validation and enforcement

use crate::{PolicyConfig, PolicyRules, RedactionEngine};
use acer_core::{AcerError, ModelRequest, PolicyDecision, Redaction, Result};

/// Policy engine
pub struct PolicyEngine {
    config: PolicyConfig,
    redaction_engine: RedactionEngine,
    current_project: Option<String>,
    current_profile: Option<String>,
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
            current_profile: None,
        }
    }

    /// Create with custom config
    pub fn with_config(config: PolicyConfig) -> Self {
        Self {
            config,
            redaction_engine: RedactionEngine::new(),
            current_project: None,
            current_profile: None,
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

    /// Set active profile
    pub fn set_profile(&mut self, profile: &str) {
        self.current_profile = Some(profile.to_string());
    }

    /// Get current rules (merged with project rules if applicable)
    pub fn current_rules(&self) -> PolicyRules {
        let mut merged = self.config.default.clone();
        if let Some(profile) = &self.current_profile {
            if let Some(profile_rules) = self.config.profiles.get(profile) {
                merged = merged.merge(profile_rules);
            }
        }
        if let Some(project) = &self.current_project {
            if let Some(project_rules) = self.config.projects.get(project) {
                merged = merged.merge(project_rules);
            }
        }
        merged
    }

    /// Validate a request against policy
    pub fn validate(&self, request: &ModelRequest) -> Result<PolicyDecision> {
        Ok(self.prepare_request(request)?.1)
    }

    /// Validate and return the policy-adjusted request.
    pub fn prepare_request(
        &self,
        request: &ModelRequest,
    ) -> Result<(ModelRequest, PolicyDecision)> {
        let rules = self.current_rules();
        let mut redactions = Vec::new();
        let mut allowed = true;
        let mut reason = None;
        let mut prepared = request.clone();

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
            for message in &mut prepared.messages {
                let (redacted, msg_redactions) = self.redaction_engine.redact(&message.content);
                message.content = redacted;
                redactions.extend(msg_redactions);
            }
        }

        // Check max tokens
        if let Some(max_tokens) = rules.max_tokens {
            if request.max_tokens.map(|t| t > max_tokens).unwrap_or(false) {
                allowed = false;
                reason = Some(format!(
                    "Max tokens {} exceeds limit {}",
                    request.max_tokens.unwrap(),
                    max_tokens
                ));
            }
        }

        let decision = PolicyDecision {
            allowed,
            reason,
            redactions,
            model_override: rules.default_model.clone(),
            cost_limit: Some(rules.max_cost_usd),
        };

        Ok((prepared, decision))
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

        let allowed = rules
            .allow_tools
            .iter()
            .any(|t| tool == t || tool.starts_with(&format!("{} ", t)));

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

    /// List configured profiles
    pub fn list_profiles(&self) -> Vec<&String> {
        self.config.profiles.keys().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PolicyRules;
    use acer_core::{Message, ModelRequest};

    #[test]
    fn prepare_request_applies_redactions() {
        let mut engine = PolicyEngine::new();
        let mut rules = PolicyRules::default();
        rules.redact_pii = true;
        engine.update_default_rules(rules);

        let request = ModelRequest {
            model: "llama2".to_string(),
            messages: vec![Message::user("email me at test@example.com")],
            temperature: None,
            max_tokens: None,
            stream: None,
        };

        let (prepared, decision) = engine.prepare_request(&request).expect("policy prep");

        assert!(decision.allowed);
        assert!(!decision.redactions.is_empty());
        assert_ne!(prepared.messages[0].content, request.messages[0].content);
    }
}

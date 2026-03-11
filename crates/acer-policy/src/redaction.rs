//! Redaction engine for sensitive data

use acer_core::{AcerError, Redaction, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Redaction pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionPattern {
    pub name: String,
    pub pattern: String,
    pub replacement: String,
    #[serde(skip)]
    compiled: Option<Regex>,
}

impl RedactionPattern {
    pub fn new(name: &str, pattern: &str, replacement: &str) -> Result<Self> {
        let compiled = Regex::new(pattern)
            .map_err(|e| AcerError::PolicyViolation(format!("Invalid regex: {}", e)))?;

        Ok(Self {
            name: name.to_string(),
            pattern: pattern.to_string(),
            replacement: replacement.to_string(),
            compiled: Some(compiled),
        })
    }

    pub fn compile(&mut self) -> Result<()> {
        if self.compiled.is_none() {
            self.compiled = Some(
                Regex::new(&self.pattern)
                    .map_err(|e| AcerError::PolicyViolation(format!("Invalid regex: {}", e)))?,
            );
        }
        Ok(())
    }

    pub fn matches<'a>(&self, text: &'a str) -> Vec<(usize, usize, &'a str)> {
        match &self.compiled {
            Some(re) => re
                .find_iter(text)
                .map(|m| (m.start(), m.end(), m.as_str()))
                .collect(),
            None => Vec::new(),
        }
    }
}

/// Redaction engine
#[derive(Debug, Clone)]
pub struct RedactionEngine {
    patterns: Vec<RedactionPattern>,
}

impl Default for RedactionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RedactionEngine {
    pub fn new() -> Self {
        Self {
            patterns: Self::default_patterns(),
        }
    }

    /// Get default PII patterns
    fn default_patterns() -> Vec<RedactionPattern> {
        vec![
            // AWS Access Key
            RedactionPattern::new(
                "aws_access_key",
                r"(?i)(A3T[A-Z0-9]|AKIA|AGPA|AIDA|AROA|AIPA|ANPA|ANVA|ASIA)[A-Z0-9]{16}",
                "[REDACTED_AWS_KEY]",
            ).unwrap(),

            // AWS Secret Key
            RedactionPattern::new(
                "aws_secret_key",
                r#"(?i)aws(.{0,20})?['"][0-9a-zA-Z/+=]{40}['"]"#,
                "[REDACTED_AWS_SECRET]",
            ).unwrap(),

            // Generic API Key patterns
            RedactionPattern::new(
                "api_key",
                r#"(?i)(api[_-]?key|apikey|api_secret)['"]?\s*[:=]\s*['"]?[a-zA-Z0-9_\-]{20,}['"]?"#,
                "[REDACTED_API_KEY]",
            ).unwrap(),

            // OpenAI API Key
            RedactionPattern::new(
                "openai_key",
                r"sk-[a-zA-Z0-9]{20,}T3BlbkFJ[a-zA-Z0-9]{20,}",
                "[REDACTED_OPENAI_KEY]",
            ).unwrap(),

            // Anthropic API Key
            RedactionPattern::new(
                "anthropic_key",
                r"sk-ant-api03-[a-zA-Z0-9\-]{80,}",
                "[REDACTED_ANTHROPIC_KEY]",
            ).unwrap(),

            // Generic Secret
            RedactionPattern::new(
                "secret",
                r#"(?i)(secret|password|passwd|pwd)['"]?\s*[:=]\s*['"]?[^\s'"]{8,}['"]?"#,
                "[REDACTED_SECRET]",
            ).unwrap(),

            // JWT Token
            RedactionPattern::new(
                "jwt",
                r"eyJ[a-zA-Z0-9_-]*\.eyJ[a-zA-Z0-9_-]*\.[a-zA-Z0-9_-]*",
                "[REDACTED_JWT]",
            ).unwrap(),

            // Email
            RedactionPattern::new(
                "email",
                r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}",
                "[REDACTED_EMAIL]",
            ).unwrap(),

            // SSN
            RedactionPattern::new(
                "ssn",
                r"\b\d{3}[-.]?\d{2}[-.]?\d{4}\b",
                "[REDACTED_SSN]",
            ).unwrap(),

            // Credit Card
            RedactionPattern::new(
                "credit_card",
                r"\b(?:\d{4}[-\s]?){3}\d{4}\b",
                "[REDACTED_CC]",
            ).unwrap(),

            // Phone Number
            RedactionPattern::new(
                "phone",
                r"\b(?:\+?1[-.]?)?\(?\d{3}\)?[-.]?\d{3}[-.]?\d{4}\b",
                "[REDACTED_PHONE]",
            ).unwrap(),

            // IP Address
            RedactionPattern::new(
                "ip_address",
                r"\b(?:\d{1,3}\.){3}\d{1,3}\b",
                "[REDACTED_IP]",
            ).unwrap(),

            // Private Key
            RedactionPattern::new(
                "private_key",
                r"-----BEGIN (?:RSA |EC |DSA )?PRIVATE KEY-----[\s\S]*?-----END (?:RSA |EC |DSA )?PRIVATE KEY-----",
                "[REDACTED_PRIVATE_KEY]",
            ).unwrap(),
        ]
    }

    /// Add a custom pattern
    pub fn add_pattern(&mut self, pattern: RedactionPattern) -> Result<()> {
        let mut pattern = pattern;
        pattern.compile()?;
        self.patterns.push(pattern);
        Ok(())
    }

    /// Add patterns from blocklist
    pub fn add_block_patterns(&mut self, patterns: &[String]) -> Result<()> {
        for (i, pattern) in patterns.iter().enumerate() {
            let redaction =
                RedactionPattern::new(&format!("blocklist_{}", i), pattern, "[BLOCKED]")?;
            self.patterns.push(redaction);
        }
        Ok(())
    }

    /// Scan text for sensitive data
    pub fn scan(&self, text: &str) -> Vec<Redaction> {
        let mut redactions = Vec::new();

        for pattern in &self.patterns {
            for (start, _end, matched) in pattern.matches(text) {
                redactions.push(Redaction {
                    original: matched.to_string(),
                    replacement: pattern.replacement.clone(),
                    pattern_type: pattern.name.clone(),
                    position: start,
                });
            }
        }

        // Sort by position
        redactions.sort_by_key(|r| r.position);
        redactions
    }

    /// Redact sensitive data from text
    pub fn redact(&self, text: &str) -> (String, Vec<Redaction>) {
        let redactions = self.scan(text);

        if redactions.is_empty() {
            return (text.to_string(), redactions);
        }

        // Filter out overlapping redactions
        let mut filtered_redactions = Vec::new();
        let mut last_end = 0;

        for redaction in &redactions {
            let start = redaction.position;
            let end = start + redaction.original.len();
            if start >= last_end {
                filtered_redactions.push(redaction.clone());
                last_end = end;
            }
        }

        let mut result = text.to_string();

        // Apply redactions in reverse order to maintain positions
        for redaction in filtered_redactions.iter().rev() {
            let start = redaction.position;
            let end = start + redaction.original.len();
            if end <= result.len() {
                result.replace_range(start..end, &redaction.replacement);
            }
        }

        (result, filtered_redactions)
    }

    /// Check if text contains sensitive data
    pub fn contains_sensitive(&self, text: &str) -> bool {
        self.patterns.iter().any(|p| !p.matches(text).is_empty())
    }

    /// Get pattern names
    pub fn pattern_names(&self) -> Vec<&str> {
        self.patterns.iter().map(|p| p.name.as_str()).collect()
    }
}

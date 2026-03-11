//! Core types for Acer Hybrid

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a run/trace
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RunId(String);

impl RunId {
    pub fn new() -> Self {
        Self(format!("run_{}", Uuid::new_v4().simple()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Provider type (local or cloud)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    Ollama,
    OpenAI,
    Anthropic,
    Gemini,
    Custom,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::Ollama => write!(f, "ollama"),
            ProviderType::OpenAI => write!(f, "openai"),
            ProviderType::Anthropic => write!(f, "anthropic"),
            ProviderType::Gemini => write!(f, "gemini"),
            ProviderType::Custom => write!(f, "custom"),
        }
    }
}

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub provider: ProviderType,
    pub is_local: bool,
    pub context_window: Option<usize>,
    pub cost_per_1k_tokens: Option<f64>,
}

/// Chat message role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            name: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            name: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            name: None,
        }
    }
}

/// Request to a model provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// Response from a model provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub id: String,
    pub model: String,
    pub content: String,
    pub usage: TokenUsage,
    pub latency_ms: u64,
    pub provider: ProviderType,
    pub finish_reason: Option<String>,
}

/// Redaction action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Redaction {
    pub original: String,
    pub replacement: String,
    pub pattern_type: String,
    pub position: usize,
}

/// Policy decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub reason: Option<String>,
    pub redactions: Vec<Redaction>,
    pub model_override: Option<String>,
    pub cost_limit: Option<f64>,
}

/// Run record for tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: RunId,
    pub timestamp: DateTime<Utc>,
    pub prompt_hash: String,
    pub model: String,
    pub provider: ProviderType,
    pub request: ModelRequest,
    pub response: Option<ModelResponse>,
    pub redactions: Vec<Redaction>,
    pub policy_decision: Option<PolicyDecision>,
    pub cost_usd: Option<f64>,
    pub latency_ms: u64,
    pub success: bool,
    pub error: Option<String>,
    pub metadata: serde_json::Value,
}

impl RunRecord {
    pub fn new(request: ModelRequest) -> Self {
        use sha2::{Digest, Sha256};
        
        let prompt_hash = {
            let mut hasher = Sha256::new();
            for msg in &request.messages {
                hasher.update(msg.content.as_bytes());
            }
            hex::encode(hasher.finalize())
        };

        Self {
            id: RunId::new(),
            timestamp: Utc::now(),
            prompt_hash,
            model: request.model.clone(),
            provider: ProviderType::Ollama, // Will be updated
            request,
            response: None,
            redactions: Vec::new(),
            policy_decision: None,
            cost_usd: None,
            latency_ms: 0,
            success: false,
            error: None,
            metadata: serde_json::json!({}),
        }
    }
}

/// Cost tracking entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEntry {
    pub timestamp: DateTime<Utc>,
    pub provider: ProviderType,
    pub model: String,
    pub tokens: TokenUsage,
    pub cost_usd: f64,
    pub run_id: RunId,
}
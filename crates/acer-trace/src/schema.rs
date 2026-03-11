//! Database schema for trace storage

use acer_core::{CostEntry, ProviderType, RunId, RunRecord, TokenUsage};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Database schema version
pub const SCHEMA_VERSION: i32 = 1;

/// SQL to create the database schema
pub const CREATE_SCHEMA: &str = r#"
-- Run records table
CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    prompt_hash TEXT NOT NULL,
    model TEXT NOT NULL,
    provider TEXT NOT NULL,
    request_json TEXT NOT NULL,
    response_json TEXT,
    redactions_json TEXT,
    policy_decision_json TEXT,
    cost_usd REAL,
    latency_ms INTEGER NOT NULL,
    success INTEGER NOT NULL,
    error TEXT,
    metadata_json TEXT
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_runs_timestamp ON runs(timestamp);
CREATE INDEX IF NOT EXISTS idx_runs_model ON runs(model);
CREATE INDEX IF NOT EXISTS idx_runs_provider ON runs(provider);
CREATE INDEX IF NOT EXISTS idx_runs_prompt_hash ON runs(prompt_hash);
CREATE INDEX IF NOT EXISTS idx_runs_success ON runs(success);

-- Cost tracking table
CREATE TABLE IF NOT EXISTS costs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    prompt_tokens INTEGER NOT NULL,
    completion_tokens INTEGER NOT NULL,
    total_tokens INTEGER NOT NULL,
    cost_usd REAL NOT NULL,
    run_id TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES runs(id)
);

CREATE INDEX IF NOT EXISTS idx_costs_timestamp ON costs(timestamp);
CREATE INDEX IF NOT EXISTS idx_costs_provider ON costs(provider);
CREATE INDEX IF NOT EXISTS idx_costs_model ON costs(model);

-- Schema version table
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
);

INSERT OR IGNORE INTO schema_version (version) VALUES (1);
"#;

/// Run record for database storage
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DbRunRecord {
    pub id: String,
    pub timestamp: String,
    pub prompt_hash: String,
    pub model: String,
    pub provider: String,
    pub request_json: String,
    pub response_json: Option<String>,
    pub redactions_json: Option<String>,
    pub policy_decision_json: Option<String>,
    pub cost_usd: Option<f64>,
    pub latency_ms: i64,
    pub success: bool,
    pub error: Option<String>,
    pub metadata_json: Option<String>,
}

impl From<RunRecord> for DbRunRecord {
    fn from(run: RunRecord) -> Self {
        Self {
            id: run.id.to_string(),
            timestamp: run.timestamp.to_rfc3339(),
            prompt_hash: run.prompt_hash,
            model: run.model,
            provider: run.provider.to_string(),
            request_json: serde_json::to_string(&run.request).unwrap_or_default(),
            response_json: run
                .response
                .map(|r| serde_json::to_string(&r).unwrap_or_default()),
            redactions_json: if run.redactions.is_empty() {
                None
            } else {
                serde_json::to_string(&run.redactions).ok()
            },
            policy_decision_json: run
                .policy_decision
                .map(|p| serde_json::to_string(&p).unwrap_or_default()),
            cost_usd: run.cost_usd,
            latency_ms: run.latency_ms as i64,
            success: run.success,
            error: run.error,
            metadata_json: if run.metadata.is_null() {
                None
            } else {
                serde_json::to_string(&run.metadata).ok()
            },
        }
    }
}

impl TryInto<RunRecord> for DbRunRecord {
    type Error = String;

    fn try_into(self) -> std::result::Result<RunRecord, String> {
        Ok(RunRecord {
            id: RunId::from(self.id),
            timestamp: DateTime::parse_from_rfc3339(&self.timestamp)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| format!("Invalid timestamp: {}", e))?,
            prompt_hash: self.prompt_hash,
            model: self.model,
            provider: match self.provider.as_str() {
                "ollama" => ProviderType::Ollama,
                "openai" => ProviderType::OpenAI,
                "anthropic" => ProviderType::Anthropic,
                "gemini" => ProviderType::Gemini,
                _ => ProviderType::Custom,
            },
            request: serde_json::from_str(&self.request_json)
                .map_err(|e| format!("Invalid request JSON: {}", e))?,
            response: self
                .response_json
                .map(|r| serde_json::from_str(&r))
                .transpose()
                .map_err(|e: serde_json::Error| format!("Invalid response JSON: {}", e))?,
            redactions: self
                .redactions_json
                .map(|r| serde_json::from_str(&r))
                .transpose()
                .unwrap_or_default()
                .unwrap_or_default(),
            policy_decision: self
                .policy_decision_json
                .map(|p| serde_json::from_str(&p))
                .transpose()
                .map_err(|e: serde_json::Error| format!("Invalid policy decision JSON: {}", e))?,
            cost_usd: self.cost_usd,
            latency_ms: self.latency_ms as u64,
            success: self.success,
            error: self.error,
            metadata: self
                .metadata_json
                .map(|m| serde_json::from_str(&m))
                .transpose()
                .unwrap_or_default()
                .unwrap_or(serde_json::json!({})),
        })
    }
}

/// Cost record for database storage
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DbCostRecord {
    pub id: i64,
    pub timestamp: String,
    pub provider: String,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cost_usd: f64,
    pub run_id: String,
}

impl From<DbCostRecord> for CostEntry {
    fn from(record: DbCostRecord) -> Self {
        Self {
            timestamp: DateTime::parse_from_rfc3339(&record.timestamp)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            provider: match record.provider.as_str() {
                "ollama" => ProviderType::Ollama,
                "openai" => ProviderType::OpenAI,
                "anthropic" => ProviderType::Anthropic,
                "gemini" => ProviderType::Gemini,
                _ => ProviderType::Custom,
            },
            model: record.model,
            tokens: TokenUsage {
                prompt_tokens: record.prompt_tokens as usize,
                completion_tokens: record.completion_tokens as usize,
                total_tokens: record.total_tokens as usize,
            },
            cost_usd: record.cost_usd,
            run_id: RunId::from(record.run_id),
        }
    }
}

/// Statistics for a time period
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageStats {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_tokens: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_cost_usd: f64,
    pub avg_latency_ms: f64,
    pub by_provider: std::collections::HashMap<String, ProviderStats>,
    pub by_model: std::collections::HashMap<String, ModelStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderStats {
    pub requests: u64,
    pub tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelStats {
    pub requests: u64,
    pub tokens: u64,
    pub cost_usd: f64,
    pub avg_latency_ms: f64,
}

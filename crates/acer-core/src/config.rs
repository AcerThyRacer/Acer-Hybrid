//! Configuration for Acer Hybrid

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcerConfig {
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub gateway: GatewayConfig,
    #[serde(default)]
    pub tracing: TracingConfig,
    #[serde(default)]
    pub vault: VaultConfig,
}

impl Default for AcerConfig {
    fn default() -> Self {
        Self {
            providers: ProvidersConfig::default(),
            policy: PolicyConfig::default(),
            gateway: GatewayConfig::default(),
            tracing: TracingConfig::default(),
            vault: VaultConfig::default(),
        }
    }
}

impl AcerConfig {
    pub fn load() -> crate::Result<Self> {
        let config_path = Self::config_path();

        if !config_path.exists() {
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }

        let content = std::fs::read_to_string(&config_path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> crate::Result<()> {
        let config_path = Self::config_path();

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    pub fn config_path() -> PathBuf {
        base_config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("acer-hybrid")
            .join("config.toml")
    }

    pub fn data_dir() -> PathBuf {
        base_data_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("acer-hybrid")
    }

    pub fn plugins_dir() -> PathBuf {
        Self::config_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("plugins")
    }

    pub fn policy_packs_dir() -> PathBuf {
        Self::config_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("policy-packs")
    }
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub ollama: OllamaConfig,
    #[serde(default)]
    pub openai: OpenAIConfig,
    #[serde(default)]
    pub anthropic: AnthropicConfig,
    #[serde(default)]
    pub gemini: GeminiConfig,
    #[serde(default)]
    pub default_provider: Option<String>,
    #[serde(default)]
    pub http: ProviderHttpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHttpConfig {
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_max_idle_connections")]
    pub max_idle_connections: usize,
    #[serde(default = "default_pool_idle_timeout_secs")]
    pub pool_idle_timeout_secs: u64,
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: u32,
}

fn default_request_timeout_secs() -> u64 {
    60
}
fn default_connect_timeout_secs() -> u64 {
    10
}
fn default_max_idle_connections() -> usize {
    32
}
fn default_pool_idle_timeout_secs() -> u64 {
    90
}
fn default_retry_attempts() -> u32 {
    2
}

impl Default for ProviderHttpConfig {
    fn default() -> Self {
        Self {
            request_timeout_secs: default_request_timeout_secs(),
            connect_timeout_secs: default_connect_timeout_secs(),
            max_idle_connections: default_max_idle_connections(),
            pool_idle_timeout_secs: default_pool_idle_timeout_secs(),
            retry_attempts: default_retry_attempts(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    #[serde(default = "default_ollama_url")]
    pub base_url: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub default_model: Option<String>,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: default_ollama_url(),
            enabled: true,
            default_model: Some("llama2".to_string()),
        }
    }
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenAIConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub default_model: Option<String>,
    // API key is stored in vault, not here
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnthropicConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeminiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub default_model: Option<String>,
}

/// Policy configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicyConfig {
    #[serde(default)]
    pub default: PolicyRules,
    #[serde(default)]
    pub projects: std::collections::HashMap<String, PolicyRules>,
    #[serde(default)]
    pub profiles: std::collections::HashMap<String, PolicyRules>,
    #[serde(default)]
    pub active_profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRules {
    #[serde(default = "default_max_cost")]
    pub max_cost_usd: f64,
    #[serde(default = "default_true")]
    pub allow_remote: bool,
    #[serde(default = "default_true")]
    pub redact_pii: bool,
    #[serde(default)]
    pub allow_tools: Vec<String>,
    #[serde(default)]
    pub block_patterns: Vec<String>,
    #[serde(default)]
    pub require_confirmation: bool,
}

fn default_max_cost() -> f64 {
    0.10
}
fn default_true() -> bool {
    true
}

impl Default for PolicyRules {
    fn default() -> Self {
        Self {
            max_cost_usd: default_max_cost(),
            allow_remote: true,
            redact_pii: true,
            allow_tools: Vec::new(),
            block_patterns: Vec::new(),
            require_confirmation: false,
        }
    }
}

/// Gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_gateway_host")]
    pub host: String,
    #[serde(default = "default_gateway_port")]
    pub port: u16,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_gateway_rate_limit_requests")]
    pub rate_limit_requests: u64,
    #[serde(default = "default_gateway_rate_limit_window_secs")]
    pub rate_limit_window_secs: u64,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub cors_allowed_origins: Vec<String>,
    #[serde(default = "default_gateway_max_request_body_bytes")]
    pub max_request_body_bytes: usize,
    #[serde(default = "default_gateway_max_messages_per_request")]
    pub max_messages_per_request: usize,
    #[serde(default = "default_gateway_max_message_chars")]
    pub max_message_chars: usize,
}

fn default_gateway_host() -> String {
    "127.0.0.1".to_string()
}
fn default_gateway_port() -> u16 {
    8080
}
fn default_gateway_rate_limit_requests() -> u64 {
    60
}
fn default_gateway_rate_limit_window_secs() -> u64 {
    60
}
fn default_gateway_max_request_body_bytes() -> usize {
    1024 * 1024
}
fn default_gateway_max_messages_per_request() -> usize {
    128
}
fn default_gateway_max_message_chars() -> usize {
    32 * 1024
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: default_gateway_host(),
            port: default_gateway_port(),
            enabled: true,
            rate_limit_requests: default_gateway_rate_limit_requests(),
            rate_limit_window_secs: default_gateway_rate_limit_window_secs(),
            api_key_env: None,
            cors_allowed_origins: Vec::new(),
            max_request_body_bytes: default_gateway_max_request_body_bytes(),
            max_messages_per_request: default_gateway_max_messages_per_request(),
            max_message_chars: default_gateway_max_message_chars(),
        }
    }
}

/// Tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub database_path: Option<PathBuf>,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    #[serde(default = "default_trace_max_connections")]
    pub max_connections: u32,
}

fn default_retention_days() -> u32 {
    30
}
fn default_trace_max_connections() -> u32 {
    5
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            database_path: None,
            retention_days: default_retention_days(),
            max_connections: default_trace_max_connections(),
        }
    }
}

/// Vault configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub vault_path: Option<PathBuf>,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            vault_path: None,
        }
    }
}

fn base_config_dir() -> Option<PathBuf> {
    dirs::config_dir().or_else(|| dirs::home_dir().map(|home| home.join(".config")))
}

fn base_data_dir() -> Option<PathBuf> {
    dirs::data_dir().or_else(|| dirs::home_dir().map(|home| home.join(".local/share")))
}

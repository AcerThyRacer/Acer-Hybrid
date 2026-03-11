//! Error types for Acer Hybrid

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AcerError>;

#[derive(Error, Debug)]
pub enum AcerError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    #[error("Vault error: {0}")]
    Vault(String),

    #[error("Trace store error: {0}")]
    TraceStore(String),

    #[error("Gateway error: {0}")]
    Gateway(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Redaction required: {0}")]
    RedactionRequired(String),

    #[error("Command blocked by policy: {0}")]
    CommandBlocked(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl From<toml::de::Error> for AcerError {
    fn from(e: toml::de::Error) -> Self {
        AcerError::Config(e.to_string())
    }
}

impl From<toml::ser::Error> for AcerError {
    fn from(e: toml::ser::Error) -> Self {
        AcerError::Config(e.to_string())
    }
}
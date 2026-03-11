//! Acer Hybrid Core - Shared types and error handling

pub mod error;
pub mod types;
pub mod config;

pub use error::{AcerError, Result};
pub use types::*;
pub use config::AcerConfig;
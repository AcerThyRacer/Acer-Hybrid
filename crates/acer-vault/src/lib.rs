//! Encrypted secrets vault for Acer Hybrid

mod encryption;
mod vault;

pub use encryption::EncryptionKey;
pub use vault::{keys, SecretsVault};

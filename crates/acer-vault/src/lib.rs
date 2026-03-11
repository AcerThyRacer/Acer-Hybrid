//! Encrypted secrets vault for Acer Hybrid

mod vault;
mod encryption;

pub use vault::SecretsVault;
pub use encryption::EncryptionKey;
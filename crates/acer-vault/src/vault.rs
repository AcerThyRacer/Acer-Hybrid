//! Secrets vault for secure credential storage

use crate::EncryptionKey;
use acer_core::{AcerError, Result};
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use zeroize::Zeroize;

/// A stored secret
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    pub key: String,
    pub value: String, // Encrypted, base64-encoded
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, String>,
}

/// The secrets vault
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsVault {
    secrets: HashMap<String, Secret>,
    #[serde(default = "default_salt")]
    salt: String,
    #[serde(skip)]
    encryption_key: Option<EncryptionKey>,
    vault_path: PathBuf,
}

impl SecretsVault {
    /// Create a new vault
    pub fn new(vault_path: PathBuf) -> Self {
        Self {
            secrets: HashMap::new(),
            salt: Self::generate_salt(),
            encryption_key: None,
            vault_path,
        }
    }

    /// Load vault from disk
    pub fn load(path: PathBuf, password: Option<&str>) -> Result<Self> {
        if !path.exists() {
            let mut vault = Self::new(path);
            if let Some(pwd) = password {
                vault.unlock(pwd)?;
            }
            return Ok(vault);
        }

        let content = std::fs::read_to_string(&path)?;
        let mut vault: Self = serde_json::from_str(&content)
            .map_err(|e| AcerError::Vault(format!("Failed to parse vault: {}", e)))?;

        if let Some(pwd) = password {
            vault.unlock(pwd)?;
        }

        Ok(vault)
    }

    /// Unlock the vault with a password
    pub fn unlock(&mut self, password: &str) -> Result<()> {
        let salt = base64::engine::general_purpose::STANDARD
            .decode(&self.salt)
            .map_err(|e| AcerError::Vault(format!("Invalid vault salt: {}", e)))?;
        self.encryption_key = Some(EncryptionKey::from_password(password, Some(&salt)));
        Ok(())
    }

    /// Lock the vault (remove encryption key from memory)
    pub fn lock(&mut self) {
        self.encryption_key = None;
    }

    /// Check if vault is unlocked
    pub fn is_unlocked(&self) -> bool {
        self.encryption_key.is_some()
    }

    /// Store a secret
    pub fn store(&mut self, key: &str, value: &str) -> Result<()> {
        let encryption_key = self
            .encryption_key
            .as_ref()
            .ok_or_else(|| AcerError::Vault("Vault is locked".to_string()))?;

        let encrypted = encryption_key.encrypt(value.as_bytes())?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&encrypted);

        let now = chrono::Utc::now();
        let secret = if let Some(existing) = self.secrets.get(key) {
            Secret {
                key: key.to_string(),
                value: encoded,
                created_at: existing.created_at,
                updated_at: now,
                metadata: existing.metadata.clone(),
            }
        } else {
            Secret {
                key: key.to_string(),
                value: encoded,
                created_at: now,
                updated_at: now,
                metadata: HashMap::new(),
            }
        };

        self.secrets.insert(key.to_string(), secret);
        self.save()?;

        Ok(())
    }

    /// Retrieve a secret
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let encryption_key = self
            .encryption_key
            .as_ref()
            .ok_or_else(|| AcerError::Vault("Vault is locked".to_string()))?;

        let secret = match self.secrets.get(key) {
            Some(s) => s,
            None => return Ok(None),
        };

        let encrypted = base64::engine::general_purpose::STANDARD
            .decode(&secret.value)
            .map_err(|e| AcerError::Vault(format!("Failed to decode secret: {}", e)))?;

        let mut decrypted = encryption_key.decrypt(&encrypted)?;
        let value = String::from_utf8(decrypted.clone())
            .map_err(|e| AcerError::Vault(format!("Invalid UTF-8 in secret: {}", e)))?;
        decrypted.zeroize();
        Ok(Some(value))
    }

    /// Delete a secret
    pub fn delete(&mut self, key: &str) -> Result<bool> {
        if self.secrets.remove(key).is_some() {
            self.save()?;
            return Ok(true);
        }
        Ok(false)
    }

    /// List all secret keys (not values)
    pub fn list_keys(&self) -> Vec<&str> {
        self.secrets.keys().map(|k| k.as_str()).collect()
    }

    /// Check if a secret exists
    pub fn exists(&self, key: &str) -> bool {
        self.secrets.contains_key(key)
    }

    /// Save vault to disk
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.vault_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Serialize without the encryption key
        let json = serde_json::to_string_pretty(&self)
            .map_err(|e| AcerError::Vault(format!("Failed to serialize vault: {}", e)))?;

        std::fs::write(&self.vault_path, json)?;
        Ok(())
    }

    /// Get the vault path
    pub fn path(&self) -> &std::path::Path {
        &self.vault_path
    }

    /// Rotate the encryption key
    pub fn rotate_key(&mut self, new_password: &str) -> Result<()> {
        let old_key = self
            .encryption_key
            .as_ref()
            .ok_or_else(|| AcerError::Vault("Vault is locked".to_string()))?;

        // Decrypt all secrets
        let mut decrypted_secrets: HashMap<String, String> = HashMap::new();
        for (key, secret) in &self.secrets {
            let encrypted = base64::engine::general_purpose::STANDARD
                .decode(&secret.value)
                .map_err(|e| AcerError::Vault(format!("Failed to decode secret: {}", e)))?;
            let mut decrypted = old_key.decrypt(&encrypted)?;
            let value = String::from_utf8(decrypted.clone())
                .map_err(|e| AcerError::Vault(format!("Invalid UTF-8 in secret: {}", e)))?;
            decrypted.zeroize();
            decrypted_secrets.insert(key.clone(), value);
        }

        // Create new key
        let mut new_salt = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut new_salt);
        self.salt = base64::engine::general_purpose::STANDARD.encode(new_salt);
        self.encryption_key = Some(EncryptionKey::from_password(new_password, Some(&new_salt)));

        // Re-encrypt all secrets
        let now = chrono::Utc::now();
        for (key, mut value) in decrypted_secrets {
            let encrypted = self
                .encryption_key
                .as_ref()
                .unwrap()
                .encrypt(value.as_bytes())?;
            let encoded = base64::engine::general_purpose::STANDARD.encode(&encrypted);

            if let Some(secret) = self.secrets.get_mut(&key) {
                secret.value = encoded;
                secret.updated_at = now;
            }
            value.zeroize();
        }

        self.save()?;
        Ok(())
    }
}

impl Drop for SecretsVault {
    fn drop(&mut self) {
        self.lock();
    }
}

impl SecretsVault {
    fn generate_salt() -> String {
        let mut salt = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt);
        base64::engine::general_purpose::STANDARD.encode(salt)
    }
}

fn default_salt() -> String {
    SecretsVault::generate_salt()
}

/// Common secret keys
pub mod keys {
    pub const OPENAI_API_KEY: &str = "openai_api_key";
    pub const ANTHROPIC_API_KEY: &str = "anthropic_api_key";
    pub const GEMINI_API_KEY: &str = "gemini_api_key";
    pub const CUSTOM_API_KEY: &str = "custom_api_key";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_clears_unlock_state() {
        let mut vault = SecretsVault::new(std::env::temp_dir().join("acer-vault-test.json"));
        vault.unlock("test-password").expect("unlock");
        assert!(vault.is_unlocked());
        vault.lock();
        assert!(!vault.is_unlocked());
    }
}

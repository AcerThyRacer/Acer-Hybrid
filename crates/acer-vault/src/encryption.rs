//! Encryption utilities using ring

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::digest::{digest, SHA256};
use ring::pbkdf2;
use std::num::NonZeroU32;

/// Encryption key for the vault
#[derive(Clone)]
pub struct EncryptionKey {
    key: LessSafeKey,
}

impl EncryptionKey {
    /// Create a new encryption key from a password
    pub fn from_password(password: &str, salt: Option<&[u8]>) -> Self {
        let salt = salt.map(|s| s.to_vec()).unwrap_or_else(|| {
            let mut s = vec![0u8; 16];
            rand::thread_rng().fill_bytes(&mut s);
            s
        });

        let mut key_bytes = [0u8; 32];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(100_000).unwrap(),
            &salt,
            password.as_bytes(),
            &mut key_bytes,
        );

        let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes).unwrap();
        Self {
            key: LessSafeKey::new(unbound_key),
        }
    }

    /// Generate a random encryption key
    pub fn generate() -> Self {
        let mut key_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key_bytes);
        
        let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes).unwrap();
        Self {
            key: LessSafeKey::new(unbound_key),
        }
    }

    /// Encrypt data
    pub fn encrypt(&self, plaintext: &[u8]) -> crate::Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut ciphertext = plaintext.to_vec();
        ciphertext.extend_from_slice(&[0u8; 16]); // Space for tag

        self.key.seal_in_place_append_tag(
            nonce,
            Aad::empty(),
            &mut ciphertext[..plaintext.len()],
        ).map_err(|e| acer_core::AcerError::Vault(format!("Encryption failed: {}", e)))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);
        Ok(result)
    }

    /// Decrypt data
    pub fn decrypt(&self, ciphertext: &[u8]) -> crate::Result<Vec<u8>> {
        if ciphertext.len() < 12 + 16 {
            return Err(acer_core::AcerError::Vault("Ciphertext too short".to_string()));
        }

        let nonce_bytes: [u8; 12] = ciphertext[..12]
            .try_into()
            .map_err(|_| acer_core::AcerError::Vault("Invalid nonce".to_string()))?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut plaintext = ciphertext[12..].to_vec();
        let plaintext_len = self.key.open_in_place(nonce, Aad::empty(), &mut plaintext)
            .map_err(|e| acer_core::AcerError::Vault(format!("Decryption failed: {}", e)))?
            .len();

        plaintext.truncate(plaintext_len);
        Ok(plaintext)
    }

    /// Hash a value
    pub fn hash(value: &str) -> String {
        let hash = digest(&SHA256, value.as_bytes());
        hex::encode(hash.as_ref())
    }
}

impl std::fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionKey")
            .field("algorithm", &"AES-256-GCM")
            .finish_non_exhaustive()
    }
}
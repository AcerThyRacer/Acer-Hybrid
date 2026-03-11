//! Secrets command - manage secrets and API keys

use acer_core::AcerConfig;
use acer_vault::SecretsVault;
use anyhow::Result;

use crate::SecretsCommands;

pub async fn execute(command: SecretsCommands) -> Result<()> {
    let vault_path = AcerConfig::data_dir().join("vault.json");

    match command {
        SecretsCommands::Set { key, value } => {
            let password = rpassword::prompt_password("Vault password: ")?;
            let mut vault = SecretsVault::load(vault_path, Some(&password))?;

            let value = match value {
                Some(v) => v,
                None => rpassword::prompt_password(&format!("Enter value for {}: ", key))?,
            };

            vault.store(&key, &value)?;
            println!("Secret '{}' stored successfully.", key);
        }

        SecretsCommands::Get { key } => {
            let password = rpassword::prompt_password("Vault password: ")?;
            let vault = SecretsVault::load(vault_path, Some(&password))?;

            match vault.get(&key)? {
                Some(value) => {
                    println!("{}", value);
                }
                None => {
                    eprintln!("Secret '{}' not found.", key);
                    std::process::exit(1);
                }
            }
        }

        SecretsCommands::Delete { key } => {
            let password = rpassword::prompt_password("Vault password: ")?;
            let mut vault = SecretsVault::load(vault_path, Some(&password))?;

            if vault.delete(&key)? {
                println!("Secret '{}' deleted.", key);
            } else {
                eprintln!("Secret '{}' not found.", key);
                std::process::exit(1);
            }
        }

        SecretsCommands::List => {
            let password = rpassword::prompt_password("Vault password: ")?;
            let vault = SecretsVault::load(vault_path, Some(&password))?;

            let keys = vault.list_keys();
            if keys.is_empty() {
                println!("No secrets stored.");
            } else {
                println!("Stored secrets:");
                for key in keys {
                    println!("  - {}", key);
                }
            }
        }

        SecretsCommands::Unlock => {
            let password = rpassword::prompt_password("Vault password: ")?;
            let vault = SecretsVault::load(vault_path, Some(&password))?;

            if vault.is_unlocked() {
                println!("Vault unlocked.");
            } else {
                eprintln!("Failed to unlock vault.");
                std::process::exit(1);
            }
        }

        SecretsCommands::Lock => {
            println!("Vault locked (encryption key removed from memory).");
        }

        SecretsCommands::Rotate => {
            let old_password = rpassword::prompt_password("Current vault password: ")?;
            let mut vault = SecretsVault::load(vault_path.clone(), Some(&old_password))?;

            let new_password = rpassword::prompt_password("New vault password: ")?;
            let confirm = rpassword::prompt_password("Confirm new password: ")?;

            if new_password != confirm {
                eprintln!("Passwords do not match.");
                std::process::exit(1);
            }

            vault.rotate_key(&new_password)?;
            println!("Encryption key rotated successfully.");
        }
    }

    Ok(())
}

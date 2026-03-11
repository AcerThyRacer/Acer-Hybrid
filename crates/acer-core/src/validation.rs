use crate::{AcerError, Message, Result};
use regex::Regex;
use std::sync::OnceLock;

const DEFAULT_MAX_TOKENS: usize = 128_000;

pub fn validate_temperature(value: Option<f32>) -> Result<()> {
    if let Some(value) = value {
        if !(0.0..=2.0).contains(&value) {
            return Err(AcerError::InvalidRequest(format!(
                "Invalid temperature {}. Expected a value between 0.0 and 2.0.",
                value
            )));
        }
    }

    Ok(())
}

pub fn validate_max_tokens(value: Option<usize>, max_allowed: Option<usize>) -> Result<()> {
    if let Some(value) = value {
        if value == 0 {
            return Err(AcerError::InvalidRequest(
                "Invalid max_tokens 0. Expected a positive integer.".to_string(),
            ));
        }

        let limit = max_allowed.unwrap_or(DEFAULT_MAX_TOKENS);
        if value > limit {
            return Err(AcerError::InvalidRequest(format!(
                "Invalid max_tokens {}. Expected a value <= {}.",
                value, limit
            )));
        }
    }

    Ok(())
}

pub fn validate_identifier(kind: &str, value: &str) -> Result<()> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AcerError::InvalidRequest(format!(
            "{} cannot be empty.",
            kind
        )));
    }

    if value.len() > 128 {
        return Err(AcerError::InvalidRequest(format!(
            "{} '{}' is too long.",
            kind, value
        )));
    }

    let pattern = IDENTIFIER_RE.get_or_init(|| {
        Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$").expect("valid identifier regex")
    });
    if !pattern.is_match(value) {
        return Err(AcerError::InvalidRequest(format!(
            "{} '{}' contains unsupported characters.",
            kind, value
        )));
    }

    Ok(())
}

pub fn validate_messages(
    messages: &[Message],
    max_messages: usize,
    max_message_chars: usize,
) -> Result<()> {
    if messages.is_empty() {
        return Err(AcerError::InvalidRequest(
            "At least one message is required.".to_string(),
        ));
    }

    if messages.len() > max_messages {
        return Err(AcerError::InvalidRequest(format!(
            "Request contains {} messages; limit is {}.",
            messages.len(),
            max_messages
        )));
    }

    for (index, message) in messages.iter().enumerate() {
        if message.content.len() > max_message_chars {
            return Err(AcerError::InvalidRequest(format!(
                "Message {} exceeds the {} character limit.",
                index, max_message_chars
            )));
        }
    }

    Ok(())
}

static IDENTIFIER_RE: OnceLock<Regex> = OnceLock::new();

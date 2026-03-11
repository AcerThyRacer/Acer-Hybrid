use acer_core::{AcerError, ProviderHttpConfig, Result};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use std::time::Duration;

pub fn build_http_client(config: &ProviderHttpConfig) -> Client {
    Client::builder()
        .timeout(Duration::from_secs(config.request_timeout_secs))
        .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
        .pool_max_idle_per_host(config.max_idle_connections)
        .pool_idle_timeout(Duration::from_secs(config.pool_idle_timeout_secs))
        .build()
        .unwrap_or_else(|error| {
            tracing::warn!(
                "Failed to build configured HTTP client (falling back to defaults): {}",
                error
            );
            Client::new()
        })
}

pub async fn send_with_retries<F>(mut build: F, retries: u32, operation: &str) -> Result<Response>
where
    F: FnMut() -> RequestBuilder,
{
    let mut last_error = None;

    for attempt in 0..=retries {
        match build().send().await {
            Ok(response) if should_retry_status(response.status()) && attempt < retries => {
                tokio::time::sleep(backoff_duration(attempt)).await;
            }
            Ok(response) => return Ok(response),
            Err(error) if error.is_timeout() || error.is_connect() => {
                last_error = Some(format!(
                    "{} (attempt {}): {}",
                    operation,
                    attempt + 1,
                    error
                ));
                if attempt < retries {
                    tokio::time::sleep(backoff_duration(attempt)).await;
                    continue;
                }
            }
            Err(error) => {
                return Err(AcerError::Http(format!("{} failed: {}", operation, error)));
            }
        }
    }

    Err(AcerError::Http(format!(
        "{} failed after {} attempts: {}",
        operation,
        retries + 1,
        last_error.unwrap_or_else(|| "unknown transient error".to_string())
    )))
}

fn should_retry_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn backoff_duration(attempt: u32) -> Duration {
    Duration::from_millis(200 * (attempt as u64 + 1))
}

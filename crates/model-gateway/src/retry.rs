//! LLM HTTP retry / backoff (Stage 7.16).
//!
//! Retries the initial chat-completions request only (before SSE tokens).
//! Honors `Retry-After` (delta-seconds) when present.
//!
//! Env:
//! - `EVOHIME_LLM_MAX_RETRIES` (default 3) — retries after the first attempt
//! - `EVOHIME_LLM_RETRY_BASE_MS` (default 250)
//! - `EVOHIME_LLM_RETRY_MAX_MS` (default 5000)

use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::StatusCode;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl RetryPolicy {
    pub fn from_env() -> Self {
        Self {
            max_retries: env_u32("EVOHIME_LLM_MAX_RETRIES", 3),
            base_delay: Duration::from_millis(env_u64("EVOHIME_LLM_RETRY_BASE_MS", 250).max(1)),
            max_delay: Duration::from_millis(env_u64("EVOHIME_LLM_RETRY_MAX_MS", 5_000).max(1)),
        }
    }

    pub fn none() -> Self {
        Self {
            max_retries: 0,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(1),
        }
    }

    pub fn for_tests(max_retries: u32) -> Self {
        Self {
            max_retries,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(50),
        }
    }
}

pub fn is_retryable_status(status: StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504)
}

/// Parse `Retry-After` as delta-seconds. HTTP-date forms are ignored (use backoff).
pub fn parse_retry_after_seconds(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    let seconds: u64 = value.parse().ok()?;
    Some(Duration::from_secs(seconds.min(300)))
}

/// Delay before the next attempt. `attempt` is 0-based (0 = first retry after failure).
pub fn compute_backoff(
    attempt: u32,
    policy: &RetryPolicy,
    retry_after: Option<Duration>,
) -> Duration {
    if let Some(retry_after) = retry_after {
        return clamp_delay(retry_after, policy);
    }
    let factor = 2u32.saturating_pow(attempt.min(16));
    let scaled = policy
        .base_delay
        .checked_mul(factor)
        .unwrap_or(policy.max_delay);
    clamp_delay(scaled, policy)
}

fn clamp_delay(delay: Duration, policy: &RetryPolicy) -> Duration {
    if delay > policy.max_delay {
        policy.max_delay
    } else if delay.is_zero() {
        Duration::from_millis(1)
    } else {
        delay
    }
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn retryable_statuses() {
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(is_retryable_status(StatusCode::GATEWAY_TIMEOUT));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn parses_retry_after_delta_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("2"));
        assert_eq!(
            parse_retry_after_seconds(&headers),
            Some(Duration::from_secs(2))
        );
    }

    #[test]
    fn ignores_http_date_retry_after() {
        let mut headers = HeaderMap::new();
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
        );
        assert_eq!(parse_retry_after_seconds(&headers), None);
    }

    #[test]
    fn backoff_grows_and_caps() {
        let policy = RetryPolicy {
            max_retries: 5,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(300),
        };
        assert_eq!(
            compute_backoff(0, &policy, None),
            Duration::from_millis(100)
        );
        assert_eq!(
            compute_backoff(1, &policy, None),
            Duration::from_millis(200)
        );
        assert_eq!(
            compute_backoff(2, &policy, None),
            Duration::from_millis(300)
        );
        assert_eq!(
            compute_backoff(0, &policy, Some(Duration::from_secs(1))),
            Duration::from_millis(300)
        );
    }
}

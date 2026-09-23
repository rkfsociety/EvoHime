//! LLM HTTP retry / backoff (Stage 7.16).
//!
//! Retries the initial chat-completions request only (before SSE tokens).
//! Honors `Retry-After` (delta-seconds) when present.
//!
//! Env:
//! - `EVOHIME_LLM_MAX_RETRIES` (default 3) — retries after the first attempt
//! - `EVOHIME_LLM_RETRY_BASE_MS` (default 250)
//! - `EVOHIME_LLM_RETRY_MAX_MS` (default 5000)

use futures_util::StreamExt;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::StatusCode;
use std::time::Duration;

pub(crate) const MAX_RATE_LIMIT_BODY_BYTES: usize = 16 * 1024;

/// Retry bounds applied to initial provider requests before streaming begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Number of retries after the first failed attempt.
    pub max_retries: u32,
    /// Initial exponential backoff delay.
    pub base_delay: Duration,
    /// Maximum delay between attempts.
    pub max_delay: Duration,
}

/// Classification for provider responses indicating rate limiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitClass {
    /// A temporary rate limit for which retry may succeed.
    Transient,
    /// Provider quota or billing limit is exhausted.
    Exhausted,
    /// Response does not establish whether the limit is temporary or exhausted.
    Unknown,
}

/// Classifies a provider status/body as a known rate limit, when applicable.
pub fn classify_rate_limit(status: StatusCode, body: &str) -> Option<RateLimitClass> {
    let lower = body.to_ascii_lowercase();
    let rate_limited = status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN && lower.contains("rate limit"));
    if !rate_limited {
        return None;
    }
    if [
        "quota exceeded",
        "quota_exceeded",
        "daily limit",
        "monthly limit",
        "hourly limit",
        "limit exceeded",
        "billing",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        Some(RateLimitClass::Exhausted)
    } else if lower.contains("retry") || lower.contains("temporar") {
        Some(RateLimitClass::Transient)
    } else {
        Some(RateLimitClass::Unknown)
    }
}

/// Read only a small prefix used to classify a rate-limit response.
///
/// The provider body is deliberately not returned from this helper's callers:
/// it may contain URLs, credentials or other provider diagnostics. The bound
/// also prevents an error response from becoming an unbounded allocation.
pub(crate) async fn read_bounded_rate_limit_body(response: reqwest::Response) -> String {
    let mut body = Vec::with_capacity(
        response
            .content_length()
            .unwrap_or_default()
            .try_into()
            .unwrap_or(0)
            .min(MAX_RATE_LIMIT_BODY_BYTES),
    );
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            break;
        };
        let remaining = MAX_RATE_LIMIT_BODY_BYTES.saturating_sub(body.len());
        if remaining == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    String::from_utf8_lossy(&body).into_owned()
}

impl RetryPolicy {
    /// Loads bounded retry settings from the `EVOHIME_LLM_*` environment variables.
    pub fn from_env() -> Self {
        Self {
            max_retries: env_u32("EVOHIME_LLM_MAX_RETRIES", 3),
            base_delay: Duration::from_millis(env_u64("EVOHIME_LLM_RETRY_BASE_MS", 250).max(1)),
            max_delay: Duration::from_millis(env_u64("EVOHIME_LLM_RETRY_MAX_MS", 5_000).max(1)),
        }
    }

    /// Creates a policy that disables retries.
    pub fn none() -> Self {
        Self {
            max_retries: 0,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(1),
        }
    }

    /// Creates a short-delay policy for deterministic tests.
    pub fn for_tests(max_retries: u32) -> Self {
        Self {
            max_retries,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(50),
        }
    }
}

/// Reports whether an HTTP status is eligible for an initial-request retry.
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
    fn classifies_quota_and_ambiguous_rate_limits() {
        assert_eq!(
            classify_rate_limit(StatusCode::TOO_MANY_REQUESTS, "quota exceeded"),
            Some(RateLimitClass::Exhausted)
        );
        assert_eq!(
            classify_rate_limit(StatusCode::FORBIDDEN, "temporary rate limit; retry later"),
            Some(RateLimitClass::Transient)
        );
        assert_eq!(
            classify_rate_limit(StatusCode::TOO_MANY_REQUESTS, "rate limit"),
            Some(RateLimitClass::Unknown)
        );
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

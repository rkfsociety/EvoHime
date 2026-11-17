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
use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
use reqwest::StatusCode;
use serde_json::Value;
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

/// Markers that identify an exhausted quota or billing limit.
///
/// `"limit exceeded"` is intentionally absent: providers also describe a
/// temporary pacing limit as "rate limit exceeded", and treating that phrase
/// as an exhausted quota disables the retry that would recover from it.
const QUOTA_MARKERS: &[&str] = &[
    "quota",
    "daily limit",
    "monthly limit",
    "hourly limit",
    "billing",
    "insufficient credit",
    "out of credit",
    "credit balance",
    "credit limit",
    "exhausted",
];

/// Markers that identify a temporary pacing limit which retry may clear.
const TRANSIENT_MARKERS: &[&str] = &[
    "retry",
    "temporar",
    "second",
    "minute",
    "cooldown",
    "wait",
    "slow down",
];

/// Upper bound applied to a provider-supplied pacing hint.
const MAX_RATE_LIMIT_DELAY_SECS: u64 = 300;

/// Classifies a provider status/body as a known rate limit, when applicable.
pub fn classify_rate_limit(status: StatusCode, body: &str) -> Option<RateLimitClass> {
    let lower = body.to_ascii_lowercase();
    let rate_limited = status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN && lower.contains("rate limit"));
    if !rate_limited {
        return None;
    }
    if QUOTA_MARKERS.iter().any(|marker| lower.contains(marker)) {
        Some(RateLimitClass::Exhausted)
    } else if TRANSIENT_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
    {
        Some(RateLimitClass::Transient)
    } else {
        Some(RateLimitClass::Unknown)
    }
}

/// Extracts the provider's own pacing hint from a rate-limit body.
///
/// LiteRouter answers an over-eager client with
/// `"Rate limit exceeded for your tier (7 seconds between messages)"`, so the
/// minimum delay between requests is taken from the message when present.
pub fn parse_rate_limit_delay(body: &str) -> Option<Duration> {
    let lower = body.to_ascii_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '.'))
        .filter(|token| !token.is_empty())
        .collect();
    for (index, token) in tokens.iter().enumerate() {
        let Ok(value) = token.parse::<u64>() else {
            continue;
        };
        let Some(unit) = tokens.get(index + 1) else {
            continue;
        };
        let seconds = if unit.starts_with("second") {
            value
        } else if unit.starts_with("minute") {
            value.saturating_mul(60)
        } else {
            continue;
        };
        return Some(Duration::from_secs(seconds.min(MAX_RATE_LIMIT_DELAY_SECS)));
    }
    None
}

/// Reads a bounded provider error body for classification and structured diagnostics.
///
/// Raw body text is returned only to the adapter for in-memory rate-limit
/// classification. Diagnostics retain bounded fields only, and provider error
/// bodies cannot make allocation unbounded.
pub(crate) struct BoundedProviderErrorBody {
    pub(crate) text: String,
    pub(crate) diagnostics: crate::providers::ProviderErrorDiagnostic,
}

pub(crate) async fn read_bounded_provider_error_body(
    response: reqwest::Response,
) -> BoundedProviderErrorBody {
    let request_id = response
        .headers()
        .get("x-request-id")
        .or_else(|| response.headers().get("request-id"))
        .or_else(|| response.headers().get("x-ds-trace-id"))
        .or_else(|| response.headers().get("cf-ray"))
        .and_then(safe_request_id);
    let declared_body_bytes = response.content_length();
    let mut body = Vec::with_capacity(
        declared_body_bytes
            .unwrap_or_default()
            .try_into()
            .unwrap_or(0)
            .min(MAX_RATE_LIMIT_BODY_BYTES),
    );
    let mut stream = response.bytes_stream();
    let mut truncated = false;
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            break;
        };
        let remaining = MAX_RATE_LIMIT_BODY_BYTES.saturating_sub(body.len());
        if remaining == 0 {
            truncated = !chunk.is_empty();
            break;
        }
        let accepted = chunk.len().min(remaining);
        body.extend_from_slice(&chunk[..accepted]);
        if accepted < chunk.len() {
            truncated = true;
            break;
        }
    }
    let text = String::from_utf8_lossy(&body).into_owned();
    let truncated =
        truncated || declared_body_bytes.is_some_and(|declared| declared > body.len() as u64);
    let diagnostics = provider_error_diagnostics(&text, body.len(), truncated, request_id);
    BoundedProviderErrorBody { text, diagnostics }
}

fn provider_error_diagnostics(
    text: &str,
    body_bytes: usize,
    truncated: bool,
    request_id: Option<String>,
) -> crate::providers::ProviderErrorDiagnostic {
    let parsed = serde_json::from_str::<Value>(text).ok();
    let error = parsed
        .as_ref()
        .and_then(|value| value.get("error"))
        .unwrap_or_else(|| parsed.as_ref().unwrap_or(&Value::Null));
    let request_id = request_id.or_else(|| {
        parsed
            .as_ref()
            .and_then(|value| value.get("request_id"))
            .and_then(Value::as_str)
            .and_then(|value| {
                (!value.is_empty()
                    && value.len() <= 128
                    && value.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    }))
                .then(|| value.to_owned())
            })
    });
    crate::providers::ProviderErrorDiagnostic {
        error_type: json_string(error, "type"),
        code: json_scalar_string(error, "code"),
        param: json_scalar_string(error, "param"),
        message: json_string(error, "message"),
        request_id,
        response_body_bytes: body_bytes,
        response_body_truncated: truncated,
        response_body_is_json: parsed.is_some(),
        provider_attempts: 0,
    }
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|value| value.chars().take(512).collect())
}

fn json_scalar_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|value| match value {
        Value::String(value) => Some(value.chars().take(128).collect()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    })
}

fn safe_request_id(value: &HeaderValue) -> Option<String> {
    let value = value.to_str().ok()?;
    (!value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')))
    .then(|| value.to_owned())
}

#[cfg(test)]
mod provider_error_body_tests {
    use super::*;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn bounded_reader_extracts_diagnostics_without_retaining_unknown_body_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(400)
                    .insert_header("x-request-id", "req_safe-123")
                    .set_body_raw(
                        r#"{"error":{"type":"invalid_request_error","code":"context_length_exceeded","param":"messages","message":"too many tokens"},"debug":"do not copy"}"#,
                        "application/json",
                    ),
            )
            .mount(&server)
            .await;
        let response = reqwest::Client::new()
            .get(server.uri())
            .send()
            .await
            .expect("mock response");

        let bounded = read_bounded_provider_error_body(response).await;
        assert!(bounded.text.contains("do not copy"));
        assert_eq!(
            bounded.diagnostics.code.as_deref(),
            Some("context_length_exceeded")
        );
        assert_eq!(
            bounded.diagnostics.request_id.as_deref(),
            Some("req_safe-123")
        );
        let unsafe_header = HeaderValue::from_static("req/with?secret=1");
        assert!(safe_request_id(&unsafe_header).is_none());
    }
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

impl Default for RetryPolicy {
    /// Documented retry defaults, independent of the current environment.
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(250),
            max_delay: Duration::from_millis(5_000),
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
    fn extracts_only_bounded_openai_error_fields() {
        let body = r#"{"error":{"type":"invalid_request_error","code":"context_length_exceeded","param":"messages","message":"Request too large"},"debug":"must not persist"}"#;
        let diagnostic =
            provider_error_diagnostics(body, body.len(), false, Some("req_123".into()));
        assert_eq!(
            diagnostic.error_type.as_deref(),
            Some("invalid_request_error")
        );
        assert_eq!(diagnostic.code.as_deref(), Some("context_length_exceeded"));
        assert_eq!(diagnostic.param.as_deref(), Some("messages"));
        assert_eq!(diagnostic.message.as_deref(), Some("Request too large"));
        assert_eq!(diagnostic.request_id.as_deref(), Some("req_123"));
        assert!(diagnostic.response_body_is_json);
        assert!(!diagnostic.response_body_truncated);
        let serialized = serde_json::to_string(&diagnostic).expect("diagnostics serialize");
        assert!(!serialized.contains("must not persist"));
    }

    #[test]
    fn malformed_or_truncated_error_body_retains_read_metadata() {
        let diagnostic = provider_error_diagnostics("not-json", 16_384, true, None);
        assert_eq!(diagnostic.response_body_bytes, 16_384);
        assert!(diagnostic.response_body_truncated);
        assert!(!diagnostic.response_body_is_json);
        assert!(diagnostic.message.is_none());
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
    fn classifies_tier_pacing_limit_as_transient() {
        assert_eq!(
            classify_rate_limit(
                StatusCode::FORBIDDEN,
                "[LiteRouter] Rate limit exceeded for your tier (7 seconds between messages). \
                 You can upgrade your tier to reduce the cooldown period.",
            ),
            Some(RateLimitClass::Transient)
        );
    }

    #[test]
    fn keeps_real_quota_limits_exhausted() {
        assert_eq!(
            classify_rate_limit(
                StatusCode::TOO_MANY_REQUESTS,
                "quota exceeded for this billing period",
            ),
            Some(RateLimitClass::Exhausted)
        );
        assert_eq!(
            classify_rate_limit(StatusCode::TOO_MANY_REQUESTS, "daily limit reached"),
            Some(RateLimitClass::Exhausted)
        );
    }

    #[test]
    fn parses_provider_pacing_hint() {
        assert_eq!(
            parse_rate_limit_delay(
                "[LiteRouter] Rate limit exceeded for your tier (7 seconds between messages)"
            ),
            Some(Duration::from_secs(7))
        );
        assert_eq!(
            parse_rate_limit_delay("try again in 2 minutes"),
            Some(Duration::from_secs(120))
        );
        assert_eq!(parse_rate_limit_delay("slow down"), None);
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

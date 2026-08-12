//! Drives a real, policy-gated, SSRF-protected HTTP GET through the
//! `research_pipeline` state machine and produces `research::ResearchEvidence`
//! plus a matching `research_pipeline::Citation`.
//!
//! Documented simplifications (Stage 3, this pass only):
//! - No search-engine/query integration: the input is a direct URL, not a
//!   search query.
//! - No LLM-based summarization: the `Summarizing` state is a real pipeline
//!   transition, but the work performed there is bounded truncation plus the
//!   deterministic redaction already implemented by
//!   `research::ResearchEvidence::capture` (which calls
//!   `research::redact_excerpt`). No model-gateway call happens here.
//! - `Extracting` decodes the fetched bytes to UTF-8 text (lossy); it does
//!   not strip HTML markup or boilerplate.
//!
//! SSRF protection is not reimplemented here: every check reuses
//! `evohime_tool_runtime::assert_safe_http_url` verbatim, both before the
//! initial request and again on the final (post-redirect) URL, mirroring
//! `evohime_tool_runtime::tools::http::fetch`.

use std::fmt;
use std::time::{Duration, SystemTime};

use futures_util::StreamExt;
use reqwest::{redirect::Policy, Client, Url};

use crate::research::{ResearchEvidence, SourceMetadata, MAX_EXCERPT_CHARS};
use crate::research_pipeline::{
    transition, validate_citations, Citation, PipelineState, ResearchPolicy, ResearchRequest,
};

/// Outcome of a completed research fetch: the persisted evidence, its
/// citation, and the terminal pipeline state (`Completed` on success).
#[derive(Debug, Clone)]
pub struct ResearchFetchOutcome {
    pub evidence: ResearchEvidence,
    pub citation: Citation,
    pub state: PipelineState,
}

/// A failed or cancelled research fetch. `state` is the terminal pipeline
/// state reached (`Failed` or `Cancelled`) so callers can audit it without
/// re-deriving the transition.
#[derive(Debug, Clone)]
pub struct ResearchFetchError {
    pub state: PipelineState,
    pub message: String,
}

impl fmt::Display for ResearchFetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "research fetch {:?}: {}", self.state, self.message)
    }
}

impl std::error::Error for ResearchFetchError {}

/// Drives `Planned -> Fetching -> Extracting -> Summarizing -> Completed`
/// (or `-> Failed` / `-> Cancelled`) using `research_pipeline::transition`
/// for every state change. Performs a real, SSRF-protected `GET` bounded by
/// `policy.max_bytes` / `policy.max_latency_ms`, then builds and validates a
/// `Citation` before returning the captured `ResearchEvidence`.
pub async fn run_research_fetch(
    request_id: &str,
    url: &str,
    title: &str,
    policy: &ResearchPolicy,
    ttl_ms: u64,
    cancelled: bool,
) -> Result<ResearchFetchOutcome, ResearchFetchError> {
    let mut state = PipelineState::Planned;

    let parsed = Url::parse(url).map_err(|error| fail(state, false, error.to_string()))?;
    let domain = parsed
        .host_str()
        .map(|host| host.to_ascii_lowercase())
        .ok_or_else(|| fail(state, false, "url has no host".to_string()))?;

    let request = ResearchRequest {
        request_id: request_id.to_string(),
        domain: domain.clone(),
        max_bytes: policy.max_bytes,
        max_latency_ms: policy.max_latency_ms,
        max_cost_micros: 0,
        cancelled,
    };
    request
        .validate(policy)
        .map_err(|error| fail(state, is_cancelled(&error), error.to_string()))?;

    state = transition(state, PipelineState::Fetching)
        .map_err(|error| fail(state, false, error.to_string()))?;

    evohime_tool_runtime::assert_safe_http_url(&parsed)
        .map_err(|message| fail(state, false, format!("ssrf blocked url: {message}")))?;

    let client = Client::builder()
        .timeout(Duration::from_millis(policy.max_latency_ms))
        .redirect(Policy::custom(|attempt| {
            if evohime_tool_runtime::assert_safe_http_url(attempt.url()).is_ok() {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .user_agent("EvoHime/0.1 core.research_fetch")
        .build()
        .map_err(|error| fail(state, false, format!("client setup failed: {error}")))?;

    let response = client
        .get(parsed.clone())
        .send()
        .await
        .map_err(|error| fail(state, false, format!("http request failed: {error}")))?;

    let final_url = response.url().clone();
    evohime_tool_runtime::assert_safe_http_url(&final_url)
        .map_err(|message| fail(state, false, format!("ssrf blocked final url: {message}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(fail(
            state,
            false,
            format!("http endpoint returned {status}"),
        ));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| "application/octet-stream".to_string());

    if let Some(declared_len) = response.content_length() {
        if declared_len > policy.max_bytes {
            return Err(fail(
                state,
                false,
                format!(
                    "response declared {declared_len} bytes, exceeds policy limit {}",
                    policy.max_bytes
                ),
            ));
        }
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|error| fail(state, false, format!("failed to read response: {error}")))?;
        body.extend_from_slice(&chunk);
        if body.len() as u64 > policy.max_bytes {
            return Err(fail(
                state,
                false,
                format!(
                    "response body exceeded policy limit {} bytes",
                    policy.max_bytes
                ),
            ));
        }
    }

    state = transition(state, PipelineState::Extracting)
        .map_err(|error| fail(state, false, error.to_string()))?;

    // "Extracting": decode the fetched bytes to UTF-8 text. No HTML
    // boilerplate stripping is performed in this pass.
    let text = String::from_utf8_lossy(&body).into_owned();

    state = transition(state, PipelineState::Summarizing)
        .map_err(|error| fail(state, false, error.to_string()))?;

    // "Summarizing": bounded truncation only. Redaction of secret-shaped
    // tokens happens inside `ResearchEvidence::capture`, which calls
    // `research::redact_excerpt`. No model-gateway call is made here.
    let truncated: String = text.chars().take(MAX_EXCERPT_CHARS).collect();

    let retrieved_at_ms = now_ms();
    let source = SourceMetadata::new(
        final_url.as_str(),
        title,
        domain.as_str(),
        content_type,
        retrieved_at_ms,
    )
    .map_err(|error| fail(state, false, error.to_string()))?;

    let captured_at_ms = now_ms();
    let evidence = ResearchEvidence::capture(source, truncated, captured_at_ms, ttl_ms)
        .map_err(|error| fail(state, false, error.to_string()))?;

    let citation = Citation {
        // `research_pipeline::validate_citations` parses citation URLs with
        // its own minimal host parser, which does not accept a port (it
        // treats `:` as invalid). Ports are dropped here so a fetched URL
        // with a non-default port (as used by local test servers) still
        // produces a citation the pipeline contract can validate; the
        // scheme, host, path, and query are preserved unchanged.
        url: citation_url(&final_url),
        source_hash: crate::research::sha256_hex(&body),
        excerpt_hash: evidence.excerpt_sha256.clone(),
    };
    validate_citations(&domain, std::slice::from_ref(&citation))
        .map_err(|error| fail(state, false, error.to_string()))?;

    let final_state = transition(state, PipelineState::Completed)
        .map_err(|error| fail(state, false, error.to_string()))?;

    Ok(ResearchFetchOutcome {
        evidence,
        citation,
        state: final_state,
    })
}

fn is_cancelled(error: &crate::research_pipeline::PipelineError) -> bool {
    matches!(error, crate::research_pipeline::PipelineError::Cancelled)
}

fn fail(state: PipelineState, cancelled: bool, message: String) -> ResearchFetchError {
    let target = if cancelled {
        PipelineState::Cancelled
    } else {
        PipelineState::Failed
    };
    let final_state = transition(state, target).unwrap_or(target);
    ResearchFetchError {
        state: final_state,
        message,
    }
}

fn citation_url(url: &Url) -> String {
    let mut rendered = format!(
        "{}://{}{}",
        url.scheme(),
        url.host_str().unwrap_or_default(),
        url.path()
    );
    if let Some(query) = url.query() {
        rendered.push('?');
        rendered.push_str(query);
    }
    rendered
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_tool_runtime::lock_private_override;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn policy(allowed_domains: Vec<String>, max_bytes: u64) -> ResearchPolicy {
        ResearchPolicy {
            network_allowed: true,
            allowed_domains,
            max_bytes,
            max_latency_ms: 5_000,
            max_cost_micros: 0,
        }
    }

    fn domain_of(server: &MockServer) -> String {
        Url::parse(&server.uri())
            .expect("mock server uri parses")
            .host_str()
            .expect("mock server has host")
            .to_ascii_lowercase()
    }

    #[tokio::test]
    async fn fetches_and_produces_redacted_evidence_and_citation() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/article"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("Useful finding sk-secret alice@example.test")
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&server)
            .await;
        let _private = lock_private_override(Some(true));

        let domain = domain_of(&server);
        let outcome = run_research_fetch(
            "req-1",
            &format!("{}/article", server.uri()),
            "Example Article",
            &policy(vec![domain], 4096),
            3_600_000,
            false,
        )
        .await
        .expect("fetch succeeds");

        assert_eq!(outcome.state, PipelineState::Completed);
        assert_eq!(
            outcome.evidence.excerpt,
            "Useful finding [REDACTED] [REDACTED]"
        );
        assert_eq!(
            outcome.evidence.excerpt_sha256,
            outcome.citation.excerpt_hash
        );
        assert_eq!(outcome.citation.source_hash.len(), 64);
        assert!(outcome
            .citation
            .source_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn rejects_domain_not_in_allowlist_without_any_network_call() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/article"))
            .respond_with(ResponseTemplate::new(200).set_body_string("should not be fetched"))
            .mount(&server)
            .await;
        let _private = lock_private_override(Some(true));

        let error = run_research_fetch(
            "req-2",
            &format!("{}/article", server.uri()),
            "Example Article",
            &policy(vec!["not-the-mock-domain.example".to_string()], 4096),
            3_600_000,
            false,
        )
        .await
        .expect_err("domain must be denied");

        assert_eq!(error.state, PipelineState::Failed);
        assert!(error.message.contains("domain"));
        assert_eq!(
            server
                .received_requests()
                .await
                .expect("requests tracked")
                .len(),
            0
        );
    }

    #[tokio::test]
    async fn rejects_ssrf_blocked_targets_without_the_override() {
        let _private = lock_private_override(Some(false));

        let error = run_research_fetch(
            "req-3",
            "http://127.0.0.1:9/",
            "Loopback",
            &policy(vec!["127.0.0.1".to_string()], 4096),
            3_600_000,
            false,
        )
        .await
        .expect_err("loopback must be blocked");

        assert_eq!(error.state, PipelineState::Failed);
        assert!(error.message.contains("ssrf"));
    }

    #[tokio::test]
    async fn rejects_oversized_responses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/big"))
            .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(4_096)))
            .mount(&server)
            .await;
        let _private = lock_private_override(Some(true));

        let domain = domain_of(&server);
        let error = run_research_fetch(
            "req-4",
            &format!("{}/big", server.uri()),
            "Big Article",
            &policy(vec![domain], 16),
            3_600_000,
            false,
        )
        .await
        .expect_err("oversized response must be rejected");

        assert_eq!(error.state, PipelineState::Failed);
        assert!(error.message.contains("exceed") || error.message.contains("declared"));
    }
}

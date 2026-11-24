use crate::provider_contract::{ImageOutputCapability, ImageProviderRequest, ProviderImageOutput};
use crate::tools::{ChatResult, ChatStreamItem, ToolSpec};
use futures_util::Stream;
use std::future::Future;
use std::pin::Pin;

/// Cloudflare Workers AI catalog discovery for its OpenAI-compatible transport.
pub mod cloudflare_workers_ai;
/// LiteRouter provider adapter.
pub mod literouter;
/// Supervisor-authenticated local model adapter.
pub mod local;
/// Deterministic mock provider used by tests.
pub mod mock;
/// Ollama provider adapter.
pub mod ollama;
/// Generic OpenAI-compatible chat-completions adapter.
pub mod openai_compatible;
/// OpenAI Responses API adapter.
pub mod openai_responses;

pub use crate::tools::LlmUsage;
use serde::{Deserialize, Serialize};

/// Supported provider implementations and wire protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// LiteRouter hosted OpenAI-compatible API.
    LiteRouter,
    /// Any provider implementing the OpenAI-compatible chat protocol.
    OpenAICompatible,
    /// Provider implementing the OpenAI Responses protocol.
    OpenAIResponses,
    /// Local Ollama service.
    Ollama,
    /// Supervisor-authenticated local model service.
    Local,
    /// Test-only deterministic adapter.
    #[serde(skip)]
    Mock,
}

impl ProviderKind {
    /// Parses a provider name or supported spelling alias.
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "literouter" | "lite_router" | "lite-router" => Some(Self::LiteRouter),
            "openai_compatible" | "openai-compatible" | "openai" => Some(Self::OpenAICompatible),
            "openai_responses" | "openai-responses" | "responses" => Some(Self::OpenAIResponses),
            "ollama" => Some(Self::Ollama),
            "local" | "local_slm" | "local-slm" => Some(Self::Local),
            "mock" => Some(Self::Mock),
            _ => None,
        }
    }

    /// Returns the stable serialized provider identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LiteRouter => "literouter",
            Self::OpenAICompatible => "openai_compatible",
            Self::OpenAIResponses => "openai_responses",
            Self::Ollama => "ollama",
            Self::Local => "local",
            Self::Mock => "mock",
        }
    }

    /// Wave 3B: Check if provider supports extended thinking
    pub fn supports_thinking(self) -> bool {
        matches!(
            self,
            Self::LiteRouter | Self::OpenAIResponses | Self::Local | Self::Mock
        )
    }
}

/// Conversation role attached to a provider message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    /// System instruction supplied by the caller.
    System,
    /// User-authored message.
    User,
    /// Assistant-authored message, possibly containing tool calls.
    Assistant,
    /// Observation returned by a tool call.
    Tool,
}

impl ChatRole {
    /// Returns the stable serialized role name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

/// One conversation message exchanged with a model provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Semantic role of the message.
    pub role: ChatRole,
    /// Text content associated with the message.
    pub content: String,
    /// Native tool calls requested by an assistant message.
    pub tool_calls: Vec<crate::tools::NativeToolCall>,
    /// Tool-call identifier for a tool observation.
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    /// Creates a plain-text message with no tool metadata.
    pub fn text(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// Creates an assistant message containing native tool-call requests.
    pub fn assistant_tool_calls(
        content: impl Into<String>,
        tool_calls: Vec<crate::tools::NativeToolCall>,
    ) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
            tool_calls,
            tool_call_id: None,
        }
    }

    /// Creates a tool observation associated with the originating call ID.
    pub fn tool_observation(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// Sendable stream of provider output chunks or errors.
pub type TokenStream = Pin<Box<dyn Stream<Item = Result<ChatStreamItem, ProviderError>> + Send>>;
/// Sendable future resolving to a non-streaming provider response.
pub type ChatFuture = Pin<Box<dyn Future<Output = Result<ChatResult, ProviderError>> + Send>>;

/// Optional extended-thinking configuration for providers that support it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    /// Provider-specific thinking mode identifier.
    pub kind: String, // "enabled"
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Optional upper bound for thinking tokens.
    pub budget_tokens: Option<u32>,
}

/// Optional per-call bounds for a non-streaming provider completion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChatRequestOptions {
    /// Maximum generated tokens when the provider supports the field.
    pub max_output_tokens: Option<u32>,
    /// Maximum retries after the first attempt; `Some(0)` disables retries.
    pub max_retries: Option<u32>,
}

/// Configuration, transport, API, or stream failure returned by a provider adapter.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// Provider configuration or request parameters are invalid.
    #[error("configuration error: {0}")]
    Config(String),
    /// HTTP transport failed before a valid response was received.
    #[error("http error: {0}")]
    Http(String),
    /// Provider returned an API-level error response.
    #[error("api error: {0}")]
    Api(String),
    /// Provider rejected a request and supplied bounded structured diagnostics.
    #[error("{message}")]
    ApiWithDiagnostics {
        /// Fixed safe status summary, without provider-controlled message text.
        message: String,
        /// Stable safe failure code for task result projection.
        error_code: &'static str,
        /// HTTP status retained separately from the provider-controlled payload.
        status: u16,
        /// Bounded provider metadata retained for Core's redacted trace projection.
        diagnostics: Box<ProviderErrorDiagnostic>,
    },
    /// Provider output stream failed or was malformed.
    #[error("streaming error: {0}")]
    Stream(String),
    /// Route preflight rejected the image request before adapter dispatch.
    #[error("image route preflight rejected")]
    ImagePreflightRejected,
    /// The provider changed its image capability after Core froze the request.
    #[error("image capability epoch is stale")]
    ImageCapabilityStale,
    /// All ranked fallback models are currently rate-limited or exhausted.
    #[error("all models exhausted")]
    AllModelsExhausted,
}

/// Bounded, structured diagnostics extracted from a rejected provider response.
///
/// Values are provider-controlled and must be redacted before persistent logging.
/// Request/response bodies and credentials are never attached as fields.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderErrorDiagnostic {
    /// Stable error type reported by an OpenAI-compatible provider.
    pub error_type: Option<String>,
    /// Stable provider error code, when supplied.
    pub code: Option<String>,
    /// Request parameter the provider rejected, when supplied.
    pub param: Option<String>,
    /// Bounded provider message; redact before persistence.
    pub message: Option<String>,
    /// Provider-issued request identifier, if it has a safe bounded form.
    pub request_id: Option<String>,
    /// Number of response-body bytes captured, capped by the adapter read limit.
    pub response_body_bytes: usize,
    /// Whether the adapter reached its response-body read limit.
    pub response_body_truncated: bool,
    /// Whether the response body parsed as JSON.
    pub response_body_is_json: bool,
    /// Number of provider HTTP requests made for this call, including retries.
    pub provider_attempts: u32,
}

impl std::fmt::Debug for ProviderErrorDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderErrorDiagnostic")
            .field("error_type_present", &self.error_type.is_some())
            .field("code_present", &self.code.is_some())
            .field("param_present", &self.param.is_some())
            .field("message_present", &self.message.is_some())
            .field("request_id_present", &self.request_id.is_some())
            .field("response_body_bytes", &self.response_body_bytes)
            .field("response_body_truncated", &self.response_body_truncated)
            .field("response_body_is_json", &self.response_body_is_json)
            .field("provider_attempts", &self.provider_attempts)
            .finish()
    }
}

/// Maximum length of the provider message prefix inspected for an HTTP status.
const HTTP_STATUS_PREFIX_CHARS: usize = 3;

impl ProviderError {
    /// Creates an API error with bounded structured diagnostics attached for Core.
    ///
    /// The supplied diagnostic fields remain internal to this error variant;
    /// formatted and debug representations contain only stable safe codes.
    pub fn api_with_diagnostics(status: u16, diagnostics: &ProviderErrorDiagnostic) -> Self {
        if !(100..=599).contains(&status) {
            return Self::Api("provider request failed".into());
        }
        let provider_markers = [
            diagnostics.code.as_deref(),
            diagnostics.error_type.as_deref(),
            diagnostics.message.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
        let structured_markers = format!(
            "{} {}",
            diagnostics
                .code
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase(),
            diagnostics
                .error_type
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase()
        );
        let code = match status {
            401 => "provider_auth_rejected",
            403 if provider_markers.contains("rate limit")
                || structured_markers.contains("rate_limit") =>
            {
                "provider_rate_limited"
            }
            403 => "provider_auth_rejected",
            402 => "provider_quota_exhausted",
            408 => "provider_timeout",
            429 if provider_markers.contains("quota") => "provider_quota_exhausted",
            429 => "provider_rate_limited",
            500..=599 => "provider_unavailable",
            404 => "provider_model_not_found",
            _ if structured_markers.contains("model_not_found")
                || structured_markers.contains("model_not_exist") =>
            {
                "provider_model_not_found"
            }
            _ if [
                "context_length",
                "too_many_tokens",
                "max_tokens",
                "context_window",
                "request_too_large",
            ]
            .iter()
            .any(|marker| structured_markers.contains(marker))
                || [
                    "context length",
                    "too many tokens",
                    "maximum context",
                    "context window",
                    "token limit",
                    "too large",
                ]
                .iter()
                .any(|marker| provider_markers.contains(marker)) =>
            {
                "provider_context_length_exceeded"
            }
            _ if structured_markers.contains("unsupported_parameter") => {
                "provider_unsupported_parameter"
            }
            _ if status == 400 => "provider_request_rejected_http_400",
            _ => "provider_request_rejected",
        };
        let safe_message = match code {
            "provider_model_not_found" => "provider model not found",
            "provider_context_length_exceeded" => "provider context length exceeded",
            "provider_unsupported_parameter" => "provider rejected unsupported parameter",
            "provider_quota_exhausted" => "provider quota exhausted",
            "provider_auth_rejected" => "provider authentication rejected",
            "provider_timeout" => "provider request timed out",
            "provider_rate_limited" => "provider rate limit exceeded",
            "provider_unavailable" => "provider unavailable",
            "provider_request_rejected_http_400" => "provider request rejected (HTTP 400)",
            _ => "provider request rejected",
        };
        Self::ApiWithDiagnostics {
            message: if code == "provider_request_rejected" {
                format!("{safe_message} (HTTP {status})")
            } else {
                safe_message.into()
            },
            error_code: code,
            status,
            diagnostics: Box::new(diagnostics.clone()),
        }
    }

    /// Returns structured diagnostics attached by an HTTP response adapter.
    pub fn diagnostics(&self) -> Option<ProviderErrorDiagnostic> {
        match self {
            Self::ApiWithDiagnostics { diagnostics, .. } => Some((**diagnostics).clone()),
            _ => None,
        }
    }

    /// Returns a lowercased message used only for internal failure classification.
    fn classification_message(&self) -> Option<String> {
        let message = match self {
            Self::Http(message) | Self::Api(message) | Self::Stream(message) => {
                Some(message.as_str())
            }
            Self::ApiWithDiagnostics { diagnostics, .. } => diagnostics.message.as_deref(),
            Self::Config(_)
            | Self::ImagePreflightRejected
            | Self::ImageCapabilityStale
            | Self::AllModelsExhausted => None,
        }?;
        Some(message.to_ascii_lowercase())
    }

    /// Returns the HTTP status supplied by an adapter, when available.
    ///
    /// Structured HTTP errors retain status separately. Legacy errors may
    /// provide it as a numeric message prefix; transport/configuration errors
    /// without such a prefix have no status.
    pub fn http_status(&self) -> Option<u16> {
        let message = match self {
            Self::Http(message) | Self::Api(message) | Self::Stream(message) => message,
            Self::ApiWithDiagnostics { status, .. } => return Some(*status),
            Self::Config(_)
            | Self::ImagePreflightRejected
            | Self::ImageCapabilityStale
            | Self::AllModelsExhausted => return None,
        };
        let prefix = message.split(':').next()?.trim();
        if prefix.len() != HTTP_STATUS_PREFIX_CHARS
            || !prefix.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let status = prefix.parse::<u16>().ok()?;
        (100..=599).contains(&status).then_some(status)
    }

    /// Returns the bounded stable failure code shown in diagnostics and the UI.
    ///
    /// The code never contains provider text: only fixed tokens derived from
    /// the adapter error class, embedded HTTP status and bounded diagnostic
    /// message markers. Unrecognized failures fall back to adapter-level codes.
    pub fn failure_code(&self) -> &'static str {
        match self {
            Self::Config(_) => "provider_configuration",
            Self::ImagePreflightRejected => "image_preflight_rejected",
            Self::ImageCapabilityStale => "image_capability_stale",
            Self::AllModelsExhausted => "all_models_exhausted",
            Self::ApiWithDiagnostics { error_code, .. } => error_code,
            Self::Http(_) | Self::Api(_) | Self::Stream(_) => {
                classify_failure_message(self, &self.classification_message().unwrap_or_default())
            }
        }
    }
}

/// Maps a provider message and its embedded status to a stable failure code.
fn classify_failure_message(error: &ProviderError, message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    let status = error.http_status();
    if lower.contains("timeout") || lower.contains("timed out") || status == Some(408) {
        "provider_timeout"
    } else if lower.contains("connect") || lower.contains("dns") || lower.contains("unreachable") {
        "provider_unreachable"
    } else if lower.contains("quota")
        || lower.contains("insufficient")
        || lower.contains("balance")
        || lower.contains("billing")
        || status == Some(402)
    {
        "provider_quota_exhausted"
    } else if lower.contains("rate limit") || status == Some(429) {
        "provider_rate_limited"
    } else if lower.contains("model not exist")
        || lower.contains("model_not_found")
        || lower.contains("does not exist")
        || status == Some(404)
    {
        "provider_model_not_found"
    } else if matches!(status, Some(401 | 403)) {
        "provider_auth_rejected"
    } else if status.is_some_and(|status| status >= 500) {
        "provider_unavailable"
    } else if status.is_some_and(|status| (400..500).contains(&status)) {
        "provider_request_rejected"
    } else if matches!(error, ProviderError::Stream(_)) {
        "provider_stream"
    } else if matches!(error, ProviderError::Http(_)) {
        "provider_transport"
    } else {
        "provider_api"
    }
}

/// Публичный trait `ModelProvider` для общего контракта поведения.
pub trait ModelProvider: Send + Sync {
    /// Returns the provider kind implemented by this adapter.
    fn kind(&self) -> ProviderKind;
    /// Returns the configured default model name.
    fn model_name(&self) -> &str;
    /// Returns the configured provider endpoint.
    fn base_url(&self) -> &str;

    /// Returns bounded image-output support; existing adapters remain unsupported by default.
    fn image_output_capability(&self) -> Option<ImageOutputCapability> {
        None
    }

    /// Performs an image request for adapters that advertise the operation.
    ///
    /// Implementations must return encoded bytes only; Core validates the
    /// declared MIME type and fully decodes each image before publication.
    fn generate_image<'a>(&'a self, _request: ImageProviderRequest) -> ImageOutputFuture<'a> {
        Box::pin(async { Err(ProviderError::Config("image_output_unsupported".into())) })
    }

    /// Reports native structured-output support; defaults to false.
    fn supports_structured_output(&self) -> bool {
        false
    }

    /// Reports whether this adapter implements the native tool-call contract.
    fn supports_tool_calls(&self) -> bool {
        false
    }

    /// Starts a streaming conversation using the provider's configured model.
    fn stream_chat(&self, messages: &[ChatMessage]) -> TokenStream;

    /// Starts a streaming conversation with an explicit model override.
    fn stream_chat_with_model(&self, model: &str, messages: &[ChatMessage]) -> TokenStream {
        let _ = model;
        self.stream_chat(messages)
    }

    /// Stream with optional extended thinking support (Wave 3B).
    /// Default implementation ignores thinking config (for providers that don't support it).
    /// Providers that support thinking should override this method.
    fn stream_with_thinking(
        &self,
        messages: &[ChatMessage],
        thinking: Option<ThinkingConfig>,
        tools: Option<&[ToolSpec]>,
    ) -> TokenStream {
        let _ = thinking;
        let _ = tools;
        self.stream_chat(messages)
    }

    /// Non-streaming OpenAI-compatible completion with optional `tools` (Stage 7.28).
    fn chat_with_tools(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> ChatFuture {
        let _ = (model, messages, tools);
        Box::pin(async {
            Err(ProviderError::Config(
                "provider does not support native tool_calls".into(),
            ))
        })
    }

    /// Non-streaming completion with explicit bounded-request options.
    ///
    /// Providers that do not support the options retain their existing
    /// behavior; callers that require a hard bound must use a supporting
    /// adapter and validate the returned usage.
    fn chat_with_tools_with_options(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        options: ChatRequestOptions,
    ) -> ChatFuture {
        let _ = options;
        self.chat_with_tools(model, messages, tools)
    }
}

/// Future returned by an image-output provider call.
pub type ImageOutputFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<ProviderImageOutput>, ProviderError>> + Send + 'a>>;

pub(crate) fn current_unix_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(i64::MAX)
}

pub(crate) fn compute_request_limit_delay(
    timestamps: &mut std::collections::VecDeque<(std::time::Instant, i64)>,
    limits: &crate::config::ModelRateLimitConfig,
    reset_schedule: crate::config::LimitResetSchedule,
    now_instant: std::time::Instant,
    now_ms: i64,
) -> Option<std::time::Duration> {
    while timestamps.front().is_some_and(|(started, _)| {
        now_instant.duration_since(*started) >= std::time::Duration::from_secs(86_400)
    }) {
        timestamps.pop_front();
    }
    if reset_schedule.enabled {
        let reset_start_ms = reset_schedule.window_start_ms(now_ms);
        while timestamps
            .front()
            .is_some_and(|(_, recorded_ms)| *recorded_ms < reset_start_ms)
        {
            timestamps.pop_front();
        }
    }
    let mut delay: Option<std::time::Duration> = None;
    for (cap, window) in limits.request_windows() {
        if cap == 0 {
            continue;
        }
        let count = timestamps
            .iter()
            .filter(|(started, _)| now_instant.duration_since(*started) < window)
            .count();
        if count >= cap as usize {
            if let Some(wait) = timestamps
                .iter()
                .filter(|(started, _)| now_instant.duration_since(*started) < window)
                .nth(count - cap as usize)
                .map(|(started, _)| window.saturating_sub(now_instant.duration_since(*started)))
            {
                delay = Some(delay.map_or(wait, |current| current.max(wait)));
            }
        }
    }
    if reset_schedule.enabled {
        if let Some(current_wait) = delay {
            let until_reset_ms = reset_schedule.next_reset_ms(now_ms).saturating_sub(now_ms);
            let until_reset =
                std::time::Duration::from_millis(u64::try_from(until_reset_ms.max(0)).unwrap_or(0));
            delay = Some(current_wait.min(until_reset));
        }
    }
    delay
}

#[cfg(test)]
mod failure_code_tests {
    use super::{ProviderError, ProviderErrorDiagnostic};

    #[test]
    fn reads_http_status_only_from_adapter_prefixed_messages() {
        assert_eq!(
            ProviderError::Api("401: provider request failed".into()).http_status(),
            Some(401)
        );
        assert_eq!(
            ProviderError::Http("503: provider request failed".into()).http_status(),
            Some(503)
        );
        assert_eq!(
            ProviderError::Http("model timeout after 120 seconds".into()).http_status(),
            None
        );
        assert_eq!(
            ProviderError::Http("error sending request for url https://provider.test/v1".into())
                .http_status(),
            None
        );
        assert_eq!(
            ProviderError::Config("provider_route_not_configured".into()).http_status(),
            None
        );
    }

    #[test]
    fn maps_rejected_credentials_and_models_to_stable_codes() {
        let cases = [
            (
                ProviderError::Api("401: provider request failed".into()),
                "provider_auth_rejected",
            ),
            (
                ProviderError::Api("403: provider request failed".into()),
                "provider_auth_rejected",
            ),
            (
                ProviderError::Api("402: provider request failed".into()),
                "provider_quota_exhausted",
            ),
            (
                ProviderError::Api("404: provider request failed".into()),
                "provider_model_not_found",
            ),
            (
                ProviderError::Api("429: rate limit".into()),
                "provider_rate_limited",
            ),
            (
                ProviderError::Api("400: provider request failed".into()),
                "provider_request_rejected",
            ),
            (
                ProviderError::Api("500: provider request failed".into()),
                "provider_unavailable",
            ),
            (
                ProviderError::Http("model timeout after 120 seconds".into()),
                "provider_timeout",
            ),
            (
                ProviderError::Http("error sending request: connect error".into()),
                "provider_unreachable",
            ),
            (
                ProviderError::Api("provider request failed".into()),
                "provider_api",
            ),
            (
                ProviderError::Config("provider_route_not_configured".into()),
                "provider_configuration",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.failure_code(), expected, "{error}");
        }
    }

    #[test]
    fn failure_codes_never_repeat_provider_text() {
        let error = ProviderError::Api(
            "401: provider request failed for https://provider.test/v1?token=secret".into(),
        );
        assert_eq!(error.failure_code(), "provider_auth_rejected");
        assert!(!error.failure_code().contains("provider.test"));
        assert!(!error.failure_code().contains("secret"));
    }

    #[test]
    fn structured_provider_diagnostics_preserve_status_and_failure_classification() {
        let diagnostics = ProviderErrorDiagnostic {
            error_type: Some("invalid_request_error".into()),
            code: Some("context_length_exceeded".into()),
            param: Some("messages".into()),
            message: Some("maximum context length is 65536 tokens".into()),
            request_id: Some("req_123".into()),
            response_body_bytes: 128,
            response_body_truncated: false,
            response_body_is_json: true,
            provider_attempts: 1,
        };
        let error = ProviderError::api_with_diagnostics(400, &diagnostics);
        assert_eq!(error.http_status(), Some(400));
        assert_eq!(error.failure_code(), "provider_context_length_exceeded");
        assert_eq!(error.to_string(), "provider context length exceeded");
        assert_eq!(error.diagnostics(), Some(diagnostics));
        assert!(format!("{error:?}").contains("provider_context_length_exceeded"));
        assert!(!error.to_string().contains("context_length_exceeded"));
        assert!(!format!("{error:?}").contains("maximum context length"));
    }

    #[test]
    fn diagnostic_message_improves_request_rejection_classification() {
        let diagnostics = ProviderErrorDiagnostic {
            error_type: Some("invalid_request_error".into()),
            code: Some("model_not_found".into()),
            param: Some("model".into()),
            message: Some("The model does not exist".into()),
            request_id: None,
            response_body_bytes: 64,
            response_body_truncated: false,
            response_body_is_json: true,
            provider_attempts: 1,
        };
        let error = ProviderError::api_with_diagnostics(400, &diagnostics);
        assert_eq!(error.failure_code(), "provider_model_not_found");
        assert_eq!(error.to_string(), "provider model not found");
        assert_eq!(
            ProviderError::api_with_diagnostics(404, &diagnostics).failure_code(),
            "provider_model_not_found"
        );
    }

    #[test]
    fn context_limit_and_unsupported_parameter_have_actionable_codes() {
        let mut diagnostics = ProviderErrorDiagnostic {
            error_type: Some("invalid_request_error".into()),
            code: Some("context_length_exceeded".into()),
            param: Some("messages".into()),
            message: Some("maximum context length exceeded".into()),
            request_id: None,
            response_body_bytes: 64,
            response_body_truncated: false,
            response_body_is_json: true,
            provider_attempts: 1,
        };
        assert_eq!(
            ProviderError::api_with_diagnostics(400, &diagnostics).failure_code(),
            "provider_context_length_exceeded"
        );
        diagnostics.code = Some("unsupported_parameter".into());
        diagnostics.message = Some("unsupported parameter: reasoning_effort".into());
        assert_eq!(
            ProviderError::api_with_diagnostics(400, &diagnostics).failure_code(),
            "provider_unsupported_parameter"
        );
    }

    #[test]
    fn retryable_http_statuses_keep_their_resilience_classification() {
        let diagnostics = ProviderErrorDiagnostic {
            error_type: None,
            code: None,
            param: None,
            message: Some("temporary provider error".into()),
            request_id: None,
            response_body_bytes: 32,
            response_body_truncated: false,
            response_body_is_json: false,
            provider_attempts: 1,
        };
        for status in [408, 429, 500, 502, 503, 504] {
            let error = ProviderError::api_with_diagnostics(status, &diagnostics);
            assert!(matches!(
                crate::provider_contract::classify_provider_error(&error),
                crate::provider_contract::FailureCategory::Timeout
                    | crate::provider_contract::FailureCategory::RateLimited
                    | crate::provider_contract::FailureCategory::ServerError
            ));
        }
        assert_eq!(
            ProviderError::api_with_diagnostics(429, &diagnostics).failure_code(),
            "provider_rate_limited"
        );
        assert_eq!(
            ProviderError::api_with_diagnostics(503, &diagnostics).failure_code(),
            "provider_unavailable"
        );
    }

    #[test]
    fn diagnostic_error_formatting_never_contains_provider_supplied_text() {
        let diagnostics = ProviderErrorDiagnostic {
            error_type: Some("invalid_request_error".into()),
            code: Some("context_length_exceeded".into()),
            param: Some("api_key=secret".into()),
            message: Some(
                "token=secret at https://provider.test/?key=leak Authorization: Bearer xyz".into(),
            ),
            request_id: Some("secret-request-id".into()),
            response_body_bytes: 256,
            response_body_truncated: false,
            response_body_is_json: true,
            provider_attempts: 2,
        };
        let error = ProviderError::api_with_diagnostics(400, &diagnostics);
        assert!(!error.to_string().contains("secret"));
        assert!(!error.to_string().contains("provider.test"));
        assert!(!format!("{error:?}").contains("secret"));
        assert!(!format!("{error:?}").contains("provider.test"));
    }
}

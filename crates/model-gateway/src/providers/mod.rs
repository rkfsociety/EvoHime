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
    /// Provider output stream failed or was malformed.
    #[error("streaming error: {0}")]
    Stream(String),
    /// Route preflight rejected the image request before adapter dispatch.
    #[error("image route preflight rejected")]
    ImagePreflightRejected,
    /// The provider changed its image capability after Core froze the request.
    #[error("image capability epoch is stale")]
    ImageCapabilityStale,
}

/// Maximum length of the provider message prefix inspected for an HTTP status.
const HTTP_STATUS_PREFIX_CHARS: usize = 3;

impl ProviderError {
    /// Returns the HTTP status code the adapter embedded in the error text.
    ///
    /// Adapters format API failures as `"<status>: message"`. Transport
    /// failures, configuration errors and provider text without that prefix
    /// have no status, so callers must treat `None` as "unknown".
    pub fn http_status(&self) -> Option<u16> {
        let message = match self {
            Self::Http(message) | Self::Api(message) | Self::Stream(message) => message,
            Self::Config(_) | Self::ImagePreflightRejected | Self::ImageCapabilityStale => {
                return None
            }
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
    /// the adapter error class, the embedded HTTP status and a small set of
    /// well-known provider markers. Unrecognized failures fall back to the
    /// adapter-level codes so they stay distinguishable.
    pub fn failure_code(&self) -> &'static str {
        match self {
            Self::Config(_) => "provider_configuration",
            Self::ImagePreflightRejected => "image_preflight_rejected",
            Self::ImageCapabilityStale => "image_capability_stale",
            Self::Http(message) | Self::Api(message) | Self::Stream(message) => {
                classify_failure_message(self, message)
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

#[cfg(test)]
mod failure_code_tests {
    use super::ProviderError;

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
}

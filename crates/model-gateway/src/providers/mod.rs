use crate::tools::{ChatResult, ChatStreamItem, ToolSpec};
use futures_util::Stream;
use std::future::Future;
use std::pin::Pin;

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
}

/// Публичный trait `ModelProvider` для общего контракта поведения.
pub trait ModelProvider: Send + Sync {
    /// Returns the provider kind implemented by this adapter.
    fn kind(&self) -> ProviderKind;
    /// Returns the configured default model name.
    fn model_name(&self) -> &str;
    /// Returns the configured provider endpoint.
    fn base_url(&self) -> &str;

    /// Reports native structured-output support; defaults to false.
    fn supports_structured_output(&self) -> bool {
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
}

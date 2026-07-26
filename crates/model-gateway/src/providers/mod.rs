use crate::tools::{ChatResult, ChatStreamItem, ToolSpec};
use futures_util::Stream;
use std::future::Future;
use std::pin::Pin;

pub mod literouter;
pub mod mock;
pub mod openai_compatible;

pub use crate::tools::LlmUsage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    LiteRouter,
    OpenAICompatible,
    #[serde(skip)]
    Mock,
}

impl ProviderKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "literouter" | "lite_router" | "lite-router" => Some(Self::LiteRouter),
            "openai_compatible" | "openai-compatible" | "openai" => Some(Self::OpenAICompatible),
            "mock" => Some(Self::Mock),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::LiteRouter => "literouter",
            Self::OpenAICompatible => "openai_compatible",
            Self::Mock => "mock",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

impl ChatRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub tool_calls: Vec<crate::tools::NativeToolCall>,
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn text(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

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

    pub fn tool_observation(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

pub type TokenStream = Pin<Box<dyn Stream<Item = Result<ChatStreamItem, ProviderError>> + Send>>;
pub type ChatFuture = Pin<Box<dyn Future<Output = Result<ChatResult, ProviderError>> + Send>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub kind: String,  // "enabled"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("http error: {0}")]
    Http(String),
    #[error("api error: {0}")]
    Api(String),
    #[error("streaming error: {0}")]
    Stream(String),
}

pub trait ModelProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn model_name(&self) -> &str;
    fn base_url(&self) -> &str;

    fn stream_chat(&self, messages: &[ChatMessage]) -> TokenStream;

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

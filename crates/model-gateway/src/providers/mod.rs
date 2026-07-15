use futures_util::Stream;
use std::pin::Pin;

pub mod literouter;
pub mod mock;

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
}

impl ChatRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

pub type TokenStream = Pin<Box<dyn Stream<Item = Result<String, ProviderError>> + Send>>;

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
}

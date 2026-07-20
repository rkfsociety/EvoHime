use crate::providers::{ChatFuture, ChatMessage, ModelProvider, ProviderKind, TokenStream};
use crate::tools::{ChatResult, ChatStreamItem, NativeToolCall, ToolSpec};
use async_stream::stream;
use std::sync::Arc;

/// Deterministic provider for tests.
pub struct MockProvider {
    model: String,
    chunks: Arc<Vec<String>>,
    tool_calls: Arc<Vec<NativeToolCall>>,
}

impl MockProvider {
    pub fn new(model: impl Into<String>, chunks: Vec<String>) -> Self {
        Self {
            model: model.into(),
            chunks: Arc::new(chunks),
            tool_calls: Arc::new(Vec::new()),
        }
    }

    pub fn with_tool_calls(
        model: impl Into<String>,
        chunks: Vec<String>,
        tool_calls: Vec<NativeToolCall>,
    ) -> Self {
        Self {
            model: model.into(),
            chunks: Arc::new(chunks),
            tool_calls: Arc::new(tool_calls),
        }
    }
}

impl ModelProvider for MockProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Mock
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn base_url(&self) -> &str {
        "mock://local"
    }

    fn stream_chat(&self, _messages: &[ChatMessage]) -> TokenStream {
        let chunks = self.chunks.clone();
        Box::pin(stream! {
            for chunk in chunks.iter() {
                yield Ok(ChatStreamItem::Delta(chunk.clone()));
            }
        })
    }

    fn chat_with_tools(
        &self,
        _model: Option<&str>,
        _messages: &[ChatMessage],
        _tools: &[ToolSpec],
    ) -> ChatFuture {
        let content = self.chunks.join("");
        let tool_calls = (*self.tool_calls).clone();
        Box::pin(async move {
            Ok(ChatResult {
                content,
                tool_calls,
                usage: None,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ChatRole;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn streams_configured_chunks() {
        let provider = MockProvider::new("mock-model", vec!["Hello".into(), " world".into()]);
        let mut stream = provider.stream_chat(&[ChatMessage::text(ChatRole::User, "hi")]);

        let first = stream.next().await.unwrap().unwrap();
        let second = stream.next().await.unwrap().unwrap();
        assert_eq!(first, ChatStreamItem::Delta("Hello".into()));
        assert_eq!(second, ChatStreamItem::Delta(" world".into()));
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn chat_with_tools_returns_configured_calls() {
        let provider = MockProvider::with_tool_calls(
            "mock-model",
            vec![],
            vec![NativeToolCall {
                id: "c1".into(),
                name: "filesystem.read".into(),
                arguments: r#"{"path":"a.txt"}"#.into(),
            }],
        );
        let result = provider.chat_with_tools(None, &[], &[]).await.expect("ok");
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "filesystem.read");
    }
}

use crate::providers::{ChatFuture, ChatMessage, ModelProvider, ProviderKind, TokenStream};
use crate::tools::{ChatResult, ChatStreamItem, NativeToolCall, ToolSpec};
use async_stream::stream;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Deterministic provider for tests.
pub struct MockProvider {
    model: String,
    chunks: Arc<Vec<String>>,
    tool_calls: Arc<Vec<NativeToolCall>>,
    tool_call_sequence: Arc<Mutex<VecDeque<ChatResult>>>,
}

impl MockProvider {
    pub fn new(model: impl Into<String>, chunks: Vec<String>) -> Self {
        Self {
            model: model.into(),
            chunks: Arc::new(chunks),
            tool_calls: Arc::new(Vec::new()),
            tool_call_sequence: Arc::new(Mutex::new(VecDeque::new())),
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
            tool_call_sequence: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn with_tool_call_sequence(model: impl Into<String>, sequence: Vec<ChatResult>) -> Self {
        Self {
            model: model.into(),
            chunks: Arc::new(Vec::new()),
            tool_calls: Arc::new(Vec::new()),
            tool_call_sequence: Arc::new(Mutex::new(sequence.into())),
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
        if let Some(result) = self
            .tool_call_sequence
            .lock()
            .expect("tool call sequence")
            .pop_front()
        {
            return Box::pin(async move { Ok(result) });
        }
        let content = self.chunks.join("");
        let tool_calls = (*self.tool_calls).clone();
        Box::pin(async move {
            Ok(ChatResult {
                content,
                thinking: None,
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

    #[tokio::test]
    async fn chat_with_tools_returns_native_sequence_in_order() {
        let provider = MockProvider::with_tool_call_sequence(
            "mock-model",
            vec![
                ChatResult {
                    content: String::new(),
                    tool_calls: vec![NativeToolCall {
                        id: "c1".into(),
                        name: "filesystem.read".into(),
                        arguments: r#"{"path":"a.txt"}"#.into(),
                    }],
                    usage: None,
                },
                ChatResult {
                    content: "done".into(),
                    tool_calls: vec![],
                    usage: None,
                },
            ],
        );
        let first = provider
            .chat_with_tools(None, &[], &[])
            .await
            .expect("first");
        let second = provider
            .chat_with_tools(None, &[], &[])
            .await
            .expect("second");

        assert_eq!(first.tool_calls[0].id, "c1");
        assert_eq!(second.content, "done");
    }
}

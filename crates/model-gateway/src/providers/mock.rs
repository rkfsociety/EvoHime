use crate::providers::{
    ChatMessage, ModelProvider, ProviderError, ProviderKind, TokenStream,
};
use async_stream::stream;
use std::sync::Arc;

/// Deterministic provider for tests.
pub struct MockProvider {
    model: String,
    chunks: Arc<Vec<String>>,
}

impl MockProvider {
    pub fn new(model: impl Into<String>, chunks: Vec<String>) -> Self {
        Self {
            model: model.into(),
            chunks: Arc::new(chunks),
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
                yield Ok(chunk.clone());
            }
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
        let mut stream = provider.stream_chat(&[ChatMessage {
            role: ChatRole::User,
            content: "hi".to_string(),
        }]);

        let first = stream.next().await.unwrap().unwrap();
        let second = stream.next().await.unwrap().unwrap();
        assert_eq!(first, "Hello");
        assert_eq!(second, " world");
        assert!(stream.next().await.is_none());
    }
}

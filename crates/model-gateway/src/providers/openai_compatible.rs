use crate::config::LiteRouterConfig;
use crate::providers::{
    ChatFuture, ChatMessage, ModelProvider, ProviderError, ProviderKind, TokenStream,
};
use crate::retry::RetryPolicy;
use crate::tools::ToolSpec;
/// Standalone OpenAI-compatible provider.
///
/// It deliberately shares the proven wire implementation with LiteRouter while
/// exposing its own provider identity and configuration path.
#[derive(Debug)]
pub struct OpenAICompatibleProvider {
    inner: super::literouter::LiteRouterProvider,
}

impl OpenAICompatibleProvider {
    pub fn new(config: LiteRouterConfig) -> Result<Self, ProviderError> {
        Self::with_retry(config, RetryPolicy::from_env())
    }

    pub fn with_retry(config: LiteRouterConfig, retry: RetryPolicy) -> Result<Self, ProviderError> {
        if config.api_key.is_empty() {
            return Err(ProviderError::Config(
                "OpenAI-compatible API key must not be empty".to_string(),
            ));
        }
        Ok(Self {
            inner: super::literouter::LiteRouterProvider::with_retry(config, retry)?,
        })
    }

    pub fn config(&self) -> &LiteRouterConfig {
        self.inner.config()
    }

    pub fn retry_policy(&self) -> &RetryPolicy {
        self.inner.retry_policy()
    }

    pub fn chat_completions_url(&self) -> String {
        self.inner.chat_completions_url()
    }
}

impl ModelProvider for OpenAICompatibleProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAICompatible
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn base_url(&self) -> &str {
        self.inner.base_url()
    }

    fn stream_chat(&self, messages: &[ChatMessage]) -> TokenStream {
        self.inner.stream_chat(messages)
    }

    fn stream_chat_with_model(&self, model: &str, messages: &[ChatMessage]) -> TokenStream {
        self.inner.stream_chat_with_model(model, messages)
    }

    fn chat_with_tools(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> ChatFuture {
        self.inner.chat_with_tools(model, messages, tools)
    }
}

#[cfg(test)]
mod tests {
    use super::{ModelProvider, OpenAICompatibleProvider};
    use crate::config::LiteRouterConfig;
    use crate::providers::ProviderKind;

    #[test]
    fn has_distinct_openai_compatible_provider_kind() {
        let provider = OpenAICompatibleProvider::new(LiteRouterConfig {
            api_key: "sk-test".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o-mini".to_string(),
        })
        .expect("provider");

        assert_eq!(provider.kind(), ProviderKind::OpenAICompatible);
        assert_eq!(provider.model_name(), "gpt-4o-mini");
        assert_eq!(provider.base_url(), "https://api.openai.com/v1");
    }

    #[test]
    fn rejects_empty_openai_compatible_key_with_provider_specific_error() {
        let error = OpenAICompatibleProvider::new(LiteRouterConfig {
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o-mini".to_string(),
        })
        .expect_err("empty key rejected");

        assert!(error.to_string().contains("OpenAI-compatible API key"));
    }
}

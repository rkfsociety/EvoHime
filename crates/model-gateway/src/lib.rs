use crate::config::{LiteRouterConfig, ModelGatewayConfig};
use crate::providers::{
    literouter::LiteRouterProvider, mock::MockProvider, ChatMessage, ModelProvider,
    ProviderError, ProviderKind, TokenStream,
};
use serde::Serialize;
use std::sync::Arc;

/// Entry point for chat completions.
pub struct ModelGateway {
    inner: Arc<dyn ModelProvider>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelConfigResponse {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub configured: bool,
    pub available_models: Vec<String>,
}

impl ModelGateway {
    pub fn from_config(config: &ModelGatewayConfig) -> Result<Self, ProviderError> {
        let inner: Arc<dyn ModelProvider> = match config.provider {
            ProviderKind::LiteRouter => {
                Arc::new(LiteRouterProvider::new(config.literouter.clone())?)
            }
            ProviderKind::Mock => {
                return Err(ProviderError::Config(
                    "mock provider is only for tests".to_string(),
                ));
            }
        };

        Ok(Self { inner })
    }

    pub fn from_provider(provider: Arc<dyn ModelProvider>) -> Self {
        Self { inner: provider }
    }

    pub fn try_from_env() -> Result<Self, ProviderError> {
        Self::from_config(&ModelGatewayConfig::from_env()?)
    }

    pub fn config_response(config: &ModelGatewayConfig) -> ModelConfigResponse {
        let configured = !config.literouter.api_key.is_empty();
        ModelConfigResponse {
            provider: config.provider.as_str().to_string(),
            model: config.literouter.model.clone(),
            base_url: config.literouter.base_url.clone(),
            configured,
            available_models: crate::config::LITEROUTER_FREE_MODELS
                .iter()
                .map(|model| (*model).to_string())
                .collect(),
        }
    }

    pub fn provider_kind(&self) -> ProviderKind {
        self.inner.kind()
    }

    pub fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    pub fn base_url(&self) -> &str {
        self.inner.base_url()
    }

    pub fn stream_chat(&self, messages: &[ChatMessage]) -> TokenStream {
        self.inner.stream_chat(messages)
    }
}

/// Test helper — builds a gateway backed by `MockProvider`.
pub fn mock_gateway(chunks: Vec<String>) -> ModelGateway {
    ModelGateway::from_provider(Arc::new(MockProvider::new("mock-model", chunks)))
}

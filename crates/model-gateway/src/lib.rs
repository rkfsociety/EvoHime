pub mod config;
pub mod providers;

pub use crate::config::{ModelGatewayConfig, ModelRouteConfig};
use crate::providers::{
    literouter::LiteRouterProvider, mock::MockProvider, ChatMessage, ModelProvider, ProviderError,
    ProviderKind, TokenStream,
};
use async_stream::stream;
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};

/// Entry point for chat completions.
pub struct ModelGateway {
    default_route: String,
    routes: HashMap<String, Arc<dyn ModelProvider>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelConfigResponse {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub configured: bool,
    pub available_models: Vec<String>,
    pub default_route: String,
    pub routes: Vec<ModelRouteResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelRouteResponse {
    pub name: String,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub configured: bool,
    pub available_models: Vec<String>,
    pub billing_mode: String,
}

impl ModelGateway {
    pub fn from_config(config: &ModelGatewayConfig) -> Result<Self, ProviderError> {
        let mut routes = HashMap::new();
        for (name, route_config) in &config.routes {
            routes.insert(name.clone(), build_provider(route_config)?);
        }

        Ok(Self {
            default_route: config.default_route.clone(),
            routes,
        })
    }

    pub fn from_provider(provider: Arc<dyn ModelProvider>) -> Self {
        Self {
            default_route: "default".to_string(),
            routes: HashMap::from([("default".to_string(), provider)]),
        }
    }

    pub fn from_routes(
        default_route: impl Into<String>,
        routes: HashMap<String, Arc<dyn ModelProvider>>,
    ) -> Self {
        Self {
            default_route: default_route.into(),
            routes,
        }
    }

    pub fn try_from_env() -> Result<Self, ProviderError> {
        Self::from_config(&ModelGatewayConfig::from_env()?)
    }

    pub fn config_response(config: &ModelGatewayConfig) -> ModelConfigResponse {
        let default_route = config.routes.get(&config.default_route).unwrap_or_else(|| {
            panic!(
                "default model route '{}' not configured",
                config.default_route
            )
        });
        let mut routes: Vec<ModelRouteResponse> = config
            .routes
            .iter()
            .map(|(name, route)| ModelRouteResponse {
                name: name.clone(),
                provider: route.provider.as_str().to_string(),
                model: route.literouter.model.clone(),
                base_url: route.literouter.base_url.clone(),
                configured: route.configured(),
                available_models: route.available_models(),
                billing_mode: if route.provider == ProviderKind::LiteRouter && route.literouter.model.ends_with(":free") {
                    "free".to_string()
                } else {
                    "paid".to_string()
                },
            })
            .collect();
        routes.sort_by(|left, right| left.name.cmp(&right.name));

        ModelConfigResponse {
            provider: default_route.provider.as_str().to_string(),
            model: default_route.literouter.model.clone(),
            base_url: default_route.literouter.base_url.clone(),
            configured: default_route.configured(),
            available_models: default_route.available_models(),
            default_route: config.default_route.clone(),
            routes,
        }
    }

    pub fn provider_kind(&self) -> ProviderKind {
        self.default_provider().kind()
    }

    pub fn model_name(&self) -> &str {
        self.default_provider().model_name()
    }

    pub fn base_url(&self) -> &str {
        self.default_provider().base_url()
    }

    pub fn stream_chat(&self, messages: &[ChatMessage]) -> TokenStream {
        match self.stream_chat_for_route(&self.default_route, messages) {
            Ok(stream) => stream,
            Err(error) => Box::pin(stream! {
                yield Err(error);
            }),
        }
    }

    pub fn stream_chat_for_route(
        &self,
        route: &str,
        messages: &[ChatMessage],
    ) -> Result<TokenStream, ProviderError> {
        Ok(self.provider_for_route(route)?.stream_chat(messages))
    }

    pub fn stream_chat_for_route_with_model(
        &self,
        route: &str,
        model: Option<&str>,
        messages: &[ChatMessage],
    ) -> Result<TokenStream, ProviderError> {
        let provider = self.provider_for_route(route)?;
        Ok(match model {
            Some(model) if !model.trim().is_empty() => provider.stream_chat_with_model(model, messages),
            _ => provider.stream_chat(messages),
        })
    }

    fn provider_for_route(&self, route: &str) -> Result<&Arc<dyn ModelProvider>, ProviderError> {
        self.routes
            .get(route)
            .ok_or_else(|| ProviderError::Config(format!("unknown model route: {route}")))
    }

    fn default_provider(&self) -> &Arc<dyn ModelProvider> {
        self.routes.get(&self.default_route).unwrap_or_else(|| {
            panic!(
                "default model route '{}' not configured",
                self.default_route
            )
        })
    }
}

/// Test helper — builds a gateway backed by `MockProvider`.
pub fn mock_gateway(chunks: Vec<String>) -> ModelGateway {
    ModelGateway::from_provider(Arc::new(MockProvider::new("mock-model", chunks)))
}

fn build_provider(route: &ModelRouteConfig) -> Result<Arc<dyn ModelProvider>, ProviderError> {
    match route.provider {
        ProviderKind::LiteRouter | ProviderKind::OpenAICompatible => {
            Ok(Arc::new(LiteRouterProvider::new(route.literouter.clone())?))
        }
        ProviderKind::Mock => Ok(Arc::new(MockProvider::new(
            route.literouter.model.clone(),
            vec![],
        ))),
    }
}

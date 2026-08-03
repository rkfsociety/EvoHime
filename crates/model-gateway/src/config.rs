use crate::providers::{ProviderError, ProviderKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;

/// Default LiteRouter OpenAI-compatible base URL.
pub const LITEROUTER_DEFAULT_BASE_URL: &str = "https://api.literouter.com/v1";

/// Default OpenAI-compatible base URL.
pub const OPENAI_DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Default OpenAI-compatible model.
pub const OPENAI_DEFAULT_MODEL: &str = "gpt-4o-mini";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiteRouterConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl LiteRouterConfig {
    pub fn from_env() -> Self {
        let api_key = env::var("LITEROUTER_API_KEY").unwrap_or_default();
        let base_url = env::var("LITEROUTER_BASE_URL")
            .unwrap_or_else(|_| LITEROUTER_DEFAULT_BASE_URL.to_string());
        let model = env::var("LITEROUTER_MODEL").unwrap_or_default();

        Self {
            api_key,
            base_url: normalize_base_url(&base_url),
            model,
        }
    }

    pub fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    pub fn openai_compatible_from_env() -> Self {
        let api_key = env::var("OPENAI_API_KEY").unwrap_or_default();
        let base_url =
            env::var("OPENAI_BASE_URL").unwrap_or_else(|_| OPENAI_DEFAULT_BASE_URL.to_string());
        let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| OPENAI_DEFAULT_MODEL.to_string());

        Self {
            api_key,
            base_url: normalize_base_url(&base_url),
            model,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouteConfig {
    pub provider: ProviderKind,
    pub literouter: LiteRouterConfig,
    /// Wave 3B: Provider supports extended thinking
    #[serde(default = "default_thinking_support")]
    pub supports_thinking: bool,
}

fn default_thinking_support() -> bool {
    true
}

impl ModelRouteConfig {
    fn with_provider(
        provider: ProviderKind,
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        // LiteRouter via Claude API supports thinking; other providers may not
        let supports_thinking = matches!(provider, ProviderKind::LiteRouter);
        Self {
            provider,
            literouter: LiteRouterConfig {
                api_key: api_key.into(),
                base_url: normalize_base_url(&base_url.into()),
                model: model.into(),
            },
            supports_thinking,
        }
    }

    pub fn literouter(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::with_provider(ProviderKind::LiteRouter, api_key, base_url, model)
    }

    pub fn openai_compatible(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::with_provider(ProviderKind::OpenAICompatible, api_key, base_url, model)
    }

    pub fn mock(model: impl Into<String>) -> Self {
        Self {
            provider: ProviderKind::Mock,
            literouter: LiteRouterConfig {
                api_key: String::new(),
                base_url: "mock://local".to_string(),
                model: model.into(),
            },
            supports_thinking: true, // Mock supports all features
        }
    }

    pub fn configured(&self) -> bool {
        match self.provider {
            ProviderKind::LiteRouter | ProviderKind::OpenAICompatible => {
                !self.literouter.api_key.is_empty()
            }
            ProviderKind::Mock => true,
        }
    }

}

#[derive(Debug, Clone)]
pub struct ModelGatewayConfig {
    pub default_route: String,
    pub routes: HashMap<String, ModelRouteConfig>,
}

impl ModelGatewayConfig {
    pub fn from_env() -> Result<Self, ProviderError> {
        if let Ok(raw_routes) = env::var("MODEL_ROUTES_JSON") {
            return parse_routes_from_json(&raw_routes);
        }

        let default_route =
            env::var("MODEL_DEFAULT_ROUTE").unwrap_or_else(|_| "default".to_string());
        let provider = env::var("MODEL_PROVIDER")
            .ok()
            .and_then(|value| ProviderKind::parse(&value))
            .unwrap_or(ProviderKind::LiteRouter);
        let literouter = LiteRouterConfig::from_env();
        let route = match provider {
            ProviderKind::LiteRouter => ModelRouteConfig {
                provider,
                literouter,
                supports_thinking: true,
            },
            ProviderKind::OpenAICompatible => {
                let openai = LiteRouterConfig::openai_compatible_from_env();
                ModelRouteConfig::openai_compatible(openai.api_key, openai.base_url, openai.model)
            }
            ProviderKind::Mock => ModelRouteConfig::mock(literouter.model),
        };

        Ok(Self {
            default_route: default_route.clone(),
            routes: HashMap::from([(default_route, route)]),
        })
    }
}

fn normalize_base_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

#[derive(Debug, Deserialize)]
struct EnvRouteConfig {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

fn parse_routes_from_json(raw_routes: &str) -> Result<ModelGatewayConfig, ProviderError> {
    let routes: HashMap<String, EnvRouteConfig> = serde_json::from_str(raw_routes)
        .map_err(|error| ProviderError::Config(error.to_string()))?;
    let default_route = env::var("MODEL_DEFAULT_ROUTE").unwrap_or_else(|_| "default".to_string());
    let mut parsed_routes = HashMap::new();

    for (name, route) in routes {
        let provider = route
            .provider
            .as_deref()
            .and_then(ProviderKind::parse)
            .unwrap_or(ProviderKind::LiteRouter);
        let model = route.model.unwrap_or_else(|| match provider {
            ProviderKind::OpenAICompatible => OPENAI_DEFAULT_MODEL.to_string(),
            _ => String::new(),
        });
        let default_base_url = match provider {
            ProviderKind::OpenAICompatible => OPENAI_DEFAULT_BASE_URL,
            _ => LITEROUTER_DEFAULT_BASE_URL,
        };
        let route_config = match provider {
            ProviderKind::LiteRouter => ModelRouteConfig::literouter(
                route.api_key.unwrap_or_default(),
                route
                    .base_url
                    .unwrap_or_else(|| default_base_url.to_string()),
                model,
            ),
            ProviderKind::OpenAICompatible => ModelRouteConfig::openai_compatible(
                route.api_key.unwrap_or_default(),
                route
                    .base_url
                    .unwrap_or_else(|| default_base_url.to_string()),
                model,
            ),
            ProviderKind::Mock => ModelRouteConfig::mock(model),
        };
        parsed_routes.insert(name, route_config);
    }

    if !parsed_routes.contains_key(&default_route) {
        return Err(ProviderError::Config(format!(
            "default model route '{default_route}' is missing from MODEL_ROUTES_JSON"
        )));
    }

    Ok(ModelGatewayConfig {
        default_route,
        routes: parsed_routes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_chat_completions_url() {
        let config = LiteRouterConfig {
            api_key: "test".to_string(),
            base_url: "https://api.literouter.com/v1".to_string(),
            model: "deepseek:free".to_string(),
        };

        assert_eq!(
            config.chat_completions_url(),
            "https://api.literouter.com/v1/chat/completions"
        );
    }
}

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
/// Default model for the OpenAI Responses transport.
pub const OPENAI_CODEX_DEFAULT_MODEL: &str = "gpt-5-codex";
/// Default loopback endpoint for the supervisor-authenticated local provider.
pub const LOCAL_DEFAULT_BASE_URL: &str = "http://127.0.0.1:49152/v1";
/// Default loopback endpoint for Ollama's OpenAI-compatible API.
pub const OLLAMA_DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434/v1";

/// Endpoint, model, and credential configuration shared by compatible providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiteRouterConfig {
    /// Provider API key; callers must keep it out of serialized UI responses.
    pub api_key: String,
    /// Normalized provider base URL.
    pub base_url: String,
    /// Default model identifier for this provider.
    pub model: String,
}

impl LiteRouterConfig {
    /// Reads LiteRouter settings from `LITEROUTER_*` environment variables.
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

    /// Returns the normalized `/chat/completions` endpoint URL.
    pub fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// Reads OpenAI-compatible settings from `OPENAI_*` environment variables.
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

    /// Reads OpenAI Responses settings from `OPENAI_*` environment variables.
    pub fn openai_responses_from_env() -> Self {
        let api_key = env::var("OPENAI_API_KEY").unwrap_or_default();
        let base_url =
            env::var("OPENAI_BASE_URL").unwrap_or_else(|_| OPENAI_DEFAULT_BASE_URL.to_string());
        let model =
            env::var("OPENAI_MODEL").unwrap_or_else(|_| OPENAI_CODEX_DEFAULT_MODEL.to_string());

        Self {
            api_key,
            base_url: normalize_base_url(&base_url),
            model,
        }
    }
}

/// Provider-specific configuration for one named model route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouteConfig {
    /// Transport/provider implementation selected for the route.
    pub provider: ProviderKind,
    /// Endpoint, model, and credential values consumed by the provider adapter.
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
        let supports_thinking = matches!(
            provider,
            ProviderKind::LiteRouter | ProviderKind::OpenAIResponses
        );
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

    /// Builds a LiteRouter route with explicit credentials, endpoint, and model.
    pub fn literouter(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::with_provider(ProviderKind::LiteRouter, api_key, base_url, model)
    }

    /// Builds a route using the OpenAI-compatible chat-completions transport.
    pub fn openai_compatible(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::with_provider(ProviderKind::OpenAICompatible, api_key, base_url, model)
    }

    /// Builds a route using the OpenAI Responses transport.
    pub fn openai_responses(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::with_provider(ProviderKind::OpenAIResponses, api_key, base_url, model)
    }

    /// A local SLM route. `api_key` is a short-lived supervisor-issued
    /// capability, never a cloud credential and never exposed to the shell.
    pub fn local(
        session_capability: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self::with_provider(ProviderKind::Local, session_capability, base_url, model)
    }

    /// Builds a local Ollama route without an API credential.
    pub fn ollama(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self::with_provider(ProviderKind::Ollama, String::new(), base_url, model)
    }

    /// Builds the mock provider route for tests and local contract checks.
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

    /// Reports whether the provider has the minimum endpoint/credential settings.
    pub fn configured(&self) -> bool {
        match self.provider {
            ProviderKind::LiteRouter
            | ProviderKind::OpenAICompatible
            | ProviderKind::OpenAIResponses => !self.literouter.api_key.is_empty(),
            ProviderKind::Ollama => !self.literouter.base_url.trim().is_empty(),
            ProviderKind::Local => !self.literouter.model.trim().is_empty(),
            ProviderKind::Mock => true,
        }
    }
}

/// Named route map and default route used to construct a model gateway.
#[derive(Debug, Clone)]
pub struct ModelGatewayConfig {
    /// Name of the route used by default operations.
    pub default_route: String,
    /// Configured provider routes keyed by their local route name.
    pub routes: HashMap<String, ModelRouteConfig>,
}

impl ModelGatewayConfig {
    /// Loads either `MODEL_ROUTES_JSON` or the single-route environment settings.
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
            ProviderKind::OpenAIResponses => {
                let openai = LiteRouterConfig::openai_responses_from_env();
                ModelRouteConfig::openai_responses(openai.api_key, openai.base_url, openai.model)
            }
            ProviderKind::Ollama => ModelRouteConfig::ollama(
                env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| OLLAMA_DEFAULT_BASE_URL.to_string()),
                env::var("OLLAMA_MODEL").unwrap_or_default(),
            ),
            ProviderKind::Mock => {
                return Err(ProviderError::Config(
                    "mock provider is available only to tests".into(),
                ))
            }
            ProviderKind::Local => ModelRouteConfig::local(
                env::var("LOCAL_PROVIDER_SESSION").unwrap_or_default(),
                env::var("LOCAL_PROVIDER_BASE_URL")
                    .unwrap_or_else(|_| LOCAL_DEFAULT_BASE_URL.to_string()),
                env::var("LOCAL_PROVIDER_MODEL").unwrap_or_else(|_| "local-slm".to_string()),
            ),
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
            ProviderKind::OpenAIResponses => OPENAI_CODEX_DEFAULT_MODEL.to_string(),
            _ => String::new(),
        });
        let default_base_url = match provider {
            ProviderKind::OpenAICompatible | ProviderKind::OpenAIResponses => {
                OPENAI_DEFAULT_BASE_URL
            }
            ProviderKind::Local => LOCAL_DEFAULT_BASE_URL,
            ProviderKind::Ollama => OLLAMA_DEFAULT_BASE_URL,
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
            ProviderKind::OpenAIResponses => ModelRouteConfig::openai_responses(
                route.api_key.unwrap_or_default(),
                route
                    .base_url
                    .unwrap_or_else(|| default_base_url.to_string()),
                model,
            ),
            ProviderKind::Local => ModelRouteConfig::local(
                route.api_key.unwrap_or_default(),
                route
                    .base_url
                    .unwrap_or_else(|| default_base_url.to_string()),
                model,
            ),
            ProviderKind::Ollama => ModelRouteConfig::ollama(
                route
                    .base_url
                    .unwrap_or_else(|| default_base_url.to_string()),
                model,
            ),
            ProviderKind::Mock => {
                return Err(ProviderError::Config(
                    "mock provider is available only to tests".into(),
                ))
            }
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

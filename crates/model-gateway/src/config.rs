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

/// Stable provider identity selected independently from the wire transport.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProfileId {
    /// Custom OpenAI-compatible endpoint without a known vendor profile.
    #[default]
    Custom,
    /// OpenAI hosted API profile.
    OpenAi,
    /// OpenRouter hosted API profile.
    OpenRouter,
    /// Groq hosted API profile.
    Groq,
    /// Google Gemini OpenAI-compatible API profile.
    Gemini,
    /// Mistral hosted API profile.
    Mistral,
    /// Cloudflare Workers AI account-scoped API profile.
    CloudflareWorkersAi,
    /// NVIDIA NIM hosted API profile.
    NvidiaNim,
    /// Cerebras hosted API profile.
    Cerebras,
    /// Hugging Face Inference Providers profile.
    HuggingFace,
}

impl ProviderProfileId {
    /// Returns the stable serialized identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::OpenAi => "openai",
            Self::OpenRouter => "openrouter",
            Self::Groq => "groq",
            Self::Gemini => "gemini",
            Self::Mistral => "mistral",
            Self::CloudflareWorkersAi => "cloudflare_workers_ai",
            Self::NvidiaNim => "nvidia_nim",
            Self::Cerebras => "cerebras",
            Self::HuggingFace => "hugging_face",
        }
    }

    /// Parses a stable serialized identifier without accepting aliases.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "custom" => Self::Custom,
            "openai" => Self::OpenAi,
            "openrouter" => Self::OpenRouter,
            "groq" => Self::Groq,
            "gemini" => Self::Gemini,
            "mistral" => Self::Mistral,
            "cloudflare_workers_ai" => Self::CloudflareWorkersAi,
            "nvidia_nim" => Self::NvidiaNim,
            "cerebras" => Self::Cerebras,
            "hugging_face" => Self::HuggingFace,
            _ => return None,
        })
    }

    /// Returns the trusted base URL for a profile with a fixed endpoint.
    pub fn default_base_url(self) -> Option<&'static str> {
        match self {
            Self::Custom => None,
            Self::OpenAi => Some(OPENAI_DEFAULT_BASE_URL),
            Self::OpenRouter => Some("https://openrouter.ai/api/v1"),
            Self::Groq => Some("https://api.groq.com/openai/v1"),
            Self::Gemini => Some("https://generativelanguage.googleapis.com/v1beta/openai"),
            Self::Mistral => Some("https://api.mistral.ai/v1"),
            Self::CloudflareWorkersAi => None,
            Self::NvidiaNim => Some("https://integrate.api.nvidia.com/v1"),
            Self::Cerebras => Some("https://api.cerebras.ai/v1"),
            Self::HuggingFace => Some("https://router.huggingface.co/v1"),
        }
    }

    /// Builds the trusted Workers AI base URL for a bounded account ID.
    pub fn cloudflare_base_url(account_id: &str) -> Option<String> {
        if !Self::valid_cloudflare_account_id(account_id) {
            return None;
        }
        Some(format!(
            "https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1"
        ))
    }

    /// Checks the documented fixed-width hexadecimal Cloudflare account ID.
    pub fn valid_cloudflare_account_id(account_id: &str) -> bool {
        account_id.len() == 32 && account_id.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

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
    /// Explicit vendor identity, independent from `provider` transport.
    #[serde(default)]
    pub provider_profile_id: Option<ProviderProfileId>,
    /// Account identifier required by account-scoped provider profiles.
    #[serde(default)]
    pub provider_account_id: Option<String>,
    /// Stable opaque handle for the configured credential; it is never a key or digest.
    #[serde(default)]
    pub provider_credential_binding: Option<String>,
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
            provider_profile_id: None,
            provider_account_id: None,
            provider_credential_binding: None,
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

    /// Selects an explicit vendor identity for an OpenAI-compatible route.
    ///
    /// The optional account ID is used only by account-scoped profiles; endpoint
    /// validation is performed before the route is created or dispatched.
    pub fn with_provider_profile(
        mut self,
        profile_id: ProviderProfileId,
        account_id: Option<String>,
    ) -> Self {
        self.provider_profile_id = Some(profile_id);
        self.provider_account_id = account_id;
        self
    }

    /// Associates a stable opaque scope handle with the configured provider credential.
    pub fn with_credential_binding(mut self, binding: Option<String>) -> Self {
        self.provider_credential_binding = binding;
        self
    }

    /// Validates that an explicit profile uses its trusted transport endpoint.
    pub fn validate_provider_profile(&self) -> Result<(), ProviderError> {
        if self
            .provider_credential_binding
            .as_deref()
            .is_some_and(|binding| !valid_credential_binding(binding))
        {
            return Err(ProviderError::Config(
                "provider credential binding is invalid".into(),
            ));
        }
        let Some(profile_id) = self.provider_profile_id else {
            if self.provider_account_id.is_some() {
                return Err(ProviderError::Config(
                    "provider profile account is not configured".into(),
                ));
            }
            return Ok(());
        };
        if self.provider != ProviderKind::OpenAICompatible {
            return Err(ProviderError::Config(
                "provider profile requires OpenAI-compatible transport".into(),
            ));
        }
        let expected_base_url = match profile_id {
            ProviderProfileId::Custom => {
                if self.provider_account_id.is_some() {
                    return Err(ProviderError::Config(
                        "provider profile account is not configured".into(),
                    ));
                }
                return Ok(());
            }
            ProviderProfileId::CloudflareWorkersAi => {
                let account_id = self.provider_account_id.as_deref().ok_or_else(|| {
                    ProviderError::Config("provider profile account is required".into())
                })?;
                ProviderProfileId::cloudflare_base_url(account_id).ok_or_else(|| {
                    ProviderError::Config("provider profile account is invalid".into())
                })?
            }
            fixed => {
                if self.provider_account_id.is_some() {
                    return Err(ProviderError::Config(
                        "provider profile account is not configured".into(),
                    ));
                }
                fixed
                    .default_base_url()
                    .ok_or_else(|| ProviderError::Config("provider profile is invalid".into()))?
                    .to_owned()
            }
        };
        if self.literouter.base_url.trim_end_matches('/') != expected_base_url {
            return Err(ProviderError::Config(
                "provider profile endpoint does not match".into(),
            ));
        }
        Ok(())
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
            provider_profile_id: None,
            provider_account_id: None,
            provider_credential_binding: None,
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
        let mut route = match provider {
            ProviderKind::LiteRouter => ModelRouteConfig {
                provider,
                literouter,
                provider_profile_id: None,
                provider_account_id: None,
                provider_credential_binding: None,
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
        route = route.with_credential_binding(env::var("MODEL_PROVIDER_CREDENTIAL_BINDING").ok());
        if provider == ProviderKind::OpenAICompatible {
            let profile_id =
                match env::var("MODEL_PROVIDER_PROFILE_ID") {
                    Ok(value) => Some(ProviderProfileId::parse(&value).ok_or_else(|| {
                        ProviderError::Config("provider profile is invalid".into())
                    })?),
                    Err(_) => None,
                };
            let account_id = env::var("MODEL_PROVIDER_ACCOUNT_ID").ok();
            if profile_id.is_some() || account_id.is_some() {
                route = route.with_provider_profile(profile_id.unwrap_or_default(), account_id);
            }
        }
        route.validate_provider_profile()?;

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
    #[serde(default)]
    provider_profile_id: Option<ProviderProfileId>,
    #[serde(default)]
    provider_account_id: Option<String>,
    #[serde(default)]
    provider_credential_binding: Option<String>,
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
        let mut route_config = match provider {
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
        if route.provider_profile_id.is_some() || route.provider_account_id.is_some() {
            let profile_id = route.provider_profile_id.ok_or_else(|| {
                ProviderError::Config("provider profile identity is not configured".into())
            })?;
            route_config =
                route_config.with_provider_profile(profile_id, route.provider_account_id);
        }
        route_config = route_config.with_credential_binding(route.provider_credential_binding);
        route_config.validate_provider_profile()?;
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

fn valid_credential_binding(value: &str) -> bool {
    let Some((prefix, id)) = value.split_once(':') else {
        return false;
    };
    prefix == "credential"
        && id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
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

    #[test]
    fn explicit_provider_profiles_require_their_trusted_route() {
        let route = ModelRouteConfig::openai_compatible(
            "test-token",
            "https://api.groq.com/openai/v1",
            "llama-3.3-70b-versatile",
        )
        .with_provider_profile(ProviderProfileId::Groq, None);
        assert!(route.validate_provider_profile().is_ok());

        let mismatched =
            ModelRouteConfig::openai_compatible("test-token", "https://custom.example/v1", "model")
                .with_provider_profile(ProviderProfileId::Groq, None);
        assert!(mismatched.validate_provider_profile().is_err());
    }

    #[test]
    fn cloudflare_profile_bounds_account_and_builds_openai_compatible_url() {
        let account_id = "0123456789abcdef0123456789abcdef";
        let base_url = ProviderProfileId::cloudflare_base_url(account_id).expect("valid account");
        assert_eq!(
            base_url,
            "https://api.cloudflare.com/client/v4/accounts/0123456789abcdef0123456789abcdef/ai/v1"
        );
        let route = ModelRouteConfig::openai_compatible("token", base_url, "@cf/model")
            .with_provider_profile(
                ProviderProfileId::CloudflareWorkersAi,
                Some(account_id.into()),
            );
        assert!(route.validate_provider_profile().is_ok());

        for invalid in ["", "../", "0123456789abcdef0123456789abcdeg"] {
            assert!(ProviderProfileId::cloudflare_base_url(invalid).is_none());
        }
    }

    #[test]
    fn routes_json_preserves_an_explicit_cloudflare_profile() {
        let account_id = "0123456789abcdef0123456789abcdef";
        let raw = serde_json::json!({
            "default": {
                "provider": "openai_compatible",
                "api_key": "test-token",
                "base_url": ProviderProfileId::cloudflare_base_url(account_id),
                "model": "@cf/meta/llama-3.1-8b-instruct",
                "provider_profile_id": "cloudflare_workers_ai",
                "provider_account_id": account_id
            }
        });

        let parsed = parse_routes_from_json(&raw.to_string()).expect("routes parse");
        let route = parsed.routes.get("default").expect("route exists");
        assert_eq!(
            route.provider_profile_id,
            Some(ProviderProfileId::CloudflareWorkersAi)
        );
        assert_eq!(route.provider_account_id.as_deref(), Some(account_id));
        assert!(route.validate_provider_profile().is_ok());
    }
}

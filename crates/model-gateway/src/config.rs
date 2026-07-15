use crate::providers::{ProviderError, ProviderKind};
use std::env;

/// Default LiteRouter OpenAI-compatible base URL.
pub const LITEROUTER_DEFAULT_BASE_URL: &str = "https://api.literouter.com/v1";

/// Default free-tier model.
pub const LITEROUTER_DEFAULT_MODEL: &str = "deepseek:free";

/// Known LiteRouter free models (non-exhaustive).
pub const LITEROUTER_FREE_MODELS: &[&str] = &["deepseek:free", "mistral:free", "llama:free"];

#[derive(Debug, Clone)]
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
        let model =
            env::var("LITEROUTER_MODEL").unwrap_or_else(|_| LITEROUTER_DEFAULT_MODEL.to_string());

        Self {
            api_key,
            base_url: normalize_base_url(&base_url),
            model,
        }
    }

    pub fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

#[derive(Debug, Clone)]
pub struct ModelGatewayConfig {
    pub provider: ProviderKind,
    pub literouter: LiteRouterConfig,
}

impl ModelGatewayConfig {
    pub fn from_env() -> Result<Self, ProviderError> {
        let provider = env::var("MODEL_PROVIDER")
            .ok()
            .and_then(|value| ProviderKind::parse(&value))
            .unwrap_or(ProviderKind::LiteRouter);

        Ok(Self {
            provider,
            literouter: LiteRouterConfig::from_env(),
        })
    }
}

fn normalize_base_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
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

pub mod config;
pub mod providers;
pub mod retry;
pub mod routing_policy;
pub mod routing_runtime;
pub mod tools;

pub use crate::config::{ModelGatewayConfig, ModelRouteConfig};
use crate::providers::{
    literouter::LiteRouterProvider, mock::MockProvider,
    openai_compatible::OpenAICompatibleProvider, ChatMessage, ModelProvider, ProviderError,
    ProviderKind, TokenStream,
};
pub use crate::retry::RetryPolicy;
// `routing_runtime` embeds its own private copy of `routing_policy` when
// this crate is built with `cfg(test)` (see the comment in
// `routing_runtime.rs`), so `RoutingRuntime::plan` there expects
// `RouteCandidate`/`RoutingRequest`/`PrivacyClass` from that embedded copy
// rather than from `crate::routing_policy`. Mirror the same split here so
// candidates built in this file always match the type `plan` expects.
#[cfg(not(test))]
pub use crate::routing_policy::{PrivacyClass, RouteCandidate, RoutingRequest};
#[cfg(test)]
pub use crate::routing_runtime::routing_policy::{PrivacyClass, RouteCandidate, RoutingRequest};
pub use crate::routing_runtime::{RoutingMode, RoutingRuntime, RuntimeError, RuntimeLimits};
pub use crate::tools::{
    ChatResult, ChatStreamItem, FunctionSpec, LlmUsage, NativeToolCall, ToolSpec,
};
use async_stream::stream;
use serde::{Deserialize, Serialize};
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
    /// Wave 3B: Provider supports extended thinking
    pub supports_thinking: bool,
}

#[derive(Debug, Deserialize)]
struct ProviderModelsResponse {
    data: Vec<ProviderModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ProviderModelEntry {
    id: String,
    /// OpenAI-compatible aggregators report the context window here. Plain
    /// OpenAI does not, so the field stays optional and callers treat a missing
    /// window as "unknown", never as "unlimited".
    #[serde(default)]
    context_length: Option<u64>,
    #[serde(default)]
    top_provider: Option<ProviderModelTop>,
}

#[derive(Debug, Deserialize)]
struct ProviderModelTop {
    #[serde(default)]
    max_completion_tokens: Option<u64>,
}

/// A model as the provider describes it: identifier plus the limits that decide
/// whether a request can fit at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelCatalogEntry {
    pub id: String,
    pub context_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

/// Fetches the provider's current model catalog without exposing the API key
/// to the desktop UI. The provider API is OpenAI-compatible and returns
/// `{ "data": [{ "id": "...", "context_length": 128000 }] }`.
pub async fn fetch_model_catalog(
    route: &ModelRouteConfig,
) -> Result<Vec<ModelCatalogEntry>, ProviderError> {
    if route.provider == ProviderKind::Mock {
        return Ok(if route.literouter.model.is_empty() {
            Vec::new()
        } else {
            vec![ModelCatalogEntry {
                id: route.literouter.model.clone(),
                context_tokens: None,
                max_output_tokens: None,
            }]
        });
    }
    if route.literouter.api_key.is_empty() {
        return Err(ProviderError::Config(
            "provider API key is not configured".into(),
        ));
    }

    let url = format!("{}/models", route.literouter.base_url.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .get(url)
        .bearer_auth(&route.literouter.api_key)
        .send()
        .await
        .map_err(|error| ProviderError::Http(error.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderError::Api(format!("{status}: {body}")));
    }

    let payload = response
        .json::<ProviderModelsResponse>()
        .await
        .map_err(|error| ProviderError::Api(error.to_string()))?;
    let mut models: Vec<_> = payload
        .data
        .into_iter()
        .filter(|entry| !entry.id.trim().is_empty())
        .map(|entry| ModelCatalogEntry {
            id: entry.id,
            context_tokens: entry.context_length.and_then(clamp_tokens),
            max_output_tokens: entry
                .top_provider
                .and_then(|top| top.max_completion_tokens)
                .and_then(clamp_tokens),
        })
        .collect();
    models.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    models.dedup_by(|left, right| left.id == right.id);
    Ok(models)
}

/// A window that does not fit `u32` is a provider bug, and a zero window would
/// make every request look impossible; both are reported as "unknown".
fn clamp_tokens(value: u64) -> Option<u32> {
    (value > 0).then(|| u32::try_from(value).unwrap_or(u32::MAX))
}

pub async fn fetch_available_models(
    route: &ModelRouteConfig,
) -> Result<Vec<String>, ProviderError> {
    Ok(fetch_model_catalog(route)
        .await?
        .into_iter()
        .map(|entry| entry.id)
        .collect())
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
        Self::config_response_with_models(config, &HashMap::new())
    }

    pub fn config_response_with_models(
        config: &ModelGatewayConfig,
        available_models: &HashMap<String, Vec<String>>,
    ) -> ModelConfigResponse {
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
                available_models: available_models.get(name).cloned().unwrap_or_default(),
                billing_mode: if route.provider == ProviderKind::LiteRouter
                    && route.literouter.model.ends_with(":free")
                {
                    "free".to_string()
                } else {
                    "paid".to_string()
                },
                supports_thinking: route.supports_thinking,
            })
            .collect();
        routes.sort_by(|left, right| left.name.cmp(&right.name));

        ModelConfigResponse {
            provider: default_route.provider.as_str().to_string(),
            model: default_route.literouter.model.clone(),
            base_url: default_route.literouter.base_url.clone(),
            configured: default_route.configured(),
            available_models: available_models
                .get(&config.default_route)
                .cloned()
                .unwrap_or_default(),
            default_route: config.default_route.clone(),
            routes,
        }
    }

    pub fn provider_kind(&self) -> ProviderKind {
        self.default_provider().kind()
    }

    pub fn route_provider_kind(&self, route: &str) -> Result<ProviderKind, ProviderError> {
        Ok(self.provider_for_route(route)?.kind())
    }

    pub fn model_name(&self) -> &str {
        self.default_provider().model_name()
    }

    pub fn resolve_model_name(
        &self,
        route: &str,
        model: Option<&str>,
    ) -> Result<String, ProviderError> {
        let provider = self.provider_for_route(route)?;
        Ok(model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| provider.model_name())
            .to_string())
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
            Some(model) if !model.trim().is_empty() => {
                provider.stream_chat_with_model(model, messages)
            }
            _ => provider.stream_chat(messages),
        })
    }

    /// Streams a completion against the configured default route while using
    /// a per-request model override. Review orchestration uses this instead of
    /// mutating the shell-wide selected model, so concurrent review calls do
    /// not race with normal chat traffic.
    pub fn stream_chat_with_model(
        &self,
        model: &str,
        messages: &[ChatMessage],
    ) -> Result<TokenStream, ProviderError> {
        self.stream_chat_for_route_with_model(&self.default_route, Some(model), messages)
    }

    pub async fn chat_with_tools_for_route(
        &self,
        route: &str,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> Result<ChatResult, ProviderError> {
        self.provider_for_route(route)?
            .chat_with_tools(model, messages, tools)
            .await
    }

    /// Plans a route using the bounded routing contract (`routing_policy` /
    /// `routing_runtime`) instead of trusting a caller-supplied route name.
    /// This is the real entry point for policy-governed selection: mode
    /// (local-first/balanced/cloud-research/offline), capability/cost/
    /// latency/privacy filtering, and visible fallback are all decided by
    /// `RoutingRuntime::plan`, not by ad hoc string checks in this crate.
    pub fn plan_route(
        &self,
        mode: RoutingMode,
        request: &RoutingRequest,
        limits: RuntimeLimits,
    ) -> Result<RoutingRuntime, RuntimeError> {
        let candidates = self.route_candidates();
        RoutingRuntime::plan(mode, request, &candidates, limits)
    }

    /// Streams a chat completion using a route chosen by the routing policy
    /// contract for the given mode, rather than a route name specified
    /// directly by the caller.
    pub fn stream_chat_with_policy(
        &self,
        mode: RoutingMode,
        request: &RoutingRequest,
        messages: &[ChatMessage],
    ) -> Result<TokenStream, ProviderError> {
        let mut runtime = self
            .plan_route(mode, request, RuntimeLimits::default())
            .map_err(|error| ProviderError::Config(error.to_string()))?;
        runtime
            .start()
            .map_err(|error| ProviderError::Config(error.to_string()))?;
        let route =
            runtime.decision().selected_route.clone().ok_or_else(|| {
                ProviderError::Config("routing policy selected no route".to_string())
            })?;
        self.stream_chat_for_route(&route, messages)
    }

    /// Builds routing-policy candidates from the configured routes.
    ///
    /// Only fields with a real, non-fabricated source in this crate are
    /// populated:
    /// - `route_id` / `model`: the configured route name and the
    ///   provider's model name.
    /// - `cost_micros_per_1k_tokens`: reuses the existing `:free` model-tier
    ///   convention (0 for free-tier models, 1 for paid) as a coarse cost
    ///   proxy, since no richer per-route billing metadata exists yet.
    /// - `available`: routes only enter `self.routes` after
    ///   `build_provider` succeeds (which itself requires a non-empty API
    ///   key for non-mock providers), so every route present is available.
    ///
    /// Two dimensions are intentionally left as neutral no-ops rather than
    /// invented: `p95_latency_ms` is always `0` (no latency telemetry is
    /// collected per route today) and `privacy` is always
    /// `PrivacyClass::Internal` (this crate has no per-route privacy
    /// classification and no local/on-device provider implementation, so
    /// `Public`/`Sensitive`/`Restricted` cannot be assigned meaningfully).
    /// `RoutingMode::LocalFirst`/`Offline` still work because
    /// `routing_runtime` classifies "local" routes by route id / model name
    /// substring, independent of this candidate metadata.
    fn route_candidates(&self) -> Vec<RouteCandidate> {
        let mut route_ids: Vec<&String> = self.routes.keys().collect();
        route_ids.sort();
        route_ids
            .into_iter()
            .map(|route_id| {
                let provider = &self.routes[route_id];
                let model = provider.model_name().to_string();
                let cost_micros_per_1k_tokens = if model.ends_with(":free") { 0 } else { 1 };
                RouteCandidate {
                    route_id: route_id.clone(),
                    model,
                    capabilities: vec!["chat".to_string()],
                    cost_micros_per_1k_tokens,
                    p95_latency_ms: 0,
                    privacy: PrivacyClass::Internal,
                    available: true,
                    fallback_rank: 0,
                }
            })
            .collect()
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
        ProviderKind::LiteRouter => {
            Ok(Arc::new(LiteRouterProvider::new(route.literouter.clone())?))
        }
        ProviderKind::OpenAICompatible => Ok(Arc::new(OpenAICompatibleProvider::new(
            route.literouter.clone(),
        )?)),
        ProviderKind::Mock => Ok(Arc::new(MockProvider::new(
            route.literouter.model.clone(),
            vec![],
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[test]
    fn builds_openai_compatible_route_as_distinct_provider() {
        let config = ModelGatewayConfig {
            default_route: "openai".to_string(),
            routes: HashMap::from([(
                "openai".to_string(),
                ModelRouteConfig::openai_compatible(
                    "sk-test",
                    "https://api.openai.com/v1",
                    "gpt-4o-mini",
                ),
            )]),
        };

        let gateway = ModelGateway::from_config(&config).expect("gateway");
        assert_eq!(
            gateway.route_provider_kind("openai").expect("route"),
            ProviderKind::OpenAICompatible
        );
    }

    fn policy_request() -> RoutingRequest {
        RoutingRequest {
            required_capabilities: vec!["chat".to_string()],
            max_cost_micros_per_1k_tokens: None,
            max_latency_ms: None,
            required_privacy: PrivacyClass::Internal,
            allow_fallback: true,
        }
    }

    fn gateway_with_routes(routes: Vec<(&str, &str)>) -> ModelGateway {
        let routes = routes
            .into_iter()
            .map(|(route_id, model)| {
                (
                    route_id.to_string(),
                    Arc::new(MockProvider::new(model, vec![format!("{route_id}-chunk")]))
                        as Arc<dyn ModelProvider>,
                )
            })
            .collect();
        ModelGateway::from_routes("local", routes)
    }

    #[test]
    fn local_first_plan_route_picks_local_provider_when_available() {
        let gateway = gateway_with_routes(vec![("local", "local-model"), ("cloud", "cloud-model")]);
        let runtime = gateway
            .plan_route(
                RoutingMode::LocalFirst,
                &policy_request(),
                RuntimeLimits::default(),
            )
            .expect("plan");
        assert_eq!(runtime.decision().selected_route.as_deref(), Some("local"));
        assert!(runtime.telemetry().fallback.is_none());
    }

    #[tokio::test]
    async fn local_first_falls_back_to_cloud_when_no_local_route_exists() {
        let gateway = gateway_with_routes(vec![("cloud", "cloud-model")]);
        let mut stream = gateway
            .stream_chat_with_policy(RoutingMode::LocalFirst, &policy_request(), &[])
            .expect("policy-selected stream");
        let first = stream.next().await.expect("chunk").expect("ok");
        assert_eq!(first, ChatStreamItem::Delta("cloud-chunk".to_string()));

        // Re-plan directly to inspect the visible fallback telemetry.
        let runtime = gateway
            .plan_route(
                RoutingMode::LocalFirst,
                &policy_request(),
                RuntimeLimits::default(),
            )
            .expect("plan");
        assert_eq!(runtime.decision().selected_route.as_deref(), Some("cloud"));
        assert_eq!(
            runtime
                .telemetry()
                .fallback
                .as_ref()
                .map(|notice| notice.reason.as_str()),
            Some("local_route_unavailable")
        );
    }

    #[test]
    fn policy_denies_route_when_no_candidate_satisfies_required_capability() {
        let gateway = gateway_with_routes(vec![("local", "local-model"), ("cloud", "cloud-model")]);
        let request = RoutingRequest {
            required_capabilities: vec!["vision".to_string()],
            max_cost_micros_per_1k_tokens: None,
            max_latency_ms: None,
            required_privacy: PrivacyClass::Internal,
            allow_fallback: true,
        };
        let runtime = gateway
            .plan_route(RoutingMode::Balanced, &request, RuntimeLimits::default())
            .expect("plan runs even when denied");
        assert_eq!(runtime.decision().selected_route, None);

        let result = gateway.stream_chat_with_policy(RoutingMode::Balanced, &request, &[]);
        assert!(matches!(result, Err(ProviderError::Config(_))));
    }
}

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
//! Model-provider routing, request contracts, retries, and structured outputs.
//!
//! The gateway selects providers from validated policy snapshots and exposes
//! typed outcomes while keeping provider-specific transport behind adapters.
//!
//! ```
//! use evohime_model_gateway::{ResponseContract, ResponseStrategy};
//! use serde_json::json;
//!
//! let contract = ResponseContract::new(
//!     "summary",
//!     1,
//!     json!({"type": "object", "required": ["text"]}),
//!     ResponseStrategy::Auto,
//! ).unwrap();
//! contract.validate_value(&json!({"text": "done"})).unwrap();
//! ```

/// Gateway configuration and per-model route settings.
pub mod config;
/// Provider request, health, and route snapshot contracts.
pub mod provider_contract;
/// Provider adapter implementations.
pub mod providers;
/// Retry behavior for transient provider failures.
pub mod retry;
/// Evaluation catalog used to select routing candidates.
pub mod routing_catalog;
/// Privacy and capability rules for candidate selection.
pub mod routing_policy;
/// Runtime coordinator for model routing and fallback.
pub mod routing_runtime;
/// Privacy-filtered routing decision and health traces.
pub mod routing_trace;
/// Validation contracts for structured model responses.
pub mod structured_response;
/// Tool call and chat response types exposed by the gateway.
pub mod tools;

pub use crate::config::{ModelGatewayConfig, ModelRouteConfig, ProviderProfileId};
pub use crate::provider_contract::{
    select_route_snapshot, select_route_snapshot_cached, AttemptTrace, CandidateEntry,
    CapabilityMetadata, CircuitState, ExecutionClass, FailureCategory, HealthStatus,
    ImageOutputCapability, ImageOutputOperation, ImageProviderRequest, PolicyHashes, ProbeConfig,
    ProbeFailure, ProbeResult, ProviderImageOutput, RetryConfig, RoutePolicySnapshot,
    RunHealthOverlay, RunResult, RunTrace, SnapshotCandidateDecision, SnapshotError,
    SnapshotRouteDecision,
};
pub use crate::providers::ChatRequestOptions;
pub use crate::providers::ImageOutputFuture;
use crate::providers::{
    literouter::LiteRouterProvider, local::LocalProvider, mock::MockProvider,
    ollama::OllamaProvider, openai_compatible::OpenAICompatibleProvider,
    openai_responses::OpenAIResponsesProvider, ChatMessage, ModelProvider, ProviderError,
    ProviderKind, TokenStream,
};
pub use crate::retry::RetryPolicy;
pub use crate::routing_catalog::{CatalogError, CatalogStore, EvaluationCatalog, EvaluationRecord};
pub use crate::routing_policy::{PrivacyClass, RouteCandidate, RoutingRequest};
pub use crate::routing_runtime::{RoutingMode, RoutingRuntime, RuntimeError, RuntimeLimits};
pub use crate::routing_trace::{
    HealthState, PrivacyLabel, RoutingTrace, SafeNextAction, TerminalStatus, TraceCandidate,
};
pub use crate::structured_response::{
    ResponseContract, ResponseError, ResponseResult, ResponseStrategy,
    STRUCTURED_RESPONSE_SCHEMA_VERSION,
};
pub use crate::tools::{
    ChatResult, ChatStreamItem, FunctionSpec, LlmUsage, NativeToolCall, ToolSpec,
};
use async_stream::stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::Digest;
#[cfg(not(test))]
use std::sync::OnceLock;
use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

/// Maximum response bytes accepted when fetching a provider model catalog.
pub const MAX_MODEL_CATALOG_BYTES: usize = 512 * 1024;
/// Maximum number of model entries accepted from one provider catalog.
pub const MAX_MODEL_CATALOG_ENTRIES: usize = 2_048;
/// Maximum model identifier length accepted from catalog responses.
pub const MAX_MODEL_ID_CHARS: usize = 256;

// Ниже — хелперы политики маршрутизации, к которым обращается только ветка
// `#[cfg(not(test))]` в `chat_with_tools_with_policy_and_route`: в тестовой
// сборке крейта эта ветка выключена, поэтому и хелперы собираются вместе с
// ней, а не висят мёртвым кодом.
fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(not(test))]
fn builtin_routing_catalog() -> Option<&'static EvaluationCatalog> {
    static CATALOG: OnceLock<Option<EvaluationCatalog>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            load_runtime_catalog().or_else(|| {
                EvaluationCatalog::load_jsonl(include_str!("../resources/routing-v1.jsonl"), None)
                    .ok()
            })
        })
        .as_ref()
}

#[cfg(not(test))]
fn load_runtime_catalog() -> Option<EvaluationCatalog> {
    let mut paths = Vec::new();
    if let Ok(path) = std::env::var("EVOHIME_ROUTING_CATALOG") {
        if !path.trim().is_empty() {
            paths.push(std::path::PathBuf::from(path));
        }
    }
    if let Ok(data_dir) = std::env::var("EVOHIME_DATA_DIR") {
        paths.push(std::path::PathBuf::from(data_dir).join("routing/routing-v1.jsonl"));
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            paths.push(parent.join("routing/routing-v1.jsonl"));
            paths.push(parent.join("resources/routing-v1.jsonl"));
        }
    }
    paths
        .into_iter()
        .find_map(|path| EvaluationCatalog::load_file(&path, None).ok())
}

#[cfg(not(test))]
fn classify_failure(error: &ProviderError) -> FailureCategory {
    match error {
        ProviderError::Config(_) => FailureCategory::InvalidRequest,
        ProviderError::ImagePreflightRejected | ProviderError::ImageCapabilityStale => {
            FailureCategory::InvalidRequest
        }
        ProviderError::Http(message) if message.contains("timeout") => FailureCategory::Timeout,
        ProviderError::Http(message) if message.contains("connection") => {
            FailureCategory::ConnectionRefused
        }
        ProviderError::Http(_) | ProviderError::Api(_) => FailureCategory::ServerError,
        ProviderError::Stream(_) => FailureCategory::MalformedResponse,
    }
}

/// Entry point for chat completions.
pub struct ModelGateway {
    default_route: String,
    default_provider: Arc<dyn ModelProvider>,
    routes: HashMap<String, Arc<dyn ModelProvider>>,
    route_preflight: Option<Arc<dyn RoutePreflight>>,
}

/// Core-owned gate evaluated after policy selection and immediately before a
/// provider call. The gateway owns transport; Core owns durable catalog and
/// credential/health lifecycle metadata.
pub trait RoutePreflight: Send + Sync {
    /// Checks that the selected route/model is still eligible immediately before dispatch.
    fn check(&self, route: &str, model: Option<&str>, now_ms: u64) -> Result<(), ProviderError>;

    /// Checks request-specific requirements before dispatch.
    ///
    /// Implementations that do not use capability evidence remain compatible
    /// through the default delegation to [`Self::check`].
    fn check_for_request(
        &self,
        route: &str,
        model: Option<&str>,
        requires_tool_calls: bool,
        now_ms: u64,
    ) -> Result<(), ProviderError> {
        let _ = requires_tool_calls;
        self.check(route, model, now_ms)
    }

    /// Performs request preflight when the policy owner needs asynchronous evidence work.
    fn check_for_request_async<'a>(
        &'a self,
        route: &'a str,
        model: Option<&'a str>,
        requires_tool_calls: bool,
        now_ms: u64,
    ) -> Pin<Box<dyn Future<Output = Result<(), ProviderError>> + Send + 'a>> {
        Box::pin(async move { self.check_for_request(route, model, requires_tool_calls, now_ms) })
    }

    /// Records only bounded usage metadata after a successful user request.
    fn observe_success_async<'a>(
        &'a self,
        _route: &'a str,
        _model: Option<&'a str>,
        _result: &'a ChatResult,
        _now_ms: u64,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }

    /// Records completion of a successful stream without retaining its content.
    fn observe_stream_success_async<'a>(
        &'a self,
        _route: &'a str,
        _model: Option<&'a str>,
        _now_ms: u64,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }

    /// Reports whether route policy requires verified-free candidates only.
    fn requires_verified_free_route(&self) -> bool {
        false
    }

    /// Reports whether verified-free candidates should rank ahead of paid routes.
    fn prefers_verified_free_routes(&self) -> bool {
        false
    }

    /// Checks the current route/model against authoritative free-access evidence.
    fn is_verified_free_route(&self, _route: &str, _model: &str, _now_ms: u64) -> bool {
        false
    }
}

/// Provider and route configuration exposed to the local desktop client.
#[derive(Debug, Clone, Serialize)]
pub struct ModelConfigResponse {
    /// Default provider identifier.
    pub provider: String,
    /// Default model identifier.
    pub model: String,
    /// Base endpoint for the default route.
    pub base_url: String,
    /// Whether the default route has the credentials/configuration it needs.
    pub configured: bool,
    /// Model identifiers discovered for the default provider.
    pub available_models: Vec<String>,
    /// Name of the route selected for default requests.
    pub default_route: String,
    /// Configured routes safe to expose to the desktop client.
    pub routes: Vec<ModelRouteResponse>,
}

/// User-visible summary of one configured model route.
#[derive(Debug, Clone, Serialize)]
pub struct ModelRouteResponse {
    /// Route name selected by callers.
    pub name: String,
    /// Provider identifier for this route.
    pub provider: String,
    /// Configured model identifier.
    pub model: String,
    /// Public provider endpoint, with credentials omitted.
    pub base_url: String,
    /// Whether the route is configured with required credentials.
    pub configured: bool,
    /// Models discovered for this route.
    pub available_models: Vec<String>,
    /// Billing classification used by the local UI.
    pub billing_mode: String,
    /// Wave 3B: Provider supports extended thinking
    pub supports_thinking: bool,
}

/// Provider result paired with the route decision and bounded attempt trace.
#[derive(Debug, Clone)]
pub struct PolicyChatResult {
    /// Route that produced the result.
    pub selected_route: String,
    /// Remaining route order available for fallback.
    pub fallback_chain: Vec<String>,
    /// Provider response from the selected route.
    pub result: ChatResult,
    /// Evaluated routing decision, when policy snapshot selection was used.
    pub decision: Option<SnapshotRouteDecision>,
    /// Digest of the route snapshot used for this request.
    pub snapshot_hash: Option<String>,
    /// Bounded route attempt trace, when collected.
    pub attempt_trace: Option<RunTrace>,
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

pub(crate) async fn read_bounded_response(
    response: reqwest::Response,
    label: &str,
) -> Result<Vec<u8>, ProviderError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MODEL_CATALOG_BYTES as u64)
    {
        return Err(ProviderError::Api(format!(
            "{label} response exceeds size limit"
        )));
    }
    let mut bytes = Vec::with_capacity(
        response
            .content_length()
            .unwrap_or_default()
            .try_into()
            .unwrap_or(0)
            .min(MAX_MODEL_CATALOG_BYTES),
    );
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| ProviderError::Stream(error.to_string()))?;
        if bytes.len().saturating_add(chunk.len()) > MAX_MODEL_CATALOG_BYTES {
            return Err(ProviderError::Api(format!(
                "{label} response exceeds size limit"
            )));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// A model as the provider describes it: identifier plus the limits that decide
/// whether a request can fit at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelCatalogEntry {
    /// Stable model identifier assigned by the provider.
    pub id: String,
    /// Advertised input context capacity, when known.
    pub context_tokens: Option<u32>,
    /// Advertised output token limit, when known.
    pub max_output_tokens: Option<u32>,
}

/// Redacted pricing observation from the fixed OpenRouter model-detail API.
///
/// Pricing strings and response bodies are discarded after normalization;
/// callers receive only model identity, the all-zero decision, and a digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRouterPricingObservation {
    /// Exact model identifier returned by the provider.
    pub model_id: String,
    /// Whether every declared charge dimension was an exact decimal zero.
    pub all_dimensions_zero: bool,
    /// Digest of normalized typed model pricing metadata.
    pub source_hash: String,
}

/// Safe, typed failure returned by the bounded OpenRouter pricing fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenRouterPricingError {
    /// Route is not the trusted OpenRouter profile or lacks a credential.
    InvalidConfiguration,
    /// Network transport failed before a response was received.
    Transport,
    /// OpenRouter rejected the request with an HTTP status.
    HttpStatus(u16),
    /// Response exceeded bounds or did not match the typed contract.
    InvalidResponse,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelDetailEnvelope {
    data: OpenRouterModelDetail,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelDetail {
    id: String,
    pricing: BTreeMap<String, String>,
}

/// Fetches and normalizes model pricing from OpenRouter's fixed profile only.
///
/// This endpoint is intentionally unavailable to custom endpoints and other
/// provider profiles. It never reads a model suffix as evidence.
pub async fn fetch_openrouter_model_pricing(
    route: &ModelRouteConfig,
    model_id: &str,
) -> Result<OpenRouterPricingObservation, OpenRouterPricingError> {
    route
        .validate_provider_profile()
        .map_err(|_| OpenRouterPricingError::InvalidConfiguration)?;
    if route.provider_profile_id != Some(ProviderProfileId::OpenRouter) {
        return Err(OpenRouterPricingError::InvalidConfiguration);
    }
    if route.literouter.api_key.is_empty() {
        return Err(OpenRouterPricingError::InvalidConfiguration);
    }
    let (author, slug) = model_id
        .split_once('/')
        .filter(|(author, slug)| {
            !author.is_empty()
                && !slug.is_empty()
                && !slug.contains('/')
                && model_id.len() <= MAX_MODEL_ID_CHARS
                && model_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
        })
        .ok_or(OpenRouterPricingError::InvalidConfiguration)?;

    let mut url = reqwest::Url::parse("https://openrouter.ai/api/v1/model/")
        .map_err(|_| OpenRouterPricingError::InvalidConfiguration)?;
    url.path_segments_mut()
        .map_err(|_| OpenRouterPricingError::InvalidConfiguration)?
        .push(author)
        .push(slug);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|_| OpenRouterPricingError::InvalidConfiguration)?;
    let response = client
        .get(url)
        .bearer_auth(&route.literouter.api_key)
        .send()
        .await
        .map_err(|_| OpenRouterPricingError::Transport)?;
    if !response.status().is_success() {
        return Err(OpenRouterPricingError::HttpStatus(
            response.status().as_u16(),
        ));
    }
    let body = read_bounded_response(response, "openrouter model pricing")
        .await
        .map_err(|_| OpenRouterPricingError::InvalidResponse)?;
    let detail = serde_json::from_slice::<OpenRouterModelDetailEnvelope>(&body)
        .map_err(|_| OpenRouterPricingError::InvalidResponse)?
        .data;
    normalize_openrouter_model_pricing(detail, model_id)
}

fn normalize_openrouter_model_pricing(
    detail: OpenRouterModelDetail,
    requested_model_id: &str,
) -> Result<OpenRouterPricingObservation, OpenRouterPricingError> {
    if detail.id != requested_model_id
        || detail.pricing.is_empty()
        || detail.pricing.len() > 32
        || !detail.pricing.contains_key("prompt")
        || !detail.pricing.contains_key("completion")
        || !detail.pricing.contains_key("request")
        || detail.pricing.iter().any(|(name, value)| {
            name.is_empty()
                || name.len() > 64
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
                || !valid_nonnegative_decimal(value)
        })
    {
        return Err(OpenRouterPricingError::InvalidResponse);
    }
    let all_dimensions_zero = detail.pricing.values().all(|value| {
        value
            .bytes()
            .filter(|byte| *byte != b'.')
            .all(|byte| byte == b'0')
    });
    let normalized = serde_json::to_vec(&(detail.id.as_str(), &detail.pricing))
        .map_err(|_| OpenRouterPricingError::InvalidResponse)?;
    let source_hash = hex::encode(sha2::Sha256::digest(normalized));
    Ok(OpenRouterPricingObservation {
        model_id: detail.id,
        all_dimensions_zero,
        source_hash,
    })
}

fn valid_nonnegative_decimal(value: &str) -> bool {
    if value.is_empty() || value.len() > 64 {
        return false;
    }
    let mut decimal_points = 0_u8;
    let mut digits = 0_u8;
    value.bytes().all(|byte| {
        if byte == b'.' {
            decimal_points = decimal_points.saturating_add(1);
            decimal_points <= 1
        } else if byte.is_ascii_digit() {
            digits = digits.saturating_add(1);
            true
        } else {
            false
        }
    }) && digits > 0
        && !value.starts_with('.')
        && !value.ends_with('.')
}

/// Fetches the provider's current model catalog without exposing the API key
/// to the desktop UI. The provider API is OpenAI-compatible and returns
/// `{ "data": [{ "id": "...", "context_length": 128000 }] }`.
pub async fn fetch_model_catalog(
    route: &ModelRouteConfig,
) -> Result<Vec<ModelCatalogEntry>, ProviderError> {
    route.validate_provider_profile()?;
    if route.provider == ProviderKind::Ollama {
        return providers::ollama::fetch_installed_models(&route.literouter).await;
    }
    if route.provider_profile_id == Some(ProviderProfileId::CloudflareWorkersAi) {
        let account_id = route
            .provider_account_id
            .as_deref()
            .ok_or_else(|| ProviderError::Config("provider profile account is required".into()))?;
        return providers::cloudflare_workers_ai::fetch_model_catalog(
            &route.literouter,
            account_id,
        )
        .await;
    }
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
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| ProviderError::Http(error.to_string()))?;
    let response = client
        .get(url)
        .bearer_auth(&route.literouter.api_key)
        .send()
        .await
        .map_err(|error| ProviderError::Http(error.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        return Err(ProviderError::Api(format!(
            "provider model catalog request failed with HTTP {status}"
        )));
    }

    let body = read_bounded_response(response, "provider model catalog").await?;
    let payload = serde_json::from_slice::<ProviderModelsResponse>(&body)
        .map_err(|_| ProviderError::Api("provider model catalog response is invalid".into()))?;
    if payload.data.len() > MAX_MODEL_CATALOG_ENTRIES {
        return Err(ProviderError::Api(
            "provider model catalog contains too many entries".into(),
        ));
    }
    if payload.data.iter().any(|entry| {
        let id = entry.id.trim();
        id.chars().count() > MAX_MODEL_ID_CHARS || id.chars().any(char::is_control)
    }) {
        return Err(ProviderError::Api(
            "provider model catalog contains an invalid model id".into(),
        ));
    }
    let mut models: Vec<_> = payload
        .data
        .into_iter()
        .filter(|entry| !entry.id.trim().is_empty())
        .map(|entry| ModelCatalogEntry {
            id: entry.id.trim().to_string(),
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

/// Fetches the configured route's current model identifiers.
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
    /// Builds a gateway from validated route configuration.
    pub fn from_config(config: &ModelGatewayConfig) -> Result<Self, ProviderError> {
        if !config.routes.contains_key(&config.default_route) {
            return Err(ProviderError::Config(format!(
                "default model route '{}' not configured",
                config.default_route
            )));
        }
        for route in config.routes.values() {
            route.validate_provider_profile()?;
        }
        let mut routes = HashMap::new();
        for (name, route_config) in &config.routes {
            routes.insert(name.clone(), build_provider(route_config)?);
        }

        let default_provider = routes
            .get(&config.default_route)
            .cloned()
            .ok_or_else(|| ProviderError::Config("default model route unavailable".into()))?;
        Ok(Self {
            default_route: config.default_route.clone(),
            default_provider,
            routes,
            route_preflight: None,
        })
    }

    /// Builds a gateway around one provider for simple or test configurations.
    pub fn from_provider(provider: Arc<dyn ModelProvider>) -> Self {
        Self {
            default_route: "default".to_string(),
            default_provider: provider.clone(),
            routes: HashMap::from([("default".to_string(), provider)]),
            route_preflight: None,
        }
    }

    /// Builds a gateway with an explicit default route and route/provider map.
    pub fn from_routes(
        default_route: impl Into<String>,
        routes: HashMap<String, Arc<dyn ModelProvider>>,
    ) -> Result<Self, ProviderError> {
        let default_route = default_route.into();
        let default_provider = routes.get(&default_route).cloned().ok_or_else(|| {
            ProviderError::Config(format!("unknown default route: {default_route}"))
        })?;
        Ok(Self {
            default_route,
            default_provider,
            routes,
            route_preflight: None,
        })
    }

    /// Adds a Core-owned gate checked after selection and before every provider call.
    pub fn with_route_preflight(mut self, preflight: Arc<dyn RoutePreflight>) -> Self {
        self.route_preflight = Some(preflight);
        self
    }

    /// Loads route configuration from the process environment.
    pub fn try_from_env() -> Result<Self, ProviderError> {
        Self::from_config(&ModelGatewayConfig::from_env()?)
    }

    /// Builds a configuration response without waiting for catalog discovery.
    pub fn config_response(
        config: &ModelGatewayConfig,
    ) -> Result<ModelConfigResponse, ProviderError> {
        Self::config_response_with_models(config, &HashMap::new())
    }

    /// Builds a configuration response with discovered models grouped by route.
    pub fn config_response_with_models(
        config: &ModelGatewayConfig,
        available_models: &HashMap<String, Vec<String>>,
    ) -> Result<ModelConfigResponse, ProviderError> {
        let default_route = config.routes.get(&config.default_route).ok_or_else(|| {
            ProviderError::Config(format!(
                "default model route '{}' not configured",
                config.default_route
            ))
        })?;
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

        Ok(ModelConfigResponse {
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
        })
    }

    /// Returns the provider kind for the default route.
    pub fn provider_kind(&self) -> ProviderKind {
        self.default_provider.kind()
    }

    /// Returns the provider kind for a named configured route.
    pub fn route_provider_kind(&self, route: &str) -> Result<ProviderKind, ProviderError> {
        Ok(self.provider_for_route(route)?.kind())
    }

    /// Reports whether a named route supports structured output.
    pub fn route_supports_structured_output(&self, route: &str) -> Result<bool, ProviderError> {
        Ok(self.provider_for_route(route)?.supports_structured_output())
    }

    /// Returns the configured model name for the default route.
    pub fn model_name(&self) -> &str {
        self.default_provider.model_name()
    }

    /// Resolves an optional request model against the route's configured default.
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

    /// Returns the endpoint for the default route.
    pub fn base_url(&self) -> &str {
        self.default_provider.base_url()
    }

    /// Starts a streaming chat request using the default route.
    pub fn stream_chat(&self, messages: &[ChatMessage]) -> TokenStream {
        match self.stream_chat_for_route(&self.default_route, messages) {
            Ok(stream) => stream,
            Err(error) => Box::pin(stream! {
                yield Err(error);
            }),
        }
    }

    /// Starts a streaming request using a named route and its configured model.
    pub fn stream_chat_for_route(
        &self,
        route: &str,
        messages: &[ChatMessage],
    ) -> Result<TokenStream, ProviderError> {
        if let Some(preflight) = &self.route_preflight {
            preflight.check(route, None, current_time_ms())?;
        }
        self.dispatch_stream(route, None, messages)
    }

    /// Returns the configured default route identifier for Core-owned snapshots.
    pub fn default_route_id(&self) -> &str {
        &self.default_route
    }

    /// Returns a validated image-output capability for one configured route.
    pub fn image_output_capability_for_route(
        &self,
        route: &str,
    ) -> Result<Option<crate::provider_contract::ImageOutputCapability>, ProviderError> {
        let Some(capability) = self.provider_for_route(route)?.image_output_capability() else {
            return Ok(None);
        };
        if !capability.validate() {
            return Err(ProviderError::Config(
                "invalid_image_output_capability".into(),
            ));
        }
        Ok(Some(capability))
    }

    /// Validates route capability, privacy and live route eligibility before Core records dispatch.
    pub fn preflight_image_output_for_route(
        &self,
        route: &str,
        expected_epoch: Option<u64>,
        request: &crate::provider_contract::ImageProviderRequest,
    ) -> Result<crate::provider_contract::ImageOutputCapability, ProviderError> {
        let provider = self.provider_for_route(route)?;
        let capability = provider
            .image_output_capability()
            .ok_or(if expected_epoch.is_some() {
                ProviderError::ImageCapabilityStale
            } else {
                ProviderError::Config("image_output_unsupported".into())
            })?;
        let route_is_cloud = provider.kind() != ProviderKind::Local
            && provider.kind() != ProviderKind::Ollama
            && provider.kind() != ProviderKind::Mock;
        if !capability.validate()
            || expected_epoch.is_some_and(|epoch| capability.capability_epoch != epoch)
            || !capability.operations.contains(&request.operation)
            || !capability.mime_types.contains(&request.mime_type)
            || request.count == 0
            || request.count > capability.max_outputs
            || request.width == 0
            || request.width > capability.max_width
            || request.height == 0
            || request.height > capability.max_height
            || request.required_privacy > capability.privacy_boundary
            || (route_is_cloud && !request.allow_cloud)
            || (capability.execution_class == ExecutionClass::Local && route_is_cloud)
            || (capability.execution_class == ExecutionClass::Cloud && !route_is_cloud)
            || u64::from(request.width).saturating_mul(u64::from(request.height))
                > capability.max_pixels
        {
            if expected_epoch.is_some_and(|epoch| capability.capability_epoch != epoch) {
                return Err(ProviderError::ImageCapabilityStale);
            }
            return Err(ProviderError::Config(
                "image_request_outside_capability".into(),
            ));
        }
        if let Some(preflight) = &self.route_preflight {
            preflight.check(route, None, current_time_ms())?;
        }
        Ok(capability)
    }

    /// Calls an image-capable provider route after checking its declared bounds.
    ///
    /// This method does not fetch returned URLs: provider adapters return bytes
    /// only, and Core validates and decodes them before storing artifacts.
    pub async fn generate_image_for_route(
        &self,
        route: &str,
        request: crate::provider_contract::ImageProviderRequest,
    ) -> Result<Vec<crate::provider_contract::ProviderImageOutput>, ProviderError> {
        self.generate_image_for_route_at_epoch(route, None, request)
            .await
    }

    /// Dispatches only when the route still advertises the frozen capability epoch.
    pub async fn generate_image_for_route_at_epoch(
        &self,
        route: &str,
        expected_epoch: Option<u64>,
        request: crate::provider_contract::ImageProviderRequest,
    ) -> Result<Vec<crate::provider_contract::ProviderImageOutput>, ProviderError> {
        let provider = self.provider_for_route(route)?;
        if self.image_output_capability_for_route(route)?.is_none() {
            return Err(ProviderError::Config("image_output_unsupported".into()));
        }
        self.preflight_image_output_for_route(route, expected_epoch, &request)
            .map_err(|error| match error {
                ProviderError::ImageCapabilityStale => ProviderError::ImageCapabilityStale,
                _ => ProviderError::ImagePreflightRejected,
            })?;
        provider.generate_image(request).await
    }

    /// Starts a streaming request using an explicit route and model override.
    pub fn stream_chat_for_route_with_model(
        &self,
        route: &str,
        model: Option<&str>,
        messages: &[ChatMessage],
    ) -> Result<TokenStream, ProviderError> {
        if let Some(preflight) = &self.route_preflight {
            preflight.check(route, model, current_time_ms())?;
        }
        self.dispatch_stream(route, model, messages)
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

    /// Executes a tool-enabled chat request on the named route and model.
    pub async fn chat_with_tools_for_route(
        &self,
        route: &str,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> Result<ChatResult, ProviderError> {
        if let Some(preflight) = &self.route_preflight {
            preflight
                .check_for_request_async(route, model, !tools.is_empty(), current_time_ms())
                .await?;
        }
        let result = self.dispatch_chat(route, model, messages, tools).await?;
        if let Some(preflight) = &self.route_preflight {
            preflight
                .observe_success_async(route, model, &result, current_time_ms())
                .await;
        }
        Ok(result)
    }

    /// Executes a bounded non-streaming request on one named route.
    ///
    /// The normal route preflight still runs before provider dispatch. Provider
    /// adapters that do not support the requested bounds may ignore them, so
    /// the caller must also validate the response and usage.
    pub async fn chat_with_tools_for_route_bounded(
        &self,
        route: &str,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        options: ChatRequestOptions,
    ) -> Result<ChatResult, ProviderError> {
        if let Some(preflight) = &self.route_preflight {
            preflight
                .check_for_request_async(route, model, !tools.is_empty(), current_time_ms())
                .await?;
        }
        let result = self
            .provider_for_route(route)?
            .chat_with_tools_with_options(model, messages, tools, options)
            .await?;
        if let Some(preflight) = &self.route_preflight {
            preflight
                .observe_success_async(route, model, &result, current_time_ms())
                .await;
        }
        Ok(result)
    }

    /// Единственная внутренняя граница к provider implementation. Provenance
    /// checkpoint располагается перед этим слоем; feature-код не получает
    /// `ModelProvider` или raw dispatch handle.
    fn dispatch_stream(
        &self,
        route: &str,
        model: Option<&str>,
        messages: &[ChatMessage],
    ) -> Result<TokenStream, ProviderError> {
        let provider = self.provider_for_route(route)?;
        let response = match model {
            Some(model) if !model.trim().is_empty() => {
                provider.stream_chat_with_model(model, messages)
            }
            _ => provider.stream_chat(messages),
        };
        let Some(preflight) = self.route_preflight.clone() else {
            return Ok(response);
        };
        let route = route.to_owned();
        let model = model.map(str::to_owned);
        Ok(Box::pin(stream! {
            let mut response = response;
            let mut has_content = false;
            let mut completed = true;
            while let Some(item) = response.next().await {
                match &item {
                    Ok(ChatStreamItem::Delta(chunk) | ChatStreamItem::Thinking(chunk))
                        if !chunk.trim().is_empty() => has_content = true,
                    Err(_) => completed = false,
                    _ => {}
                }
                yield item;
            }
            if completed && has_content {
                preflight
                    .observe_stream_success_async(&route, model.as_deref(), current_time_ms())
                    .await;
            }
        }))
    }

    async fn dispatch_chat(
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

    /// Policy entry point used by Core's agent loop. The caller supplies only
    /// classification/capability metadata; the route name is selected here.
    pub async fn chat_with_tools_with_policy(
        &self,
        mode: RoutingMode,
        request: &RoutingRequest,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> Result<ChatResult, ProviderError> {
        Ok(self
            .chat_with_tools_with_policy_and_route(mode, request, model, messages, tools)
            .await?
            .result)
    }

    /// Executes a tool-enabled request under route policy and returns selection evidence.
    pub async fn chat_with_tools_with_policy_and_route(
        &self,
        mode: RoutingMode,
        request: &RoutingRequest,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> Result<PolicyChatResult, ProviderError> {
        #[cfg(not(test))]
        if request.task_class.is_some() || request.offline || request.estimated_input_tokens > 0 {
            let now_ms = current_time_ms();
            let snapshot = self
                .route_policy_snapshot_with_model(request, model, now_ms)
                .map_err(|error| ProviderError::Config(error.to_string()))?;
            let overlay = RunHealthOverlay::new(&snapshot.run_id);
            let decision = select_route_snapshot_cached(
                request,
                &snapshot,
                &overlay,
                builtin_routing_catalog(),
                0,
                now_ms,
            )
            .map_err(|error| ProviderError::Config(error.to_string()))?;
            let snapshot_hash = snapshot.round_trip_hash().ok();
            let policy_hash = snapshot_hash
                .clone()
                .unwrap_or_else(|| "snapshot-hash-unavailable".into());
            let mut trace = RunTrace::new(
                snapshot.run_id.clone(),
                policy_hash,
                snapshot.schema_version.clone(),
            );
            let mut routes = Vec::new();
            if let Some(route) = decision.selected_route.clone() {
                routes.push(route);
            }
            routes.extend(decision.fallback_chain.clone());
            let retry = RetryConfig::default();
            let mut last_error = None;
            let mut attempt_id = 1_u32;
            'routes: for route in routes.into_iter().take(retry.max_attempts as usize) {
                for route_attempt in 0..retry.max_attempts_per_route {
                    let capability_epoch = snapshot
                        .candidates
                        .iter()
                        .find(|candidate| candidate.route_id == route)
                        .map(|candidate| candidate.capabilities.capability_epoch)
                        .unwrap_or_default();
                    let backoff = if attempt_id == 1 {
                        0
                    } else {
                        retry
                            .compute_backoff(attempt_id - 1, &snapshot.run_id, &route)
                            .as_millis() as u64
                    };
                    let selection_now_ms = snapshot.created_at.saturating_add(attempt_id as u64);
                    trace.add_attempt(AttemptTrace {
                        attempt_id,
                        now_ms: selection_now_ms,
                        route_id: route.clone(),
                        capability_epoch,
                        selection_reason: if attempt_id == 1 {
                            decision.reason_code.clone()
                        } else {
                            "fallback_after_provider_failure".into()
                        },
                        failure_category: None,
                        backoff_ms: backoff,
                        overlay_generation: overlay.generation(),
                    });
                    if backoff > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
                    }
                    if let Some(preflight) = &self.route_preflight {
                        if let Err(error) = preflight
                            .check_for_request_async(
                                &route,
                                model,
                                !tools.is_empty(),
                                current_time_ms(),
                            )
                            .await
                        {
                            if let Some(attempt) = trace.attempts.last_mut() {
                                attempt.failure_category = Some(FailureCategory::InvalidRequest);
                            }
                            last_error = Some(error);
                            attempt_id = attempt_id.saturating_add(1);
                            if attempt_id > retry.max_attempts {
                                break 'routes;
                            }
                            break;
                        }
                    }
                    match self
                        .chat_with_tools_for_route(&route, model, messages, tools)
                        .await
                    {
                        Ok(result) => {
                            trace.set_result(RunResult::Success);
                            trace.circuit_opened_during_run = overlay.circuit_opened_during_run();
                            return Ok(PolicyChatResult {
                                selected_route: route,
                                fallback_chain: decision.fallback_chain.clone(),
                                result,
                                decision: Some(decision),
                                snapshot_hash,
                                attempt_trace: Some(trace),
                            });
                        }
                        Err(error) => {
                            let category = classify_failure(&error);
                            if let Some(attempt) = trace.attempts.last_mut() {
                                attempt.failure_category = Some(category);
                            }
                            let _ = overlay.record_failure_at(
                                &route,
                                attempt_id,
                                category,
                                &retry,
                                snapshot.created_at.saturating_add(attempt_id as u64),
                            );
                            last_error = Some(error);
                            attempt_id = attempt_id.saturating_add(1);
                            if category == FailureCategory::InvalidRequest {
                                break 'routes;
                            }
                            if attempt_id > retry.max_attempts {
                                break 'routes;
                            }
                            if route_attempt + 1 >= retry.max_attempts_per_route {
                                break;
                            }
                        }
                    }
                }
            }
            trace.set_result(RunResult::RouteExhausted);
            return Err(last_error.unwrap_or(ProviderError::Config(decision.reason_code)));
        }
        let runtime = self
            .plan_route(mode, request, RuntimeLimits::default())
            .map_err(|error| ProviderError::Config(error.to_string()))?;
        let route = runtime
            .decision()
            .selected_route
            .as_deref()
            .ok_or_else(|| ProviderError::Config("routing policy selected no route".into()))?;
        if let Some(preflight) = &self.route_preflight {
            preflight
                .check_for_request_async(route, model, !tools.is_empty(), current_time_ms())
                .await?;
        }
        let result = self
            .chat_with_tools_for_route(route, model, messages, tools)
            .await?;
        Ok(PolicyChatResult {
            selected_route: route.to_owned(),
            fallback_chain: runtime.decision().fallback_chain.clone(),
            result,
            decision: None,
            snapshot_hash: None,
            attempt_trace: None,
        })
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

    /// Freezes eligible route candidates and policy inputs at the supplied time.
    pub fn route_policy_snapshot(
        &self,
        request: &RoutingRequest,
        now_ms: u64,
    ) -> Result<RoutePolicySnapshot, SnapshotError> {
        self.route_policy_snapshot_with_model(request, None, now_ms)
    }

    fn route_policy_snapshot_with_model(
        &self,
        request: &RoutingRequest,
        model_override: Option<&str>,
        now_ms: u64,
    ) -> Result<RoutePolicySnapshot, SnapshotError> {
        let candidates =
            self.route_candidates()
                .into_iter()
                .map(|candidate| {
                    let model = if candidate.route_id == self.default_route {
                        model_override
                            .map(str::trim)
                            .filter(|model| !model.is_empty())
                            .unwrap_or(&candidate.model)
                            .to_owned()
                    } else {
                        candidate.model
                    };
                    let execution_class = if self
                        .routes
                        .get(&candidate.route_id)
                        .is_some_and(|provider| provider.kind() == ProviderKind::Local)
                    {
                        ExecutionClass::Local
                    } else {
                        ExecutionClass::Cloud
                    };
                    let image_output = self
                        .routes
                        .get(&candidate.route_id)
                        .and_then(|provider| provider.image_output_capability())
                        .filter(crate::provider_contract::ImageOutputCapability::validate);
                    CandidateEntry {
                        route_id: candidate.route_id,
                        model,
                        capabilities: CapabilityMetadata {
                            schema_version: "capability-metadata-v1".into(),
                            provider_version: "gateway".into(),
                            capability_epoch: 1,
                            tool_calling: true,
                            structured_output: true,
                            context_limit: None,
                            streaming: true,
                            vision: false,
                            image_output,
                            execution_class,
                            privacy_boundary: crate::provider_contract::PrivacyClass::Internal,
                        },
                        initial_health:
                            crate::provider_contract::CandidateHealthSnapshot::unknown_at(now_ms),
                        cost_micros_per_1k_tokens: candidate.cost_micros_per_1k_tokens,
                        p95_latency_ms: candidate.p95_latency_ms,
                        privacy: crate::provider_contract::PrivacyClass::Internal,
                        fallback_rank: candidate.fallback_rank,
                    }
                })
                .collect();
        let preference = crate::provider_contract::UserPreference {
            preferred_order: request.preferred_route.clone().into_iter().collect(),
            avoid: Vec::new(),
        };
        RoutePolicySnapshot::new_at(
            format!("run-{now_ms}"),
            candidates,
            PolicyHashes::from_canonical_json(
                b"routing-v1",
                b"approval-v1",
                b"tools-v1",
                b"sandbox-v1",
                b"retry-v1",
            ),
            preference,
            None,
            now_ms,
        )
    }

    /// Returns the exact immutable routing snapshot commitment used by the
    /// policy dispatch path. Core calls this immediately before committing a
    /// model-request envelope and passes the same request to dispatch.
    /// Computes a provenance digest for the selected route snapshot.
    pub fn provenance_route_snapshot_hash(
        &self,
        request: &RoutingRequest,
    ) -> Result<String, ProviderError> {
        self.provenance_route_snapshot_hash_with_model(request, None)
    }

    /// Computes a provenance digest including an optional model override.
    pub fn provenance_route_snapshot_hash_with_model(
        &self,
        request: &RoutingRequest,
        model: Option<&str>,
    ) -> Result<String, ProviderError> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.route_policy_snapshot_with_model(request, model, now_ms)
            .map_err(|error| ProviderError::Config(error.to_string()))?
            .round_trip_hash()
            .map_err(|error| ProviderError::Config(error.to_string()))
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
        let now_ms = current_time_ms();
        let requires_verified_free = self
            .route_preflight
            .as_ref()
            .is_some_and(|preflight| preflight.requires_verified_free_route());
        let prefers_verified_free = self
            .route_preflight
            .as_ref()
            .is_some_and(|preflight| preflight.prefers_verified_free_routes());
        route_ids
            .into_iter()
            .filter_map(|route_id| {
                let provider = &self.routes[route_id];
                let model = provider.model_name().to_string();
                let verified_free = self.route_preflight.as_ref().is_some_and(|preflight| {
                    preflight.is_verified_free_route(route_id, &model, now_ms)
                });
                if requires_verified_free && !verified_free {
                    return None;
                }
                let cost_micros_per_1k_tokens = if prefers_verified_free {
                    if verified_free {
                        0
                    } else {
                        1
                    }
                } else if model.ends_with(":free") {
                    0
                } else {
                    1
                };
                let mut capabilities = vec!["chat".to_string()];
                if provider
                    .image_output_capability()
                    .is_some_and(|capability| capability.validate())
                {
                    capabilities.push("image_output".to_string());
                }
                Some(RouteCandidate {
                    route_id: route_id.clone(),
                    model,
                    capabilities,
                    cost_micros_per_1k_tokens,
                    p95_latency_ms: 0,
                    privacy: PrivacyClass::Internal,
                    available: true,
                    fallback_rank: 0,
                })
            })
            .collect()
    }

    fn provider_for_route(&self, route: &str) -> Result<&Arc<dyn ModelProvider>, ProviderError> {
        self.routes
            .get(route)
            .ok_or_else(|| ProviderError::Config(format!("unknown model route: {route}")))
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
        ProviderKind::OpenAIResponses => Ok(Arc::new(OpenAIResponsesProvider::new(
            route.literouter.clone(),
        )?)),
        ProviderKind::Ollama => Ok(Arc::new(OllamaProvider::new(route.literouter.clone())?)),
        ProviderKind::Local => {
            LocalProvider::validate_loopback(&route.literouter.base_url)?;
            Ok(Arc::new(LocalProvider::new(route.literouter.clone())?))
        }
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

    #[tokio::test]
    async fn chat_only_provider_reports_typed_image_unsupported() {
        let gateway = mock_gateway(vec![]);
        let route = gateway.default_route_id().to_owned();
        assert!(gateway
            .image_output_capability_for_route(&route)
            .expect("valid route")
            .is_none());
        let request = ImageProviderRequest {
            operation: ImageOutputOperation::Generate,
            prompt: "a blue square".into(),
            width: 64,
            height: 64,
            count: 1,
            mime_type: "image/png".into(),
            required_privacy: PrivacyClass::Restricted,
            allow_cloud: false,
            input_images: Vec::new(),
            mask_image: None,
        };
        let error = match gateway.generate_image_for_route(&route, request).await {
            Err(error) => error,
            Ok(_) => panic!("mock provider unexpectedly generated images"),
        };
        assert!(error.to_string().contains("image_output_unsupported"));
    }

    fn pricing_detail(id: &str, pricing: &[(&str, &str)]) -> OpenRouterModelDetail {
        OpenRouterModelDetail {
            id: id.to_string(),
            pricing: pricing
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
        }
    }

    #[test]
    fn openrouter_pricing_requires_exact_zero_for_every_declared_dimension() {
        let zero = normalize_openrouter_model_pricing(
            pricing_detail(
                "author/model:free",
                &[
                    ("prompt", "0"),
                    ("completion", "0.000"),
                    ("request", "0"),
                    ("image", "0.0"),
                ],
            ),
            "author/model:free",
        )
        .expect("valid zero pricing");
        assert!(zero.all_dimensions_zero);
        assert_eq!(zero.source_hash.len(), 64);

        let paid = normalize_openrouter_model_pricing(
            pricing_detail(
                "author/model:free",
                &[("prompt", "0"), ("completion", "0"), ("request", "0.1")],
            ),
            "author/model:free",
        )
        .expect("valid paid dimension");
        assert!(!paid.all_dimensions_zero);
    }

    #[test]
    fn openrouter_pricing_rejects_missing_dimensions_identity_drift_and_bad_decimals() {
        assert!(normalize_openrouter_model_pricing(
            pricing_detail("author/model", &[("prompt", "0"), ("completion", "0")]),
            "author/model",
        )
        .is_err());
        assert!(normalize_openrouter_model_pricing(
            pricing_detail(
                "author/other",
                &[("prompt", "0"), ("completion", "0"), ("request", "0")],
            ),
            "author/model",
        )
        .is_err());
        for invalid in ["-0", "+0", "0e0", "NaN", "1.2.3", ".0", "0."] {
            assert!(normalize_openrouter_model_pricing(
                pricing_detail(
                    "author/model",
                    &[("prompt", invalid), ("completion", "0"), ("request", "0")],
                ),
                "author/model",
            )
            .is_err());
        }
    }

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
            preferred_route: None,
            task_class: None,
            offline: false,
            allow_cloud: true,
            estimated_input_tokens: 0,
            quality_delta: 0.05,
        }
    }

    struct FreeEvidencePreflight {
        strict: bool,
    }

    impl RoutePreflight for FreeEvidencePreflight {
        fn check(
            &self,
            _route: &str,
            _model: Option<&str>,
            _now_ms: u64,
        ) -> Result<(), ProviderError> {
            Ok(())
        }

        fn requires_verified_free_route(&self) -> bool {
            self.strict
        }

        fn prefers_verified_free_routes(&self) -> bool {
            !self.strict
        }

        fn is_verified_free_route(&self, route: &str, _model: &str, _now_ms: u64) -> bool {
            route == "confirmed"
        }
    }

    #[test]
    fn free_only_filters_unverified_candidates_and_prefer_free_ranks_only_evidence() {
        let routes = vec![
            ("advertised", "author/model:free"),
            ("confirmed", "author/model"),
            ("paid", "author/paid"),
        ];
        let prefer_free = gateway_with_routes(routes.clone())
            .with_route_preflight(Arc::new(FreeEvidencePreflight { strict: false }));
        let candidates = prefer_free.route_candidates();
        let by_route: HashMap<_, _> = candidates
            .iter()
            .map(|candidate| {
                (
                    candidate.route_id.as_str(),
                    candidate.cost_micros_per_1k_tokens,
                )
            })
            .collect();
        assert_eq!(by_route.get("confirmed"), Some(&0));
        assert_eq!(by_route.get("advertised"), Some(&1));

        let free_only = gateway_with_routes(routes)
            .with_route_preflight(Arc::new(FreeEvidencePreflight { strict: true }));
        let candidates = free_only.route_candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].route_id, "confirmed");
    }

    fn gateway_with_routes(routes: Vec<(&str, &str)>) -> ModelGateway {
        let default_route = routes
            .iter()
            .find(|(route_id, _)| *route_id == "local")
            .or_else(|| routes.first())
            .map(|(route_id, _)| *route_id)
            .expect("at least one route exists");
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
        ModelGateway::from_routes(default_route, routes).expect("default route exists")
    }

    struct RejectingPreflight;

    impl RoutePreflight for RejectingPreflight {
        fn check(
            &self,
            _route: &str,
            _model: Option<&str>,
            _now_ms: u64,
        ) -> Result<(), ProviderError> {
            Err(ProviderError::Config("preflight_rejected".into()))
        }
    }

    #[tokio::test]
    async fn gateway_runs_route_preflight_before_provider_dispatch() {
        let gateway = gateway_with_routes(vec![("local", "local-model")])
            .with_route_preflight(Arc::new(RejectingPreflight));
        let error = gateway
            .chat_with_tools_with_policy_and_route(
                RoutingMode::Balanced,
                &policy_request(),
                None,
                &[ChatMessage::text(crate::providers::ChatRole::User, "hello")],
                &[],
            )
            .await
            .expect_err("preflight must reject before provider dispatch");
        assert!(matches!(error, ProviderError::Config(code) if code == "preflight_rejected"));
    }

    struct ToolCallAwarePreflight;

    impl RoutePreflight for ToolCallAwarePreflight {
        fn check(
            &self,
            _route: &str,
            _model: Option<&str>,
            _now_ms: u64,
        ) -> Result<(), ProviderError> {
            Err(ProviderError::Config(
                "request_requirements_not_forwarded".into(),
            ))
        }

        fn check_for_request(
            &self,
            _route: &str,
            _model: Option<&str>,
            requires_tool_calls: bool,
            _now_ms: u64,
        ) -> Result<(), ProviderError> {
            if requires_tool_calls {
                Ok(())
            } else {
                Err(ProviderError::Config(
                    "tool_call_requirement_missing".into(),
                ))
            }
        }
    }

    #[tokio::test]
    async fn gateway_forwards_tool_call_requirement_to_route_preflight() {
        let gateway = gateway_with_routes(vec![("local", "local-model")])
            .with_route_preflight(Arc::new(ToolCallAwarePreflight));
        let tools = [ToolSpec::function(
            "example",
            "example tool",
            serde_json::json!({"type":"object","properties":{}}),
        )];
        let result = gateway
            .chat_with_tools_with_policy_and_route(
                RoutingMode::Balanced,
                &policy_request(),
                None,
                &[ChatMessage::text(crate::providers::ChatRole::User, "hello")],
                &tools,
            )
            .await
            .expect("tool requirement should reach preflight");
        assert_eq!(result.selected_route, "local");
    }

    #[test]
    fn policy_snapshot_uses_selected_model_for_default_route() {
        let gateway = gateway_with_routes(vec![("local", "")]);

        let snapshot = gateway
            .route_policy_snapshot_with_model(&policy_request(), Some("provider-model"), 1)
            .expect("selected model should make the snapshot valid");

        assert_eq!(snapshot.candidates[0].model, "provider-model");
        assert_eq!(
            snapshot.candidates[0].initial_health.status,
            HealthStatus::Unknown
        );
    }

    #[test]
    fn policy_snapshot_reports_empty_model_as_model_error() {
        let gateway = gateway_with_routes(vec![("local", "")]);

        let error = gateway
            .route_policy_snapshot(&policy_request(), 1)
            .expect_err("empty configured model must be rejected");

        assert_eq!(
            error.to_string(),
            "invalid model name: model is empty or too long"
        );
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
            preferred_route: None,
            task_class: None,
            offline: false,
            allow_cloud: true,
            estimated_input_tokens: 0,
            quality_delta: 0.05,
        };
        let runtime = gateway
            .plan_route(RoutingMode::Balanced, &request, RuntimeLimits::default())
            .expect("plan runs even when denied");
        assert_eq!(runtime.decision().selected_route, None);

        let result = gateway.stream_chat_with_policy(RoutingMode::Balanced, &request, &[]);
        assert!(matches!(result, Err(ProviderError::Config(_))));
    }
}

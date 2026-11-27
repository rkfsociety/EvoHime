use crate::providers::{ProviderError, ProviderKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::time::Duration;

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

/// Minimum spacing between provider request starts.
///
/// The interval is a local pacing guard, not a rate-limit retry: it spaces
/// successful attempts so the provider does not answer with a rate limit at
/// all. Providers other than LiteRouter do not need it and keep it disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestPacingConfig {
    /// Whether the adapter spaces request starts.
    pub enabled: bool,
    /// Minimum interval for regular models, in milliseconds.
    pub interval_ms: u64,
    /// Minimum interval for `:free` models, in milliseconds.
    pub free_interval_ms: u64,
}

impl Default for RequestPacingConfig {
    fn default() -> Self {
        Self::disabled()
    }
}

impl RequestPacingConfig {
    /// Largest accepted interval; a misconfigured value stays bounded.
    pub const MAX_INTERVAL_MS: u64 = 3_600_000;
    /// Builds a policy that never waits between request starts.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            interval_ms: 0,
            free_interval_ms: 0,
        }
    }

    /// Returns the built-in policy for a provider transport.
    ///
    /// No transport gets an implicit pause: pacing is an explicit user setting.
    pub fn for_provider(_provider: ProviderKind) -> Self {
        Self::disabled()
    }

    /// Reads the `MODEL_PROVIDER_REQUEST_PACING_*` environment variables,
    /// falling back to an unlimited, disabled policy.
    pub fn from_env(provider: ProviderKind) -> Self {
        Self::from_lookup(provider, &|name| env::var(name).ok())
    }

    /// Reads `MODEL_PROVIDER_REQUEST_PACING_*` settings from a key-value map.
    pub fn from_lookup(provider: ProviderKind, lookup: &dyn Fn(&str) -> Option<String>) -> Self {
        let default = Self::for_provider(provider);
        Self {
            enabled: lookup_bool(
                lookup,
                "MODEL_PROVIDER_REQUEST_PACING_ENABLED",
                default.enabled,
            ),
            interval_ms: lookup_u64(
                lookup,
                "MODEL_PROVIDER_REQUEST_PACING_INTERVAL_MS",
                default.interval_ms,
            ),
            free_interval_ms: lookup_u64(
                lookup,
                "MODEL_PROVIDER_REQUEST_PACING_FREE_INTERVAL_MS",
                default.free_interval_ms,
            ),
        }
        .normalized()
    }

    /// Clamps the intervals into the accepted range.
    pub fn normalized(self) -> Self {
        Self {
            enabled: self.enabled,
            interval_ms: self.interval_ms.min(Self::MAX_INTERVAL_MS),
            free_interval_ms: self.free_interval_ms.min(Self::MAX_INTERVAL_MS),
        }
    }

    /// Returns the pacing interval for a model, or `None` when pacing is off.
    pub fn interval_for_model(self, model: &str) -> Option<Duration> {
        if !self.enabled {
            return None;
        }
        let interval_ms = if model.trim().to_ascii_lowercase().ends_with(":free") {
            self.free_interval_ms
        } else {
            self.interval_ms
        };
        (interval_ms > 0).then(|| Duration::from_millis(interval_ms))
    }
}

/// Optional per-model token pricing in USD per 1M tokens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelPricingConfig {
    /// Price in USD per 1M uncached input tokens.
    #[serde(default, alias = "inputUsdPer1M")]
    pub input_usd_per_1m: f64,
    /// Price in USD per 1M cache-hit input tokens.
    #[serde(default, alias = "cachedInputUsdPer1M")]
    pub cached_input_usd_per_1m: f64,
    /// Price in USD per 1M output tokens.
    #[serde(default, alias = "outputUsdPer1M")]
    pub output_usd_per_1m: f64,
}

/// Schedule defining when provider-level request caps and daily token quotas reset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitResetSchedule {
    /// Whether all request caps and daily token quotas reset at a fixed daily time.
    ///
    /// When `false`, request caps use rolling windows (`60s`, `1h`, `4h`, `24h`)
    /// and token quotas use a rolling 24-hour window.
    #[serde(default, alias = "resetScheduleEnabled")]
    pub enabled: bool,
    /// Minute of day in the provider's timezone (`0..=1439`, e.g. `0` for `00:00`).
    #[serde(default, alias = "resetTimeMinutes")]
    pub time_minutes: u16,
    /// Fixed UTC offset in hours (`-12..=14`, e.g. `+2` for `UTC+2`, `-3` for `UTC-3`).
    #[serde(default, alias = "resetUtcOffsetHours")]
    pub utc_offset_hours: i8,
}

impl LimitResetSchedule {
    /// Returns whether `time_minutes` and `utc_offset_hours` are within valid calendar bounds.
    pub fn is_valid(self) -> bool {
        self.time_minutes < 1_440 && (-12..=14).contains(&self.utc_offset_hours)
    }

    /// Returns the Unix timestamp in milliseconds of the start of the active window.
    pub fn window_start_ms(self, now_ms: i64) -> i64 {
        if !self.enabled {
            return now_ms.saturating_sub(86_400_000);
        }
        let offset_ms = i64::from(self.utc_offset_hours.clamp(-12, 14)) * 3_600_000;
        let local_ms = now_ms.saturating_add(offset_ms);
        let day_start_local = local_ms.div_euclid(86_400_000) * 86_400_000;
        let candidate_local = day_start_local + i64::from(self.time_minutes.min(1_439)) * 60_000;
        let reset_local = if candidate_local <= local_ms {
            candidate_local
        } else {
            candidate_local - 86_400_000
        };
        reset_local - offset_ms
    }

    /// Returns the Unix timestamp in milliseconds of the next scheduled reset (or +1h if rolling).
    pub fn next_reset_ms(self, now_ms: i64) -> i64 {
        if !self.enabled {
            return now_ms.saturating_add(3_600_000);
        }
        self.window_start_ms(now_ms).saturating_add(86_400_000)
    }
}

/// Optional rolling request caps and token pricing for one exact model identifier.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelRateLimitConfig {
    /// Whether any configured cap is enforced.
    #[serde(default)]
    pub enabled: bool,
    /// Maximum requests in a rolling minute; zero means unlimited.
    #[serde(default)]
    pub rpm: u32,
    /// Maximum requests in a rolling hour; zero means unlimited.
    #[serde(default)]
    pub rph: u32,
    /// Maximum requests in a rolling 4-hour window; zero means unlimited.
    #[serde(default)]
    pub rp4h: u32,
    /// Maximum requests in a rolling day; zero means unlimited.
    #[serde(default)]
    pub rpd: u32,
    /// Optional daily token ceiling for this route (normally set on `"*"`).
    #[serde(
        default,
        alias = "dailyTokenQuota",
        skip_serializing_if = "Option::is_none"
    )]
    pub daily_token_quota: Option<u64>,
    /// Whether the token ceiling resets at UTC midnight (legacy flag).
    #[serde(default, alias = "quotaResetUtcMidnight")]
    pub quota_reset_utc_midnight: bool,
    /// Optional user-configured maximum context window (tokens).
    #[serde(
        default,
        alias = "contextLengthTokens",
        skip_serializing_if = "Option::is_none"
    )]
    pub context_length_tokens: Option<u32>,
    /// Optional user-configured context compression threshold (tokens).
    #[serde(
        default,
        alias = "contextCompressionTokens",
        skip_serializing_if = "Option::is_none"
    )]
    pub context_compression_tokens: Option<u32>,
    /// Whether all provider limits reset on a fixed daily schedule.
    #[serde(default, alias = "resetScheduleEnabled")]
    pub reset_schedule_enabled: bool,
    /// Minute of day (`0..=1439`) when the scheduled reset occurs.
    #[serde(default, alias = "resetTimeMinutes")]
    pub reset_time_minutes: u16,
    /// Fixed UTC offset in hours (`-12..=14`) for the scheduled reset.
    #[serde(default, alias = "resetUtcOffsetHours")]
    pub reset_utc_offset_hours: i8,
    /// Optional user-configured token pricing in USD per 1M tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<ModelPricingConfig>,
}

impl ModelRateLimitConfig {
    /// Returns whether any rolling request window cap is enabled and non-zero.
    pub fn has_request_caps(&self) -> bool {
        self.enabled && (self.rpm > 0 || self.rph > 0 || self.rp4h > 0 || self.rpd > 0)
    }

    /// Returns `(cap, window_duration)` pairs across all rolling request windows.
    pub fn request_windows(&self) -> [(u32, Duration); 4] {
        [
            (self.rpm, Duration::from_secs(60)),
            (self.rph, Duration::from_secs(3_600)),
            (self.rp4h, Duration::from_secs(4 * 3_600)),
            (self.rpd, Duration::from_secs(86_400)),
        ]
    }

    /// Returns the normalized daily reset schedule for this configuration.
    pub fn reset_schedule(&self) -> LimitResetSchedule {
        LimitResetSchedule {
            enabled: self.reset_schedule_enabled || self.quota_reset_utc_midnight,
            time_minutes: self.reset_time_minutes.min(1_439),
            utc_offset_hours: self.reset_utc_offset_hours.clamp(-12, 14),
        }
    }
}

/// Upper bound accepted for one route's configured daily token quota.
///
/// The bound only rejects implausible configuration values; it is not a
/// product-imposed budget. It matches the ceiling the shell form and
/// `HiveSettings` accept, so a value that reaches Core is never rejected here
/// and the whole provider configuration keeps loading.
pub const MAX_DAILY_TOKEN_QUOTA: u64 = 1_000_000_000_000;

/// Unified provider request rate limits across rolling time windows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderLimits {
    /// Maximum requests per rolling minute (`0` means unlimited).
    #[serde(default)]
    pub rpm: u32,
    /// Maximum requests per rolling hour (`0` means unlimited).
    #[serde(default)]
    pub rph: u32,
    /// Maximum requests per rolling 4-hour window (`0` means unlimited).
    #[serde(default)]
    pub rp4h: u32,
    /// Maximum requests per rolling day (`0` means unlimited).
    #[serde(default)]
    pub rpd: u32,
}

fn lookup_bool(lookup: &dyn Fn(&str) -> Option<String>, name: &str, default: bool) -> bool {
    match lookup(name) {
        Some(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        None => default,
    }
}

fn lookup_u64(lookup: &dyn Fn(&str) -> Option<String>, name: &str, default: u64) -> u64 {
    lookup(name)
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

/// Stable provider identity selected independently from the wire transport.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderProfileId {
    /// Custom OpenAI-compatible endpoint without a known vendor profile.
    #[default]
    #[serde(rename = "custom")]
    Custom,
    /// OpenAI hosted API profile.
    #[serde(rename = "openai", alias = "open_ai")]
    OpenAi,
    /// OpenRouter hosted API profile.
    #[serde(rename = "openrouter", alias = "open_router")]
    OpenRouter,
    /// Groq hosted API profile.
    #[serde(rename = "groq")]
    Groq,
    /// Google Gemini OpenAI-compatible API profile.
    #[serde(rename = "gemini")]
    Gemini,
    /// Mistral hosted API profile.
    #[serde(rename = "mistral")]
    Mistral,
    /// Cloudflare Workers AI account-scoped API profile.
    #[serde(rename = "cloudflare_workers_ai")]
    CloudflareWorkersAi,
    /// NVIDIA NIM hosted API profile.
    #[serde(rename = "nvidia_nim")]
    NvidiaNim,
    /// Cerebras hosted API profile.
    #[serde(rename = "cerebras")]
    Cerebras,
    /// Hugging Face Inference Providers profile.
    #[serde(rename = "hugging_face")]
    HuggingFace,
    /// DeepSeek hosted API profile.
    #[serde(rename = "deepseek")]
    DeepSeek,
    /// xAI (Grok) hosted API profile.
    #[serde(rename = "xai")]
    XAi,
    /// Together AI hosted API profile.
    #[serde(rename = "together_ai")]
    TogetherAi,
    /// Fireworks AI hosted API profile.
    #[serde(rename = "fireworks_ai")]
    FireworksAi,
    /// Moonshot (Kimi) hosted API profile.
    #[serde(rename = "moonshot")]
    Moonshot,
    /// Z.AI (GLM) hosted API profile.
    #[serde(rename = "zai")]
    ZAi,
    /// SambaNova hosted API profile.
    #[serde(rename = "sambanova")]
    SambaNova,
    /// Nebius Token Factory hosted API profile.
    #[serde(rename = "nebius")]
    Nebius,
    /// Rout free token-pool hosted API profile (650k tokens/day, resets at 00:00 UTC).
    #[serde(rename = "rout", alias = "routmy")]
    Rout,
    /// AnyModel hosted API profile.
    #[serde(rename = "anymodel")]
    AnyModel,
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
            Self::DeepSeek => "deepseek",
            Self::XAi => "xai",
            Self::TogetherAi => "together_ai",
            Self::FireworksAi => "fireworks_ai",
            Self::Moonshot => "moonshot",
            Self::ZAi => "zai",
            Self::SambaNova => "sambanova",
            Self::Nebius => "nebius",
            Self::Rout => "rout",
            Self::AnyModel => "anymodel",
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
            "deepseek" => Self::DeepSeek,
            "xai" => Self::XAi,
            "together_ai" => Self::TogetherAi,
            "fireworks_ai" => Self::FireworksAi,
            "moonshot" => Self::Moonshot,
            "zai" => Self::ZAi,
            "sambanova" => Self::SambaNova,
            "nebius" => Self::Nebius,
            "rout" => Self::Rout,
            "anymodel" => Self::AnyModel,
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
            Self::DeepSeek => Some("https://api.deepseek.com/v1"),
            Self::XAi => Some("https://api.x.ai/v1"),
            Self::TogetherAi => Some("https://api.together.ai/v1"),
            Self::FireworksAi => Some("https://api.fireworks.ai/inference/v1"),
            Self::Moonshot => Some("https://api.moonshot.ai/v1"),
            Self::ZAi => Some("https://api.z.ai/api/paas/v4"),
            Self::SambaNova => Some("https://api.sambanova.ai/v1"),
            Self::Nebius => Some("https://api.tokenfactory.nebius.com/v1"),
            Self::Rout => Some("https://api.rout.my/v1"),
            Self::AnyModel => Some("https://anymodel.org/v1"),
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
        Self::from_lookup(&|name| env::var(name).ok())
    }

    pub(crate) fn from_lookup(lookup: &dyn Fn(&str) -> Option<String>) -> Self {
        let api_key = lookup("LITEROUTER_API_KEY").unwrap_or_default();
        let base_url = lookup("LITEROUTER_BASE_URL")
            .unwrap_or_else(|| LITEROUTER_DEFAULT_BASE_URL.to_string());
        let model = lookup("LITEROUTER_MODEL").unwrap_or_default();

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
        Self::openai_compatible_from_lookup(&|name| env::var(name).ok())
    }

    pub(crate) fn openai_compatible_from_lookup(lookup: &dyn Fn(&str) -> Option<String>) -> Self {
        let api_key = lookup("OPENAI_API_KEY").unwrap_or_default();
        let base_url =
            lookup("OPENAI_BASE_URL").unwrap_or_else(|| OPENAI_DEFAULT_BASE_URL.to_string());
        let model = lookup("OPENAI_MODEL").unwrap_or_else(|| OPENAI_DEFAULT_MODEL.to_string());

        Self {
            api_key,
            base_url: normalize_base_url(&base_url),
            model,
        }
    }

    /// Reads OpenAI Responses settings from `OPENAI_*` environment variables.
    pub fn openai_responses_from_env() -> Self {
        Self::openai_responses_from_lookup(&|name| env::var(name).ok())
    }

    pub(crate) fn openai_responses_from_lookup(lookup: &dyn Fn(&str) -> Option<String>) -> Self {
        let api_key = lookup("OPENAI_API_KEY").unwrap_or_default();
        let base_url =
            lookup("OPENAI_BASE_URL").unwrap_or_else(|| OPENAI_DEFAULT_BASE_URL.to_string());
        let model =
            lookup("OPENAI_MODEL").unwrap_or_else(|| OPENAI_CODEX_DEFAULT_MODEL.to_string());

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
    /// Minimum spacing between request starts for this route.
    #[serde(default)]
    pub request_pacing: RequestPacingConfig,
    /// Independent request caps keyed by exact model ID.
    #[serde(default)]
    pub model_rate_limits: HashMap<String, ModelRateLimitConfig>,
    /// Default provider-wide request limits when per-model overrides are absent.
    #[serde(default)]
    pub provider_limits: ProviderLimits,
    /// Optional daily token quota (e.g., 650,000 free tokens/day for rout.my).
    #[serde(default)]
    pub daily_token_quota: Option<u64>,
    /// Whether `daily_token_quota` resets at 00:00 UTC.
    #[serde(default)]
    pub quota_reset_utc_midnight: bool,
    /// Daily reset schedule for all provider limits and token quotas.
    #[serde(default)]
    pub limit_reset_schedule: LimitResetSchedule,
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
            request_pacing: RequestPacingConfig::for_provider(provider),
            model_rate_limits: HashMap::new(),
            provider_limits: ProviderLimits::default(),
            daily_token_quota: None,
            quota_reset_utc_midnight: false,
            limit_reset_schedule: LimitResetSchedule::default(),
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
        if profile_id == ProviderProfileId::Rout && self.literouter.model.trim().is_empty() {
            self.literouter.model = crate::model_ranker::ROUT_DEFAULT_FREE_MODEL.to_owned();
        }
        self
    }

    /// Configures a daily token quota and reset schedule.
    pub fn with_daily_token_quota_schedule(
        mut self,
        daily_token_quota: Option<u64>,
        schedule: LimitResetSchedule,
    ) -> Self {
        self.daily_token_quota = daily_token_quota.filter(|quota| *quota > 0);
        self.quota_reset_utc_midnight = schedule.enabled;
        self.limit_reset_schedule = schedule;
        self
    }

    /// Configures a daily token quota and whether it resets at 00:00 UTC.
    pub fn with_daily_token_quota(
        self,
        daily_token_quota: Option<u64>,
        quota_reset_utc_midnight: bool,
    ) -> Self {
        self.with_daily_token_quota_schedule(
            daily_token_quota,
            LimitResetSchedule {
                enabled: quota_reset_utc_midnight,
                time_minutes: 0,
                utc_offset_hours: 0,
            },
        )
    }

    /// Replaces the route-wide limits with a wildcard model entry and reset schedule.
    pub fn with_route_wide_limits_with_schedule(
        mut self,
        limits: ProviderLimits,
        daily_token_quota: Option<u64>,
        schedule: LimitResetSchedule,
    ) -> Self {
        // A zero quota is "no quota", matching the rest of the settings paths.
        let daily_token_quota = daily_token_quota.filter(|quota| *quota > 0);
        self.model_rate_limits.insert(
            "*".to_owned(),
            ModelRateLimitConfig {
                enabled: limits.rpm > 0
                    || limits.rph > 0
                    || limits.rp4h > 0
                    || limits.rpd > 0
                    || daily_token_quota.is_some(),
                rpm: limits.rpm,
                rph: limits.rph,
                rp4h: limits.rp4h,
                rpd: limits.rpd,
                daily_token_quota,
                quota_reset_utc_midnight: schedule.enabled,
                reset_schedule_enabled: schedule.enabled,
                reset_time_minutes: schedule.time_minutes,
                reset_utc_offset_hours: schedule.utc_offset_hours,
                pricing: None,
                ..ModelRateLimitConfig::default()
            },
        );
        self
    }

    /// Replaces the route-wide limits with a wildcard model entry.
    pub fn with_route_wide_limits(
        self,
        limits: ProviderLimits,
        daily_token_quota: Option<u64>,
        quota_reset_utc_midnight: bool,
    ) -> Self {
        self.with_route_wide_limits_with_schedule(
            limits,
            daily_token_quota,
            LimitResetSchedule {
                enabled: quota_reset_utc_midnight,
                time_minutes: 0,
                utc_offset_hours: 0,
            },
        )
    }

    /// Returns the effective daily reset schedule for this route.
    pub fn effective_reset_schedule(&self) -> LimitResetSchedule {
        if let Some(wildcard) = self
            .model_rate_limits
            .get("*")
            .filter(|limits| limits.enabled)
        {
            return wildcard.reset_schedule();
        }
        if self.limit_reset_schedule.enabled {
            return self.limit_reset_schedule;
        }
        LimitResetSchedule {
            enabled: self.quota_reset_utc_midnight,
            time_minutes: 0,
            utc_offset_hours: 0,
        }
    }

    /// Associates a stable opaque scope handle with the configured provider credential.
    pub fn with_credential_binding(mut self, binding: Option<String>) -> Self {
        self.provider_credential_binding = binding;
        self
    }

    /// Replaces the request pacing policy for this route.
    pub fn with_request_pacing(mut self, request_pacing: RequestPacingConfig) -> Self {
        self.request_pacing = request_pacing.normalized();
        self
    }

    /// Replaces per-model request limits for this provider route.
    pub fn with_model_rate_limits(
        mut self,
        model_rate_limits: HashMap<String, ModelRateLimitConfig>,
    ) -> Self {
        self.model_rate_limits = model_rate_limits;
        self
    }

    /// Replaces default provider-wide request limits for this route.
    pub fn with_provider_limits(mut self, provider_limits: ProviderLimits) -> Self {
        self.provider_limits = provider_limits;
        self
    }

    /// Returns effective request limits for `model`.
    ///
    /// Precedence:
    /// 1. Enabled per-model `model_rate_limits` entry for `model`.
    /// 2. Route-wide `"*"` entry in `model_rate_limits`.
    /// 3. Route-level `provider_limits` fallback.
    pub fn effective_limits(&self, model: &str) -> ProviderLimits {
        if let Some(limits) = self
            .model_rate_limits
            .get(model)
            .filter(|limits| limits.enabled)
        {
            return ProviderLimits {
                rpm: limits.rpm,
                rph: limits.rph,
                rp4h: limits.rp4h,
                rpd: limits.rpd,
            };
        }

        if let Some(limits) = self
            .model_rate_limits
            .get("*")
            .filter(|limits| limits.enabled)
        {
            return ProviderLimits {
                rpm: limits.rpm,
                rph: limits.rph,
                rp4h: limits.rp4h,
                rpd: limits.rpd,
            };
        }

        self.provider_limits
    }

    /// Returns the configured token quota for `model`, with an enabled wildcard fallback.
    pub fn effective_daily_token_quota(&self, model: &str) -> Option<(u64, bool)> {
        self.model_rate_limits
            .get(model)
            .filter(|limits| limits.enabled && limits.daily_token_quota.is_some())
            .or_else(|| {
                self.model_rate_limits
                    .get("*")
                    .filter(|limits| limits.enabled && limits.daily_token_quota.is_some())
            })
            .map(|limits| {
                (
                    limits.daily_token_quota.unwrap_or_default(),
                    limits.quota_reset_utc_midnight,
                )
            })
            .or_else(|| {
                self.daily_token_quota
                    .map(|quota| (quota, self.quota_reset_utc_midnight))
            })
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
            request_pacing: RequestPacingConfig::disabled(),
            model_rate_limits: HashMap::new(),
            provider_limits: ProviderLimits::default(),
            daily_token_quota: None,
            quota_reset_utc_midnight: false,
            limit_reset_schedule: LimitResetSchedule::default(),
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
        Self::from_lookup(&|name| env::var(name).ok())
    }

    /// Loads gateway configuration from an explicit key-value map using the
    /// same schema as [`Self::from_env`].
    pub fn from_env_map(vars: &HashMap<String, String>) -> Result<Self, ProviderError> {
        Self::from_lookup(&|name| vars.get(name).cloned())
    }

    fn from_lookup(lookup: &dyn Fn(&str) -> Option<String>) -> Result<Self, ProviderError> {
        if let Some(raw_routes) = lookup("MODEL_ROUTES_JSON") {
            return parse_routes_json_with_lookup(&raw_routes, true, lookup);
        }

        let default_route = lookup("MODEL_DEFAULT_ROUTE").unwrap_or_else(|| "default".to_string());
        let provider = lookup("MODEL_PROVIDER")
            .and_then(|value| ProviderKind::parse(&value))
            .unwrap_or(ProviderKind::LiteRouter);
        let literouter = LiteRouterConfig::from_lookup(lookup);
        let mut route = match provider {
            ProviderKind::LiteRouter => ModelRouteConfig {
                provider,
                literouter,
                provider_profile_id: None,
                provider_account_id: None,
                provider_credential_binding: None,
                supports_thinking: true,
                request_pacing: RequestPacingConfig::for_provider(provider),
                model_rate_limits: HashMap::new(),
                provider_limits: ProviderLimits::default(),
                daily_token_quota: None,
                quota_reset_utc_midnight: false,
                limit_reset_schedule: LimitResetSchedule::default(),
            },
            ProviderKind::OpenAICompatible => {
                let openai = LiteRouterConfig::openai_compatible_from_lookup(lookup);
                ModelRouteConfig::openai_compatible(openai.api_key, openai.base_url, openai.model)
            }
            ProviderKind::OpenAIResponses => {
                let openai = LiteRouterConfig::openai_responses_from_lookup(lookup);
                ModelRouteConfig::openai_responses(openai.api_key, openai.base_url, openai.model)
            }
            ProviderKind::Ollama => ModelRouteConfig::ollama(
                lookup("OLLAMA_BASE_URL").unwrap_or_else(|| OLLAMA_DEFAULT_BASE_URL.to_string()),
                lookup("OLLAMA_MODEL").unwrap_or_default(),
            ),
            ProviderKind::Mock => {
                return Err(ProviderError::Config(
                    "mock provider is available only to tests".into(),
                ));
            }
            ProviderKind::Local => ModelRouteConfig::local(
                lookup("LOCAL_PROVIDER_SESSION").unwrap_or_default(),
                lookup("LOCAL_PROVIDER_BASE_URL")
                    .unwrap_or_else(|| LOCAL_DEFAULT_BASE_URL.to_string()),
                lookup("LOCAL_PROVIDER_MODEL").unwrap_or_else(|| "local-slm".to_string()),
            ),
        };
        route = route
            .with_credential_binding(lookup("MODEL_PROVIDER_CREDENTIAL_BINDING"))
            .with_request_pacing(RequestPacingConfig::from_lookup(provider, lookup))
            .with_model_rate_limits(model_rate_limits_from_lookup(lookup)?);
        if let Some((quota, reset)) = route
            .model_rate_limits
            .get("*")
            .filter(|limits| limits.enabled)
            .map(|limits| {
                (
                    limits.daily_token_quota.filter(|quota| *quota > 0),
                    limits.quota_reset_utc_midnight,
                )
            })
        {
            route = route.with_daily_token_quota(quota, reset);
        }
        if provider == ProviderKind::OpenAICompatible {
            let profile_id =
                match lookup("MODEL_PROVIDER_PROFILE_ID") {
                    Some(value) => Some(ProviderProfileId::parse(&value).ok_or_else(|| {
                        ProviderError::Config("provider profile is invalid".into())
                    })?),
                    None => None,
                };
            let account_id = lookup("MODEL_PROVIDER_ACCOUNT_ID");
            if profile_id.is_some() || account_id.is_some() {
                route = route.with_provider_profile(profile_id.unwrap_or_default(), account_id);
            }
        }
        route.validate_provider_profile()?;

        let mut routes = HashMap::from([(default_route.clone(), route)]);
        let rout_key = lookup("ROUT_API_KEY")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                lookup("ROUTMY_API_KEY")
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            });
        if let Some(rout_key) = rout_key {
            if !routes.contains_key("rout") {
                let rout_base = lookup("ROUT_BASE_URL")
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .or_else(|| {
                        lookup("ROUTMY_BASE_URL")
                            .map(|value| value.trim().to_string())
                            .filter(|value| !value.is_empty())
                    })
                    .unwrap_or_else(|| "https://api.rout.my/v1".to_string());
                let rout_model = lookup("ROUT_MODEL")
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .or_else(|| {
                        lookup("ROUTMY_MODEL")
                            .map(|value| value.trim().to_string())
                            .filter(|value| !value.is_empty())
                    })
                    .unwrap_or_default();
                let rout_route =
                    ModelRouteConfig::openai_compatible(rout_key, rout_base, rout_model)
                        .with_provider_profile(ProviderProfileId::Rout, None);
                if rout_route.validate_provider_profile().is_ok() {
                    routes.insert("rout".to_string(), rout_route);
                }
            }
        }

        if let Some(raw_additional) = lookup("MODEL_ADDITIONAL_ROUTES_JSON") {
            if !raw_additional.trim().is_empty() {
                // Additional routes intentionally do not contain the primary
                // route selected by MODEL_DEFAULT_ROUTE. Parse and validate
                // their entries, but do not require them to define that route.
                let additional = parse_routes_json_with_lookup(&raw_additional, false, lookup)?;
                for (name, additional_route) in additional.routes {
                    if name != default_route && name != "hive" && name != "hive_fallback" {
                        routes.entry(name).or_insert(additional_route);
                    }
                }
            }
        }

        let hive_keys_map = parse_hive_provider_keys_from_lookup(lookup, "HIVE_API_KEYS_JSON");
        let hive_profile_raw = lookup("HIVE_PROVIDER_PROFILE")
            .or_else(|| lookup("hive_provider_profile"))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let hive_key = if let Some(ref keys) = hive_keys_map {
            let effective_profile = hive_profile_raw
                .as_deref()
                .map(|p| p.trim().to_ascii_lowercase())
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| "rout".to_string());
            keys.get(&effective_profile).cloned()
        } else {
            lookup("HIVE_API_KEY")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        if let Some(hive_key) = hive_key {
            if !routes.contains_key("hive") {
                let hive_base = lookup("HIVE_BASE_URL")
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                let hive_model = lookup("HIVE_MODEL")
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_default();
                let hive_route = Self::build_hive_route(
                    hive_key,
                    hive_profile_raw.as_deref(),
                    hive_base,
                    hive_model,
                );
                if hive_route.validate_provider_profile().is_ok() {
                    routes.insert("hive".to_string(), hive_route);
                }
            }
        }

        let hive_fallback_keys_map =
            parse_hive_provider_keys_from_lookup(lookup, "HIVE_FALLBACK_API_KEYS_JSON");
        let fallback_profile_raw = lookup("HIVE_FALLBACK_PROVIDER_PROFILE")
            .or_else(|| lookup("hive_fallback_provider_profile"))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let hive_fallback_key = if let Some(ref keys) = hive_fallback_keys_map {
            fallback_profile_raw
                .as_deref()
                .map(|p| p.trim().to_ascii_lowercase())
                .filter(|p| !p.is_empty())
                .and_then(|profile| keys.get(&profile).cloned())
        } else {
            lookup("HIVE_FALLBACK_API_KEY")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        if let Some(hive_fallback_key) = hive_fallback_key {
            if !routes.contains_key("hive_fallback") {
                let fallback_base = lookup("HIVE_FALLBACK_BASE_URL")
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                let fallback_model = lookup("HIVE_FALLBACK_MODEL")
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_default();
                let fallback_route = Self::build_hive_route(
                    hive_fallback_key,
                    fallback_profile_raw.as_deref(),
                    fallback_base,
                    fallback_model,
                );
                if fallback_route.validate_provider_profile().is_ok() {
                    routes.insert("hive_fallback".to_string(), fallback_route);
                }
            }
        }

        Ok(Self {
            default_route,
            routes,
        })
    }

    /// Builds an isolated Hive route config for the given profile and model.
    pub fn build_hive_route(
        hive_key: String,
        hive_profile: Option<&str>,
        hive_base: Option<String>,
        hive_model: String,
    ) -> ModelRouteConfig {
        match hive_profile
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some("literouter") => ModelRouteConfig::literouter(
                hive_key,
                hive_base.unwrap_or_else(|| LITEROUTER_DEFAULT_BASE_URL.to_string()),
                if hive_model.is_empty() {
                    crate::model_ranker::get_ranked_models()
                        .into_iter()
                        .next()
                        .unwrap_or_default()
                } else {
                    hive_model
                },
            ),
            Some("openai_responses") => ModelRouteConfig::openai_responses(
                hive_key,
                hive_base.unwrap_or_else(|| OPENAI_DEFAULT_BASE_URL.to_string()),
                if hive_model.is_empty() {
                    OPENAI_CODEX_DEFAULT_MODEL.to_string()
                } else {
                    hive_model
                },
            ),
            Some("ollama") => ModelRouteConfig::ollama(
                hive_base.unwrap_or_else(|| OLLAMA_DEFAULT_BASE_URL.to_string()),
                hive_model,
            ),
            other => {
                let profile_id = other
                    .and_then(ProviderProfileId::parse)
                    .unwrap_or(ProviderProfileId::Rout);
                let base_url = hive_base
                    .or_else(|| profile_id.default_base_url().map(str::to_owned))
                    .unwrap_or_else(|| OPENAI_DEFAULT_BASE_URL.to_string());
                ModelRouteConfig::openai_compatible(hive_key, base_url, hive_model)
                    .with_provider_profile(profile_id, None)
            }
        }
    }

    fn apply_named_hive_route_override(
        &mut self,
        route_key: &str,
        profile: Option<&str>,
        model: Option<&str>,
    ) {
        let normalized_profile = profile.map(str::trim).filter(|value| !value.is_empty());
        let explicit_model = model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let keys_env_var = if route_key == "hive_fallback" {
            "HIVE_FALLBACK_API_KEYS_JSON"
        } else {
            "HIVE_API_KEYS_JSON"
        };
        if let Some(keys) =
            parse_hive_provider_keys_from_lookup(&|name| env::var(name).ok(), keys_env_var)
        {
            let Some(target_profile) = (if route_key == "hive_fallback" {
                normalized_profile
            } else {
                Some(normalized_profile.unwrap_or("rout"))
            }) else {
                self.routes.remove(route_key);
                return;
            };
            let existing = self.routes.get(route_key).cloned();
            let profile_changed = existing.as_ref().is_none_or(|route| {
                !(route
                    .provider_profile_id
                    .is_some_and(|id| id.as_str().eq_ignore_ascii_case(target_profile))
                    || route.provider.as_str().eq_ignore_ascii_case(target_profile))
            });
            let normalized_model = explicit_model.unwrap_or_else(|| {
                if !profile_changed {
                    existing
                        .as_ref()
                        .map(|route| route.literouter.model.clone())
                        .unwrap_or_default()
                } else {
                    String::new()
                }
            });
            if target_profile.eq_ignore_ascii_case("ollama") {
                if let Some(ref route) = existing {
                    if !profile_changed && normalized_model == route.literouter.model {
                        return;
                    }
                }
                let ollama_route =
                    Self::build_hive_route(String::new(), Some("ollama"), None, normalized_model);
                if ollama_route.validate_provider_profile().is_ok() {
                    self.routes.insert(route_key.to_string(), ollama_route);
                }
                return;
            }
            let Some(hive_key) = keys.get(&target_profile.to_ascii_lowercase()).cloned() else {
                self.routes.remove(route_key);
                return;
            };
            if let Some(ref route) = existing {
                if !profile_changed
                    && route.literouter.api_key == hive_key
                    && normalized_model == route.literouter.model
                {
                    return;
                }
            }
            let rebuilt =
                Self::build_hive_route(hive_key, Some(target_profile), None, normalized_model);
            if rebuilt.validate_provider_profile().is_ok() {
                self.routes.insert(route_key.to_string(), rebuilt);
            }
            return;
        }
        let Some(existing) = self.routes.get(route_key).cloned() else {
            if normalized_profile.is_some_and(|p| p.eq_ignore_ascii_case("ollama")) {
                let ollama_route = Self::build_hive_route(
                    String::new(),
                    Some("ollama"),
                    None,
                    explicit_model.unwrap_or_default(),
                );
                if ollama_route.validate_provider_profile().is_ok() {
                    self.routes.insert(route_key.to_string(), ollama_route);
                }
            }
            return;
        };
        let Some(requested_profile) = normalized_profile else {
            if route_key == "hive_fallback" {
                self.routes.remove(route_key);
                return;
            }
            if let Some(new_model) = explicit_model {
                if new_model != existing.literouter.model {
                    if let Some(route) = self.routes.get_mut(route_key) {
                        route.literouter.model = new_model;
                    }
                }
            }
            return;
        };
        let profile_changed = !(existing
            .provider_profile_id
            .is_some_and(|id| id.as_str().eq_ignore_ascii_case(requested_profile))
            || existing
                .provider
                .as_str()
                .eq_ignore_ascii_case(requested_profile));
        let normalized_model = explicit_model.unwrap_or_else(|| {
            if profile_changed {
                String::new()
            } else {
                existing.literouter.model.clone()
            }
        });
        if !profile_changed && normalized_model == existing.literouter.model {
            return;
        }
        let hive_key = existing.literouter.api_key;
        let rebuilt =
            Self::build_hive_route(hive_key, Some(requested_profile), None, normalized_model);
        if rebuilt.validate_provider_profile().is_ok() {
            self.routes.insert(route_key.to_string(), rebuilt);
        }
    }

    /// Rebuilds the isolated `"hive"` route (when `HIVE_API_KEY` created one,
    /// or when Hive explicitly targets local `ollama`) using the durable
    /// `hive_provider_profile` and `hive_model` settings.
    pub fn apply_hive_route_overrides(&mut self, profile: Option<&str>, model: Option<&str>) {
        self.apply_named_hive_route_override("hive", profile, model);
    }

    /// Applies durable request, token caps, and reset schedule to the isolated Hive routes.
    pub fn apply_hive_route_limits_with_schedule(
        &mut self,
        route_key: &str,
        limits: ProviderLimits,
        token_quota: u64,
        schedule: LimitResetSchedule,
    ) {
        if let Some(route) = self.routes.get_mut(route_key) {
            let token_quota = (token_quota > 0).then_some(token_quota);
            *route = route
                .clone()
                .with_route_wide_limits_with_schedule(limits, token_quota, schedule)
                .with_daily_token_quota_schedule(token_quota, schedule);
        }
    }

    /// Applies durable request and token caps to the isolated Hive routes.
    pub fn apply_hive_route_limits(
        &mut self,
        route_key: &str,
        limits: ProviderLimits,
        token_quota: u64,
        quota_reset_utc_midnight: bool,
    ) {
        self.apply_hive_route_limits_with_schedule(
            route_key,
            limits,
            token_quota,
            LimitResetSchedule {
                enabled: quota_reset_utc_midnight,
                time_minutes: 0,
                utc_offset_hours: 0,
            },
        );
    }

    /// Rebuilds the isolated `"hive_fallback"` route (when `HIVE_FALLBACK_API_KEY`
    /// created one, or when fallback explicitly targets local `ollama`) using the
    /// durable `hive_fallback_provider_profile` and `hive_fallback_model` settings.
    pub fn apply_hive_fallback_route_overrides(
        &mut self,
        profile: Option<&str>,
        model: Option<&str>,
    ) {
        self.apply_named_hive_route_override("hive_fallback", profile, model);
    }
}

fn parse_hive_provider_keys_from_lookup(
    lookup: &dyn Fn(&str) -> Option<String>,
    var_name: &str,
) -> Option<HashMap<String, String>> {
    let raw = lookup(var_name)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed: HashMap<String, String> = serde_json::from_str(trimmed).ok()?;
    let normalized: HashMap<String, String> = parsed
        .into_iter()
        .filter_map(|(provider, key)| {
            let provider = provider.trim().to_ascii_lowercase();
            let key = key.trim().to_string();
            (!provider.is_empty() && !key.is_empty()).then_some((provider, key))
        })
        .collect();
    Some(normalized)
}

/// Parses `MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON` from the process environment.
pub fn model_rate_limits_from_env() -> Result<HashMap<String, ModelRateLimitConfig>, ProviderError>
{
    model_rate_limits_from_lookup(&|name| env::var(name).ok())
}

/// Parses `MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON` from an explicit key-value map.
pub fn model_rate_limits_from_env_map(
    vars: &HashMap<String, String>,
) -> Result<HashMap<String, ModelRateLimitConfig>, ProviderError> {
    model_rate_limits_from_lookup(&|name| vars.get(name).cloned())
}

fn model_rate_limits_from_lookup(
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<HashMap<String, ModelRateLimitConfig>, ProviderError> {
    let Some(raw) = lookup("MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON") else {
        return Ok(HashMap::new());
    };
    if raw.len() > 64 * 1024 {
        return Err(ProviderError::Config(
            "provider model rate limits exceed the environment bound".into(),
        ));
    }
    let limits: HashMap<String, ModelRateLimitConfig> = serde_json::from_str(&raw)
        .map_err(|_| ProviderError::Config("provider model rate limits are invalid".into()))?;
    validate_model_rate_limits(&limits)?;
    Ok(limits)
}

fn valid_pricing_rate(value: f64) -> bool {
    value.is_finite() && (0.0..=10_000.0).contains(&value)
}

fn validate_model_rate_limits(
    limits: &HashMap<String, ModelRateLimitConfig>,
) -> Result<(), ProviderError> {
    if limits.len() > 64
        || limits.iter().any(|(model, limit)| {
            model.is_empty()
                || model.chars().count() > 128
                || model.contains('\0')
                || model.chars().any(char::is_whitespace)
                || limit.reset_time_minutes >= 1_440
                || !(-12..=14).contains(&limit.reset_utc_offset_hours)
                || limit
                    .daily_token_quota
                    .is_some_and(|quota| quota > MAX_DAILY_TOKEN_QUOTA)
                || limit.pricing.is_some_and(|pricing| {
                    !valid_pricing_rate(pricing.input_usd_per_1m)
                        || !valid_pricing_rate(pricing.cached_input_usd_per_1m)
                        || !valid_pricing_rate(pricing.output_usd_per_1m)
                })
        })
    {
        return Err(ProviderError::Config(
            "provider model rate limits are out of bounds".into(),
        ));
    }
    Ok(())
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
    #[serde(default)]
    request_pacing: Option<RequestPacingConfig>,
    #[serde(default)]
    model_rate_limits: HashMap<String, ModelRateLimitConfig>,
}

#[cfg(test)]
fn parse_routes_from_json(raw_routes: &str) -> Result<ModelGatewayConfig, ProviderError> {
    parse_routes_json_with_lookup(raw_routes, true, &|name| env::var(name).ok())
}

#[cfg(test)]
fn parse_additional_routes_from_json(
    raw_routes: &str,
) -> Result<ModelGatewayConfig, ProviderError> {
    parse_routes_json_with_lookup(raw_routes, false, &|name| env::var(name).ok())
}

fn parse_routes_json_with_lookup(
    raw_routes: &str,
    require_default_route: bool,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<ModelGatewayConfig, ProviderError> {
    let routes: HashMap<String, EnvRouteConfig> = serde_json::from_str(raw_routes)
        .map_err(|error| ProviderError::Config(error.to_string()))?;
    let default_route = lookup("MODEL_DEFAULT_ROUTE").unwrap_or_else(|| "default".to_string());
    let mut parsed_routes = HashMap::new();

    for (name, route) in routes {
        let provider = route
            .provider
            .as_deref()
            .and_then(ProviderKind::parse)
            .unwrap_or(ProviderKind::LiteRouter);
        let request_pacing = route
            .request_pacing
            .map(RequestPacingConfig::normalized)
            .unwrap_or_else(|| RequestPacingConfig::for_provider(provider));
        let model_rate_limits = route.model_rate_limits;
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
                ));
            }
        };
        if route.provider_profile_id.is_some() || route.provider_account_id.is_some() {
            let profile_id = route.provider_profile_id.ok_or_else(|| {
                ProviderError::Config("provider profile identity is not configured".into())
            })?;
            route_config =
                route_config.with_provider_profile(profile_id, route.provider_account_id);
        }
        let route_wide_quota = model_rate_limits
            .get("*")
            .filter(|limit| limit.enabled)
            .and_then(|limit| limit.daily_token_quota)
            .filter(|quota| *quota > 0);
        let route_wide_reset = model_rate_limits
            .get("*")
            .filter(|limit| limit.enabled)
            .map(ModelRateLimitConfig::reset_schedule)
            .unwrap_or_default();
        route_config = route_config
            .with_credential_binding(route.provider_credential_binding)
            .with_request_pacing(request_pacing)
            .with_model_rate_limits(model_rate_limits)
            .with_daily_token_quota_schedule(route_wide_quota, route_wide_reset);
        validate_model_rate_limits(&route_config.model_rate_limits)?;
        route_config.validate_provider_profile()?;
        parsed_routes.insert(name, route_config);
    }

    if require_default_route && !parsed_routes.contains_key(&default_route) {
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
    fn provider_profile_serde_uses_stable_identifiers() {
        let profiles = [
            ProviderProfileId::Custom,
            ProviderProfileId::OpenAi,
            ProviderProfileId::OpenRouter,
            ProviderProfileId::Groq,
            ProviderProfileId::Gemini,
            ProviderProfileId::Mistral,
            ProviderProfileId::CloudflareWorkersAi,
            ProviderProfileId::NvidiaNim,
            ProviderProfileId::Cerebras,
            ProviderProfileId::HuggingFace,
            ProviderProfileId::DeepSeek,
            ProviderProfileId::XAi,
            ProviderProfileId::TogetherAi,
            ProviderProfileId::FireworksAi,
            ProviderProfileId::Moonshot,
            ProviderProfileId::ZAi,
            ProviderProfileId::SambaNova,
            ProviderProfileId::Nebius,
            ProviderProfileId::Rout,
            ProviderProfileId::AnyModel,
        ];

        for profile in profiles {
            let encoded = serde_json::to_string(&profile).expect("profile serializes");
            assert_eq!(encoded, format!("\"{}\"", profile.as_str()));
            let decoded: ProviderProfileId =
                serde_json::from_str(&encoded).expect("profile deserializes");
            assert_eq!(decoded, profile);
        }
        assert_eq!(
            serde_json::from_str::<ProviderProfileId>("\"open_ai\"").expect("legacy alias"),
            ProviderProfileId::OpenAi
        );
        assert_eq!(
            serde_json::from_str::<ProviderProfileId>("\"open_router\"").expect("legacy alias"),
            ProviderProfileId::OpenRouter
        );
    }

    #[test]
    fn added_vendor_profiles_round_trip_with_trusted_endpoints() {
        let profiles = [
            (
                ProviderProfileId::DeepSeek,
                "deepseek",
                "https://api.deepseek.com/v1",
            ),
            (ProviderProfileId::XAi, "xai", "https://api.x.ai/v1"),
            (
                ProviderProfileId::TogetherAi,
                "together_ai",
                "https://api.together.ai/v1",
            ),
            (
                ProviderProfileId::FireworksAi,
                "fireworks_ai",
                "https://api.fireworks.ai/inference/v1",
            ),
            (
                ProviderProfileId::Moonshot,
                "moonshot",
                "https://api.moonshot.ai/v1",
            ),
            (
                ProviderProfileId::ZAi,
                "zai",
                "https://api.z.ai/api/paas/v4",
            ),
            (
                ProviderProfileId::SambaNova,
                "sambanova",
                "https://api.sambanova.ai/v1",
            ),
            (
                ProviderProfileId::Nebius,
                "nebius",
                "https://api.tokenfactory.nebius.com/v1",
            ),
            (ProviderProfileId::Rout, "rout", "https://api.rout.my/v1"),
            (
                ProviderProfileId::AnyModel,
                "anymodel",
                "https://anymodel.org/v1",
            ),
        ];

        for (profile, id, endpoint) in profiles {
            assert_eq!(profile.as_str(), id);
            assert_eq!(ProviderProfileId::parse(id), Some(profile));
            assert_eq!(profile.default_base_url(), Some(endpoint));
            let route = ModelRouteConfig::openai_compatible("test-token", endpoint, "model")
                .with_provider_profile(profile, None);
            assert!(route.validate_provider_profile().is_ok(), "{id}");
        }
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
    fn request_pacing_defaults_are_disabled_for_all_providers() {
        let literouter = RequestPacingConfig::for_provider(ProviderKind::LiteRouter);
        assert_eq!(literouter, RequestPacingConfig::disabled());
        assert_eq!(literouter.interval_for_model("deepseek:free"), None);
        assert_eq!(literouter.interval_for_model("deepseek"), None);

        for provider in [
            ProviderKind::OpenAICompatible,
            ProviderKind::OpenAIResponses,
            ProviderKind::Ollama,
            ProviderKind::Local,
        ] {
            let pacing = RequestPacingConfig::for_provider(provider);
            assert!(!pacing.enabled, "{provider:?}");
            assert_eq!(pacing.interval_for_model("any-model"), None);
        }
    }

    #[test]
    fn request_pacing_clamps_and_honors_enabled_flag() {
        let pacing = RequestPacingConfig {
            enabled: true,
            interval_ms: u64::MAX,
            free_interval_ms: 12,
        }
        .normalized();
        assert_eq!(pacing.interval_ms, RequestPacingConfig::MAX_INTERVAL_MS);
        assert_eq!(pacing.free_interval_ms, 12);
        assert_eq!(
            pacing.interval_for_model("model:free"),
            Some(Duration::from_millis(12))
        );
        assert_eq!(
            RequestPacingConfig {
                enabled: false,
                interval_ms: 5_000,
                free_interval_ms: 5_000,
            }
            .interval_for_model("model"),
            None
        );
    }

    #[test]
    fn routes_json_reads_request_pacing() {
        let raw = serde_json::json!({
            "default": {
                "provider": "openai_compatible",
                "api_key": "test-token",
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-4o-mini",
                "request_pacing": {
                    "enabled": true,
                    "interval_ms": 1500,
                    "free_interval_ms": 2500
                }
                ,"model_rate_limits": {
                    "gpt-4o-mini": {"enabled": true, "rpm": 10, "rph": 200, "rpd": 1000}
                }
            },
            "literouter": {
                "provider": "literouter",
                "api_key": "lr-test",
                "model": "deepseek:free"
            }
        });

        let parsed = parse_routes_from_json(&raw.to_string()).expect("routes parse");
        let openai = parsed.routes.get("default").expect("default route");
        assert!(openai.request_pacing.enabled);
        assert_eq!(openai.request_pacing.interval_ms, 1_500);
        assert_eq!(openai.request_pacing.free_interval_ms, 2_500);
        let limits = openai
            .model_rate_limits
            .get("gpt-4o-mini")
            .expect("model limits");
        assert!(limits.enabled);
        assert_eq!(limits.rpm, 10);
        assert_eq!(limits.rph, 200);
        assert_eq!(limits.rpd, 1000);
        let literouter = parsed.routes.get("literouter").expect("literouter route");
        assert!(!literouter.request_pacing.enabled);
        assert_eq!(literouter.request_pacing.interval_ms, 0);
        assert!(literouter.model_rate_limits.is_empty());
    }

    #[test]
    fn additional_routes_json_does_not_require_a_default_route() {
        let raw = serde_json::json!({
            "literouter": {
                "provider": "literouter",
                "api_key": "lr-test",
                "model": "deepseek:free"
            },
            "openai_compatible:deepseek": {
                "provider": "openai_compatible",
                "api_key": "deepseek-test",
                "base_url": "https://api.deepseek.com/v1",
                "model": "deepseek-chat",
                "provider_profile_id": "deepseek"
            }
        });

        let parsed = parse_additional_routes_from_json(&raw.to_string())
            .expect("additional routes do not need a primary route");

        assert_eq!(parsed.routes.len(), 2);
        assert!(parsed.routes.contains_key("literouter"));
        assert_eq!(
            parsed.routes["openai_compatible:deepseek"].provider_profile_id,
            Some(ProviderProfileId::DeepSeek)
        );
        assert!(parse_routes_from_json(&raw.to_string()).is_err());
    }

    #[test]
    fn rejects_unbounded_model_limit_maps() {
        let limits = (0..65)
            .map(|index| (format!("model-{index}"), ModelRateLimitConfig::default()))
            .collect();
        assert!(validate_model_rate_limits(&limits).is_err());
        assert!(validate_model_rate_limits(&HashMap::from([(
            "bad model".to_owned(),
            ModelRateLimitConfig::default(),
        )]))
        .is_err());
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

    #[test]
    fn provider_limits_require_explicit_configuration() {
        let route = ModelRouteConfig::literouter("lr-key", LITEROUTER_DEFAULT_BASE_URL, "");

        assert_eq!(
            route.effective_limits("some-model:free"),
            ProviderLimits::default()
        );

        let overridden = route.clone().with_model_rate_limits(HashMap::from([(
            "*".to_owned(),
            ModelRateLimitConfig {
                enabled: true,
                rpm: 12,
                rph: 120,
                rp4h: 360,
                rpd: 1_000,
                daily_token_quota: None,
                quota_reset_utc_midnight: false,
                reset_schedule_enabled: false,
                reset_time_minutes: 0,
                reset_utc_offset_hours: 0,
                pricing: None,
                ..ModelRateLimitConfig::default()
            },
        )]));
        assert_eq!(
            overridden.effective_limits("some-model:free"),
            ProviderLimits {
                rpm: 12,
                rph: 120,
                rp4h: 360,
                rpd: 1_000,
            }
        );

        let fallback_limits = ProviderLimits {
            rpm: 20,
            rph: 300,
            rp4h: 900,
            rpd: 5_000,
        };
        let openai = ModelRouteConfig::openai_compatible(
            "oa-key",
            OPENAI_DEFAULT_BASE_URL,
            OPENAI_DEFAULT_MODEL,
        )
        .with_provider_limits(fallback_limits);
        assert_eq!(
            openai.effective_limits(OPENAI_DEFAULT_MODEL),
            fallback_limits
        );
    }

    #[test]
    fn daily_token_quota_uses_model_then_wildcard_then_route_defaults() {
        let mut route = ModelRouteConfig::literouter("key", LITEROUTER_DEFAULT_BASE_URL, "model");
        assert_eq!(route.effective_daily_token_quota("model"), None);

        route = route.with_daily_token_quota(Some(1_000), false);
        assert_eq!(
            route.effective_daily_token_quota("model"),
            Some((1_000, false))
        );

        let limits = |enabled, quota, reset| ModelRateLimitConfig {
            enabled,
            rpm: 0,
            rph: 0,
            rp4h: 0,
            rpd: 0,
            daily_token_quota: Some(quota),
            quota_reset_utc_midnight: reset,
            reset_schedule_enabled: reset,
            reset_time_minutes: 0,
            reset_utc_offset_hours: 0,
            pricing: None,
            ..ModelRateLimitConfig::default()
        };
        route
            .model_rate_limits
            .insert("*".into(), limits(true, 2_000, true));
        assert_eq!(
            route.effective_daily_token_quota("model"),
            Some((2_000, true))
        );

        route
            .model_rate_limits
            .insert("model".into(), limits(true, 3_000, false));
        assert_eq!(
            route.effective_daily_token_quota("model"),
            Some((3_000, false))
        );

        route
            .model_rate_limits
            .insert("model".into(), limits(false, 3_000, false));
        assert_eq!(
            route.effective_daily_token_quota("model"),
            Some((2_000, true))
        );

        route
            .model_rate_limits
            .get_mut("*")
            .expect("wildcard")
            .enabled = false;
        assert_eq!(
            route.effective_daily_token_quota("model"),
            Some((1_000, false))
        );
    }

    #[test]
    fn enabled_model_limits_override_wildcard_and_disabled_model_falls_back() {
        let mut route = ModelRouteConfig::literouter("key", LITEROUTER_DEFAULT_BASE_URL, "model");
        let limits = |enabled, rpm| ModelRateLimitConfig {
            enabled,
            rpm,
            rph: 0,
            rp4h: 0,
            rpd: 0,
            daily_token_quota: None,
            quota_reset_utc_midnight: false,
            reset_schedule_enabled: false,
            reset_time_minutes: 0,
            reset_utc_offset_hours: 0,
            pricing: None,
            ..ModelRateLimitConfig::default()
        };
        route.model_rate_limits.insert("*".into(), limits(true, 10));
        route
            .model_rate_limits
            .insert("model".into(), limits(true, 3));
        assert_eq!(route.effective_limits("model").rpm, 3);
        route
            .model_rate_limits
            .insert("model".into(), limits(false, 3));
        assert_eq!(route.effective_limits("model").rpm, 10);
        assert_eq!(route.effective_limits("other").rpm, 10);
    }

    #[test]
    fn rout_profile_has_no_implicit_daily_token_quota() {
        assert_eq!(
            ProviderProfileId::parse("rout"),
            Some(ProviderProfileId::Rout)
        );
        assert_eq!(ProviderProfileId::parse("routmy"), None);
        assert_eq!(
            serde_json::from_str::<ProviderProfileId>("\"routmy\"").ok(),
            Some(ProviderProfileId::Rout)
        );
        assert_eq!(ProviderProfileId::Rout.as_str(), "rout");
        assert_eq!(
            ProviderProfileId::Rout.default_base_url(),
            Some("https://api.rout.my/v1")
        );

        let route = ModelRouteConfig::openai_compatible(
            "rout-key",
            "https://api.rout.my/v1",
            "deepseek-chat",
        )
        .with_provider_profile(ProviderProfileId::Rout, None);
        assert_eq!(route.daily_token_quota, None);
        assert!(!route.quota_reset_utc_midnight);
        assert!(!route.request_pacing.enabled);
        assert!(route.validate_provider_profile().is_ok());

        let custom_quota = route.with_daily_token_quota(Some(100_000), false);
        assert_eq!(custom_quota.daily_token_quota, Some(100_000));
        assert!(!custom_quota.quota_reset_utc_midnight);
    }

    #[test]
    fn rout_uses_free_default_model() {
        let route = ModelRouteConfig::openai_compatible("rout-key", "https://api.rout.my/v1", "")
            .with_provider_profile(ProviderProfileId::Rout, None);

        assert_eq!(route.literouter.model, "openai/gpt-6-luna");
        assert_eq!(
            route.literouter.model,
            crate::model_ranker::ROUT_DEFAULT_FREE_MODEL
        );
        assert_eq!(
            crate::model_ranker::get_rout_ranked_models(),
            vec![
                "openai/gpt-6-luna",
                "deepseek/deepseek-v4.1-flash",
                "z-ai/glm-5.3-flash",
                "deepseek/deepseek-v4-pro",
                "qwen/qwen3.7-max",
            ]
        );
    }

    #[test]
    fn hive_route_uses_separate_api_key() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev_hive_key = env::var("HIVE_API_KEY").ok();
        let prev_hive_profile = env::var("HIVE_PROVIDER_PROFILE").ok();
        let prev_routes = env::var("MODEL_ROUTES_JSON").ok();
        let prev_provider = env::var("MODEL_PROVIDER").ok();

        env::remove_var("MODEL_ROUTES_JSON");
        env::remove_var("HIVE_PROVIDER_PROFILE");
        env::set_var("MODEL_PROVIDER", "literouter");
        env::set_var("HIVE_API_KEY", "sk-hive-isolated");

        let config = ModelGatewayConfig::from_env().expect("from_env succeeds");
        let hive_route = config.routes.get("hive").expect("hive route must exist");
        assert_eq!(hive_route.literouter.api_key, "sk-hive-isolated");
        assert_eq!(
            hive_route.provider_profile_id,
            Some(ProviderProfileId::Rout)
        );
        assert!(hive_route.configured());

        match prev_hive_key {
            Some(v) => env::set_var("HIVE_API_KEY", v),
            None => env::remove_var("HIVE_API_KEY"),
        }
        match prev_hive_profile {
            Some(v) => env::set_var("HIVE_PROVIDER_PROFILE", v),
            None => env::remove_var("HIVE_PROVIDER_PROFILE"),
        }
        match prev_routes {
            Some(v) => env::set_var("MODEL_ROUTES_JSON", v),
            None => env::remove_var("MODEL_ROUTES_JSON"),
        }
        match prev_provider {
            Some(v) => env::set_var("MODEL_PROVIDER", v),
            None => env::remove_var("MODEL_PROVIDER"),
        }
    }

    static HIVE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn apply_hive_route_overrides_rebuilds_hive_route_for_literouter() {
        let _guard = HIVE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut routes = HashMap::new();
        routes.insert(
            "hive".to_string(),
            ModelRouteConfig::openai_compatible("sk-hive-isolated", "https://api.rout.my/v1", "")
                .with_provider_profile(ProviderProfileId::Rout, None),
        );
        let mut config = ModelGatewayConfig {
            default_route: "hive".to_string(),
            routes,
        };

        config.apply_hive_route_overrides(Some("literouter"), Some("glm-5.3-flash-cheap:free"));
        let hive = config.routes.get("hive").expect("hive route remains");
        assert_eq!(hive.provider, ProviderKind::LiteRouter);
        assert_eq!(hive.provider_profile_id, None);
        assert_eq!(hive.literouter.api_key, "sk-hive-isolated");
        assert_eq!(hive.literouter.model, "glm-5.3-flash-cheap:free");
        assert_eq!(hive.literouter.base_url, LITEROUTER_DEFAULT_BASE_URL);
    }

    #[test]
    fn apply_hive_route_overrides_resets_rout_default_model_when_switching_to_literouter() {
        let _guard = HIVE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut routes = HashMap::new();
        routes.insert(
            "hive".to_string(),
            ModelRouteConfig::openai_compatible("sk-hive-isolated", "https://api.rout.my/v1", "")
                .with_provider_profile(ProviderProfileId::Rout, None),
        );
        let mut config = ModelGatewayConfig {
            default_route: "hive".to_string(),
            routes,
        };

        // Initial Rout route gets `ROUT_DEFAULT_FREE_MODEL` (`openai/gpt-6-luna`).
        assert_eq!(
            config
                .routes
                .get("hive")
                .expect("hive route")
                .literouter
                .model,
            crate::model_ranker::ROUT_DEFAULT_FREE_MODEL
        );

        // Switching Hive to LiteRouter without an explicit model must not carry
        // Rout's default model over into LiteRouter.
        config.apply_hive_route_overrides(Some("literouter"), None);
        let hive = config.routes.get("hive").expect("hive route remains");
        assert_eq!(hive.provider, ProviderKind::LiteRouter);
        assert_eq!(hive.provider_profile_id, None);
        assert_ne!(
            hive.literouter.model,
            crate::model_ranker::ROUT_DEFAULT_FREE_MODEL
        );
        assert_eq!(
            hive.literouter.model,
            crate::model_ranker::get_ranked_models()[0]
        );
    }

    #[test]
    fn apply_hive_fallback_route_overrides_builds_isolated_hive_fallback_route() {
        let mut routes = HashMap::new();
        routes.insert(
            "hive_fallback".to_string(),
            ModelRouteConfig::openai_compatible(
                "sk-hive-fallback-isolated",
                "https://api.rout.my/v1",
                "",
            )
            .with_provider_profile(ProviderProfileId::Rout, None),
        );
        let mut config = ModelGatewayConfig {
            default_route: "default".to_string(),
            routes,
        };

        config.apply_hive_fallback_route_overrides(Some("rout"), Some("openai/gpt-6-luna"));
        let fallback = config
            .routes
            .get("hive_fallback")
            .expect("hive_fallback route remains");
        assert_eq!(fallback.provider, ProviderKind::OpenAICompatible);
        assert_eq!(fallback.provider_profile_id, Some(ProviderProfileId::Rout));
        assert_eq!(fallback.literouter.api_key, "sk-hive-fallback-isolated");
        assert_eq!(fallback.literouter.model, "openai/gpt-6-luna");
    }

    #[test]
    fn apply_hive_route_overrides_uses_per_provider_keys_and_drops_unconfigured_provider() {
        let _guard = HIVE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev_keys = env::var("HIVE_API_KEYS_JSON").ok();

        env::set_var(
            "HIVE_API_KEYS_JSON",
            r#"{"literouter":"sk-hive-literouter","deepseek":"sk-hive-deepseek"}"#,
        );

        let mut config = ModelGatewayConfig {
            default_route: "default".to_string(),
            routes: HashMap::new(),
        };

        config.apply_hive_route_overrides(Some("literouter"), Some("glm-5.3-flash-cheap:free"));
        let hive = config.routes.get("hive").expect("literouter route built");
        assert_eq!(hive.provider, ProviderKind::LiteRouter);
        assert_eq!(hive.literouter.api_key, "sk-hive-literouter");

        // Switching to a provider without a stored key must drop the route instead of reusing LiteRouter's key.
        config.apply_hive_route_overrides(Some("openai_responses"), None);
        assert!(!config.routes.contains_key("hive"));

        // Switching to DeepSeek picks up DeepSeek's own stored key.
        config.apply_hive_route_overrides(Some("deepseek"), Some("deepseek-chat"));
        let hive = config.routes.get("hive").expect("deepseek route built");
        assert_eq!(hive.provider, ProviderKind::OpenAICompatible);
        assert_eq!(hive.provider_profile_id, Some(ProviderProfileId::DeepSeek));
        assert_eq!(hive.literouter.api_key, "sk-hive-deepseek");

        match prev_keys {
            Some(v) => env::set_var("HIVE_API_KEYS_JSON", v),
            None => env::remove_var("HIVE_API_KEYS_JSON"),
        }
    }

    #[test]
    fn from_env_map_parses_provider_and_rate_limits_without_process_env() {
        let mut vars = HashMap::new();
        vars.insert("MODEL_PROVIDER".into(), "openai_compatible".into());
        vars.insert("OPENAI_API_KEY".into(), "sk-test-map".into());
        vars.insert(
            "OPENAI_BASE_URL".into(),
            "https://api.example.com/v1".into(),
        );
        vars.insert("OPENAI_MODEL".into(), "gpt-4o-mini".into());
        vars.insert(
            "MODEL_PROVIDER_MODEL_RATE_LIMITS_JSON".into(),
            r#"{"*":{"enabled":true,"rpm":12,"daily_token_quota":50000,"reset_schedule_enabled":true,"reset_time_minutes":90,"reset_utc_offset_hours":2}}"#.into(),
        );
        let config = ModelGatewayConfig::from_env_map(&vars).expect("valid env map");
        let route = config
            .routes
            .get(&config.default_route)
            .expect("default route");
        assert_eq!(route.provider, ProviderKind::OpenAICompatible);
        assert_eq!(route.literouter.model, "gpt-4o-mini");
        assert_eq!(route.daily_token_quota, Some(50_000));
        let star = route.model_rate_limits.get("*").expect("star limit");
        assert_eq!(star.rpm, 12);
        assert_eq!(star.reset_time_minutes, 90);
        assert_eq!(star.reset_utc_offset_hours, 2);
    }
}

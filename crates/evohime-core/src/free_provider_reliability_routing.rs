//! Bounded provider access/reliability metadata; gateway remains transport owner.
use evohime_model_gateway::providers::ProviderError;
use evohime_model_gateway::{ModelCatalogEntry, ModelRouteConfig, RoutePreflight};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};

pub const CONTRACT_ID: &str = "free-provider-reliability-routing-v1";
pub const PROVIDER_PROFILE_SCHEMA_VERSION: u16 = 1;
pub const PROVIDER_MODEL_DESCRIPTOR_SCHEMA_VERSION: u16 = 1;
pub const MAX_PROVIDER_PROFILE_ID_BYTES: usize = 128;
pub const MAX_PROVIDER_PROFILE_TRANSPORT_BYTES: usize = 64;
pub const MAX_PROVIDER_PROFILE_ENDPOINT_BYTES: usize = 512;
pub const MAX_PROVIDER_PROFILE_REGION_BYTES: usize = 64;
pub const MAX_PROVIDER_PROFILE_CREDENTIAL_BINDING_BYTES: usize = 128;
pub const MAX_PROVIDER_MODEL_ID_BYTES: usize = 256;
pub const MAX_RELIABILITY_LATENCY_MS: f64 = 86_400_000.0;
pub const FREE_ACCESS_EVIDENCE_SCHEMA_VERSION: u16 = 1;
pub const MAX_FREE_ACCESS_LIMITS: usize = 16;
pub const MAX_FREE_ACCESS_SAMPLES: u32 = 256;
pub const MAX_FREE_ACCESS_CONFIDENCE_BPS: u16 = 10_000;
pub const MAX_FREE_ACCESS_TTL_MS: u64 = 31 * 24 * 60 * 60 * 1_000;
pub const MAX_PROVIDER_MODEL_CAPABILITIES: usize = 16;
pub const MAX_PROVIDER_CATALOG_ENTRIES: usize = 2_048;
pub const PROVIDER_CATALOG_SCHEMA_VERSION: u16 = 1;
pub const MAX_PROVIDER_CATALOG_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;

pub type ProviderCatalogCache = Arc<std::sync::RwLock<HashMap<String, ProviderCatalogSnapshot>>>;

pub fn new_provider_catalog_cache() -> ProviderCatalogCache {
    Arc::new(std::sync::RwLock::new(HashMap::new()))
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFamily {
    OpenRouter,
    Groq,
    Gemini,
    Mistral,
    CloudflareWorkersAi,
    NvidiaNim,
    Cerebras,
    HuggingFace,
    LiteRouter,
    OpenAi,
    Ollama,
    Local,
    Mock,
    #[default]
    Unknown,
}

impl ProviderFamily {
    fn as_str(self) -> &'static str {
        match self {
            Self::OpenRouter => "openrouter",
            Self::Groq => "groq",
            Self::Gemini => "gemini",
            Self::Mistral => "mistral",
            Self::CloudflareWorkersAi => "cloudflare_workers_ai",
            Self::NvidiaNim => "nvidia_nim",
            Self::Cerebras => "cerebras",
            Self::HuggingFace => "hugging_face",
            Self::LiteRouter => "literouter",
            Self::OpenAi => "open_ai",
            Self::Ollama => "ollama",
            Self::Local => "local",
            Self::Mock => "mock",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    OpenAiCompatible,
    OpenAiResponses,
    Ollama,
    Local,
    Mock,
    #[default]
    Unknown,
}

impl TransportKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai_compatible",
            Self::OpenAiResponses => "openai_responses",
            Self::Ollama => "ollama",
            Self::Local => "local",
            Self::Mock => "mock",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderProfile {
    #[serde(default = "default_provider_profile_schema_version")]
    pub schema_version: u16,
    pub provider_id: String,
    #[serde(default)]
    pub provider_family: ProviderFamily,
    pub transport: String,
    #[serde(default)]
    pub transport_kind: TransportKind,
    pub endpoint: String,
    pub region: String,
    pub credential_binding: String,
    pub content_hash: String,
    #[serde(default = "default_revision")]
    pub revision: u64,
}

fn default_provider_profile_schema_version() -> u16 {
    PROVIDER_PROFILE_SCHEMA_VERSION
}

fn default_revision() -> u64 {
    1
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    Chat,
    Streaming,
    ToolCalls,
    StructuredOutput,
    Vision,
    Reasoning,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Supported,
    Unsupported,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityProvenance {
    ProviderDeclared,
    Observed,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityFlag {
    pub capability: ModelCapability,
    pub state: CapabilityState,
    pub provenance: CapabilityProvenance,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    LocalOnly,
    ProviderControlled,
    ProviderRetained,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    ProviderReported,
    GatewayMeasured,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageMetadata {
    pub input_unit: CreditUnit,
    pub output_unit: CreditUnit,
    pub source: UsageSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelLimits {
    pub context_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelLifecycle {
    Active,
    Deprecated,
    Unavailable,
    #[default]
    Unknown,
}

/// One immutable model snapshot adapted from the gateway's canonical catalog
/// entry. It carries provenance and policy metadata, but never a raw response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderModelDescriptor {
    pub schema_version: u16,
    pub provider_id: String,
    pub provider_family: ProviderFamily,
    pub transport_kind: TransportKind,
    pub model_id: String,
    pub profile_revision: u64,
    pub profile_content_hash: String,
    pub catalog_revision: u64,
    pub catalog_content_hash: String,
    pub limits: ModelLimits,
    pub capabilities: Vec<CapabilityFlag>,
    pub privacy: PrivacyClass,
    pub usage: UsageMetadata,
    pub lifecycle: ModelLifecycle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCatalogState {
    Fresh,
    Stale,
    Unavailable,
    CredentialRejected,
    DiscoveryUnsupported,
}

impl ProviderCatalogState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
            Self::CredentialRejected => "credential_rejected",
            Self::DiscoveryUnsupported => "discovery_unsupported",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "fresh" => Some(Self::Fresh),
            "stale" => Some(Self::Stale),
            "unavailable" => Some(Self::Unavailable),
            "credential_rejected" => Some(Self::CredentialRejected),
            "discovery_unsupported" => Some(Self::DiscoveryUnsupported),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CatalogFailureCode {
    Network,
    Timeout,
    CredentialRejected,
    RateLimited,
    ModelNotFound,
    MalformedResponse,
    ResponseTooLarge,
    EntryLimitExceeded,
    ProtocolMismatch,
    DiscoveryUnsupported,
    Unknown,
}

impl CatalogFailureCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::CredentialRejected => "credential_rejected",
            Self::RateLimited => "rate_limited",
            Self::ModelNotFound => "model_not_found",
            Self::MalformedResponse => "malformed_response",
            Self::ResponseTooLarge => "response_too_large",
            Self::EntryLimitExceeded => "entry_limit_exceeded",
            Self::ProtocolMismatch => "protocol_mismatch",
            Self::DiscoveryUnsupported => "discovery_unsupported",
            Self::Unknown => "unknown",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "network" => Some(Self::Network),
            "timeout" => Some(Self::Timeout),
            "credential_rejected" => Some(Self::CredentialRejected),
            "rate_limited" => Some(Self::RateLimited),
            "model_not_found" => Some(Self::ModelNotFound),
            "malformed_response" => Some(Self::MalformedResponse),
            "response_too_large" => Some(Self::ResponseTooLarge),
            "entry_limit_exceeded" => Some(Self::EntryLimitExceeded),
            "protocol_mismatch" => Some(Self::ProtocolMismatch),
            "discovery_unsupported" => Some(Self::DiscoveryUnsupported),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// Immutable, safe projection of one provider catalog observation. The
/// gateway remains the network owner; this contract owns lifecycle and route
/// eligibility semantics only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCatalogSnapshot {
    pub schema_version: u16,
    pub provider_id: String,
    pub credential_binding: String,
    pub region: String,
    pub profile_revision: u64,
    pub profile_content_hash: String,
    pub revision: u64,
    pub catalog_content_hash: String,
    pub state: ProviderCatalogState,
    pub models: Vec<ProviderModelDescriptor>,
    pub observed_at_ms: u64,
    pub expires_at_ms: u64,
    pub failure: Option<CatalogFailureCode>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FreeAccessState {
    Free,
    FreeTierLimited,
    TrialCredits,
    Paid,
    Unknown,
    Experimental,
    UnknownNeedsRefresh,
}

/// Evidence state is deliberately more precise than the historical advisory
/// `FreeAccessState`: trial credit, one-time credit and recurring free access
/// must never collapse into one boolean.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservedFreeAccessState {
    VerifiedFreeLimited,
    TrialOnly,
    CreditOnly,
    ActivationRequired,
    PaidOnly,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationState {
    NotRequired,
    Required,
    Completed,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AllowanceKind {
    Recurring,
    TrialCredit,
    OneTimeCredit,
    None,
    Unknown,
}

/// Units remain typed and opaque. In particular, credits are never converted
/// to tokens or currency without an authoritative provider contract.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreditUnit {
    Requests,
    Tokens,
    Characters,
    Seconds,
    CurrencyMicros,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLimitScope {
    Account,
    Provider,
    Model,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLimitSource {
    ProviderDeclared,
    Observed,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceInvalidation {
    BillingRequired,
    AccountRestricted,
    QuotaExhausted,
    CatalogChanged,
    CredentialChanged,
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceFreshness {
    Fresh,
    Stale,
    Expired,
    Invalidated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FreeAccessLimit {
    pub scope: EvidenceLimitScope,
    pub source: EvidenceLimitSource,
    pub unit: CreditUnit,
    pub allowance: AllowanceKind,
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    pub observed_at_ms: u64,
    pub resets_at_ms: Option<u64>,
}

impl FreeAccessLimit {
    fn validate(&self) -> Result<(), &'static str> {
        if self.observed_at_ms == 0
            || self.limit.is_some_and(|value| value == 0)
            || self
                .remaining
                .zip(self.limit)
                .is_some_and(|(remaining, limit)| remaining > limit)
            || self
                .resets_at_ms
                .is_some_and(|resets_at| resets_at <= self.observed_at_ms)
        {
            return Err("invalid free access limit");
        }
        Ok(())
    }
}

/// Core-owned, metadata-only evidence used by later probe and routing stages.
/// `credential_binding` is an opaque scope handle, never credential material.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FreeAccessEvidence {
    pub schema_version: u16,
    pub provider_id: String,
    pub model_id: String,
    pub credential_binding: String,
    pub region: String,
    pub advertised_state: FreeAccessState,
    pub observed_state: ObservedFreeAccessState,
    pub activation: ActivationState,
    pub allowance: AllowanceKind,
    pub limits: Vec<FreeAccessLimit>,
    pub successful_sample_count: u32,
    pub confidence_bps: u16,
    pub observed_at_ms: u64,
    pub expires_at_ms: u64,
    pub invalidation: Option<EvidenceInvalidation>,
    pub failure_reason: Option<String>,
    pub content_hash: String,
    pub revision: u64,
}

impl FreeAccessEvidence {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != FREE_ACCESS_EVIDENCE_SCHEMA_VERSION
            || !valid_profile_token(&self.provider_id, MAX_PROVIDER_PROFILE_ID_BYTES)
            || !valid_model_id(&self.model_id)
            || !valid_credential_binding(&self.credential_binding)
            || !valid_profile_token(&self.region, MAX_PROVIDER_PROFILE_REGION_BYTES)
            || self.limits.len() > MAX_FREE_ACCESS_LIMITS
            || self.successful_sample_count > MAX_FREE_ACCESS_SAMPLES
            || self.confidence_bps > MAX_FREE_ACCESS_CONFIDENCE_BPS
            || self.observed_at_ms == 0
            || self.expires_at_ms <= self.observed_at_ms
            || self.expires_at_ms.saturating_sub(self.observed_at_ms) > MAX_FREE_ACCESS_TTL_MS
            || self.revision == 0
            || !valid_content_hash(&self.content_hash)
            || self
                .failure_reason
                .as_deref()
                .is_some_and(|reason| !valid_profile_token(reason, 128))
            || self.limits.iter().any(|limit| limit.validate().is_err())
        {
            return Err("invalid free access evidence");
        }

        let consistent = match self.observed_state {
            ObservedFreeAccessState::VerifiedFreeLimited => {
                self.allowance == AllowanceKind::Recurring && self.successful_sample_count > 0
            }
            ObservedFreeAccessState::TrialOnly => self.allowance == AllowanceKind::TrialCredit,
            ObservedFreeAccessState::CreditOnly => self.allowance == AllowanceKind::OneTimeCredit,
            ObservedFreeAccessState::ActivationRequired => {
                self.activation == ActivationState::Required
            }
            ObservedFreeAccessState::PaidOnly => self.allowance == AllowanceKind::None,
            ObservedFreeAccessState::Unknown => true,
        };
        if !consistent {
            return Err("inconsistent free access evidence");
        }
        Ok(())
    }

    pub fn freshness_at(&self, now_ms: u64) -> EvidenceFreshness {
        if self.invalidation.is_some() {
            EvidenceFreshness::Invalidated
        } else if now_ms < self.observed_at_ms {
            EvidenceFreshness::Stale
        } else if now_ms >= self.expires_at_ms {
            EvidenceFreshness::Expired
        } else {
            EvidenceFreshness::Fresh
        }
    }

    /// The strict gate used by a future `FreeOnly` resolver. Advisory labels,
    /// trial credits, one-time credits and stale evidence do not pass it.
    pub fn is_strictly_free_at(&self, now_ms: u64) -> bool {
        self.validate().is_ok()
            && self.freshness_at(now_ms) == EvidenceFreshness::Fresh
            && self.observed_state == ObservedFreeAccessState::VerifiedFreeLimited
            && matches!(
                self.activation,
                ActivationState::NotRequired | ActivationState::Completed
            )
            && self.allowance == AllowanceKind::Recurring
            && self.successful_sample_count > 0
    }

    pub fn to_storage_record(
        &self,
    ) -> Result<
        evohime_local_storage::free_access_evidence_store::FreeAccessEvidenceRecord,
        &'static str,
    > {
        self.validate()?;
        let evidence_json = serde_json::to_vec(self).map_err(|_| "invalid free access evidence")?;
        Ok(
            evohime_local_storage::free_access_evidence_store::FreeAccessEvidenceRecord {
                provider_id: self.provider_id.clone(),
                model_id: self.model_id.clone(),
                credential_binding: self.credential_binding.clone(),
                region: self.region.clone(),
                revision: i64::try_from(self.revision)
                    .map_err(|_| "invalid free access evidence")?,
                content_hash: self.content_hash.clone(),
                evidence_json,
                observed_at_ms: i64::try_from(self.observed_at_ms)
                    .map_err(|_| "invalid free access evidence")?,
                expires_at_ms: i64::try_from(self.expires_at_ms)
                    .map_err(|_| "invalid free access evidence")?,
                invalidation: self.invalidation.map(|reason| {
                    serde_json::to_string(&reason)
                        .unwrap_or_else(|_| "unknown".to_string())
                        .trim_matches('"')
                        .to_string()
                }),
            },
        )
    }
}

fn valid_content_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityClass {
    Excellent,
    Healthy,
    Degraded,
    Unstable,
    CoolingDown,
    QuotaLimited,
    Unavailable,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReliabilitySnapshot {
    pub provider_id: String,
    pub model_id: String,
    pub sample_count: u32,
    pub success_rate: f64,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub class: ReliabilityClass,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteSelectionExplanation {
    pub provider_id: String,
    pub model_id: String,
    pub reason: String,
    pub free_state: FreeAccessState,
    pub reliability: ReliabilityClass,
}

impl ProviderProfile {
    pub fn from_route_config(route: &ModelRouteConfig) -> Result<Self, &'static str> {
        let (provider_id, provider_family, transport_kind) = match route.provider {
            evohime_model_gateway::providers::ProviderKind::LiteRouter => (
                "literouter",
                ProviderFamily::LiteRouter,
                TransportKind::OpenAiCompatible,
            ),
            evohime_model_gateway::providers::ProviderKind::OpenAICompatible => (
                "openai",
                ProviderFamily::OpenAi,
                TransportKind::OpenAiCompatible,
            ),
            evohime_model_gateway::providers::ProviderKind::OpenAIResponses => (
                "openai_responses",
                ProviderFamily::OpenAi,
                TransportKind::OpenAiResponses,
            ),
            evohime_model_gateway::providers::ProviderKind::Ollama => {
                ("ollama", ProviderFamily::Ollama, TransportKind::Ollama)
            }
            evohime_model_gateway::providers::ProviderKind::Local => {
                ("local", ProviderFamily::Local, TransportKind::Local)
            }
            evohime_model_gateway::providers::ProviderKind::Mock => {
                ("mock", ProviderFamily::Mock, TransportKind::Mock)
            }
        };
        let (provider_id, provider_family) =
            if route.provider == evohime_model_gateway::providers::ProviderKind::OpenAICompatible {
                builtin_provider_profiles()
                    .into_iter()
                    .find(|profile| profile.endpoint == route.literouter.base_url)
                    .map(|profile| (profile.provider_id, profile.provider_family))
                    .unwrap_or_else(|| (provider_id.to_owned(), provider_family))
            } else {
                (provider_id.to_owned(), provider_family)
            };
        let endpoint = if matches!(
            route.provider,
            evohime_model_gateway::providers::ProviderKind::Mock
        ) {
            "http://127.0.0.1/mock".to_string()
        } else {
            route.literouter.base_url.clone()
        };
        let credential_binding = format!("credential:{provider_id}");
        let mut profile = Self {
            schema_version: PROVIDER_PROFILE_SCHEMA_VERSION,
            provider_id,
            provider_family,
            transport: transport_kind.as_str().to_string(),
            transport_kind,
            endpoint,
            region: "global".into(),
            credential_binding,
            content_hash: String::new(),
            revision: 1,
        };
        profile.validate_without_hash()?;
        profile.content_hash = profile_hash(&profile);
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        self.validate_without_hash()?;
        if !valid_content_hash(&self.content_hash) {
            return Err("invalid provider profile");
        }
        Ok(())
    }

    fn validate_without_hash(&self) -> Result<(), &'static str> {
        let parsed_transport = parsed_transport_kind(&self.transport);
        if self.schema_version != PROVIDER_PROFILE_SCHEMA_VERSION
            || self.revision == 0
            || !valid_profile_token(&self.provider_id, MAX_PROVIDER_PROFILE_ID_BYTES)
            || !valid_profile_token(&self.transport, MAX_PROVIDER_PROFILE_TRANSPORT_BYTES)
            || !valid_profile_endpoint(&self.endpoint)
            || !valid_profile_token(&self.region, MAX_PROVIDER_PROFILE_REGION_BYTES)
            || !valid_credential_binding(&self.credential_binding)
            || (self.transport_kind != TransportKind::Unknown
                && parsed_transport != TransportKind::Unknown
                && parsed_transport != self.transport_kind)
        {
            return Err("invalid provider profile");
        }
        if !self.content_hash.is_empty() && !valid_content_hash(&self.content_hash) {
            return Err("invalid provider profile");
        }
        Ok(())
    }

    pub fn resolved_transport_kind(&self) -> TransportKind {
        if self.transport_kind != TransportKind::Unknown {
            return self.transport_kind;
        }
        parsed_transport_kind(&self.transport)
    }

    pub fn to_storage_record(
        &self,
        descriptors: &[ProviderModelDescriptor],
        revision: u64,
        catalog_content_hash: impl Into<String>,
        updated_at_ms: u64,
    ) -> Result<
        evohime_local_storage::provider_profile_catalog_store::ProviderProfileCatalogRecord,
        &'static str,
    > {
        self.validate()?;
        let catalog_content_hash = catalog_content_hash.into();
        if revision == 0
            || updated_at_ms == 0
            || descriptors.len() > MAX_PROVIDER_CATALOG_ENTRIES
            || !valid_content_hash(&catalog_content_hash)
            || descriptors.iter().any(|descriptor| {
                descriptor.validate().is_err()
                    || descriptor.provider_id != self.provider_id
                    || descriptor.profile_revision != self.revision
            })
        {
            return Err("invalid provider profile catalog");
        }
        let profile_json =
            serde_json::to_vec(self).map_err(|_| "invalid provider profile catalog")?;
        let catalog_json =
            serde_json::to_vec(descriptors).map_err(|_| "invalid provider profile catalog")?;
        Ok(
            evohime_local_storage::provider_profile_catalog_store::ProviderProfileCatalogRecord {
                provider_id: self.provider_id.clone(),
                credential_binding: self.credential_binding.clone(),
                region: self.region.clone(),
                revision: i64::try_from(revision)
                    .map_err(|_| "invalid provider profile catalog")?,
                profile_content_hash: self.content_hash.clone(),
                profile_json,
                catalog_content_hash,
                catalog_json,
                updated_at_ms: i64::try_from(updated_at_ms)
                    .map_err(|_| "invalid provider profile catalog")?,
                state: ProviderCatalogState::Fresh.as_str().into(),
                observed_at_ms: i64::try_from(updated_at_ms)
                    .map_err(|_| "invalid provider profile catalog")?,
                expires_at_ms: i64::try_from(
                    updated_at_ms
                        .checked_add(MAX_PROVIDER_CATALOG_TTL_MS)
                        .ok_or("invalid provider profile catalog")?,
                )
                .map_err(|_| "invalid provider profile catalog")?,
                failure_code: None,
            },
        )
    }
}

fn parsed_transport_kind(value: &str) -> TransportKind {
    match value {
        "openai_compatible" => TransportKind::OpenAiCompatible,
        "openai_responses" => TransportKind::OpenAiResponses,
        "ollama" => TransportKind::Ollama,
        "local" => TransportKind::Local,
        "mock" => TransportKind::Mock,
        _ => TransportKind::Unknown,
    }
}

impl ModelLimits {
    fn validate(&self) -> Result<(), &'static str> {
        if self.context_tokens.is_some_and(|value| value == 0)
            || self.max_output_tokens.is_some_and(|value| value == 0)
        {
            return Err("invalid model limits");
        }
        Ok(())
    }
}

impl CapabilityFlag {
    fn validate(&self) -> Result<(), &'static str> {
        if self.state != CapabilityState::Unknown
            && self.provenance == CapabilityProvenance::Unknown
        {
            return Err("capability state lacks provenance");
        }
        Ok(())
    }
}

impl ProviderModelDescriptor {
    pub fn from_catalog_entry(
        profile: &ProviderProfile,
        entry: &ModelCatalogEntry,
        catalog_revision: u64,
        catalog_content_hash: impl Into<String>,
    ) -> Result<Self, &'static str> {
        profile.validate()?;
        let descriptor = Self {
            schema_version: PROVIDER_MODEL_DESCRIPTOR_SCHEMA_VERSION,
            provider_id: profile.provider_id.clone(),
            provider_family: profile.provider_family,
            transport_kind: profile.resolved_transport_kind(),
            model_id: entry.id.clone(),
            profile_revision: profile.revision,
            profile_content_hash: profile.content_hash.clone(),
            catalog_revision,
            catalog_content_hash: catalog_content_hash.into(),
            limits: ModelLimits {
                context_tokens: entry.context_tokens,
                max_output_tokens: entry.max_output_tokens,
            },
            capabilities: Vec::new(),
            privacy: PrivacyClass::Unknown,
            usage: UsageMetadata {
                input_unit: CreditUnit::Unknown,
                output_unit: CreditUnit::Unknown,
                source: UsageSource::Unknown,
            },
            lifecycle: ModelLifecycle::Unknown,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != PROVIDER_MODEL_DESCRIPTOR_SCHEMA_VERSION
            || !valid_profile_token(&self.provider_id, MAX_PROVIDER_PROFILE_ID_BYTES)
            || !valid_model_id(&self.model_id)
            || self.profile_revision == 0
            || self.catalog_revision == 0
            || !valid_content_hash(&self.profile_content_hash)
            || !valid_content_hash(&self.catalog_content_hash)
            || self.capabilities.len() > MAX_PROVIDER_MODEL_CAPABILITIES
            || self.limits.validate().is_err()
            || self
                .capabilities
                .iter()
                .any(|capability| capability.validate().is_err())
            || self
                .capabilities
                .iter()
                .enumerate()
                .any(|(index, capability)| {
                    self.capabilities[..index]
                        .iter()
                        .any(|previous| previous.capability == capability.capability)
                })
        {
            return Err("invalid provider model descriptor");
        }
        Ok(())
    }
}

impl ProviderCatalogSnapshot {
    pub fn fresh_from_catalog(
        profile: &ProviderProfile,
        entries: &[ModelCatalogEntry],
        revision: u64,
        catalog_content_hash: impl Into<String>,
        observed_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<Self, &'static str> {
        profile.validate()?;
        let catalog_content_hash = catalog_content_hash.into();
        if revision == 0
            || !valid_content_hash(&catalog_content_hash)
            || observed_at_ms == 0
            || expires_at_ms <= observed_at_ms
            || expires_at_ms.saturating_sub(observed_at_ms) > MAX_PROVIDER_CATALOG_TTL_MS
            || entries.len() > MAX_PROVIDER_CATALOG_ENTRIES
        {
            return Err("invalid provider catalog snapshot");
        }

        let normalized = normalize_catalog_entries(entries)?;
        let models = normalized
            .iter()
            .map(|entry| {
                ProviderModelDescriptor::from_catalog_entry(
                    profile,
                    entry,
                    revision,
                    catalog_content_hash.clone(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let snapshot = Self {
            schema_version: PROVIDER_CATALOG_SCHEMA_VERSION,
            provider_id: profile.provider_id.clone(),
            credential_binding: profile.credential_binding.clone(),
            region: profile.region.clone(),
            profile_revision: profile.revision,
            profile_content_hash: profile.content_hash.clone(),
            revision,
            catalog_content_hash,
            state: ProviderCatalogState::Fresh,
            models,
            observed_at_ms,
            expires_at_ms,
            failure: None,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn failure(
        profile: &ProviderProfile,
        revision: u64,
        catalog_content_hash: impl Into<String>,
        state: ProviderCatalogState,
        failure: CatalogFailureCode,
        observed_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<Self, &'static str> {
        profile.validate()?;
        let snapshot = Self {
            schema_version: PROVIDER_CATALOG_SCHEMA_VERSION,
            provider_id: profile.provider_id.clone(),
            credential_binding: profile.credential_binding.clone(),
            region: profile.region.clone(),
            profile_revision: profile.revision,
            profile_content_hash: profile.content_hash.clone(),
            revision,
            catalog_content_hash: catalog_content_hash.into(),
            state,
            models: Vec::new(),
            observed_at_ms,
            expires_at_ms,
            failure: Some(failure),
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != PROVIDER_CATALOG_SCHEMA_VERSION
            || !valid_profile_token(&self.provider_id, MAX_PROVIDER_PROFILE_ID_BYTES)
            || !valid_credential_binding(&self.credential_binding)
            || !valid_profile_token(&self.region, MAX_PROVIDER_PROFILE_REGION_BYTES)
            || self.profile_revision == 0
            || self.revision == 0
            || !valid_content_hash(&self.profile_content_hash)
            || !valid_content_hash(&self.catalog_content_hash)
            || self.models.len() > MAX_PROVIDER_CATALOG_ENTRIES
            || self.observed_at_ms == 0
            || self.expires_at_ms <= self.observed_at_ms
            || self.expires_at_ms.saturating_sub(self.observed_at_ms) > MAX_PROVIDER_CATALOG_TTL_MS
            || self.models.iter().any(|model| {
                model.validate().is_err()
                    || model.provider_id != self.provider_id
                    || model.profile_revision != self.profile_revision
                    || model.catalog_revision != self.revision
                    || model.catalog_content_hash != self.catalog_content_hash
            })
            || self.models.iter().enumerate().any(|(index, model)| {
                self.models[..index]
                    .iter()
                    .any(|previous| previous.model_id == model.model_id)
            })
        {
            return Err("invalid provider catalog snapshot");
        }

        let state_is_consistent = match self.state {
            ProviderCatalogState::Fresh => self.failure.is_none(),
            ProviderCatalogState::Stale => true,
            ProviderCatalogState::Unavailable => {
                self.models.is_empty()
                    && self.failure.is_some_and(|failure| {
                        !matches!(
                            failure,
                            CatalogFailureCode::CredentialRejected
                                | CatalogFailureCode::DiscoveryUnsupported
                        )
                    })
            }
            ProviderCatalogState::CredentialRejected => {
                self.models.is_empty()
                    && self.failure == Some(CatalogFailureCode::CredentialRejected)
            }
            ProviderCatalogState::DiscoveryUnsupported => {
                self.models.is_empty()
                    && self.failure == Some(CatalogFailureCode::DiscoveryUnsupported)
            }
        };
        if !state_is_consistent {
            return Err("inconsistent provider catalog snapshot");
        }
        Ok(())
    }

    pub fn route_eligible_at(&self, model_id: &str, now_ms: u64) -> bool {
        self.validate().is_ok()
            && self.state == ProviderCatalogState::Fresh
            && now_ms >= self.observed_at_ms
            && now_ms < self.expires_at_ms
            && self.models.iter().any(|model| model.model_id == model_id)
    }

    pub fn gateway_entries(&self) -> Result<Vec<ModelCatalogEntry>, &'static str> {
        self.validate()?;
        Ok(self
            .models
            .iter()
            .map(|model| ModelCatalogEntry {
                id: model.model_id.clone(),
                context_tokens: model.limits.context_tokens,
                max_output_tokens: model.limits.max_output_tokens,
            })
            .collect())
    }

    pub fn stale_after_failure(
        profile: &ProviderProfile,
        previous: &Self,
        revision: u64,
        failure: CatalogFailureCode,
    ) -> Result<Self, &'static str> {
        profile.validate()?;
        previous.validate()?;
        if revision == 0
            || previous.provider_id != profile.provider_id
            || previous.credential_binding != profile.credential_binding
            || previous.region != profile.region
            || previous.profile_revision != profile.revision
            || previous.profile_content_hash != profile.content_hash
        {
            return Err("provider catalog profile mismatch");
        }
        let models = previous
            .models
            .iter()
            .cloned()
            .map(|mut model| {
                model.catalog_revision = revision;
                model.validate()?;
                Ok(model)
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        let snapshot = Self {
            schema_version: PROVIDER_CATALOG_SCHEMA_VERSION,
            provider_id: profile.provider_id.clone(),
            credential_binding: profile.credential_binding.clone(),
            region: profile.region.clone(),
            profile_revision: profile.revision,
            profile_content_hash: profile.content_hash.clone(),
            revision,
            catalog_content_hash: previous.catalog_content_hash.clone(),
            state: ProviderCatalogState::Stale,
            models,
            observed_at_ms: previous.observed_at_ms,
            expires_at_ms: previous.expires_at_ms,
            failure: Some(failure),
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn to_storage_record(
        &self,
        profile: &ProviderProfile,
    ) -> Result<
        evohime_local_storage::provider_profile_catalog_store::ProviderProfileCatalogRecord,
        &'static str,
    > {
        self.validate()?;
        if profile.validate().is_err()
            || profile.provider_id != self.provider_id
            || profile.credential_binding != self.credential_binding
            || profile.region != self.region
            || profile.revision != self.profile_revision
            || profile.content_hash != self.profile_content_hash
        {
            return Err("provider catalog profile mismatch");
        }
        let mut record = profile.to_storage_record(
            &self.models,
            self.revision,
            self.catalog_content_hash.clone(),
            self.observed_at_ms,
        )?;
        record.state = self.state.as_str().into();
        record.observed_at_ms =
            i64::try_from(self.observed_at_ms).map_err(|_| "invalid provider catalog snapshot")?;
        record.expires_at_ms =
            i64::try_from(self.expires_at_ms).map_err(|_| "invalid provider catalog snapshot")?;
        record.failure_code = self.failure.map(|failure| failure.as_str().into());
        Ok(record)
    }

    pub fn from_storage_record(
        record: &evohime_local_storage::provider_profile_catalog_store::ProviderProfileCatalogRecord,
    ) -> Result<Self, &'static str> {
        if record.revision <= 0
            || record.updated_at_ms <= 0
            || record.observed_at_ms <= 0
            || record.expires_at_ms <= record.observed_at_ms
        {
            return Err("invalid provider catalog snapshot");
        }
        let profile: ProviderProfile =
            serde_json::from_slice(&record.profile_json).map_err(|_| "invalid provider profile")?;
        profile.validate()?;
        if profile.provider_id != record.provider_id
            || profile.credential_binding != record.credential_binding
            || profile.region != record.region
            || profile.content_hash != record.profile_content_hash
        {
            return Err("provider catalog profile mismatch");
        }
        let models: Vec<ProviderModelDescriptor> = serde_json::from_slice(&record.catalog_json)
            .map_err(|_| "invalid provider catalog snapshot")?;
        let state = ProviderCatalogState::from_str(&record.state)
            .ok_or("invalid provider catalog snapshot")?;
        let failure = record
            .failure_code
            .as_deref()
            .map(|value| {
                CatalogFailureCode::from_str(value).ok_or("invalid provider catalog snapshot")
            })
            .transpose()?;
        let snapshot = Self {
            schema_version: PROVIDER_CATALOG_SCHEMA_VERSION,
            provider_id: record.provider_id.clone(),
            credential_binding: record.credential_binding.clone(),
            region: record.region.clone(),
            profile_revision: profile.revision,
            profile_content_hash: record.profile_content_hash.clone(),
            revision: u64::try_from(record.revision)
                .map_err(|_| "invalid provider catalog snapshot")?,
            catalog_content_hash: record.catalog_content_hash.clone(),
            state,
            models,
            observed_at_ms: u64::try_from(record.observed_at_ms)
                .map_err(|_| "invalid provider catalog snapshot")?,
            expires_at_ms: u64::try_from(record.expires_at_ms)
                .map_err(|_| "invalid provider catalog snapshot")?,
            failure,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
}

pub(crate) fn provider_catalog_scope_key(profile: &ProviderProfile) -> String {
    format!(
        "{}|{}|{}",
        profile.provider_id, profile.credential_binding, profile.region
    )
}

/// Adapter between the Core-owned catalog lifecycle and the gateway's final
/// route dispatch boundary. An absent snapshot is treated as unobserved (the
/// gateway still performs its configured-provider checks); a known stale,
/// expired or failed snapshot is never allowed to reach the provider.
pub struct ProviderCatalogRoutePreflight {
    config: evohime_model_gateway::ModelGatewayConfig,
    cache: ProviderCatalogCache,
}

impl ProviderCatalogRoutePreflight {
    pub fn new(
        config: evohime_model_gateway::ModelGatewayConfig,
        cache: ProviderCatalogCache,
    ) -> Self {
        Self { config, cache }
    }
}

impl RoutePreflight for ProviderCatalogRoutePreflight {
    fn check(&self, route: &str, model: Option<&str>, now_ms: u64) -> Result<(), ProviderError> {
        let route_config = self
            .config
            .routes
            .get(route)
            .ok_or_else(|| ProviderError::Config("provider_route_not_configured".into()))?;
        if !route_config.configured() {
            return Err(ProviderError::Config(
                "provider_credential_not_configured".into(),
            ));
        }
        let profile = ProviderProfile::from_route_config(route_config)
            .map_err(|_| ProviderError::Config("provider_profile_invalid".into()))?;
        let model_id = model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| route_config.literouter.model.trim());
        if model_id.is_empty() {
            return Err(ProviderError::Config(
                "provider_model_not_configured".into(),
            ));
        }
        let snapshot = self
            .cache
            .read()
            .map_err(|_| ProviderError::Config("provider_catalog_lock_failed".into()))?
            .get(&provider_catalog_scope_key(&profile))
            .cloned();
        let Some(snapshot) = snapshot else {
            return Ok(());
        };
        match snapshot.state {
            ProviderCatalogState::Fresh if snapshot.route_eligible_at(model_id, now_ms) => Ok(()),
            ProviderCatalogState::Fresh => {
                Err(ProviderError::Config("provider_model_not_cataloged".into()))
            }
            ProviderCatalogState::Stale => {
                Err(ProviderError::Config("provider_catalog_stale".into()))
            }
            ProviderCatalogState::CredentialRejected => {
                Err(ProviderError::Config("provider_credential_rejected".into()))
            }
            ProviderCatalogState::Unavailable => {
                Err(ProviderError::Config("provider_catalog_unavailable".into()))
            }
            ProviderCatalogState::DiscoveryUnsupported => Err(ProviderError::Config(
                "provider_catalog_discovery_unsupported".into(),
            )),
        }
    }
}

pub fn normalize_catalog_entries(
    entries: &[ModelCatalogEntry],
) -> Result<Vec<ModelCatalogEntry>, &'static str> {
    if entries.len() > MAX_PROVIDER_CATALOG_ENTRIES {
        return Err("invalid provider catalog snapshot");
    }
    let mut normalized = entries.to_vec();
    normalized.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| right.context_tokens.cmp(&left.context_tokens))
            .then_with(|| right.max_output_tokens.cmp(&left.max_output_tokens))
    });
    normalized.dedup_by(|left, right| left.id == right.id);
    Ok(normalized)
}

pub fn catalog_content_hash(entries: &[ModelCatalogEntry]) -> Result<String, &'static str> {
    let normalized = normalize_catalog_entries(entries)?;
    let json = serde_json::to_vec(&normalized).map_err(|_| "invalid provider catalog snapshot")?;
    Ok(hex::encode(Sha256::digest(json)))
}

pub fn classify_catalog_error(error: &ProviderError) -> CatalogFailureCode {
    let message = match error {
        ProviderError::Config(message)
        | ProviderError::Http(message)
        | ProviderError::Api(message)
        | ProviderError::Stream(message) => message.to_ascii_lowercase(),
    };
    match error {
        ProviderError::Config(_) if message.contains("key") || message.contains("credential") => {
            CatalogFailureCode::CredentialRejected
        }
        ProviderError::Config(_) => CatalogFailureCode::DiscoveryUnsupported,
        ProviderError::Http(_) | ProviderError::Stream(_) if message.contains("timeout") => {
            CatalogFailureCode::Timeout
        }
        ProviderError::Http(_) | ProviderError::Stream(_) => CatalogFailureCode::Network,
        ProviderError::Api(_) if message.contains("401") || message.contains("403") => {
            CatalogFailureCode::CredentialRejected
        }
        ProviderError::Api(_) if message.contains("429") || message.contains("rate") => {
            CatalogFailureCode::RateLimited
        }
        ProviderError::Api(_)
            if message.contains("404")
                || message.contains("model not found")
                || message.contains("model_not_found") =>
        {
            CatalogFailureCode::ModelNotFound
        }
        ProviderError::Api(_)
            if message.contains("exceeds size") || message.contains("too large") =>
        {
            CatalogFailureCode::ResponseTooLarge
        }
        ProviderError::Api(_) if message.contains("too many") || message.contains("entries") => {
            CatalogFailureCode::EntryLimitExceeded
        }
        ProviderError::Api(_) if message.contains("invalid") || message.contains("malformed") => {
            CatalogFailureCode::MalformedResponse
        }
        ProviderError::Api(_) => CatalogFailureCode::ProtocolMismatch,
    }
}

/// Trusted, metadata-only defaults. They provide identity and transport
/// policy; credentials and provider model catalogs are still supplied by the
/// configured route or a later bounded discovery stage.
pub fn builtin_provider_profiles() -> Vec<ProviderProfile> {
    [
        (
            "openrouter",
            ProviderFamily::OpenRouter,
            "https://openrouter.ai/api/v1",
        ),
        (
            "groq",
            ProviderFamily::Groq,
            "https://api.groq.com/openai/v1",
        ),
        (
            "gemini",
            ProviderFamily::Gemini,
            "https://generativelanguage.googleapis.com/v1beta/openai",
        ),
        (
            "mistral",
            ProviderFamily::Mistral,
            "https://api.mistral.ai/v1",
        ),
        (
            "cloudflare_workers_ai",
            ProviderFamily::CloudflareWorkersAi,
            "https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/run",
        ),
        (
            "nvidia_nim",
            ProviderFamily::NvidiaNim,
            "https://integrate.api.nvidia.com/v1",
        ),
        (
            "cerebras",
            ProviderFamily::Cerebras,
            "https://api.cerebras.ai/v1",
        ),
        (
            "hugging_face",
            ProviderFamily::HuggingFace,
            "https://router.huggingface.co/v1",
        ),
    ]
    .into_iter()
    .map(|(provider_id, provider_family, endpoint)| {
        let mut profile = ProviderProfile {
            schema_version: PROVIDER_PROFILE_SCHEMA_VERSION,
            provider_id: provider_id.into(),
            provider_family,
            transport: "openai_compatible".into(),
            transport_kind: TransportKind::OpenAiCompatible,
            endpoint: endpoint.into(),
            region: "global".into(),
            credential_binding: format!("credential:{provider_id}"),
            content_hash: String::new(),
            revision: 1,
        };
        profile.content_hash = profile_hash(&profile);
        profile
    })
    .collect()
}

fn profile_hash(profile: &ProviderProfile) -> String {
    let mut hasher = Sha256::new();
    hasher.update(profile.schema_version.to_string().as_bytes());
    hasher.update(b"|");
    hasher.update(profile.provider_id.as_bytes());
    hasher.update(b"|");
    hasher.update(profile.provider_family.as_str().as_bytes());
    hasher.update(b"|");
    hasher.update(profile.transport_kind.as_str().as_bytes());
    hasher.update(b"|");
    hasher.update(profile.endpoint.as_bytes());
    hasher.update(b"|");
    hasher.update(profile.region.as_bytes());
    hasher.update(b"|");
    hasher.update(profile.credential_binding.as_bytes());
    hasher.update(b"|");
    hasher.update(profile.revision.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

fn valid_profile_token(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value == value.trim()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

fn valid_profile_endpoint(value: &str) -> bool {
    value.len() <= MAX_PROVIDER_PROFILE_ENDPOINT_BYTES
        && value == value.trim()
        && (value.starts_with("https://") || value.starts_with("http://"))
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        && !value.contains(['?', '#', '@'])
        && value
            .split_once("://")
            .is_some_and(|(_, authority)| !authority.is_empty() && !authority.starts_with('/'))
}

fn valid_credential_binding(value: &str) -> bool {
    valid_profile_token(value, MAX_PROVIDER_PROFILE_CREDENTIAL_BINDING_BYTES)
        && !value.to_ascii_lowercase().contains("secret")
        && !value.to_ascii_lowercase().contains("bearer")
        && !value.to_ascii_lowercase().starts_with("sk-")
        && !value.to_ascii_lowercase().starts_with("gsk_")
        && !value.to_ascii_lowercase().starts_with("aiza")
}
impl ReliabilitySnapshot {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_profile_token(&self.provider_id, MAX_PROVIDER_PROFILE_ID_BYTES)
            || !valid_model_id(&self.model_id)
            || self.sample_count > 256
            || !self.success_rate.is_finite()
            || !(0.0..=1.0).contains(&self.success_rate)
            || !valid_latency(self.p50_ms)
            || !valid_latency(self.p95_ms)
            || !valid_latency(self.jitter_ms)
            || self
                .p50_ms
                .zip(self.p95_ms)
                .is_some_and(|(p50, p95)| p50 > p95)
            || self.class != classify(self)
        {
            return Err("invalid reliability snapshot");
        }
        Ok(())
    }
}

fn valid_model_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PROVIDER_MODEL_ID_BYTES
        && value == value.trim()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
}

fn valid_latency(value: Option<f64>) -> bool {
    value.is_none_or(|value| {
        value.is_finite() && (0.0..=MAX_RELIABILITY_LATENCY_MS).contains(&value)
    })
}

pub fn classify(snapshot: &ReliabilitySnapshot) -> ReliabilityClass {
    if snapshot.sample_count < 3 {
        ReliabilityClass::Unknown
    } else if snapshot.success_rate >= 0.99 {
        ReliabilityClass::Excellent
    } else if snapshot.success_rate >= 0.95 {
        ReliabilityClass::Healthy
    } else if snapshot.success_rate >= 0.8 {
        ReliabilityClass::Degraded
    } else {
        ReliabilityClass::Unstable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_preflight_rejects_known_stale_catalog_before_provider_dispatch() {
        let route = ModelRouteConfig::openai_compatible(
            "test-key",
            "https://provider.example/v1",
            "model-a",
        );
        let profile = ProviderProfile::from_route_config(&route).expect("profile");
        let fresh = ProviderCatalogSnapshot::fresh_from_catalog(
            &profile,
            &[ModelCatalogEntry {
                id: "model-a".into(),
                context_tokens: None,
                max_output_tokens: None,
            }],
            1,
            "a".repeat(64),
            1_000,
            2_000,
        )
        .expect("fresh catalog");
        let stale = ProviderCatalogSnapshot::stale_after_failure(
            &profile,
            &fresh,
            2,
            CatalogFailureCode::Timeout,
        )
        .expect("stale catalog");
        let cache = new_provider_catalog_cache();
        cache
            .write()
            .expect("cache write")
            .insert(provider_catalog_scope_key(&profile), stale);
        let config = evohime_model_gateway::ModelGatewayConfig {
            default_route: "default".into(),
            routes: std::collections::HashMap::from([("default".into(), route)]),
        };
        let preflight = ProviderCatalogRoutePreflight::new(config, cache);
        let error = preflight
            .check("default", Some("model-a"), 1_500)
            .expect_err("stale catalog must fail closed");
        assert!(matches!(error, ProviderError::Config(code) if code == "provider_catalog_stale"));
    }

    fn profile() -> ProviderProfile {
        ProviderProfile {
            schema_version: PROVIDER_PROFILE_SCHEMA_VERSION,
            provider_id: "openrouter".into(),
            provider_family: ProviderFamily::OpenRouter,
            transport: "openai_compatible".into(),
            transport_kind: TransportKind::OpenAiCompatible,
            endpoint: "https://openrouter.ai/api/v1".into(),
            region: "global".into(),
            credential_binding: "credential:openrouter".into(),
            content_hash: "a".repeat(64),
            revision: 1,
        }
    }

    fn evidence() -> FreeAccessEvidence {
        FreeAccessEvidence {
            schema_version: FREE_ACCESS_EVIDENCE_SCHEMA_VERSION,
            provider_id: "openrouter".into(),
            model_id: "provider/model:free".into(),
            credential_binding: "cred:openrouter".into(),
            region: "global".into(),
            advertised_state: FreeAccessState::FreeTierLimited,
            observed_state: ObservedFreeAccessState::VerifiedFreeLimited,
            activation: ActivationState::Completed,
            allowance: AllowanceKind::Recurring,
            limits: vec![FreeAccessLimit {
                scope: EvidenceLimitScope::Model,
                source: EvidenceLimitSource::Observed,
                unit: CreditUnit::Requests,
                allowance: AllowanceKind::Recurring,
                limit: Some(20),
                remaining: Some(19),
                observed_at_ms: 1_000,
                resets_at_ms: Some(2_000),
            }],
            successful_sample_count: 3,
            confidence_bps: 9_000,
            observed_at_ms: 1_000,
            expires_at_ms: 2_000,
            invalidation: None,
            failure_reason: None,
            content_hash: "b".repeat(64),
            revision: 1,
        }
    }

    #[test]
    fn provider_profile_accepts_bounded_secret_free_metadata() {
        assert!(profile().validate().is_ok());
    }

    #[test]
    fn provider_profile_rejects_unbounded_or_secret_bearing_metadata() {
        let mut oversized = profile();
        oversized.provider_id = "p".repeat(MAX_PROVIDER_PROFILE_ID_BYTES + 1);
        assert_eq!(oversized.validate(), Err("invalid provider profile"));

        let mut endpoint_with_secret = profile();
        endpoint_with_secret.endpoint =
            "https://provider.example/v1?api_key=secret-provider-key".into();
        assert_eq!(
            endpoint_with_secret.validate(),
            Err("invalid provider profile")
        );

        let mut secret_binding = profile();
        secret_binding.credential_binding = "sk-live-provider-key".into();
        assert_eq!(secret_binding.validate(), Err("invalid provider profile"));

        let mut mismatched_transport = profile();
        mismatched_transport.transport_kind = TransportKind::Ollama;
        assert_eq!(
            mismatched_transport.validate(),
            Err("invalid provider profile")
        );
    }

    #[test]
    fn provider_profile_rejects_non_hex_content_hash() {
        let mut invalid = profile();
        invalid.content_hash = "z".repeat(64);
        assert_eq!(invalid.validate(), Err("invalid provider profile"));
    }

    #[test]
    fn legacy_profile_defaults_keep_transport_compatible() {
        let legacy = serde_json::json!({
            "provider_id": "openrouter",
            "transport": "openai_compatible",
            "endpoint": "https://openrouter.ai/api/v1",
            "region": "global",
            "credential_binding": "cred:openrouter",
            "content_hash": "a".repeat(64)
        });
        let parsed: ProviderProfile = serde_json::from_value(legacy).expect("legacy profile");
        assert!(parsed.validate().is_ok());
        assert_eq!(
            parsed.resolved_transport_kind(),
            TransportKind::OpenAiCompatible
        );
    }

    #[test]
    fn route_config_adapter_keeps_credentials_out_of_profile_metadata() {
        let route = ModelRouteConfig::openai_compatible(
            "sk-live-provider-key",
            "https://api.example/v1",
            "model",
        );
        let profile = ProviderProfile::from_route_config(&route).expect("profile");
        assert!(profile.validate().is_ok());
        assert_eq!(profile.provider_family, ProviderFamily::OpenAi);
        assert_eq!(profile.transport_kind, TransportKind::OpenAiCompatible);
        assert!(!serde_json::to_string(&profile)
            .expect("profile json")
            .contains("sk-live-provider-key"));
    }

    #[test]
    fn builtin_profiles_are_bounded_and_versioned() {
        let profiles = builtin_provider_profiles();
        assert_eq!(profiles.len(), 8);
        assert!(profiles.iter().all(|profile| profile.validate().is_ok()));
        assert!(profiles
            .iter()
            .all(|profile| profile.credential_binding.starts_with("credential:")));
        let mut ids: Vec<_> = profiles
            .iter()
            .map(|profile| profile.provider_id.as_str())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), profiles.len());
    }

    #[test]
    fn trusted_builtin_endpoints_keep_provider_identity_separate_from_transport() {
        for builtin in builtin_provider_profiles() {
            let route = ModelRouteConfig::openai_compatible(
                "provider-key",
                builtin.endpoint.clone(),
                "provider-model",
            );
            let profile = ProviderProfile::from_route_config(&route).expect("profile");
            assert_eq!(profile.provider_id, builtin.provider_id);
            assert_eq!(profile.provider_family, builtin.provider_family);
            assert_eq!(profile.transport_kind, TransportKind::OpenAiCompatible);
            assert_eq!(profile.credential_binding, builtin.credential_binding);
        }
    }

    #[test]
    fn model_descriptor_adapts_gateway_catalog_with_fail_closed_metadata() {
        let entry = ModelCatalogEntry {
            id: "provider/model".into(),
            context_tokens: Some(16_384),
            max_output_tokens: Some(2_048),
        };
        let descriptor =
            ProviderModelDescriptor::from_catalog_entry(&profile(), &entry, 7, "c".repeat(64))
                .expect("descriptor");
        assert!(descriptor.validate().is_ok());
        assert_eq!(descriptor.model_id, entry.id);
        assert_eq!(descriptor.limits.context_tokens, Some(16_384));
        assert!(descriptor.capabilities.is_empty());
        assert_eq!(descriptor.privacy, PrivacyClass::Unknown);
        assert_eq!(descriptor.usage.source, UsageSource::Unknown);

        let mut duplicate = descriptor.clone();
        duplicate.capabilities = vec![
            CapabilityFlag {
                capability: ModelCapability::Chat,
                state: CapabilityState::Supported,
                provenance: CapabilityProvenance::ProviderDeclared,
            },
            CapabilityFlag {
                capability: ModelCapability::Chat,
                state: CapabilityState::Unknown,
                provenance: CapabilityProvenance::Unknown,
            },
        ];
        assert_eq!(
            duplicate.validate(),
            Err("invalid provider model descriptor")
        );
    }

    #[test]
    fn provider_profile_snapshot_round_trips_through_metadata_store() {
        let entry = ModelCatalogEntry {
            id: "provider/model".into(),
            context_tokens: Some(8_192),
            max_output_tokens: Some(1_024),
        };
        let profile = profile();
        let descriptor =
            ProviderModelDescriptor::from_catalog_entry(&profile, &entry, 1, "c".repeat(64))
                .expect("descriptor");
        let record = profile
            .to_storage_record(&[descriptor], 1, "d".repeat(64), 1_000)
            .expect("storage record");

        let database = rusqlite::Connection::open_in_memory().expect("sqlite");
        evohime_local_storage::provider_profile_catalog_store::install_schema(&database)
            .expect("schema");
        assert!(
            evohime_local_storage::provider_profile_catalog_store::put(&database, &record)
                .expect("write")
        );
        let stored = evohime_local_storage::provider_profile_catalog_store::get(
            &database,
            "openrouter",
            "credential:openrouter",
            "global",
        )
        .expect("read")
        .expect("snapshot");
        assert!(!String::from_utf8(stored.profile_json)
            .expect("profile json")
            .contains("secret"));
        assert!(!String::from_utf8(stored.catalog_json)
            .expect("catalog json")
            .contains("prompt"));
        assert_eq!(stored.state, "fresh");
        assert_eq!(stored.failure_code, None);
    }

    #[test]
    fn catalog_snapshot_recovers_lifecycle_and_failure_from_store() {
        let profile = profile();
        let snapshot = ProviderCatalogSnapshot::fresh_from_catalog(
            &profile,
            &[ModelCatalogEntry {
                id: "provider/model".into(),
                context_tokens: Some(8_192),
                max_output_tokens: Some(1_024),
            }],
            1,
            "c".repeat(64),
            1_000,
            2_000,
        )
        .expect("fresh snapshot");
        let database = rusqlite::Connection::open_in_memory().expect("sqlite");
        evohime_local_storage::provider_profile_catalog_store::install_schema(&database)
            .expect("schema");
        let record = snapshot
            .to_storage_record(&profile)
            .expect("storage record");
        assert!(
            evohime_local_storage::provider_profile_catalog_store::put(&database, &record)
                .expect("fresh write")
        );
        let stored = evohime_local_storage::provider_profile_catalog_store::get(
            &database,
            "openrouter",
            "credential:openrouter",
            "global",
        )
        .expect("fresh read")
        .expect("fresh snapshot");
        assert_eq!(
            ProviderCatalogSnapshot::from_storage_record(&stored).expect("fresh recovery"),
            snapshot
        );

        let failure = ProviderCatalogSnapshot::failure(
            &profile,
            2,
            "d".repeat(64),
            ProviderCatalogState::Unavailable,
            CatalogFailureCode::Network,
            2_000,
            3_000,
        )
        .expect("failure snapshot");
        let failure_record = failure.to_storage_record(&profile).expect("failure record");
        assert!(evohime_local_storage::provider_profile_catalog_store::put(
            &database,
            &failure_record
        )
        .expect("failure write"));
        let stored_failure = evohime_local_storage::provider_profile_catalog_store::get(
            &database,
            "openrouter",
            "credential:openrouter",
            "global",
        )
        .expect("failure read")
        .expect("failure snapshot");
        let recovered_failure =
            ProviderCatalogSnapshot::from_storage_record(&stored_failure).expect("recovery");
        assert_eq!(recovered_failure.state, ProviderCatalogState::Unavailable);
        assert_eq!(recovered_failure.failure, Some(CatalogFailureCode::Network));
        assert!(!recovered_failure.route_eligible_at("provider/model", 2_500));

        let missing_model = ProviderCatalogSnapshot::failure(
            &profile,
            3,
            "e".repeat(64),
            ProviderCatalogState::Unavailable,
            CatalogFailureCode::ModelNotFound,
            3_000,
            4_000,
        )
        .expect("model-not-found snapshot");
        let missing_model_record = missing_model
            .to_storage_record(&profile)
            .expect("model-not-found record");
        assert_eq!(
            missing_model_record.failure_code.as_deref(),
            Some("model_not_found")
        );
        assert!(evohime_local_storage::provider_profile_catalog_store::put(
            &database,
            &missing_model_record
        )
        .expect("model-not-found write"));
        let stored_missing_model = evohime_local_storage::provider_profile_catalog_store::get(
            &database,
            "openrouter",
            "credential:openrouter",
            "global",
        )
        .expect("model-not-found read")
        .expect("model-not-found snapshot");
        assert_eq!(
            ProviderCatalogSnapshot::from_storage_record(&stored_missing_model)
                .expect("model-not-found recovery")
                .failure,
            Some(CatalogFailureCode::ModelNotFound)
        );
    }

    #[test]
    fn catalog_snapshot_deduplicates_deterministically_and_fails_closed_on_expiry() {
        let profile = profile();
        let entries = vec![
            ModelCatalogEntry {
                id: "provider/z".into(),
                context_tokens: Some(8_192),
                max_output_tokens: Some(1_024),
            },
            ModelCatalogEntry {
                id: "provider/a".into(),
                context_tokens: Some(1_024),
                max_output_tokens: Some(256),
            },
            ModelCatalogEntry {
                id: "provider/a".into(),
                context_tokens: Some(4_096),
                max_output_tokens: Some(512),
            },
        ];
        let snapshot = ProviderCatalogSnapshot::fresh_from_catalog(
            &profile,
            &entries,
            3,
            "c".repeat(64),
            1_000,
            2_000,
        )
        .expect("snapshot");
        assert_eq!(
            snapshot
                .models
                .iter()
                .map(|model| model.model_id.as_str())
                .collect::<Vec<_>>(),
            vec!["provider/a", "provider/z"]
        );
        assert_eq!(snapshot.models[0].limits.context_tokens, Some(4_096));
        assert!(snapshot.route_eligible_at("provider/a", 1_500));
        assert!(!snapshot.route_eligible_at("provider/a", 2_000));

        let mut stale = snapshot;
        stale.state = ProviderCatalogState::Stale;
        assert!(stale.validate().is_ok());
        assert!(!stale.route_eligible_at("provider/a", 1_500));
    }

    #[test]
    fn stale_catalog_preserves_models_but_never_becomes_route_eligible() {
        let profile = profile();
        let fresh = ProviderCatalogSnapshot::fresh_from_catalog(
            &profile,
            &[ModelCatalogEntry {
                id: "provider/model".into(),
                context_tokens: Some(8_192),
                max_output_tokens: Some(1_024),
            }],
            1,
            "c".repeat(64),
            1_000,
            2_000,
        )
        .expect("fresh snapshot");
        let stale = ProviderCatalogSnapshot::stale_after_failure(
            &profile,
            &fresh,
            2,
            CatalogFailureCode::Timeout,
        )
        .expect("stale snapshot");
        assert_eq!(stale.state, ProviderCatalogState::Stale);
        assert_eq!(stale.failure, Some(CatalogFailureCode::Timeout));
        assert_eq!(stale.gateway_entries().expect("entries").len(), 1);
        assert!(!stale.route_eligible_at("provider/model", 1_500));
        let database = rusqlite::Connection::open_in_memory().expect("sqlite");
        evohime_local_storage::provider_profile_catalog_store::install_schema(&database)
            .expect("schema");
        let fresh_record = fresh.to_storage_record(&profile).expect("fresh record");
        assert!(evohime_local_storage::provider_profile_catalog_store::put(
            &database,
            &fresh_record
        )
        .expect("fresh write"));
        let record = stale.to_storage_record(&profile).expect("storage record");
        assert_eq!(record.state, "stale");
        assert_eq!(record.failure_code.as_deref(), Some("timeout"));
        assert!(
            evohime_local_storage::provider_profile_catalog_store::put(&database, &record)
                .expect("stale write")
        );
    }

    #[test]
    fn catalog_failures_are_typed_and_never_replay_provider_text() {
        let error = ProviderError::Api(
            "provider response body https://provider.test contains malformed JSON".into(),
        );
        assert_eq!(
            classify_catalog_error(&error),
            CatalogFailureCode::MalformedResponse
        );
        assert_eq!(
            classify_catalog_error(&ProviderError::Api(
                "provider model catalog request failed with HTTP 404".into()
            )),
            CatalogFailureCode::ModelNotFound
        );
        let encoded = serde_json::to_string(&classify_catalog_error(&error)).expect("code json");
        assert!(!encoded.contains("provider.test"));
        assert!(!encoded.contains("malformed JSON"));

        let failure = ProviderCatalogSnapshot::failure(
            &profile(),
            2,
            "d".repeat(64),
            ProviderCatalogState::CredentialRejected,
            CatalogFailureCode::CredentialRejected,
            1_000,
            2_000,
        )
        .expect("failure snapshot");
        assert!(failure.validate().is_ok());
        assert!(!failure.route_eligible_at("provider/model", 1_500));

        let mut inconsistent = failure;
        inconsistent.state = ProviderCatalogState::Fresh;
        assert_eq!(
            inconsistent.validate(),
            Err("inconsistent provider catalog snapshot")
        );
    }

    #[test]
    fn free_access_evidence_is_scoped_fresh_and_strict_only_when_verified() {
        let value = evidence();
        assert!(value.validate().is_ok());
        assert_eq!(value.freshness_at(1_500), EvidenceFreshness::Fresh);
        assert!(value.is_strictly_free_at(1_500));
        assert_eq!(value.freshness_at(2_000), EvidenceFreshness::Expired);
        assert!(!value.is_strictly_free_at(2_000));

        let mut trial = value.clone();
        trial.observed_state = ObservedFreeAccessState::TrialOnly;
        trial.allowance = AllowanceKind::TrialCredit;
        trial.activation = ActivationState::NotRequired;
        assert!(trial.validate().is_ok());
        assert!(!trial.is_strictly_free_at(1_500));
    }

    #[test]
    fn free_access_evidence_rejects_conflicting_semantics_and_raw_storage() {
        let mut invalid = evidence();
        invalid.observed_state = ObservedFreeAccessState::CreditOnly;
        assert_eq!(invalid.validate(), Err("inconsistent free access evidence"));

        let database = rusqlite::Connection::open_in_memory().expect("sqlite");
        evohime_local_storage::free_access_evidence_store::install_schema(&database)
            .expect("schema");
        let record = evidence().to_storage_record().expect("storage record");
        assert!(
            evohime_local_storage::free_access_evidence_store::put(&database, &record)
                .expect("evidence write")
        );
        let stored = evohime_local_storage::free_access_evidence_store::get(
            &database,
            "openrouter",
            "provider/model:free",
            "cred:openrouter",
            "global",
        )
        .expect("evidence read")
        .expect("stored evidence");
        let json = String::from_utf8(stored.evidence_json).expect("json");
        assert!(!json.contains("prompt"));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn sparse_samples_stay_unknown() {
        let s = ReliabilitySnapshot {
            provider_id: "p".into(),
            model_id: "m".into(),
            sample_count: 1,
            success_rate: 1.0,
            p50_ms: None,
            p95_ms: None,
            jitter_ms: None,
            class: ReliabilityClass::Unknown,
        };
        assert_eq!(classify(&s), ReliabilityClass::Unknown);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn reliability_snapshot_rejects_unbounded_or_inconsistent_metadata() {
        let mut invalid = ReliabilitySnapshot {
            provider_id: "provider".into(),
            model_id: "provider/model:free".into(),
            sample_count: 3,
            success_rate: 1.0,
            p50_ms: Some(100.0),
            p95_ms: Some(50.0),
            jitter_ms: Some(2.0),
            class: ReliabilityClass::Excellent,
        };
        assert_eq!(invalid.validate(), Err("invalid reliability snapshot"));

        invalid.p95_ms = Some(f64::NAN);
        assert_eq!(invalid.validate(), Err("invalid reliability snapshot"));

        invalid.p95_ms = Some(200.0);
        invalid.model_id = "model with spaces".into();
        assert_eq!(invalid.validate(), Err("invalid reliability snapshot"));

        invalid.model_id = "provider/model:free".into();
        invalid.class = ReliabilityClass::Healthy;
        assert_eq!(invalid.validate(), Err("invalid reliability snapshot"));
    }
}

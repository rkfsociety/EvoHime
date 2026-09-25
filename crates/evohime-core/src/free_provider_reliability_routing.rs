//! Bounded provider access/reliability metadata; gateway remains transport owner.
use evohime_model_gateway::config::OPENAI_DEFAULT_BASE_URL;
use evohime_model_gateway::providers::ProviderError;
use evohime_model_gateway::{
    ModelCatalogEntry, ModelRouteConfig, ProviderProfileId, RoutePreflight,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};

/// Stable contract identifier stored with provider reliability metadata.
pub const CONTRACT_ID: &str = "free-provider-reliability-routing-v1";
/// Schema version for persisted provider profiles.
pub const PROVIDER_PROFILE_SCHEMA_VERSION: u16 = 1;
/// Schema version for a normalized model descriptor.
pub const PROVIDER_MODEL_DESCRIPTOR_SCHEMA_VERSION: u16 = 1;
/// Maximum UTF-8 length of a provider profile identifier.
pub const MAX_PROVIDER_PROFILE_ID_BYTES: usize = 128;
/// Maximum UTF-8 length of a transport name.
pub const MAX_PROVIDER_PROFILE_TRANSPORT_BYTES: usize = 64;
/// Maximum UTF-8 length of a provider endpoint.
pub const MAX_PROVIDER_PROFILE_ENDPOINT_BYTES: usize = 512;
/// Maximum UTF-8 length of a provider region.
pub const MAX_PROVIDER_PROFILE_REGION_BYTES: usize = 64;
/// Maximum UTF-8 length of an opaque credential binding handle.
pub const MAX_PROVIDER_PROFILE_CREDENTIAL_BINDING_BYTES: usize = 128;
/// Maximum UTF-8 length of a model identifier.
pub const MAX_PROVIDER_MODEL_ID_BYTES: usize = 256;
/// Maximum accepted reliability latency sample in milliseconds.
pub const MAX_RELIABILITY_LATENCY_MS: f64 = 86_400_000.0;
/// Schema version for free-access evidence.
pub const FREE_ACCESS_EVIDENCE_SCHEMA_VERSION: u16 = 2;
/// Maximum number of limits carried by free-access evidence.
pub const MAX_FREE_ACCESS_LIMITS: usize = 16;
/// Maximum number of successful samples retained in evidence.
pub const MAX_FREE_ACCESS_SAMPLES: u32 = 256;
/// Maximum confidence value in basis points (100 percent).
pub const MAX_FREE_ACCESS_CONFIDENCE_BPS: u16 = 10_000;
/// Maximum lifetime of free-access evidence in milliseconds.
pub const MAX_FREE_ACCESS_TTL_MS: u64 = 31 * 24 * 60 * 60 * 1_000;
/// Maximum advertised capability flags for one model.
pub const MAX_PROVIDER_MODEL_CAPABILITIES: usize = 16;
/// Maximum entries accepted from one provider catalog response.
pub const MAX_PROVIDER_CATALOG_ENTRIES: usize = 2_048;
/// Schema version for provider catalog snapshots.
pub const PROVIDER_CATALOG_SCHEMA_VERSION: u16 = 1;
/// Maximum lifetime of a provider catalog snapshot in milliseconds.
pub const MAX_PROVIDER_CATALOG_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;

/// Shared, lock-protected provider catalog snapshots indexed by provider id.
pub type ProviderCatalogCache = Arc<std::sync::RwLock<HashMap<String, ProviderCatalogSnapshot>>>;
/// Shared, lock-protected free-access evidence indexed by provider/model scope.
pub type FreeAccessEvidenceCache = Arc<std::sync::RwLock<HashMap<String, FreeAccessEvidence>>>;

/// Creates an empty provider catalog cache.
pub fn new_provider_catalog_cache() -> ProviderCatalogCache {
    Arc::new(std::sync::RwLock::new(HashMap::new()))
}

/// Creates an empty free-access evidence cache.
pub fn new_free_access_evidence_cache() -> FreeAccessEvidenceCache {
    Arc::new(std::sync::RwLock::new(HashMap::new()))
}

/// Provider families recognized by the local routing metadata contract.
/// Privacy behavior declared or observed for provider processing.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFamily {
    /// OpenRouter hosted provider.
    OpenRouter,
    /// Groq hosted provider.
    Groq,
    /// Google Gemini provider.
    Gemini,
    /// Mistral hosted provider.
    Mistral,
    /// Cloudflare Workers AI provider.
    CloudflareWorkersAi,
    /// NVIDIA NIM provider.
    NvidiaNim,
    /// Cerebras hosted provider.
    Cerebras,
    /// Hugging Face provider.
    HuggingFace,
    /// LiteRouter provider.
    LiteRouter,
    /// OpenAI hosted provider.
    OpenAi,
    /// Ollama provider.
    Ollama,
    /// A local model provider not covered by another family.
    Local,
    /// A deterministic mock provider used in tests.
    Mock,
    /// Provider family is not known.
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

/// Wire protocol used to communicate with a model provider.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    /// Chat Completions-compatible transport.
    OpenAiCompatible,
    /// OpenAI Responses transport.
    OpenAiResponses,
    /// Ollama native transport.
    Ollama,
    /// Local in-process or local-server transport.
    Local,
    /// Test-only mock transport.
    Mock,
    /// Transport kind is not known.
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

/// Validated provider configuration without provider secrets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderProfile {
    /// Serialization schema version.
    #[serde(default = "default_provider_profile_schema_version")]
    pub schema_version: u16,
    /// Stable identifier of the configured provider.
    pub provider_id: String,
    /// Recognized provider family, if known.
    #[serde(default)]
    pub provider_family: ProviderFamily,
    /// Gateway transport name.
    pub transport: String,
    /// Normalized gateway transport kind.
    #[serde(default)]
    pub transport_kind: TransportKind,
    /// Provider endpoint; bounded and validated before persistence.
    pub endpoint: String,
    /// Provider region or deployment region.
    pub region: String,
    /// Opaque key binding handle; never contains credential material.
    pub credential_binding: String,
    /// Digest of the canonical profile fields.
    pub content_hash: String,
    /// Monotonic profile revision.
    #[serde(default = "default_revision")]
    pub revision: u64,
}

fn default_provider_profile_schema_version() -> u16 {
    PROVIDER_PROFILE_SCHEMA_VERSION
}

fn default_revision() -> u64 {
    1
}

/// Capabilities a model may advertise or have verified.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    /// Text/chat completion.
    Chat,
    /// Incremental response streaming.
    Streaming,
    /// Native tool/function calls.
    ToolCalls,
    /// Constrained structured output.
    StructuredOutput,
    /// Image or other visual input.
    Vision,
    /// Provider-supported reasoning mode.
    Reasoning,
}

/// Evidence state for one model capability.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    /// Capability is supported.
    Supported,
    /// Capability is explicitly not supported.
    Unsupported,
    /// Support has not been established.
    #[default]
    Unknown,
}

/// Origin of a model capability claim.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityProvenance {
    /// Capability was declared by the provider.
    ProviderDeclared,
    /// Capability was observed in a verified interaction.
    Observed,
    /// Origin has not been established.
    #[default]
    Unknown,
}

/// Capability value paired with its state and evidence origin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityFlag {
    /// Capability being described.
    pub capability: ModelCapability,
    /// Whether the capability is supported.
    pub state: CapabilityState,
    /// Source of the support claim.
    pub provenance: CapabilityProvenance,
}

/// Privacy behavior declared or observed for provider processing.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    /// Data is kept on the local machine.
    LocalOnly,
    /// Provider controls processing and retention.
    ProviderControlled,
    /// Provider may retain submitted data.
    ProviderRetained,
    /// Privacy behavior is unknown.
    #[default]
    Unknown,
}

/// Origin of token/usage measurements.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    /// Usage count was reported by the provider.
    ProviderReported,
    /// Usage count was measured by the gateway.
    GatewayMeasured,
    /// Usage source has not been established.
    #[default]
    Unknown,
}

/// Typed units and provenance for provider usage values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageMetadata {
    /// Unit used for input usage.
    pub input_unit: CreditUnit,
    /// Unit used for output usage.
    pub output_unit: CreditUnit,
    /// Source that supplied or measured the usage.
    pub source: UsageSource,
}

/// Known limits for one model; absent values mean the limit is unknown.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelLimits {
    /// Context window size in tokens, when known.
    pub context_tokens: Option<u32>,
    /// Maximum generated output in tokens, when known.
    pub max_output_tokens: Option<u32>,
}

/// Availability lifecycle of a provider model.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelLifecycle {
    /// Model is available for new calls.
    Active,
    /// Model remains known but should no longer be selected by default.
    Deprecated,
    /// Model is currently unavailable.
    Unavailable,
    /// Lifecycle state is unknown.
    #[default]
    Unknown,
}

/// One immutable model snapshot adapted from the gateway's canonical catalog
/// entry. It carries provenance and policy metadata, but never a raw response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderModelDescriptor {
    /// Descriptor schema version.
    pub schema_version: u16,
    /// Provider owning this model.
    pub provider_id: String,
    /// Normalized provider family.
    pub provider_family: ProviderFamily,
    /// Transport used to call the model.
    pub transport_kind: TransportKind,
    /// Provider's stable model identifier.
    pub model_id: String,
    /// Profile revision used to derive this descriptor.
    pub profile_revision: u64,
    /// Profile digest used to derive this descriptor.
    pub profile_content_hash: String,
    /// Catalog snapshot revision containing the model.
    pub catalog_revision: u64,
    /// Catalog snapshot digest containing the model.
    pub catalog_content_hash: String,
    /// Known context and output limits.
    pub limits: ModelLimits,
    /// Capability claims with their provenance.
    pub capabilities: Vec<CapabilityFlag>,
    /// Privacy classification for model requests.
    pub privacy: PrivacyClass,
    /// Usage units and measurement provenance.
    pub usage: UsageMetadata,
    /// Current model lifecycle state.
    pub lifecycle: ModelLifecycle,
}

/// Freshness or failure state of a provider catalog snapshot.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCatalogState {
    /// Catalog was fetched and validated successfully.
    Fresh,
    /// Last known catalog is available but past its freshness window.
    Stale,
    /// Catalog could not be fetched or validated.
    Unavailable,
    /// Provider rejected the credential binding.
    CredentialRejected,
    /// Provider does not support model discovery.
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

/// Stable failure category for provider catalog refresh.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CatalogFailureCode {
    /// Connection or network failure.
    Network,
    /// Provider did not respond within the allowed duration.
    Timeout,
    /// Provider rejected the configured credential.
    CredentialRejected,
    /// Provider rate limit prevented discovery.
    RateLimited,
    /// Requested model was not found.
    ModelNotFound,
    /// Provider response violated the expected format.
    MalformedResponse,
    /// Provider response exceeded the configured size bound.
    ResponseTooLarge,
    /// Catalog contained more entries than accepted.
    EntryLimitExceeded,
    /// Response protocol did not match the configured transport.
    ProtocolMismatch,
    /// Provider does not implement model discovery.
    DiscoveryUnsupported,
    /// Failure category is not classified.
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
    /// Catalog snapshot schema version.
    pub schema_version: u16,
    /// Provider whose models were observed.
    pub provider_id: String,
    /// Opaque credential binding that scoped the observation.
    pub credential_binding: String,
    /// Region used for model discovery.
    pub region: String,
    /// Provider profile revision used for the discovery request.
    pub profile_revision: u64,
    /// Digest of the provider profile used for discovery.
    pub profile_content_hash: String,
    /// Monotonic snapshot revision.
    pub revision: u64,
    /// Digest of the canonical catalog contents.
    pub catalog_content_hash: String,
    /// Refresh outcome and current freshness state.
    pub state: ProviderCatalogState,
    /// Validated model descriptors; bounded by `MAX_PROVIDER_CATALOG_ENTRIES`.
    pub models: Vec<ProviderModelDescriptor>,
    /// Time at which this catalog was observed in Unix milliseconds.
    pub observed_at_ms: u64,
    /// Time at which the snapshot expires in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Classified refresh failure, if the snapshot is unavailable.
    pub failure: Option<CatalogFailureCode>,
}
/// Coarse provider-declared free-access label retained for compatibility.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FreeAccessState {
    /// Provider advertises recurring free access.
    Free,
    /// Provider advertises free usage subject to limits.
    FreeTierLimited,
    /// Access depends on temporary trial credits.
    TrialCredits,
    /// Access requires payment.
    Paid,
    /// Access conditions are unknown.
    Unknown,
    /// Access conditions are experimental or unstable.
    Experimental,
    /// A previous advisory state requires a refresh before use.
    UnknownNeedsRefresh,
}

/// Evidence state is deliberately more precise than the historical advisory
/// `FreeAccessState`: trial credit, one-time credit and recurring free access
/// must never collapse into one boolean.
///
/// This state is derived from validated observations and is used by strict
/// free-only routing checks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservedFreeAccessState {
    /// Recurring free access was successfully observed.
    VerifiedFreeLimited,
    /// Access is available only through a time-limited trial.
    TrialOnly,
    /// Access is available only through a finite one-time credit.
    CreditOnly,
    /// The account must complete an activation step.
    ActivationRequired,
    /// Only paid access was observed.
    PaidOnly,
    /// Available evidence does not establish the access type.
    Unknown,
}

/// State of any provider-side activation required before access.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivationState {
    /// No activation step is required.
    NotRequired,
    /// Activation is required but has not completed.
    Required,
    /// Required activation has completed.
    Completed,
    /// Activation requirements are unknown.
    Unknown,
}

/// Kind of allowance observed for a provider account or model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AllowanceKind {
    /// Allowance renews on a recurring schedule.
    Recurring,
    /// Allowance is a time-limited trial credit.
    TrialCredit,
    /// Allowance is a finite one-time credit.
    OneTimeCredit,
    /// No free allowance was observed.
    None,
    /// Allowance kind is unknown.
    Unknown,
}

/// Typed authority used to classify a recurring no-cost allowance.
///
/// The source digest covers only normalized provider pricing metadata and is
/// never a digest of a credential or raw response body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum AllowanceProvenance {
    /// No trusted provider signal established the allowance kind.
    Unknown,
    /// A fixed OpenRouter model-detail response reported zero for every price dimension.
    OpenRouterModelPricing {
        /// SHA-256 digest of the normalized typed price dimensions.
        source_hash: String,
    },
}

/// Units remain typed and opaque. In particular, credits are never converted
/// to tokens or currency without an authoritative provider contract.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreditUnit {
    /// Number of requests.
    Requests,
    /// Number of model tokens.
    Tokens,
    /// Number of text characters.
    Characters,
    /// Duration in seconds.
    Seconds,
    /// Currency amount in micro-units.
    CurrencyMicros,
    /// Unit is not known.
    Unknown,
}

/// Scope to which an observed access limit applies.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLimitScope {
    /// Limit applies to the provider account.
    Account,
    /// Limit applies across a provider.
    Provider,
    /// Limit applies to one model.
    Model,
}

/// Source of an observed allowance limit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLimitSource {
    /// Limit was explicitly declared by the provider.
    ProviderDeclared,
    /// Limit was inferred from observed usage.
    Observed,
    /// Limit source is unknown.
    Unknown,
}

/// Event that invalidates previously collected free-access evidence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceInvalidation {
    /// Billing became required.
    BillingRequired,
    /// Provider restricted the account.
    AccountRestricted,
    /// Provider quota was exhausted.
    QuotaExhausted,
    /// Provider model catalog changed.
    CatalogChanged,
    /// Credential binding changed.
    CredentialChanged,
    /// Evidence was invalidated manually.
    Manual,
}

/// Time and invalidation status of evidence at a particular observation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceFreshness {
    /// Evidence remains within its validity interval.
    Fresh,
    /// Current time precedes the observation timestamp.
    Stale,
    /// Evidence passed its expiration timestamp.
    Expired,
    /// Evidence was explicitly invalidated.
    Invalidated,
}

/// One typed provider access limit with its source and observation time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FreeAccessLimit {
    /// Scope to which this limit applies.
    pub scope: EvidenceLimitScope,
    /// Origin of the limit value.
    pub source: EvidenceLimitSource,
    /// Unit used by `limit` and `remaining`.
    pub unit: CreditUnit,
    /// Whether the allowance renews, is finite, or is absent.
    pub allowance: AllowanceKind,
    /// Total allowance, if reported or measured.
    pub limit: Option<u64>,
    /// Remaining allowance at observation time, if known.
    pub remaining: Option<u64>,
    /// Observation time in Unix milliseconds.
    pub observed_at_ms: u64,
    /// Next reset time in Unix milliseconds, if known.
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
    /// Evidence serialization schema version.
    pub schema_version: u16,
    /// Provider that was observed.
    pub provider_id: String,
    /// Model that was observed.
    pub model_id: String,
    /// Opaque credential scope; never credential material.
    pub credential_binding: String,
    /// Region associated with the observation.
    pub region: String,
    /// Revision of the provider profile used for the observation.
    pub profile_revision: u64,
    /// Canonical content hash of the provider profile used for the observation.
    pub profile_content_hash: String,
    /// Coarse provider-advertised access label.
    pub advertised_state: FreeAccessState,
    /// More precise access state derived from validated evidence.
    pub observed_state: ObservedFreeAccessState,
    /// Activation requirement/state at observation time.
    pub activation: ActivationState,
    /// Kind of access allowance established by the evidence.
    pub allowance: AllowanceKind,
    /// Authority supporting the allowance classification.
    pub allowance_provenance: AllowanceProvenance,
    /// Typed account/provider/model limits observed.
    pub limits: Vec<FreeAccessLimit>,
    /// Number of successful calls supporting the observation.
    pub successful_sample_count: u32,
    /// Confidence score in basis points from 0 through 10,000.
    pub confidence_bps: u16,
    /// Evidence observation time in Unix milliseconds.
    pub observed_at_ms: u64,
    /// Evidence expiration time in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Reason the evidence was invalidated, when applicable.
    pub invalidation: Option<EvidenceInvalidation>,
    /// Bounded failure reason; must not contain credentials or raw payloads.
    pub failure_reason: Option<String>,
    /// Digest of the canonical evidence content.
    pub content_hash: String,
    /// Monotonic evidence revision.
    pub revision: u64,
}

impl FreeAccessEvidence {
    /// Validates schema, bounds, timestamps, canonical digest, and state consistency.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != FREE_ACCESS_EVIDENCE_SCHEMA_VERSION
            || !valid_profile_token(&self.provider_id, MAX_PROVIDER_PROFILE_ID_BYTES)
            || !valid_provider_model_id(&self.provider_id, &self.model_id)
            || !valid_bound_credential(&self.credential_binding)
            || !valid_profile_token(&self.region, MAX_PROVIDER_PROFILE_REGION_BYTES)
            || self.profile_revision == 0
            || !valid_content_hash(&self.profile_content_hash)
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
            || match &self.allowance_provenance {
                AllowanceProvenance::Unknown => false,
                AllowanceProvenance::OpenRouterModelPricing { source_hash } => {
                    !valid_content_hash(source_hash)
                }
            }
        {
            return Err("invalid free access evidence");
        }

        let consistent = match self.observed_state {
            ObservedFreeAccessState::VerifiedFreeLimited => {
                self.allowance == AllowanceKind::Recurring
                    && self.successful_sample_count > 0
                    && self.provider_id == "openrouter"
                    && matches!(
                        &self.allowance_provenance,
                        AllowanceProvenance::OpenRouterModelPricing { .. }
                    )
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
        if self.content_hash != self.calculate_content_hash()? {
            return Err("free access evidence content hash mismatch");
        }
        Ok(())
    }

    /// Computes the canonical digest after clearing its self-referential field.
    pub fn calculate_content_hash(&self) -> Result<String, &'static str> {
        let mut canonical = self.clone();
        canonical.content_hash.clear();
        let bytes = serde_json::to_vec(&canonical).map_err(|_| "invalid free access evidence")?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }

    /// Seals an evidence value with the digest of its canonical representation.
    pub fn seal(mut self) -> Result<Self, &'static str> {
        self.content_hash.clear();
        self.content_hash = self.calculate_content_hash()?;
        self.validate()?;
        Ok(self)
    }

    /// Confirms that this evidence belongs to the current provider profile.
    pub fn matches_profile(&self, profile: &ProviderProfile) -> bool {
        self.provider_id == profile.provider_id
            && self.credential_binding == profile.credential_binding
            && self.region == profile.region
            && self.profile_revision == profile.revision
            && self.profile_content_hash == profile.content_hash
    }

    /// Classifies evidence at `now_ms`, prioritizing explicit invalidation.
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
    ///
    /// This predicate deliberately requires successful observations and a
    /// recurring allowance; the coarse provider-advertised label alone is
    /// insufficient.
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

    /// Converts validated evidence into the local storage row representation.
    ///
    /// Returns an error for invalid evidence or values that cannot fit the
    /// storage schema's signed integer timestamps/revision fields.
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

    /// Loads and cross-checks evidence against its storage row metadata.
    ///
    /// A mismatch in provider/model scope, revision, digest, timestamps, or
    /// invalidation state is rejected instead of returning a partially trusted
    /// record.
    pub fn from_storage_record(
        record: &evohime_local_storage::free_access_evidence_store::FreeAccessEvidenceRecord,
    ) -> Result<Self, &'static str> {
        if record.revision <= 0
            || record.observed_at_ms <= 0
            || record.expires_at_ms <= record.observed_at_ms
        {
            return Err("invalid free access evidence");
        }
        let evidence: Self = serde_json::from_slice(&record.evidence_json)
            .map_err(|_| "invalid free access evidence")?;
        if evidence.provider_id != record.provider_id
            || evidence.model_id != record.model_id
            || evidence.credential_binding != record.credential_binding
            || evidence.region != record.region
            || i64::try_from(evidence.revision).ok() != Some(record.revision)
            || evidence.content_hash != record.content_hash
            || i64::try_from(evidence.observed_at_ms).ok() != Some(record.observed_at_ms)
            || i64::try_from(evidence.expires_at_ms).ok() != Some(record.expires_at_ms)
            || evidence.invalidation.map(invalidation_code) != record.invalidation.as_deref()
        {
            return Err("free access evidence scope mismatch");
        }
        evidence.validate()?;
        Ok(evidence)
    }
}

fn invalidation_code(value: EvidenceInvalidation) -> &'static str {
    match value {
        EvidenceInvalidation::BillingRequired => "billing_required",
        EvidenceInvalidation::AccountRestricted => "account_restricted",
        EvidenceInvalidation::QuotaExhausted => "quota_exhausted",
        EvidenceInvalidation::CatalogChanged => "catalog_changed",
        EvidenceInvalidation::CredentialChanged => "credential_changed",
        EvidenceInvalidation::Manual => "manual",
    }
}

fn valid_content_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
/// Coarse reliability category used to compare candidate model routes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityClass {
    /// Repeated observations show consistently fast successful responses.
    Excellent,
    /// Responses meet the healthy reliability thresholds.
    Healthy,
    /// Recent latency or error rate is worse than the healthy threshold.
    Degraded,
    /// Repeated observations show unstable latency or success rate.
    Unstable,
    /// Route is temporarily held in cooldown.
    CoolingDown,
    /// Provider quota currently restricts use.
    QuotaLimited,
    /// Route is known to be unavailable.
    Unavailable,
    /// Insufficient evidence exists to classify reliability.
    Unknown,
}
/// Reliability measurements for one provider/model pair.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReliabilitySnapshot {
    /// Provider owning the observed model.
    pub provider_id: String,
    /// Model identifier for the observations.
    pub model_id: String,
    /// Number of samples included in the statistics.
    pub sample_count: u32,
    /// Fraction of successful requests in the sample, from 0.0 through 1.0.
    pub success_rate: f64,
    /// 50th-percentile response latency in milliseconds, when available.
    pub p50_ms: Option<f64>,
    /// 95th-percentile response latency in milliseconds, when available.
    pub p95_ms: Option<f64>,
    /// Latency variation in milliseconds, when available.
    pub jitter_ms: Option<f64>,
    /// Derived reliability class.
    pub class: ReliabilityClass,
}
/// Human-readable reason why a provider/model route was selected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteSelectionExplanation {
    /// Selected provider identifier.
    pub provider_id: String,
    /// Selected model identifier.
    pub model_id: String,
    /// Stable route-selection rationale.
    pub reason: String,
    /// Free-access state associated with the candidate.
    pub free_state: FreeAccessState,
    /// Reliability category associated with the candidate.
    pub reliability: ReliabilityClass,
}

impl ProviderProfile {
    /// Returns whether the profile has a per-key opaque binding suitable for
    /// probe evidence. Legacy or synthetic built-in bindings are excluded.
    pub fn has_stable_credential_binding(&self) -> bool {
        valid_bound_credential(&self.credential_binding)
    }

    /// Adapts a configured gateway route into validated provider metadata.
    ///
    /// Explicit profile identity is authoritative; exact endpoint matching is
    /// retained only for routes serialized before profile IDs were introduced.
    pub fn from_route_config(route: &ModelRouteConfig) -> Result<Self, &'static str> {
        route
            .validate_provider_profile()
            .map_err(|_| "invalid provider profile")?;
        let (provider_id, provider_family, transport_kind) = match route.provider {
            evohime_model_gateway::providers::ProviderKind::LiteRouter => (
                "literouter",
                ProviderFamily::LiteRouter,
                TransportKind::OpenAiCompatible,
            ),
            evohime_model_gateway::providers::ProviderKind::OpenAICompatible => (
                "custom_openai_compatible",
                ProviderFamily::Unknown,
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
                if let Some(profile_id) = route.provider_profile_id {
                    if profile_id == ProviderProfileId::Custom {
                        (
                            "custom_openai_compatible".to_owned(),
                            ProviderFamily::Unknown,
                        )
                    } else if profile_id == ProviderProfileId::OpenAi {
                        ("openai".to_owned(), ProviderFamily::OpenAi)
                    } else {
                        let profile_id = profile_id.as_str();
                        builtin_provider_profiles()
                            .into_iter()
                            .find(|profile| profile.provider_id == profile_id)
                            .map(|profile| (profile.provider_id, profile.provider_family))
                            .ok_or("invalid provider profile")?
                    }
                } else {
                    let endpoint = route.literouter.base_url.trim_end_matches('/');
                    builtin_provider_profiles()
                        .into_iter()
                        .find(|profile| profile.endpoint.trim_end_matches('/') == endpoint)
                        .map(|profile| (profile.provider_id, profile.provider_family))
                        .or_else(|| {
                            (endpoint == OPENAI_DEFAULT_BASE_URL)
                                .then(|| ("openai".to_owned(), ProviderFamily::OpenAi))
                        })
                        .unwrap_or_else(|| (provider_id.to_owned(), provider_family))
                }
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
        let credential_binding = route
            .provider_credential_binding
            .clone()
            .unwrap_or_else(|| format!("unbound:{provider_id}"));
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

    /// Checks schema, identity, endpoint, credential binding, and content hash.
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

    /// Returns the explicit transport, or infers it from the legacy transport name.
    pub fn resolved_transport_kind(&self) -> TransportKind {
        if self.transport_kind != TransportKind::Unknown {
            return self.transport_kind;
        }
        parsed_transport_kind(&self.transport)
    }

    /// Serializes this profile and its model descriptors into the storage record shape.
    ///
    /// Rejects mismatched provider/profile revisions and invalid catalog metadata.
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
    /// Creates a descriptor from a gateway catalog entry and its source revisions.
    ///
    /// Capability, privacy, usage, and lifecycle metadata remain unknown until
    /// separately established by trusted evidence.
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

    /// Checks descriptor identity, hashes, limits, and unique capability flags.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != PROVIDER_MODEL_DESCRIPTOR_SCHEMA_VERSION
            || !valid_profile_token(&self.provider_id, MAX_PROVIDER_PROFILE_ID_BYTES)
            || !valid_provider_model_id(&self.provider_id, &self.model_id)
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
    /// Builds a fresh, normalized snapshot from a successfully discovered catalog.
    ///
    /// Duplicate model IDs are collapsed by [`normalize_catalog_entries`].
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

    /// Builds a failed catalog snapshot with no model entries.
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

    /// Checks snapshot bounds, revisions, model consistency, and state/failure pairing.
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

    /// Returns whether a named model may route under this fresh snapshot at `now_ms`.
    pub fn route_eligible_at(&self, model_id: &str, now_ms: u64) -> bool {
        self.validate().is_ok()
            && self.state == ProviderCatalogState::Fresh
            && now_ms >= self.observed_at_ms
            && now_ms < self.expires_at_ms
            && self.models.iter().any(|model| model.model_id == model_id)
    }

    /// Converts validated descriptors back to the gateway catalog representation.
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

    /// Retains a previous model list as stale evidence after a refresh failure.
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

    /// Encodes the snapshot together with its matching profile for local storage.
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

    /// Reconstructs and validates a snapshot from a persisted profile/catalog record.
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
        "{}|{}|{}|{}|{}",
        profile.provider_id,
        profile.credential_binding,
        profile.region,
        profile.revision,
        profile.content_hash
    )
}

pub(crate) fn free_access_evidence_scope_key(
    provider_id: &str,
    model_id: &str,
    credential_binding: &str,
    region: &str,
    profile_revision: u64,
    profile_content_hash: &str,
) -> String {
    format!(
        "{provider_id}|{model_id}|{credential_binding}|{region}|{profile_revision}|{profile_content_hash}"
    )
}

/// Adapter between the Core-owned catalog lifecycle and the gateway's final
/// route dispatch boundary. An absent snapshot is treated as unobserved (the
/// gateway still performs its configured-provider checks); a known stale,
/// expired or failed snapshot is never allowed to reach the provider.
pub struct ProviderCatalogRoutePreflight {
    config: evohime_model_gateway::ModelGatewayConfig,
    cache: ProviderCatalogCache,
    free_access_mode: crate::free_access_probe::FreeAccessRoutingMode,
    allow_paid_fallback: bool,
    free_access_probe: Option<Arc<crate::free_access_probe::FreeAccessProbeCoordinator>>,
    free_access_evidence: Option<FreeAccessEvidenceCache>,
}

impl ProviderCatalogRoutePreflight {
    /// Creates a dispatch preflight backed by route configuration and catalog cache.
    pub fn new(
        config: evohime_model_gateway::ModelGatewayConfig,
        cache: ProviderCatalogCache,
    ) -> Self {
        Self {
            config,
            cache,
            free_access_mode: crate::free_access_probe::FreeAccessRoutingMode::Any,
            allow_paid_fallback: false,
            free_access_probe: None,
            free_access_evidence: None,
        }
    }

    /// Applies explicit free-access routing and consented probe policy.
    pub fn with_free_access_policy(
        mut self,
        mode: crate::free_access_probe::FreeAccessRoutingMode,
        allow_paid_fallback: bool,
        probe: Option<Arc<crate::free_access_probe::FreeAccessProbeCoordinator>>,
        free_access_evidence: FreeAccessEvidenceCache,
    ) -> Self {
        self.free_access_mode = mode;
        self.allow_paid_fallback = allow_paid_fallback;
        self.free_access_probe = probe;
        self.free_access_evidence = Some(free_access_evidence);
        self
    }
}

impl RoutePreflight for ProviderCatalogRoutePreflight {
    fn check(&self, route: &str, model: Option<&str>, now_ms: u64) -> Result<(), ProviderError> {
        self.check_with_requirements(route, model, false, now_ms)
    }

    fn check_for_request(
        &self,
        route: &str,
        model: Option<&str>,
        requires_tool_calls: bool,
        now_ms: u64,
    ) -> Result<(), ProviderError> {
        self.check_with_requirements(route, model, requires_tool_calls, now_ms)
    }

    fn observe_success_async<'a>(
        &'a self,
        route: &'a str,
        model: Option<&'a str>,
        _result: &'a evohime_model_gateway::ChatResult,
        now_ms: u64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        let Some(probe) = self.free_access_probe.clone() else {
            return Box::pin(async {});
        };
        let Some(route_config) = self.config.routes.get(route) else {
            return Box::pin(async {});
        };
        let model = model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| route_config.literouter.model.trim())
            .to_owned();
        let route = route.to_owned();
        Box::pin(async move {
            probe.record_passive_success(&route, &model, now_ms).await;
        })
    }

    fn observe_stream_success_async<'a>(
        &'a self,
        route: &'a str,
        model: Option<&'a str>,
        now_ms: u64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        let Some(probe) = self.free_access_probe.clone() else {
            return Box::pin(async {});
        };
        let Some(route_config) = self.config.routes.get(route) else {
            return Box::pin(async {});
        };
        let model = model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| route_config.literouter.model.trim())
            .to_owned();
        let route = route.to_owned();
        Box::pin(async move {
            probe.record_passive_success(&route, &model, now_ms).await;
        })
    }

    fn requires_verified_free_route(&self) -> bool {
        self.free_access_mode == crate::free_access_probe::FreeAccessRoutingMode::FreeOnly
    }

    fn prefers_verified_free_routes(&self) -> bool {
        self.free_access_mode == crate::free_access_probe::FreeAccessRoutingMode::PreferFree
    }

    fn is_verified_free_route(&self, route: &str, model: &str, now_ms: u64) -> bool {
        let Some(route_config) = self.config.routes.get(route) else {
            return false;
        };
        let Ok(profile) = ProviderProfile::from_route_config(route_config) else {
            return false;
        };
        let key = free_access_evidence_scope_key(
            &profile.provider_id,
            model,
            &profile.credential_binding,
            &profile.region,
            profile.revision,
            &profile.content_hash,
        );
        self.free_access_evidence
            .as_ref()
            .and_then(|cache| cache.read().ok()?.get(&key).cloned())
            .is_some_and(|evidence| {
                evidence.matches_profile(&profile) && evidence.is_strictly_free_at(now_ms)
            })
    }
}

impl ProviderCatalogRoutePreflight {
    fn check_with_requirements(
        &self,
        route: &str,
        model: Option<&str>,
        requires_tool_calls: bool,
        now_ms: u64,
    ) -> Result<(), ProviderError> {
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
        if let Some(snapshot) = snapshot {
            let catalog_result = match snapshot.state {
                ProviderCatalogState::Fresh if snapshot.route_eligible_at(model_id, now_ms) => {
                    let descriptor = snapshot
                        .models
                        .iter()
                        .find(|candidate| candidate.model_id == model_id);
                    if descriptor.is_some_and(|descriptor| {
                        has_confirmed_unsupported_capability(descriptor, ModelCapability::Chat)
                            || (requires_tool_calls
                                && has_confirmed_unsupported_capability(
                                    descriptor,
                                    ModelCapability::ToolCalls,
                                ))
                    }) {
                        Err(ProviderError::Config(
                            "provider_model_capability_unsupported".into(),
                        ))
                    } else {
                        Ok(())
                    }
                }
                ProviderCatalogState::Fresh if now_ms >= snapshot.expires_at_ms => {
                    Err(ProviderError::Config("provider_catalog_expired".into()))
                }
                ProviderCatalogState::Fresh => {
                    Err(ProviderError::Config("provider_model_not_found".into()))
                }
                ProviderCatalogState::Stale => {
                    Err(ProviderError::Config("provider_catalog_stale".into()))
                }
                ProviderCatalogState::CredentialRejected => {
                    Err(ProviderError::Config("provider_credential_rejected".into()))
                }
                ProviderCatalogState::Unavailable
                    if snapshot.failure == Some(CatalogFailureCode::ModelNotFound) =>
                {
                    Err(ProviderError::Config("provider_model_not_found".into()))
                }
                ProviderCatalogState::Unavailable => {
                    Err(ProviderError::Config("provider_catalog_unavailable".into()))
                }
                // Catalog discovery is advisory. If the user supplied an explicit
                // model ID, a missing discovery endpoint does not prove that the
                // completion endpoint cannot serve it.
                ProviderCatalogState::DiscoveryUnsupported => Ok(()),
            };
            catalog_result?;
        }
        if let Some(probe) = &self.free_access_probe {
            probe.on_route_use(route, Some(model_id));
        }
        let verified_free = self.is_verified_free_route(route, model_id, now_ms);
        match self.free_access_mode {
            crate::free_access_probe::FreeAccessRoutingMode::Any => Ok(()),
            crate::free_access_probe::FreeAccessRoutingMode::FreeOnly if verified_free => Ok(()),
            crate::free_access_probe::FreeAccessRoutingMode::FreeOnly => {
                Err(ProviderError::Config("free_access_not_verified".into()))
            }
            crate::free_access_probe::FreeAccessRoutingMode::PreferFree if verified_free => Ok(()),
            crate::free_access_probe::FreeAccessRoutingMode::PreferFree
                if !model_id.ends_with(":free") && self.allow_paid_fallback =>
            {
                Ok(())
            }
            crate::free_access_probe::FreeAccessRoutingMode::PreferFree => Err(
                ProviderError::Config("free_access_fallback_not_allowed".into()),
            ),
        }
    }
}

fn has_confirmed_unsupported_capability(
    descriptor: &ProviderModelDescriptor,
    capability: ModelCapability,
) -> bool {
    descriptor.capabilities.iter().any(|flag| {
        flag.capability == capability
            && flag.state == CapabilityState::Unsupported
            && matches!(
                flag.provenance,
                CapabilityProvenance::ProviderDeclared | CapabilityProvenance::Observed
            )
    })
}

/// Sorts catalog entries deterministically and keeps one entry per model ID.
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

/// Computes the SHA-256 digest of the normalized catalog JSON representation.
pub fn catalog_content_hash(entries: &[ModelCatalogEntry]) -> Result<String, &'static str> {
    let normalized = normalize_catalog_entries(entries)?;
    let json = serde_json::to_vec(&normalized).map_err(|_| "invalid provider catalog snapshot")?;
    Ok(hex::encode(Sha256::digest(json)))
}

/// Maps a gateway error to a stable, non-sensitive catalog failure category.
pub fn classify_catalog_error(error: &ProviderError) -> CatalogFailureCode {
    let message = match error {
        ProviderError::Config(message)
        | ProviderError::Http(message)
        | ProviderError::Api(message)
        | ProviderError::Stream(message) => message.to_ascii_lowercase(),
    };
    match error {
        ProviderError::Config(_)
            if message.contains("model_not_found") || message.contains("model not found") =>
        {
            CatalogFailureCode::ModelNotFound
        }
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
            if message.contains("model not found") || message.contains("model_not_found") =>
        {
            CatalogFailureCode::ModelNotFound
        }
        ProviderError::Api(_) if message.contains("404") => {
            CatalogFailureCode::DiscoveryUnsupported
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
            "https://api.cloudflare.com/client/v4/accounts/00000000000000000000000000000000/ai/v1",
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

fn valid_bound_credential(value: &str) -> bool {
    let Some(id) = value.strip_prefix("credential:") else {
        return false;
    };
    id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}
impl ReliabilitySnapshot {
    /// Checks bounded metrics and verifies that the stored class matches the metrics.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_profile_token(&self.provider_id, MAX_PROVIDER_PROFILE_ID_BYTES)
            || !valid_provider_model_id(&self.provider_id, &self.model_id)
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

fn valid_provider_model_id(provider_id: &str, value: &str) -> bool {
    if provider_id == "cloudflare_workers_ai" {
        return value.strip_prefix("@cf/").is_some_and(valid_model_id);
    }
    valid_model_id(value)
}

fn valid_latency(value: Option<f64>) -> bool {
    value.is_none_or(|value| {
        value.is_finite() && (0.0..=MAX_RELIABILITY_LATENCY_MS).contains(&value)
    })
}

/// Classifies reliability using the contract's sample-count and success-rate thresholds.
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
#[path = "free_provider_reliability_routing_tests.rs"]
mod tests;

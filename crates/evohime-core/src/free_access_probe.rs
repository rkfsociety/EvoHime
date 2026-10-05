//! Bounded, consent-gated verification of provider free-access claims.

use crate::free_provider_reliability_routing::{
    free_access_evidence_scope_key, ActivationState, AllowanceKind, AllowanceProvenance,
    EvidenceFreshness, EvidenceInvalidation, FreeAccessEvidence, FreeAccessEvidenceCache,
    FreeAccessState, ObservedFreeAccessState, ProviderProfile, FREE_ACCESS_EVIDENCE_SCHEMA_VERSION,
    MAX_FREE_ACCESS_SAMPLES,
};
use evohime_model_gateway::{
    providers::{ChatMessage, ChatRole, ProviderError},
    ChatRequestOptions, ChatResult, ModelGateway, ModelRouteConfig, OpenRouterPricingError,
    OpenRouterPricingObservation, ToolSpec,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

/// Maximum number of model routes that may have a process-local probe cooldown.
const MAX_TRACKED_PROBE_SCOPES: usize = 1_024;
/// Per-scope minimum interval between manual or automatic probes.
const FREE_ACCESS_PROBE_COOLDOWN_MS: u64 = 60_000;
/// Maximum completion tokens requested from a probe model.
const FREE_ACCESS_PROBE_MAX_OUTPUT_TOKENS: u32 = 8;
/// TTL assigned to a verified free-access result.
const FREE_ACCESS_PROBE_TTL_MS: u64 = 24 * 60 * 60 * 1_000;
/// Short TTL for a failed or ambiguous probe observation.
const FREE_ACCESS_PROBE_FAILURE_TTL_MS: u64 = 5 * 60 * 1_000;

/// Persisted choices for how and when free-access evidence may be collected.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FreeAccessProbePolicy {
    /// No probe or passive observation runs by default.
    #[default]
    Disabled,
    /// Record observations from user-requested calls without synthetic requests.
    PassiveOnly,
    /// Probe once after the selected route is first used.
    OnFirstUse,
    /// Probe a bounded set of routes on a long interval.
    PeriodicBounded,
    /// Probe only after a one-shot user action.
    ManualOnly,
}

impl FreeAccessProbePolicy {
    /// Parses the stable environment/config spelling, failing closed to disabled.
    pub fn parse(value: &str) -> Self {
        match value {
            "passive_only" => Self::PassiveOnly,
            "on_first_use" => Self::OnFirstUse,
            "periodic_bounded" => Self::PeriodicBounded,
            "manual_only" => Self::ManualOnly,
            _ => Self::Disabled,
        }
    }

    /// Returns the stable serialized spelling.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::PassiveOnly => "passive_only",
            Self::OnFirstUse => "on_first_use",
            Self::PeriodicBounded => "periodic_bounded",
            Self::ManualOnly => "manual_only",
        }
    }
}

/// Route handling selected independently from provider catalog display filters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FreeAccessRoutingMode {
    /// Use the existing model route policy without a free-access constraint.
    #[default]
    Any,
    /// Prefer confirmed recurring free routes; paid use needs a separate opt-in.
    PreferFree,
    /// Dispatch only routes with fresh confirmed recurring free evidence.
    FreeOnly,
}

impl FreeAccessRoutingMode {
    /// Parses the stable shell environment spelling, defaulting to ordinary routing.
    pub fn parse(value: &str) -> Self {
        match value {
            "prefer_free" => Self::PreferFree,
            "free_only" => Self::FreeOnly,
            _ => Self::Any,
        }
    }
}

/// Core-owned coordinator for manual, passive, first-use, and periodic evidence.
pub struct FreeAccessProbeCoordinator {
    config: evohime_model_gateway::ModelGatewayConfig,
    journal: crate::EventJournal,
    cache: FreeAccessEvidenceCache,
    guard: Arc<FreeAccessProbeGuard>,
    policy: FreeAccessProbePolicy,
    consent_binding: Option<String>,
    cancellation: CancellationToken,
    automatic_attempts: std::sync::Mutex<HashSet<String>>,
}

impl FreeAccessProbeCoordinator {
    /// Creates a process-local coordinator; in-flight probes are intentionally not recovered.
    pub fn new(
        config: evohime_model_gateway::ModelGatewayConfig,
        journal: crate::EventJournal,
        cache: FreeAccessEvidenceCache,
        guard: Arc<FreeAccessProbeGuard>,
        policy: FreeAccessProbePolicy,
        consent_binding: Option<String>,
        cancellation: CancellationToken,
    ) -> Arc<Self> {
        Arc::new(Self {
            config,
            journal,
            cache,
            guard,
            policy,
            consent_binding,
            cancellation,
            automatic_attempts: std::sync::Mutex::new(HashSet::new()),
        })
    }

    /// Returns whether this route/model has current authoritative recurring-free evidence.
    /// Starts one consented first-use probe in the background when evidence is absent or stale.
    pub(crate) fn on_route_use(self: &Arc<Self>, route_name: &str, model_id: Option<&str>) {
        if self.policy != FreeAccessProbePolicy::OnFirstUse || self.cancellation.is_cancelled() {
            return;
        }
        let Some((route, profile, model)) = self.route_scope(route_name, model_id) else {
            return;
        };
        if !self.has_persistent_consent(&route) {
            return;
        }
        if self
            .cached_evidence(&profile, &model)
            .is_some_and(|evidence| {
                evidence.freshness_at(crate::task_memory::now_millis()) == EvidenceFreshness::Fresh
            })
        {
            return;
        }
        let attempt_key = profile_scope_key(&profile);
        let Ok(mut attempted) = self.automatic_attempts.lock() else {
            return;
        };
        if !attempted.insert(attempt_key) {
            return;
        }
        drop(attempted);
        let coordinator = Arc::clone(self);
        let route_name = route_name.to_owned();
        tokio::spawn(async move {
            coordinator.run_probe_and_publish(&route_name, &model).await;
        });
    }

    /// Runs bounded periodic refreshes for the configured default route until shutdown.
    pub async fn run_periodic(self: Arc<Self>) {
        if self.policy != FreeAccessProbePolicy::PeriodicBounded {
            return;
        }
        loop {
            if self.cancellation.is_cancelled() {
                return;
            }
            let route_name = self.config.default_route.clone();
            let Some(route) = self.config.routes.get(&route_name) else {
                return;
            };
            if !self.has_persistent_consent(route) {
                return;
            }
            let model = route.literouter.model.clone();
            if self
                .cached_evidence_for_route(&route_name, &model)
                .is_some_and(|evidence| {
                    evidence.freshness_at(crate::task_memory::now_millis())
                        == EvidenceFreshness::Fresh
                })
            {
                tokio::select! {
                    _ = self.cancellation.cancelled() => return,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(FREE_ACCESS_PROBE_TTL_MS)) => {}
                }
                continue;
            }
            self.run_probe_and_publish(&route_name, &model).await;
            tokio::select! {
                _ = self.cancellation.cancelled() => return,
                _ = tokio::time::sleep(std::time::Duration::from_millis(FREE_ACCESS_PROBE_TTL_MS)) => {}
            }
        }
    }

    /// Records a successful real request without making a synthetic provider call.
    pub(crate) async fn record_passive_success(
        &self,
        route_name: &str,
        model_id: &str,
        now_ms: u64,
    ) {
        if self.policy != FreeAccessProbePolicy::PassiveOnly || self.cancellation.is_cancelled() {
            return;
        }
        let Some((route, profile, model)) = self.route_scope(route_name, Some(model_id)) else {
            return;
        };
        if !self.has_persistent_consent(&route) {
            return;
        }
        let key = cache_key(&profile, &model);
        let existing = self
            .cache
            .read()
            .ok()
            .and_then(|cache| cache.get(&key).cloned());
        if existing.as_ref().is_some_and(|evidence| {
            evidence.freshness_at(now_ms) == EvidenceFreshness::Fresh
                && evidence.observed_state != ObservedFreeAccessState::Unknown
                && evidence.observed_state != ObservedFreeAccessState::VerifiedFreeLimited
        }) {
            return;
        }
        let Ok(revision) = self.next_revision(&profile, &model).await else {
            return;
        };
        let mut evidence = if let Some(mut verified) = existing.clone().filter(|evidence| {
            evidence.matches_profile(&profile)
                && evidence.observed_state == ObservedFreeAccessState::VerifiedFreeLimited
                && evidence.freshness_at(now_ms) == EvidenceFreshness::Fresh
        }) {
            verified.revision = revision;
            verified.observed_at_ms = now_ms.max(1);
            verified.successful_sample_count = verified
                .successful_sample_count
                .saturating_add(1)
                .min(MAX_FREE_ACCESS_SAMPLES);
            verified.content_hash.clear();
            verified
        } else if let Some(mut observed) = existing.filter(|evidence| {
            evidence.matches_profile(&profile)
                && evidence.observed_state == ObservedFreeAccessState::Unknown
                && evidence.invalidation.is_none()
                && evidence.freshness_at(now_ms) == EvidenceFreshness::Fresh
        }) {
            observed.revision = revision;
            observed.observed_at_ms = now_ms.max(1);
            observed.expires_at_ms = now_ms.max(1).saturating_add(FREE_ACCESS_PROBE_TTL_MS);
            observed.successful_sample_count = observed
                .successful_sample_count
                .saturating_add(1)
                .min(MAX_FREE_ACCESS_SAMPLES);
            observed.content_hash.clear();
            observed
        } else {
            let Ok(mut observed) = build_evidence(
                &profile,
                &model,
                revision,
                now_ms,
                ObservedFreeAccessState::Unknown,
                AllowanceKind::Unknown,
                AllowanceProvenance::Unknown,
                1,
                0,
                None,
                None,
            ) else {
                return;
            };
            observed.expires_at_ms = now_ms.max(1).saturating_add(FREE_ACCESS_PROBE_TTL_MS);
            observed.content_hash.clear();
            observed
        };
        evidence.observed_at_ms = now_ms.max(1);
        let Ok(evidence) = evidence.seal() else {
            return;
        };
        self.publish(evidence).await;
    }

    fn route_scope(
        &self,
        route_name: &str,
        model_id: Option<&str>,
    ) -> Option<(ModelRouteConfig, ProviderProfile, String)> {
        let route = self.config.routes.get(route_name)?.clone();
        if !matches!(
            route.provider_profile_id,
            Some(
                evohime_model_gateway::ProviderProfileId::OpenRouter
                    | evohime_model_gateway::ProviderProfileId::CloudflareWorkersAi
            )
        ) || !route.configured()
        {
            return None;
        }
        let profile = ProviderProfile::from_route_config(&route).ok()?;
        if !profile.has_stable_credential_binding() {
            return None;
        }
        let model = model_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| route.literouter.model.trim())
            .to_owned();
        if model.is_empty() || model.len() > 256 {
            return None;
        }
        Some((route, profile, model))
    }

    fn has_persistent_consent(&self, route: &ModelRouteConfig) -> bool {
        self.consent_binding
            .as_deref()
            .is_some_and(|binding| route.provider_credential_binding.as_deref() == Some(binding))
    }

    fn cached_evidence(
        &self,
        profile: &ProviderProfile,
        model_id: &str,
    ) -> Option<FreeAccessEvidence> {
        let key = cache_key(profile, model_id);
        self.cache
            .read()
            .ok()?
            .get(&key)
            .cloned()
            .filter(|evidence| evidence.matches_profile(profile))
    }

    fn cached_evidence_for_route(
        &self,
        route_name: &str,
        model_id: &str,
    ) -> Option<FreeAccessEvidence> {
        let (_, profile, model) = self.route_scope(route_name, Some(model_id))?;
        self.cached_evidence(&profile, &model)
    }

    async fn run_probe_and_publish(&self, route_name: &str, model_id: &str) {
        if self.cancellation.is_cancelled() {
            return;
        }
        let Some((route, profile, model)) = self.route_scope(route_name, Some(model_id)) else {
            return;
        };
        if !self.has_persistent_consent(&route) {
            return;
        }
        if self
            .cached_evidence(&profile, &model)
            .is_some_and(|evidence| {
                evidence.freshness_at(crate::task_memory::now_millis()) == EvidenceFreshness::Fresh
            })
        {
            return;
        }
        let now_ms = crate::task_memory::now_millis();
        let scope = profile_scope_key(&profile);
        match credential_probe_cooldown_active(&self.journal, &profile.credential_binding, now_ms)
            .await
        {
            Ok(false) => {}
            Ok(true) | Err(()) => return,
        }
        let Ok(_permit) = self.guard.reserve(&scope, now_ms).await else {
            return;
        };
        let Ok(revision) = self.next_revision(&profile, &model).await else {
            return;
        };
        let Ok(gateway) = ModelGateway::from_config(&self.config) else {
            return;
        };
        let transport = GatewayProbeTransport {
            gateway: &gateway,
            route_name,
        };
        let evidence = tokio::select! {
            _ = self.cancellation.cancelled() => return,
            result = tokio::time::timeout(
                std::time::Duration::from_secs(45),
                collect_probe_evidence(
                    &transport,
                    &route,
                    &profile,
                    &model,
                    revision,
                    now_ms,
                    &self.cancellation,
                ),
            ) => match result {
                Ok(Ok(evidence)) => evidence,
                Ok(Err(code)) => match failure_evidence(&profile, &model, revision, crate::task_memory::now_millis(), code) {
                    Ok(evidence) => evidence,
                    Err(_) => return,
                },
                Err(_) => match failure_evidence(&profile, &model, revision, crate::task_memory::now_millis(), FreeProbeFailureCode::Cancelled) {
                    Ok(evidence) => evidence,
                    Err(_) => return,
                },
            }
        };
        if self.cancellation.is_cancelled() {
            return;
        }
        self.publish(evidence).await;
    }

    async fn next_revision(&self, profile: &ProviderProfile, model_id: &str) -> Result<u64, ()> {
        let database = self.journal.database().lock().await;
        let record = evohime_local_storage::free_access_evidence_store::get(
            database.connection(),
            &profile.provider_id,
            model_id,
            &profile.credential_binding,
            &profile.region,
        )
        .map_err(|_| ())?;
        match record {
            Some(record) => u64::try_from(record.revision)
                .ok()
                .and_then(|revision| revision.checked_add(1))
                .ok_or(()),
            None => Ok(1),
        }
    }

    async fn publish(&self, evidence: FreeAccessEvidence) {
        if evidence.validate().is_err() {
            return;
        }
        let Ok(record) = evidence.to_storage_record() else {
            return;
        };
        let persisted = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::free_access_evidence_store::put(database.connection(), &record)
                .unwrap_or(false)
        };
        if !persisted {
            return;
        }
        let key = free_access_evidence_scope_key(
            &evidence.provider_id,
            &evidence.model_id,
            &evidence.credential_binding,
            &evidence.region,
            evidence.profile_revision,
            &evidence.profile_content_hash,
        );
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(key, evidence);
        }
    }
}

pub(crate) fn profile_scope_key(profile: &ProviderProfile) -> String {
    hex::encode(sha2::Sha256::digest(
        format!(
            "{}|{}|{}|{}",
            profile.provider_id, profile.content_hash, profile.credential_binding, profile.region
        )
        .as_bytes(),
    ))
}

fn cache_key(profile: &ProviderProfile, model_id: &str) -> String {
    free_access_evidence_scope_key(
        &profile.provider_id,
        model_id,
        &profile.credential_binding,
        &profile.region,
        profile.revision,
        &profile.content_hash,
    )
}

/// Checks the persisted account-wide cooldown before accepting another model probe.
pub(crate) async fn credential_probe_cooldown_active(
    journal: &crate::EventJournal,
    credential_binding: &str,
    now_ms: u64,
) -> Result<bool, ()> {
    let database = journal.database().lock().await;
    let latest =
        evohime_local_storage::free_access_evidence_store::latest_observed_at_for_credential(
            database.connection(),
            credential_binding,
        )
        .map_err(|_| ())?;
    Ok(latest
        .and_then(|timestamp| u64::try_from(timestamp).ok())
        .is_some_and(|timestamp| now_ms.saturating_sub(timestamp) < FREE_ACCESS_PROBE_COOLDOWN_MS))
}

/// Bounded, safe failure classification for an attempted free-access probe.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FreeProbeFailureCode {
    /// Provider rejected access because billing is required.
    BillingRequired,
    /// Provider requires an account activation or plan change before access.
    ActivationRequired,
    /// Provider account is restricted independently of billing.
    AccountRestricted,
    /// Account or model quota was exhausted or rate limited.
    QuotaRejected,
    /// Credential was rejected.
    CredentialRejected,
    /// Permission or access was denied; billing meaning is not established.
    PermissionDenied,
    /// The model is unavailable in the provider profile.
    ModelUnavailable,
    /// The provider response was malformed or changed contract.
    ProviderProtocolDrift,
    /// The completion returned no semantic content.
    EmptyCompletion,
    /// The reported usage did not satisfy the bounded request contract.
    InvalidUsage,
    /// A probe is already active or its per-scope cooldown has not elapsed.
    Cooldown,
    /// Probe consent was absent or the credential binding was unavailable.
    ConsentOrCredentialRequired,
    /// Cancellation or a bounded deadline stopped the probe.
    Cancelled,
    /// The provider has no supported authoritative pricing source.
    AuthorityUnavailable,
    /// A generic transport failure occurred.
    TransportFailure,
}

impl FreeProbeFailureCode {
    /// Returns a bounded code suitable for local evidence metadata and UI.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::BillingRequired => "billing_required",
            Self::ActivationRequired => "activation_required",
            Self::AccountRestricted => "account_restricted",
            Self::QuotaRejected => "quota_rejected",
            Self::CredentialRejected => "credential_rejected",
            Self::PermissionDenied => "permission_denied",
            Self::ModelUnavailable => "model_unavailable",
            Self::ProviderProtocolDrift => "provider_protocol_drift",
            Self::EmptyCompletion => "empty_completion",
            Self::InvalidUsage => "invalid_usage",
            Self::Cooldown => "cooldown",
            Self::ConsentOrCredentialRequired => "consent_or_credential_required",
            Self::Cancelled => "cancelled",
            Self::AuthorityUnavailable => "authority_unavailable",
            Self::TransportFailure => "transport_failure",
        }
    }
}

/// One-at-a-time process-local limiter with a per-credential-profile cooldown.
pub struct FreeAccessProbeGuard {
    permits: Arc<Semaphore>,
    last_started_ms: tokio::sync::Mutex<HashMap<String, u64>>,
}

impl Default for FreeAccessProbeGuard {
    fn default() -> Self {
        Self {
            permits: Arc::new(Semaphore::new(1)),
            last_started_ms: tokio::sync::Mutex::new(HashMap::new()),
        }
    }
}

impl FreeAccessProbeGuard {
    /// Reserves the single global probe slot and applies per-scope cooldown.
    pub(crate) async fn reserve(
        &self,
        scope: &str,
        now_ms: u64,
    ) -> Result<OwnedSemaphorePermit, FreeProbeFailureCode> {
        let permit = Arc::clone(&self.permits)
            .try_acquire_owned()
            .map_err(|_| FreeProbeFailureCode::Cooldown)?;
        let mut started = self.last_started_ms.lock().await;
        if started
            .get(scope)
            .is_some_and(|last| now_ms.saturating_sub(*last) < FREE_ACCESS_PROBE_COOLDOWN_MS)
        {
            return Err(FreeProbeFailureCode::Cooldown);
        }
        if !started.contains_key(scope) && started.len() >= MAX_TRACKED_PROBE_SCOPES {
            return Err(FreeProbeFailureCode::Cooldown);
        }
        started.insert(scope.to_string(), now_ms);
        Ok(permit)
    }
}

/// Provider operations used by the deterministic probe state machine.
pub(crate) trait FreeAccessProbeTransport: Send + Sync {
    /// Fetches typed pricing metadata without making a model completion request.
    async fn pricing(
        &self,
        route: &ModelRouteConfig,
        model_id: &str,
    ) -> Result<OpenRouterPricingObservation, FreeProbeFailureCode>;

    /// Requests one bounded synthetic completion through the existing gateway.
    async fn completion(
        &self,
        route: &ModelRouteConfig,
        model_id: &str,
        cancellation: &CancellationToken,
    ) -> Result<ChatResult, FreeProbeFailureCode>;
}

/// Production probe adapter using fixed OpenRouter pricing authority when
/// available and the existing model gateway for one synthetic completion.
pub(crate) struct GatewayProbeTransport<'a> {
    /// Configured gateway that owns the existing provider transport.
    pub(crate) gateway: &'a ModelGateway,
    /// Route name paired with the same gateway configuration.
    pub(crate) route_name: &'a str,
}

impl FreeAccessProbeTransport for GatewayProbeTransport<'_> {
    async fn pricing(
        &self,
        route: &ModelRouteConfig,
        model_id: &str,
    ) -> Result<OpenRouterPricingObservation, FreeProbeFailureCode> {
        evohime_model_gateway::fetch_openrouter_model_pricing(route, model_id)
            .await
            .map_err(|error| match error {
                OpenRouterPricingError::HttpStatus(status) => {
                    classify_provider_failure("openrouter", Some(status), None)
                }
                OpenRouterPricingError::InvalidConfiguration => {
                    FreeProbeFailureCode::ConsentOrCredentialRequired
                }
                OpenRouterPricingError::Transport => FreeProbeFailureCode::TransportFailure,
                OpenRouterPricingError::InvalidResponse => {
                    FreeProbeFailureCode::ProviderProtocolDrift
                }
            })
    }

    async fn completion(
        &self,
        route: &ModelRouteConfig,
        model_id: &str,
        cancellation: &CancellationToken,
    ) -> Result<ChatResult, FreeProbeFailureCode> {
        let provider_id = route
            .provider_profile_id
            .map(evohime_model_gateway::ProviderProfileId::as_str)
            .unwrap_or("unknown");
        let messages = [ChatMessage::text(
            ChatRole::User,
            "Reply with the short marker EVOHIME_FREE_ACCESS_CHECK_OK.",
        )];
        let completion = self.gateway.chat_with_tools_for_route_bounded(
            self.route_name,
            Some(model_id),
            &messages,
            &[] as &[ToolSpec],
            ChatRequestOptions {
                max_output_tokens: Some(FREE_ACCESS_PROBE_MAX_OUTPUT_TOKENS),
                max_retries: Some(0),
            },
        );
        tokio::select! {
            _ = cancellation.cancelled() => Err(FreeProbeFailureCode::Cancelled),
            result = completion => result.map_err(|error| classify_gateway_error(provider_id, &error)),
        }
    }
}

/// Collects a profile-bound observation; an untrusted or paid pricing source
/// cannot become strict-free evidence.
pub(crate) async fn collect_probe_evidence<T: FreeAccessProbeTransport>(
    transport: &T,
    route: &ModelRouteConfig,
    profile: &ProviderProfile,
    model_id: &str,
    revision: u64,
    now_ms: u64,
    cancellation: &CancellationToken,
) -> Result<FreeAccessEvidence, FreeProbeFailureCode> {
    let openrouter = route.provider_profile_id
        == Some(evohime_model_gateway::ProviderProfileId::OpenRouter)
        && profile.provider_id == "openrouter";
    let cloudflare = route.provider_profile_id
        == Some(evohime_model_gateway::ProviderProfileId::CloudflareWorkersAi)
        && profile.provider_id == "cloudflare_workers_ai";
    if (!openrouter && !cloudflare)
        || !profile.has_stable_credential_binding()
        || model_id.trim().is_empty()
        || revision == 0
    {
        return Err(FreeProbeFailureCode::ConsentOrCredentialRequired);
    }

    let pricing = if openrouter {
        let pricing = tokio::select! {
            _ = cancellation.cancelled() => return Err(FreeProbeFailureCode::Cancelled),
            result = transport.pricing(route, model_id) => result?,
        };
        if pricing.model_id != model_id {
            return Err(FreeProbeFailureCode::ProviderProtocolDrift);
        }
        if !pricing.all_dimensions_zero {
            return build_evidence(
                profile,
                model_id,
                revision,
                now_ms,
                ObservedFreeAccessState::PaidOnly,
                AllowanceKind::None,
                AllowanceProvenance::OpenRouterModelPricing {
                    source_hash: pricing.source_hash,
                },
                0,
                0,
                None,
                Some("paid_pricing_dimension".to_string()),
            )
            .map_err(|_| FreeProbeFailureCode::ProviderProtocolDrift);
        }
        Some(pricing)
    } else {
        None
    };

    let result = tokio::select! {
        _ = cancellation.cancelled() => return Err(FreeProbeFailureCode::Cancelled),
        result = transport.completion(route, model_id, cancellation) => result?,
    };
    validate_semantic_completion(&result)?;

    let Some(pricing) = pricing else {
        return Err(FreeProbeFailureCode::AuthorityUnavailable);
    };

    build_evidence(
        profile,
        model_id,
        revision,
        now_ms,
        ObservedFreeAccessState::VerifiedFreeLimited,
        AllowanceKind::Recurring,
        AllowanceProvenance::OpenRouterModelPricing {
            source_hash: pricing.source_hash,
        },
        1,
        9_000,
        None,
        None,
    )
    .map_err(|_| FreeProbeFailureCode::ProviderProtocolDrift)
}

/// Produces an Unknown or typed restriction observation without retaining the
/// provider response, raw status body, prompt, or credential.
pub(crate) fn failure_evidence(
    profile: &ProviderProfile,
    model_id: &str,
    revision: u64,
    now_ms: u64,
    failure: FreeProbeFailureCode,
) -> Result<FreeAccessEvidence, &'static str> {
    let (observed_state, allowance, invalidation) = match failure {
        FreeProbeFailureCode::BillingRequired => (
            ObservedFreeAccessState::PaidOnly,
            AllowanceKind::None,
            Some(EvidenceInvalidation::BillingRequired),
        ),
        FreeProbeFailureCode::AccountRestricted => (
            ObservedFreeAccessState::PaidOnly,
            AllowanceKind::None,
            Some(EvidenceInvalidation::AccountRestricted),
        ),
        FreeProbeFailureCode::QuotaRejected => (
            ObservedFreeAccessState::Unknown,
            AllowanceKind::Unknown,
            Some(EvidenceInvalidation::QuotaExhausted),
        ),
        FreeProbeFailureCode::ActivationRequired => (
            ObservedFreeAccessState::ActivationRequired,
            AllowanceKind::Unknown,
            None,
        ),
        _ => (
            ObservedFreeAccessState::Unknown,
            AllowanceKind::Unknown,
            None,
        ),
    };
    build_evidence(
        profile,
        model_id,
        revision,
        now_ms,
        observed_state,
        allowance,
        AllowanceProvenance::Unknown,
        0,
        0,
        invalidation,
        Some(failure.as_str().to_string()),
    )
}

/// Maps provider-specific typed status evidence without treating generic 403 as billing.
pub(crate) fn classify_provider_failure(
    provider_id: &str,
    status: Option<u16>,
    provider_code: Option<&str>,
) -> FreeProbeFailureCode {
    if provider_id == "cloudflare_workers_ai" {
        match provider_code {
            Some("5035") => return FreeProbeFailureCode::ActivationRequired,
            Some("3036") => return FreeProbeFailureCode::QuotaRejected,
            _ => {}
        }
    }
    match status {
        Some(401) => FreeProbeFailureCode::CredentialRejected,
        Some(402) if provider_id == "openrouter" => FreeProbeFailureCode::BillingRequired,
        Some(403) => FreeProbeFailureCode::PermissionDenied,
        Some(404) => FreeProbeFailureCode::ModelUnavailable,
        Some(429) => FreeProbeFailureCode::QuotaRejected,
        Some(400..=599) => FreeProbeFailureCode::TransportFailure,
        _ => FreeProbeFailureCode::ProviderProtocolDrift,
    }
}

/// Classifies a non-success response with the gateway's bounded error variant.
pub(crate) fn classify_gateway_error(
    provider_id: &str,
    error: &ProviderError,
) -> FreeProbeFailureCode {
    match error {
        ProviderError::Api(message) => {
            let status = message
                .split_once(':')
                .and_then(|(prefix, _)| prefix.trim().parse::<u16>().ok());
            let provider_code = if provider_id == "cloudflare_workers_ai" {
                message
                    .split_once(": ")
                    .and_then(|(_, details)| {
                        details.strip_prefix("Cloudflare Workers AI error code ")
                    })
                    .filter(|code| matches!(*code, "5035" | "3036"))
            } else {
                None
            };
            classify_provider_failure(provider_id, status, provider_code)
        }
        ProviderError::Config(_)
        | ProviderError::ImagePreflightRejected
        | ProviderError::ImageCapabilityStale => FreeProbeFailureCode::ConsentOrCredentialRequired,
        ProviderError::Http(_) | ProviderError::Stream(_) => FreeProbeFailureCode::TransportFailure,
    }
}

fn validate_semantic_completion(result: &ChatResult) -> Result<(), FreeProbeFailureCode> {
    let has_text = !result.content.trim().is_empty() && result.content.len() <= 256;
    let has_reasoning = result
        .thinking
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty() && value.len() <= 1_024);
    let has_valid_tool_call = result.tool_calls.iter().all(|call| {
        !call.id.trim().is_empty()
            && !call.name.trim().is_empty()
            && call.arguments.len() <= 1_024
            && serde_json::from_str::<serde_json::Value>(&call.arguments)
                .is_ok_and(|value| value.is_object())
    });
    if !(has_text || has_reasoning || (!result.tool_calls.is_empty() && has_valid_tool_call)) {
        return Err(FreeProbeFailureCode::EmptyCompletion);
    }
    let usage = result.usage.ok_or(FreeProbeFailureCode::InvalidUsage)?;
    if usage.prompt_tokens == 0
        || usage.total_tokens == 0
        || usage.total_tokens != usage.prompt_tokens.saturating_add(usage.completion_tokens)
        || usage.completion_tokens > FREE_ACCESS_PROBE_MAX_OUTPUT_TOKENS
        || usage.total_tokens > 512
    {
        return Err(FreeProbeFailureCode::InvalidUsage);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_evidence(
    profile: &ProviderProfile,
    model_id: &str,
    revision: u64,
    now_ms: u64,
    observed_state: ObservedFreeAccessState,
    allowance: AllowanceKind,
    allowance_provenance: AllowanceProvenance,
    successful_sample_count: u32,
    confidence_bps: u16,
    invalidation: Option<EvidenceInvalidation>,
    failure_reason: Option<String>,
) -> Result<FreeAccessEvidence, &'static str> {
    let ttl = if observed_state == ObservedFreeAccessState::VerifiedFreeLimited {
        FREE_ACCESS_PROBE_TTL_MS
    } else {
        FREE_ACCESS_PROBE_FAILURE_TTL_MS
    };
    FreeAccessEvidence {
        schema_version: FREE_ACCESS_EVIDENCE_SCHEMA_VERSION,
        provider_id: profile.provider_id.clone(),
        model_id: model_id.to_string(),
        credential_binding: profile.credential_binding.clone(),
        region: profile.region.clone(),
        profile_revision: profile.revision,
        profile_content_hash: profile.content_hash.clone(),
        advertised_state: if model_id.ends_with(":free") {
            FreeAccessState::FreeTierLimited
        } else {
            FreeAccessState::Unknown
        },
        observed_state,
        activation: match observed_state {
            ObservedFreeAccessState::VerifiedFreeLimited => ActivationState::NotRequired,
            ObservedFreeAccessState::ActivationRequired => ActivationState::Required,
            _ => ActivationState::Unknown,
        },
        allowance,
        allowance_provenance,
        limits: Vec::new(),
        successful_sample_count,
        confidence_bps,
        observed_at_ms: now_ms.max(1),
        expires_at_ms: now_ms.max(1).saturating_add(ttl),
        invalidation,
        failure_reason,
        content_hash: String::new(),
        revision,
    }
    .seal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_model_gateway::{
        providers::{LlmUsage, ProviderError},
        ProviderProfileId,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FixedTransport {
        pricing: Result<OpenRouterPricingObservation, FreeProbeFailureCode>,
        completion: Result<ChatResult, FreeProbeFailureCode>,
        pricing_calls: AtomicUsize,
        completion_calls: AtomicUsize,
    }

    impl FreeAccessProbeTransport for FixedTransport {
        async fn pricing(
            &self,
            _route: &ModelRouteConfig,
            _model_id: &str,
        ) -> Result<OpenRouterPricingObservation, FreeProbeFailureCode> {
            self.pricing_calls.fetch_add(1, Ordering::SeqCst);
            self.pricing.clone()
        }

        async fn completion(
            &self,
            _route: &ModelRouteConfig,
            _model_id: &str,
            _cancellation: &CancellationToken,
        ) -> Result<ChatResult, FreeProbeFailureCode> {
            self.completion_calls.fetch_add(1, Ordering::SeqCst);
            self.completion.clone()
        }
    }

    fn route_and_profile() -> (ModelRouteConfig, ProviderProfile) {
        let route = ModelRouteConfig::openai_compatible(
            "test-key",
            "https://openrouter.ai/api/v1",
            "author/model:free",
        )
        .with_provider_profile(ProviderProfileId::OpenRouter, None)
        .with_credential_binding(Some(
            "credential:01234567-89ab-4cde-8fab-0123456789ab".into(),
        ));
        let profile = ProviderProfile::from_route_config(&route).expect("profile");
        (route, profile)
    }

    fn cloudflare_route_and_profile() -> (ModelRouteConfig, ProviderProfile) {
        let account_id = "0123456789abcdef0123456789abcdef";
        let endpoint = evohime_model_gateway::ProviderProfileId::cloudflare_base_url(account_id)
            .expect("valid account id");
        let route = ModelRouteConfig::openai_compatible("test-key", endpoint, "@cf/model")
            .with_provider_profile(
                evohime_model_gateway::ProviderProfileId::CloudflareWorkersAi,
                Some(account_id.into()),
            )
            .with_credential_binding(Some(
                "credential:01234567-89ab-4cde-8fab-0123456789ab".into(),
            ));
        let profile = ProviderProfile::from_route_config(&route).expect("profile");
        (route, profile)
    }

    fn pricing(all_dimensions_zero: bool) -> OpenRouterPricingObservation {
        OpenRouterPricingObservation {
            model_id: "author/model:free".into(),
            all_dimensions_zero,
            source_hash: "d".repeat(64),
        }
    }

    fn semantic_result() -> ChatResult {
        ChatResult {
            content: "probe-ok".into(),
            thinking: None,
            tool_calls: Vec::new(),
            usage: Some(LlmUsage::from_parts(24, 2)),
        }
    }

    #[tokio::test]
    async fn strict_evidence_requires_zero_pricing_and_semantic_completion() {
        let (route, profile) = route_and_profile();
        let transport = FixedTransport {
            pricing: Ok(pricing(true)),
            completion: Ok(semantic_result()),
            pricing_calls: AtomicUsize::new(0),
            completion_calls: AtomicUsize::new(0),
        };
        let evidence = collect_probe_evidence(
            &transport,
            &route,
            &profile,
            "author/model:free",
            1,
            1_000,
            &CancellationToken::new(),
        )
        .await
        .expect("strict evidence");

        assert!(evidence.is_strictly_free_at(1_001));
        assert_eq!(evidence.successful_sample_count, 1);
        assert_eq!(transport.pricing_calls.load(Ordering::SeqCst), 1);
        assert_eq!(transport.completion_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn paid_pricing_stops_before_a_synthetic_completion() {
        let (route, profile) = route_and_profile();
        let transport = FixedTransport {
            pricing: Ok(pricing(false)),
            completion: Ok(semantic_result()),
            pricing_calls: AtomicUsize::new(0),
            completion_calls: AtomicUsize::new(0),
        };
        let evidence = collect_probe_evidence(
            &transport,
            &route,
            &profile,
            "author/model:free",
            1,
            1_000,
            &CancellationToken::new(),
        )
        .await
        .expect("paid evidence");

        assert_eq!(evidence.observed_state, ObservedFreeAccessState::PaidOnly);
        assert!(!evidence.is_strictly_free_at(1_001));
        assert_eq!(transport.completion_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn generic_permission_errors_do_not_become_billing_contradictions() {
        assert_eq!(
            classify_provider_failure("openrouter", Some(403), None),
            FreeProbeFailureCode::PermissionDenied
        );
        assert_eq!(
            classify_provider_failure("openrouter", Some(402), None),
            FreeProbeFailureCode::BillingRequired
        );
        assert_eq!(
            classify_provider_failure("cloudflare_workers_ai", Some(429), Some("3036")),
            FreeProbeFailureCode::QuotaRejected
        );
        assert_eq!(
            classify_provider_failure("cloudflare_workers_ai", Some(403), Some("5035")),
            FreeProbeFailureCode::ActivationRequired
        );
        assert_eq!(
            classify_gateway_error(
                "cloudflare_workers_ai",
                &ProviderError::Api("403: Cloudflare Workers AI error code 5035".into())
            ),
            FreeProbeFailureCode::ActivationRequired
        );
        assert_eq!(
            classify_gateway_error(
                "cloudflare_workers_ai",
                &ProviderError::Api("403: provider request failed".into())
            ),
            FreeProbeFailureCode::PermissionDenied
        );
    }

    #[tokio::test]
    async fn cloudflare_probe_records_access_without_asserting_free_allowance() {
        let (route, profile) = cloudflare_route_and_profile();
        let transport = FixedTransport {
            pricing: Ok(pricing(true)),
            completion: Ok(semantic_result()),
            pricing_calls: AtomicUsize::new(0),
            completion_calls: AtomicUsize::new(0),
        };
        let failure = collect_probe_evidence(
            &transport,
            &route,
            &profile,
            "@cf/model",
            1,
            1_000,
            &CancellationToken::new(),
        )
        .await
        .expect_err("completion without pricing authority is not free evidence");

        assert_eq!(failure, FreeProbeFailureCode::AuthorityUnavailable);
        assert_eq!(transport.pricing_calls.load(Ordering::SeqCst), 0);
        assert_eq!(transport.completion_calls.load(Ordering::SeqCst), 1);
        let evidence =
            failure_evidence(&profile, "@cf/model", 1, 1_000, failure).expect("unknown evidence");
        assert_eq!(evidence.observed_state, ObservedFreeAccessState::Unknown);
        assert!(!evidence.is_strictly_free_at(1_001));
    }

    #[test]
    fn activation_failure_is_not_a_paid_only_observation() {
        let (_, profile) = cloudflare_route_and_profile();
        let evidence = failure_evidence(
            &profile,
            "@cf/model",
            1,
            1_000,
            FreeProbeFailureCode::ActivationRequired,
        )
        .expect("activation evidence");

        assert_eq!(
            evidence.observed_state,
            ObservedFreeAccessState::ActivationRequired
        );
        assert_eq!(evidence.activation, ActivationState::Required);
        assert_eq!(evidence.allowance, AllowanceKind::Unknown);
        assert!(evidence.invalidation.is_none());
    }

    #[tokio::test]
    async fn probe_guard_serializes_work_and_enforces_cooldown() {
        let guard = FreeAccessProbeGuard::default();
        let (route, profile) = route_and_profile();
        let scope = profile_scope_key(&profile);
        let permit = guard.reserve(&scope, 1_000).await.expect("first slot");
        assert_eq!(
            guard.reserve("other", 1_001).await.err(),
            Some(FreeProbeFailureCode::Cooldown)
        );
        drop(permit);
        assert_eq!(
            guard.reserve(&scope, 1_002).await.err(),
            Some(FreeProbeFailureCode::Cooldown)
        );
        assert!(guard
            .reserve(&scope, 1_000 + FREE_ACCESS_PROBE_COOLDOWN_MS)
            .await
            .is_ok());

        let mut another_model_route = route;
        another_model_route.literouter.model = "author/another-model".into();
        let another_model_profile = ProviderProfile::from_route_config(&another_model_route)
            .expect("same provider profile");
        assert_eq!(scope, profile_scope_key(&another_model_profile));
    }
}

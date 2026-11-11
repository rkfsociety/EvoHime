//! Bounded Core-owned contract for the outbound A2A remote-agent backend.
//!
//! This module is a protocol/backend extension of the existing external-agent
//! owner in `crate::external_coding_agent_adapter`: it introduces no second
//! runtime, scheduler, storage or team-roster authority. It defines the
//! untrusted profile/card/delegation/capsule/task-binding data contracts and
//! their deterministic validation, hashing and state transitions. It performs
//! no network access; discovery, capsule materialization and dispatch belong to
//! later stages of plan 180.
//!
//! Invariants enforced here:
//!
//! - remote card claims never grant local capabilities or authority;
//! - profile, card, delegation, capsule and task binding are content-hashed;
//! - raw cards, headers, cookies, transcripts, tokens and raw objectives are
//!   never part of a persisted or projected structure — only hashes, ids,
//!   references and typed state are retained;
//! - content-derived privacy can only raise, never lower, the effective class.

use std::net::IpAddr;

use evohime_model_gateway::PrivacyClass;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::external_coding_agent_adapter::{
    ExternalAgentProtocolKind, A2A_PROTOCOL_VERSION, MAX_ID_BYTES,
};

/// Contract schema version for the A2A profile/card/task contracts.
pub const SCHEMA_VERSION: u32 = 1;

/// Protocol label used by the versioned A2A backend.
pub const A2A_PROTOCOL_LABEL: &str = "a2a";

/// Maximum byte length of an endpoint origin.
pub const MAX_ENDPOINT_BYTES: usize = 512;
/// Maximum declared skills for one remote card snapshot.
pub const MAX_CARD_SKILLS: usize = 32;
/// Maximum schema hashes retained for one remote card snapshot.
pub const MAX_SCHEMA_HASHES: usize = 16;
/// Maximum auth scheme labels retained for one remote card snapshot.
pub const MAX_AUTH_SCHEMES: usize = 8;
/// Maximum capabilities or skill names allowed in a profile allowlist.
pub const MAX_ALLOWLIST: usize = 32;
/// Maximum byte budget accepted for one delegation objective.
pub const MAX_OBJECTIVE_BYTES: usize = 8 * 1024;
/// Maximum artifact budget accepted for one delegation.
pub const MAX_ARTIFACT_BUDGET: u32 = 64;
/// Maximum number of remote tasks a profile may have in flight at once.
pub const MAX_CONCURRENCY_LIMIT: u32 = 16;
/// Maximum references allowed inside one context capsule.
pub const MAX_CAPSULE_REFS: usize = 32;
/// Largest card time-to-live accepted before a snapshot is treated as stale.
pub const MAX_CARD_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// Hostnames that are never valid remote-agent endpoints, even when a profile
/// explicitly opts into loopback access.
const BLOCKED_HOSTNAMES: &[&str] = &[
    "localhost",
    "localhost.localdomain",
    "metadata",
    "metadata.google.internal",
    "metadata.goog",
];

/// Declared network boundary for a remote-agent profile endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAgentEndpoint {
    /// Canonical origin (scheme, host, optional port) with no path or query.
    pub origin: String,
    /// Whether the profile explicitly opts into loopback/private origins.
    pub allow_loopback: bool,
    /// Ports permitted for the origin; empty means only the scheme default.
    pub allowed_ports: Vec<u16>,
}

/// Versioned, explicitly registered remote-agent profile owned by Core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAgentProfile {
    /// Contract schema version.
    pub schema_version: u32,
    /// Stable profile identifier.
    pub id: String,
    /// Monotonic profile revision used for optimistic concurrency.
    pub revision: u64,
    /// Human-readable label; never used for dispatch.
    pub display_name: String,
    /// Protocol backend; must be the versioned A2A kind.
    pub protocol_kind: ExternalAgentProtocolKind,
    /// Profile-owned endpoint boundary.
    pub endpoint: RemoteAgentEndpoint,
    /// Reference to an existing auth owner profile; never a secret value.
    pub auth_profile_ref: Option<String>,
    /// Skills the profile is willing to delegate.
    pub skill_allowlist: Vec<String>,
    /// Local capabilities the profile may request through delegation.
    pub capability_allowlist: Vec<String>,
    /// Highest privacy class the profile may receive.
    pub privacy_ceiling: PrivacyClass,
    /// Maximum concurrent remote tasks for this profile.
    pub concurrency_limit: u32,
    /// Whether new delegations may use this profile revision.
    pub enabled: bool,
    /// Digest of this profile with this field cleared.
    pub content_hash: String,
}

/// One bounded skill advertised by an untrusted remote card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAgentSkill {
    /// Skill identifier as reported by the remote agent.
    pub id: String,
    /// Capability this skill claims to satisfy.
    pub capability: String,
}

/// Bounded, untrusted, hash-bound snapshot of a remote agent card.
///
/// The snapshot never contains the raw card body, headers or credentials; it
/// retains only hashes, bounded claims and typed flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAgentCardSnapshot {
    /// Contract schema version.
    pub schema_version: u32,
    /// Owning profile identifier.
    pub profile_id: String,
    /// Profile revision this snapshot was fetched for.
    pub profile_revision: u64,
    /// Protocol version advertised by the card.
    pub protocol_version: u32,
    /// Origin the card was fetched from.
    pub origin: String,
    /// Digest of the raw response bytes.
    pub raw_hash: String,
    /// Digest of the normalized, bounded snapshot content.
    pub normalized_hash: String,
    /// Time the card was fetched in Unix milliseconds.
    pub fetched_at_ms: i64,
    /// Time the snapshot becomes stale in Unix milliseconds.
    pub expires_at_ms: i64,
    /// Bounded skills advertised by the card.
    pub skills: Vec<RemoteAgentSkill>,
    /// Digests of advertised schemas.
    pub schema_hashes: Vec<String>,
    /// Whether the card claims streaming/SSE support.
    pub supports_streaming: bool,
    /// Auth scheme labels the card claims to accept.
    pub auth_schemes: Vec<String>,
}

/// Deterministic delegation request bound to exact hashes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationRequest {
    /// Contract schema version.
    pub schema_version: u32,
    /// Stable delegation identifier.
    pub delegation_id: String,
    /// Parent workflow/grant reference authorizing the delegation.
    pub parent_grant_ref: String,
    /// Target profile identifier.
    pub profile_id: String,
    /// Exact profile revision pinned by the delegation.
    pub profile_revision: u64,
    /// Normalized card hash the delegation was built against.
    pub card_normalized_hash: String,
    /// Capability selected from the profile/card intersection.
    pub capability: String,
    /// Digest of the bounded objective text.
    pub objective_hash: String,
    /// Byte length of the bounded objective text.
    pub objective_bytes: usize,
    /// Privacy class required by the delegation.
    pub required_privacy: PrivacyClass,
    /// Idempotency identity for the remote submit.
    pub idempotency_key: String,
    /// Maximum number of artifacts the remote task may return.
    pub artifact_budget: u32,
    /// Digest of this request with this field cleared.
    pub request_hash: String,
}

/// Immutable delegation capsule carrying references, never raw workspace data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCapsule {
    /// Contract schema version.
    pub schema_version: u32,
    /// Stable capsule identifier.
    pub capsule_id: String,
    /// Delegation this capsule belongs to.
    pub delegation_id: String,
    /// Selected context namespace/loadout references.
    pub context_refs: Vec<String>,
    /// Digest of the parent grant the capsule was built under.
    pub parent_grant_hash: String,
    /// Digest of the effective capability set.
    pub capability_hash: String,
    /// Digest of the applied endpoint/privacy policy.
    pub policy_hash: String,
    /// Effective privacy class of the capsule.
    pub privacy_class: PrivacyClass,
    /// Digest of this capsule with this field cleared.
    pub content_hash: String,
}

/// Lifecycle state of a remote A2A task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteTaskState {
    /// Binding recorded but not yet dispatched.
    Registered,
    /// Submit in progress.
    Submitting,
    /// Remote accepted the task and returned an id.
    Submitted,
    /// Remote requires additional input before continuing.
    InputRequired,
    /// Remote requires (re)authentication before continuing.
    AuthRequired,
    /// Remote reports the task is running.
    Running,
    /// Remote reported validated completion.
    Completed,
    /// Remote explicitly rejected the task.
    Rejected,
    /// Remote reported failure.
    Failed,
    /// Cancellation requested and awaiting confirmation.
    CancelRequested,
    /// Remote confirmed cancellation.
    Cancelled,
    /// Submit outcome cannot be determined safely; reconciliation only.
    UnknownOutcome,
    /// Remote or protocol is unavailable.
    Unavailable,
}

impl RemoteTaskState {
    /// Returns whether the state is a non-retryable terminal state.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Rejected | Self::Failed | Self::Cancelled | Self::Unavailable
        )
    }
}

/// Recovery disposition for a remote task binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationDisposition {
    /// No reconciliation is needed for the current state.
    NotRequired,
    /// The submit outcome is ambiguous and must be reconciled.
    Pending,
    /// A known remote id was reconciled to the same task.
    ReconciledSameTask,
    /// The task was abandoned without a blind resubmit.
    Abandoned,
}

/// Durable binding between a local delegation and a remote task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTaskBinding {
    /// Contract schema version.
    pub schema_version: u32,
    /// Stable binding identifier.
    pub binding_id: String,
    /// Delegation this binding serves.
    pub delegation_id: String,
    /// Remote task id once known.
    pub remote_task_id: Option<String>,
    /// Current lifecycle state.
    pub state: RemoteTaskState,
    /// Recovery disposition.
    pub reconciliation: ReconciliationDisposition,
    /// Number of submit attempts (never incremented by blind retry).
    pub attempt: u32,
    /// Idempotency identity used for the submit.
    pub idempotency_key: String,
    /// Digest of the delegation request.
    pub request_hash: String,
    /// Digest of the context capsule.
    pub capsule_hash: String,
    /// Digest of the applied policy.
    pub policy_hash: String,
    /// Optimistic-concurrency revision.
    pub revision: u64,
    /// Creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Last update time in Unix milliseconds.
    pub updated_at_ms: i64,
    /// Digest of this binding with this field cleared.
    pub content_hash: String,
}

/// Validation failure for A2A profile/card/delegation/capsule/task contracts.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum A2aError {
    /// Stored or supplied data uses an unsupported schema version.
    #[error("unsupported a2a schema version {0}")]
    UnsupportedVersion(u32),
    /// Identifier, hash or reference metadata is invalid.
    #[error("invalid a2a identifier or reference")]
    InvalidIdentifier,
    /// Endpoint origin violates the declared policy.
    #[error("a2a endpoint is not permitted by profile policy")]
    EndpointDenied,
    /// Protocol kind is not a supported A2A backend.
    #[error("a2a protocol is unsupported")]
    UnsupportedProtocol,
    /// Card snapshot is stale relative to the supplied time.
    #[error("a2a card snapshot has expired")]
    CardExpired,
    /// Card does not belong to the referenced profile revision/origin.
    #[error("a2a card snapshot does not match the profile")]
    CardMismatch,
    /// Requested capability is absent from the profile/card intersection.
    #[error("a2a capability is not permitted")]
    CapabilityDenied,
    /// Required privacy exceeds the profile/capsule ceiling.
    #[error("a2a privacy class exceeds the permitted ceiling")]
    PrivacyExceeded,
    /// Declared concurrency limit is invalid.
    #[error("a2a concurrency limit is invalid")]
    ConcurrencyLimit,
    /// Objective, artifact or reference budget exceeded.
    #[error("a2a budget exceeded")]
    BudgetExceeded,
    /// Requested lifecycle transition is invalid.
    #[error("a2a task transition is invalid")]
    InvalidTransition,
    /// Caller expected an obsolete binding revision.
    #[error("a2a task revision is stale")]
    StaleRevision,
    /// Canonical serialization failed.
    #[error("a2a serialization failed")]
    Serialization,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-:/".contains(&b))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Returns whether a value is a non-empty, bounded ASCII identifier.
pub fn is_valid_identifier(value: &str) -> bool {
    valid_id(value)
}

/// Returns whether a value is a lowercase 64-character hex digest.
pub fn is_valid_hash(value: &str) -> bool {
    valid_hash(value)
}

fn hash_serializable<T: Serialize>(value: &T) -> Result<String, A2aError> {
    let bytes = serde_json::to_vec(value).map_err(|_| A2aError::Serialization)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn sort_dedup(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

fn blocked_hostname(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    BLOCKED_HOSTNAMES.contains(&host.as_str())
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
}

fn is_loopback_or_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_multicast()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6
                    .to_ipv4_mapped()
                    .is_some_and(|v4| is_loopback_or_private(IpAddr::V4(v4)))
        }
    }
}

/// Validates a profile endpoint against its declared boundary.
///
/// Enforces scheme, userinfo, host, blocked-name and loopback/private rules
/// without performing DNS resolution; dynamic resolution, redirect and
/// rebinding protections are applied at dispatch time by the network stage.
pub fn validate_endpoint(endpoint: &RemoteAgentEndpoint) -> Result<(), A2aError> {
    if endpoint.origin.is_empty() || endpoint.origin.len() > MAX_ENDPOINT_BYTES {
        return Err(A2aError::EndpointDenied);
    }
    let url = reqwest::Url::parse(&endpoint.origin).map_err(|_| A2aError::EndpointDenied)?;
    match url.scheme() {
        "https" => {}
        "http" if endpoint.allow_loopback => {}
        _ => return Err(A2aError::EndpointDenied),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(A2aError::EndpointDenied);
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(A2aError::EndpointDenied);
    }
    let path = url.path();
    if !(path.is_empty() || path == "/") {
        return Err(A2aError::EndpointDenied);
    }
    let host = url.host_str().ok_or(A2aError::EndpointDenied)?;
    if host.is_empty() {
        return Err(A2aError::EndpointDenied);
    }
    if !endpoint.allow_loopback {
        if let Ok(ip) = host.parse::<IpAddr>() {
            if is_loopback_or_private(ip) {
                return Err(A2aError::EndpointDenied);
            }
        } else if blocked_hostname(host) {
            return Err(A2aError::EndpointDenied);
        }
    }
    let default_port = if url.scheme() == "https" { 443 } else { 80 };
    match url.port() {
        Some(port) => {
            if endpoint.allowed_ports.is_empty() || !endpoint.allowed_ports.contains(&port) {
                return Err(A2aError::EndpointDenied);
            }
        }
        None => {
            if !(endpoint.allowed_ports.is_empty()
                || endpoint.allowed_ports.contains(&default_port))
            {
                return Err(A2aError::EndpointDenied);
            }
        }
    }
    Ok(())
}

/// Recomputes a profile's integrity digest after an edit.
pub fn refresh_profile_hash(profile: &mut RemoteAgentProfile) -> Result<(), A2aError> {
    profile.content_hash.clear();
    profile.content_hash = hash_serializable(profile)?;
    Ok(())
}

/// Validates a remote-agent profile and normalizes its allowlists.
pub fn validate_profile(mut profile: RemoteAgentProfile) -> Result<RemoteAgentProfile, A2aError> {
    if profile.schema_version != SCHEMA_VERSION {
        return Err(A2aError::UnsupportedVersion(profile.schema_version));
    }
    match profile.protocol_kind {
        ExternalAgentProtocolKind::A2a { protocol_version } => {
            if protocol_version != A2A_PROTOCOL_VERSION {
                return Err(A2aError::UnsupportedProtocol);
            }
        }
        _ => return Err(A2aError::UnsupportedProtocol),
    }
    if !valid_id(&profile.id)
        || profile.revision == 0
        || profile.display_name.is_empty()
        || profile.display_name.len() > MAX_ID_BYTES
    {
        return Err(A2aError::InvalidIdentifier);
    }
    if profile.concurrency_limit == 0 || profile.concurrency_limit > MAX_CONCURRENCY_LIMIT {
        return Err(A2aError::ConcurrencyLimit);
    }
    if profile
        .auth_profile_ref
        .as_deref()
        .is_some_and(|value| !valid_id(value))
    {
        return Err(A2aError::InvalidIdentifier);
    }
    if profile.skill_allowlist.len() > MAX_ALLOWLIST
        || profile.capability_allowlist.len() > MAX_ALLOWLIST
    {
        return Err(A2aError::BudgetExceeded);
    }
    sort_dedup(&mut profile.skill_allowlist);
    sort_dedup(&mut profile.capability_allowlist);
    if profile.skill_allowlist.iter().any(|value| !valid_id(value))
        || profile
            .capability_allowlist
            .iter()
            .any(|value| !valid_id(value))
    {
        return Err(A2aError::InvalidIdentifier);
    }
    validate_endpoint(&profile.endpoint)?;
    if !profile.content_hash.is_empty() {
        let expected = profile.content_hash.clone();
        refresh_profile_hash(&mut profile)?;
        if profile.content_hash != expected {
            return Err(A2aError::InvalidIdentifier);
        }
    }
    Ok(profile)
}

/// Builds and hashes a validated profile from already-normalized input.
pub fn build_profile(mut profile: RemoteAgentProfile) -> Result<RemoteAgentProfile, A2aError> {
    profile.content_hash.clear();
    let profile = validate_profile(profile)?;
    let mut profile = profile;
    refresh_profile_hash(&mut profile)?;
    validate_profile(profile)
}

/// Computes the deterministic intersection of profile and card capabilities.
///
/// Card claims never grant authority: the result is an upper bound that must
/// still be intersected with the parent grant before dispatch.
pub fn capability_intersection(
    profile: &RemoteAgentProfile,
    card: &RemoteAgentCardSnapshot,
) -> Vec<String> {
    let mut result = card
        .skills
        .iter()
        .map(|skill| skill.capability.clone())
        .filter(|capability| profile.capability_allowlist.iter().any(|v| v == capability))
        .collect::<Vec<_>>();
    sort_dedup(&mut result);
    result
}

/// Builds a validated, hashed, untrusted card snapshot for a profile.
#[allow(clippy::too_many_arguments)]
pub fn card_snapshot(
    profile: &RemoteAgentProfile,
    protocol_version: u32,
    raw_hash: String,
    mut skills: Vec<RemoteAgentSkill>,
    mut schema_hashes: Vec<String>,
    supports_streaming: bool,
    mut auth_schemes: Vec<String>,
    fetched_at_ms: i64,
    ttl_ms: i64,
) -> Result<RemoteAgentCardSnapshot, A2aError> {
    let profile = validate_profile(profile.clone())?;
    if protocol_version != A2A_PROTOCOL_VERSION {
        return Err(A2aError::UnsupportedProtocol);
    }
    if !valid_hash(&raw_hash) || ttl_ms <= 0 || ttl_ms > MAX_CARD_TTL_MS {
        return Err(A2aError::InvalidIdentifier);
    }
    if skills.len() > MAX_CARD_SKILLS
        || schema_hashes.len() > MAX_SCHEMA_HASHES
        || auth_schemes.len() > MAX_AUTH_SCHEMES
    {
        return Err(A2aError::BudgetExceeded);
    }
    skills.sort_by(|a, b| a.id.cmp(&b.id));
    if skills
        .iter()
        .any(|skill| !valid_id(&skill.id) || !valid_id(&skill.capability))
    {
        return Err(A2aError::InvalidIdentifier);
    }
    sort_dedup(&mut schema_hashes);
    sort_dedup(&mut auth_schemes);
    if schema_hashes.iter().any(|value| !valid_hash(value))
        || auth_schemes.iter().any(|value| !valid_id(value))
    {
        return Err(A2aError::InvalidIdentifier);
    }
    let mut snapshot = RemoteAgentCardSnapshot {
        schema_version: SCHEMA_VERSION,
        profile_id: profile.id,
        profile_revision: profile.revision,
        protocol_version,
        origin: profile.endpoint.origin,
        raw_hash,
        normalized_hash: String::new(),
        fetched_at_ms,
        expires_at_ms: fetched_at_ms.saturating_add(ttl_ms),
        skills,
        schema_hashes,
        supports_streaming,
        auth_schemes,
    };
    snapshot.normalized_hash = hash_serializable(&snapshot)?;
    Ok(snapshot)
}

/// Returns whether a card snapshot is still valid at `now_ms`.
pub fn card_is_fresh(card: &RemoteAgentCardSnapshot, now_ms: i64) -> bool {
    card.schema_version == SCHEMA_VERSION && now_ms < card.expires_at_ms
}

/// Builds and hashes a delegation request bound to exact profile/card hashes.
#[allow(clippy::too_many_arguments)]
pub fn build_delegation(
    profile: &RemoteAgentProfile,
    card: &RemoteAgentCardSnapshot,
    delegation_id: String,
    parent_grant_ref: String,
    capability: String,
    objective: &str,
    required_privacy: PrivacyClass,
    idempotency_key: String,
    artifact_budget: u32,
    now_ms: i64,
) -> Result<DelegationRequest, A2aError> {
    let profile = validate_profile(profile.clone())?;
    if !profile.enabled {
        return Err(A2aError::EndpointDenied);
    }
    if !valid_id(&delegation_id) || !valid_id(&parent_grant_ref) || !valid_id(&idempotency_key) {
        return Err(A2aError::InvalidIdentifier);
    }
    if card.schema_version != SCHEMA_VERSION
        || card.profile_id != profile.id
        || card.profile_revision != profile.revision
        || card.origin != profile.endpoint.origin
    {
        return Err(A2aError::CardMismatch);
    }
    if !card_is_fresh(card, now_ms) {
        return Err(A2aError::CardExpired);
    }
    if !capability_intersection(&profile, card)
        .iter()
        .any(|v| v == &capability)
    {
        return Err(A2aError::CapabilityDenied);
    }
    if objective.is_empty() || objective.len() > MAX_OBJECTIVE_BYTES {
        return Err(A2aError::BudgetExceeded);
    }
    if artifact_budget == 0 || artifact_budget > MAX_ARTIFACT_BUDGET {
        return Err(A2aError::BudgetExceeded);
    }
    if required_privacy > profile.privacy_ceiling {
        return Err(A2aError::PrivacyExceeded);
    }
    let mut request = DelegationRequest {
        schema_version: SCHEMA_VERSION,
        delegation_id,
        parent_grant_ref,
        profile_id: profile.id,
        profile_revision: profile.revision,
        card_normalized_hash: card.normalized_hash.clone(),
        capability,
        objective_hash: hex::encode(Sha256::digest(objective.as_bytes())),
        objective_bytes: objective.len(),
        required_privacy,
        idempotency_key,
        artifact_budget,
        request_hash: String::new(),
    };
    request.request_hash = hash_serializable(&request)?;
    Ok(request)
}

/// Validates an existing delegation request's shape and hashes.
pub fn validate_delegation(request: &DelegationRequest) -> Result<(), A2aError> {
    if request.schema_version != SCHEMA_VERSION {
        return Err(A2aError::UnsupportedVersion(request.schema_version));
    }
    if !valid_id(&request.delegation_id)
        || !valid_id(&request.parent_grant_ref)
        || !valid_id(&request.profile_id)
        || !valid_id(&request.capability)
        || !valid_id(&request.idempotency_key)
        || !valid_hash(&request.objective_hash)
        || !valid_hash(&request.card_normalized_hash)
        || request.profile_revision == 0
        || request.objective_bytes == 0
        || request.objective_bytes > MAX_OBJECTIVE_BYTES
        || request.artifact_budget == 0
        || request.artifact_budget > MAX_ARTIFACT_BUDGET
    {
        return Err(A2aError::InvalidIdentifier);
    }
    let mut canonical = request.clone();
    canonical.request_hash.clear();
    if hash_serializable(&canonical)? != request.request_hash {
        return Err(A2aError::InvalidIdentifier);
    }
    Ok(())
}

/// Builds and hashes an immutable capsule from selected context references.
pub fn build_capsule(
    request: &DelegationRequest,
    capsule_id: String,
    mut context_refs: Vec<String>,
    parent_grant_hash: String,
    policy_hash: String,
    privacy_class: PrivacyClass,
) -> Result<ContextCapsule, A2aError> {
    validate_delegation(request)?;
    if !valid_id(&capsule_id) || !valid_hash(&parent_grant_hash) || !valid_hash(&policy_hash) {
        return Err(A2aError::InvalidIdentifier);
    }
    if context_refs.is_empty() || context_refs.len() > MAX_CAPSULE_REFS {
        return Err(A2aError::BudgetExceeded);
    }
    sort_dedup(&mut context_refs);
    if context_refs.iter().any(|value| !valid_id(value)) {
        return Err(A2aError::InvalidIdentifier);
    }
    if privacy_class < request.required_privacy {
        return Err(A2aError::PrivacyExceeded);
    }
    let capability_hash = hex::encode(Sha256::digest(request.capability.as_bytes()));
    let mut capsule = ContextCapsule {
        schema_version: SCHEMA_VERSION,
        capsule_id,
        delegation_id: request.delegation_id.clone(),
        context_refs,
        parent_grant_hash,
        capability_hash,
        policy_hash,
        privacy_class,
        content_hash: String::new(),
    };
    capsule.content_hash = hash_serializable(&capsule)?;
    Ok(capsule)
}

/// Validates a capsule's shape, binding and content hash.
pub fn validate_capsule(capsule: &ContextCapsule) -> Result<(), A2aError> {
    if capsule.schema_version != SCHEMA_VERSION {
        return Err(A2aError::UnsupportedVersion(capsule.schema_version));
    }
    if !valid_id(&capsule.capsule_id)
        || !valid_id(&capsule.delegation_id)
        || !valid_hash(&capsule.parent_grant_hash)
        || !valid_hash(&capsule.capability_hash)
        || !valid_hash(&capsule.policy_hash)
        || capsule.context_refs.is_empty()
        || capsule.context_refs.len() > MAX_CAPSULE_REFS
        || capsule.context_refs.iter().any(|value| !valid_id(value))
    {
        return Err(A2aError::InvalidIdentifier);
    }
    let mut canonical = capsule.clone();
    canonical.content_hash.clear();
    if hash_serializable(&canonical)? != capsule.content_hash {
        return Err(A2aError::InvalidIdentifier);
    }
    Ok(())
}

/// Creates a hashed binding in the initial `Registered` state.
pub fn build_binding(
    request: &DelegationRequest,
    capsule: &ContextCapsule,
    binding_id: String,
    policy_hash: String,
    now_ms: i64,
) -> Result<RemoteTaskBinding, A2aError> {
    validate_delegation(request)?;
    validate_capsule(capsule)?;
    if capsule.delegation_id != request.delegation_id || !valid_id(&binding_id) {
        return Err(A2aError::InvalidIdentifier);
    }
    let mut binding = RemoteTaskBinding {
        schema_version: SCHEMA_VERSION,
        binding_id,
        delegation_id: request.delegation_id.clone(),
        remote_task_id: None,
        state: RemoteTaskState::Registered,
        reconciliation: ReconciliationDisposition::NotRequired,
        attempt: 0,
        idempotency_key: request.idempotency_key.clone(),
        request_hash: request.request_hash.clone(),
        capsule_hash: capsule.content_hash.clone(),
        policy_hash,
        revision: 1,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
        content_hash: String::new(),
    };
    refresh_binding_hash(&mut binding)?;
    Ok(binding)
}

/// Recomputes a binding's integrity digest after a state change.
pub fn refresh_binding_hash(binding: &mut RemoteTaskBinding) -> Result<(), A2aError> {
    binding.content_hash.clear();
    binding.content_hash = hash_serializable(binding)?;
    Ok(())
}

/// Validates a binding's shape, remote id and integrity digest.
pub fn validate_binding(binding: &RemoteTaskBinding) -> Result<(), A2aError> {
    if binding.schema_version != SCHEMA_VERSION {
        return Err(A2aError::UnsupportedVersion(binding.schema_version));
    }
    if !valid_id(&binding.binding_id)
        || !valid_id(&binding.delegation_id)
        || !valid_id(&binding.idempotency_key)
        || !valid_hash(&binding.request_hash)
        || !valid_hash(&binding.capsule_hash)
        || !valid_hash(&binding.policy_hash)
        || binding.revision == 0
        || binding
            .remote_task_id
            .as_deref()
            .is_some_and(|value| !valid_id(value))
    {
        return Err(A2aError::InvalidIdentifier);
    }
    let mut canonical = binding.clone();
    canonical.content_hash.clear();
    if hash_serializable(&canonical)? != binding.content_hash {
        return Err(A2aError::InvalidIdentifier);
    }
    Ok(())
}

/// Returns whether a deterministic lifecycle transition is permitted.
///
/// `UnknownOutcome` may only reconcile to a known outcome of the same task and
/// may never fall back to a fresh `Registered`/`Submitting` state, so an
/// ambiguous submit cannot become a blind resubmit.
pub fn transition_remote_task(
    current: RemoteTaskState,
    next: RemoteTaskState,
) -> Result<RemoteTaskState, A2aError> {
    use RemoteTaskState::{
        AuthRequired, CancelRequested, Cancelled, Completed, Failed, InputRequired, Registered,
        Rejected, Running, Submitted, Submitting, Unavailable, UnknownOutcome,
    };
    let allowed = matches!(
        (current, next),
        (Registered, Submitting)
            | (
                Submitting,
                Submitted | UnknownOutcome | Rejected | Failed | Unavailable
            )
            | (
                Submitted,
                Running
                    | InputRequired
                    | AuthRequired
                    | Rejected
                    | Failed
                    | CancelRequested
                    | UnknownOutcome
            )
            | (
                InputRequired,
                Submitted | Rejected | Failed | CancelRequested | Cancelled
            )
            | (
                AuthRequired,
                Submitted | Rejected | Failed | CancelRequested | Cancelled
            )
            | (
                Running,
                Completed
                    | InputRequired
                    | AuthRequired
                    | Failed
                    | CancelRequested
                    | UnknownOutcome
            )
            | (CancelRequested, Cancelled | Failed | UnknownOutcome)
            | (
                UnknownOutcome,
                Running | Completed | Failed | Rejected | Cancelled | UnknownOutcome
            )
    );
    if allowed {
        Ok(next)
    } else {
        Err(A2aError::InvalidTransition)
    }
}

/// Applies a transition guarded by an expected binding revision (CAS).
pub fn apply_transition(
    binding: &mut RemoteTaskBinding,
    next: RemoteTaskState,
    expected_revision: u64,
    now_ms: i64,
) -> Result<(), A2aError> {
    validate_binding(binding)?;
    if binding.revision != expected_revision {
        return Err(A2aError::StaleRevision);
    }
    transition_remote_task(binding.state, next)?;
    binding.state = next;
    binding.revision += 1;
    binding.updated_at_ms = now_ms;
    match next {
        RemoteTaskState::UnknownOutcome => {
            binding.reconciliation = ReconciliationDisposition::Pending;
        }
        RemoteTaskState::Running | RemoteTaskState::Completed
            if binding.remote_task_id.is_some() =>
        {
            if binding.reconciliation == ReconciliationDisposition::Pending {
                binding.reconciliation = ReconciliationDisposition::ReconciledSameTask;
            }
        }
        RemoteTaskState::Submitted => {
            binding.attempt += 1;
            binding.reconciliation = ReconciliationDisposition::NotRequired;
        }
        _ => {}
    }
    refresh_binding_hash(binding)?;
    Ok(())
}

/// Records a known remote task id for reconciliation without resubmitting.
///
/// Only the same idempotency identity may attach an id, so a reconciled task
/// can never silently replace a different remote task.
pub fn reconcile_known_task(
    binding: &mut RemoteTaskBinding,
    remote_task_id: String,
    expected_revision: u64,
    now_ms: i64,
) -> Result<(), A2aError> {
    validate_binding(binding)?;
    if binding.revision != expected_revision {
        return Err(A2aError::StaleRevision);
    }
    if !valid_id(&remote_task_id) {
        return Err(A2aError::InvalidIdentifier);
    }
    if binding
        .remote_task_id
        .as_deref()
        .is_some_and(|current| current != remote_task_id)
    {
        return Err(A2aError::InvalidTransition);
    }
    binding.remote_task_id = Some(remote_task_id);
    binding.reconciliation = ReconciliationDisposition::ReconciledSameTask;
    binding.revision += 1;
    binding.updated_at_ms = now_ms;
    refresh_binding_hash(binding)?;
    Ok(())
}

/// Projects binding status without objective, context or credential data.
pub fn binding_projection(binding: &RemoteTaskBinding) -> serde_json::Value {
    serde_json::json!({
        "schema_version": binding.schema_version,
        "binding_id": binding.binding_id,
        "delegation_id": binding.delegation_id,
        "remote_task_id": binding.remote_task_id,
        "state": binding.state,
        "reconciliation": binding.reconciliation,
        "attempt": binding.attempt,
        "profile_revision": serde_json::Value::Null,
        "request_hash": binding.request_hash,
        "capsule_hash": binding.capsule_hash,
        "policy_hash": binding.policy_hash,
        "revision": binding.revision,
        "redacted": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> RemoteAgentProfile {
        build_profile(RemoteAgentProfile {
            schema_version: SCHEMA_VERSION,
            id: "remote.agent".into(),
            revision: 1,
            display_name: "Remote Agent".into(),
            protocol_kind: ExternalAgentProtocolKind::A2a {
                protocol_version: A2A_PROTOCOL_VERSION,
            },
            endpoint: RemoteAgentEndpoint {
                origin: "https://agents.example.com".into(),
                allow_loopback: false,
                allowed_ports: vec![],
            },
            auth_profile_ref: Some("auth.remote".into()),
            skill_allowlist: vec!["code.review".into(), "code.edit".into()],
            capability_allowlist: vec!["agent.execute".into()],
            privacy_ceiling: PrivacyClass::Sensitive,
            concurrency_limit: 2,
            enabled: true,
            content_hash: String::new(),
        })
        .unwrap()
    }

    fn card(profile: &RemoteAgentProfile, fetched_at_ms: i64) -> RemoteAgentCardSnapshot {
        card_snapshot(
            profile,
            A2A_PROTOCOL_VERSION,
            hex::encode(Sha256::digest(b"raw-card")),
            vec![
                RemoteAgentSkill {
                    id: "code.review".into(),
                    capability: "agent.execute".into(),
                },
                RemoteAgentSkill {
                    id: "admin".into(),
                    capability: "agent.admin".into(),
                },
            ],
            vec![],
            true,
            vec!["bearer".into()],
            fetched_at_ms,
            MAX_CARD_TTL_MS,
        )
        .unwrap()
    }

    #[test]
    fn profile_requires_a2a_backend_and_public_https_endpoint() {
        let mut invalid = profile();
        invalid.protocol_kind = ExternalAgentProtocolKind::EvoHimeV1;
        assert_eq!(
            validate_profile(invalid),
            Err(A2aError::UnsupportedProtocol)
        );

        let mut loopback = profile();
        loopback.content_hash.clear();
        loopback.endpoint.origin = "https://127.0.0.1".into();
        assert_eq!(validate_profile(loopback), Err(A2aError::EndpointDenied));

        let mut userinfo = profile();
        userinfo.content_hash.clear();
        userinfo.endpoint.origin = "https://user:pw@agents.example.com".into();
        assert_eq!(validate_profile(userinfo), Err(A2aError::EndpointDenied));
    }

    #[test]
    fn loopback_requires_explicit_opt_in_with_http() {
        let mut local = profile();
        local.content_hash.clear();
        local.endpoint = RemoteAgentEndpoint {
            origin: "http://127.0.0.1:8080".into(),
            allow_loopback: true,
            allowed_ports: vec![8080],
        };
        assert!(validate_profile(local).is_ok());
    }

    #[test]
    fn card_claims_never_grant_local_capabilities() {
        let profile = profile();
        let card = card(&profile, 0);
        assert_eq!(
            capability_intersection(&profile, &card),
            vec!["agent.execute".to_string()]
        );
        assert!(!capability_intersection(&profile, &card).contains(&"agent.admin".to_string()));
    }

    #[test]
    fn expired_or_mismatched_cards_fail_closed() {
        let profile = profile();
        let card = card(&profile, 0);
        assert_eq!(
            build_delegation(
                &profile,
                &card,
                "d1".into(),
                "grant-1".into(),
                "agent.execute".into(),
                "objective",
                PrivacyClass::Internal,
                "idem-1".into(),
                1,
                MAX_CARD_TTL_MS + 1,
            ),
            Err(A2aError::CardExpired)
        );
        assert!(build_delegation(
            &profile,
            &card,
            "d1".into(),
            "grant-1".into(),
            "agent.execute".into(),
            "objective",
            PrivacyClass::Internal,
            "idem-1".into(),
            1,
            10,
        )
        .is_ok());
    }

    #[test]
    fn delegation_rejects_denied_capability_and_privacy_above_ceiling() {
        let profile = profile();
        let card = card(&profile, 0);
        assert_eq!(
            build_delegation(
                &profile,
                &card,
                "d1".into(),
                "grant-1".into(),
                "agent.admin".into(),
                "objective",
                PrivacyClass::Internal,
                "idem-1".into(),
                1,
                10,
            ),
            Err(A2aError::CapabilityDenied)
        );
        assert_eq!(
            build_delegation(
                &profile,
                &card,
                "d1".into(),
                "grant-1".into(),
                "agent.execute".into(),
                "objective",
                PrivacyClass::Restricted,
                "idem-1".into(),
                1,
                10,
            ),
            Err(A2aError::PrivacyExceeded)
        );
    }

    #[test]
    fn capsule_binds_hashes_and_excludes_raw_context() {
        let profile = profile();
        let card = card(&profile, 0);
        let request = build_delegation(
            &profile,
            &card,
            "d1".into(),
            "grant-1".into(),
            "agent.execute".into(),
            "objective",
            PrivacyClass::Internal,
            "idem-1".into(),
            2,
            10,
        )
        .unwrap();
        assert_eq!(
            build_capsule(
                &request,
                "cap-1".into(),
                vec!["loadout.default".into()],
                "a".repeat(64),
                "b".repeat(64),
                PrivacyClass::Public,
            ),
            Err(A2aError::PrivacyExceeded)
        );
        let capsule = build_capsule(
            &request,
            "cap-1".into(),
            vec!["loadout.default".into()],
            "a".repeat(64),
            "b".repeat(64),
            PrivacyClass::Sensitive,
        )
        .unwrap();
        validate_capsule(&capsule).unwrap();
        assert!(capsule.content_hash.len() == 64);
    }

    #[test]
    fn revisions_are_hash_bound_and_tamper_evident() {
        let profile = profile();
        let card = card(&profile, 0);
        let request = build_delegation(
            &profile,
            &card,
            "d1".into(),
            "grant-1".into(),
            "agent.execute".into(),
            "objective",
            PrivacyClass::Internal,
            "idem-1".into(),
            1,
            10,
        )
        .unwrap();
        validate_delegation(&request).unwrap();
        let mut tampered = request.clone();
        tampered.capability = "agent.admin".into();
        assert_eq!(
            validate_delegation(&tampered),
            Err(A2aError::InvalidIdentifier)
        );
        assert!(validate_profile(profile).is_ok());
    }

    #[test]
    fn ambiguous_submit_becomes_unknown_and_never_blind_resubmits() {
        let profile = profile();
        let card = card(&profile, 0);
        let request = build_delegation(
            &profile,
            &card,
            "d1".into(),
            "grant-1".into(),
            "agent.execute".into(),
            "objective",
            PrivacyClass::Internal,
            "idem-1".into(),
            1,
            10,
        )
        .unwrap();
        let capsule = build_capsule(
            &request,
            "cap-1".into(),
            vec!["loadout.default".into()],
            "a".repeat(64),
            "b".repeat(64),
            PrivacyClass::Sensitive,
        )
        .unwrap();
        let mut binding =
            build_binding(&request, &capsule, "binding-1".into(), "c".repeat(64), 10).unwrap();
        apply_transition(&mut binding, RemoteTaskState::Submitting, 1, 11).unwrap();
        let revision = binding.revision;
        apply_transition(&mut binding, RemoteTaskState::UnknownOutcome, revision, 12).unwrap();
        assert_eq!(binding.state, RemoteTaskState::UnknownOutcome);
        assert_eq!(binding.reconciliation, ReconciliationDisposition::Pending);
        assert_eq!(
            transition_remote_task(RemoteTaskState::UnknownOutcome, RemoteTaskState::Registered),
            Err(A2aError::InvalidTransition)
        );
        assert_eq!(
            transition_remote_task(RemoteTaskState::UnknownOutcome, RemoteTaskState::Submitting),
            Err(A2aError::InvalidTransition)
        );
        let revision = binding.revision;
        reconcile_known_task(&mut binding, "remote-1".into(), revision, 13).unwrap();
        assert_eq!(
            binding.reconciliation,
            ReconciliationDisposition::ReconciledSameTask
        );
        let stale = binding.revision;
        assert_eq!(
            reconcile_known_task(&mut binding, "remote-2".into(), stale, 14),
            Err(A2aError::InvalidTransition)
        );
    }

    #[test]
    fn stale_revision_is_rejected_and_input_auth_states_are_typed() {
        let profile = profile();
        let card = card(&profile, 0);
        let request = build_delegation(
            &profile,
            &card,
            "d1".into(),
            "grant-1".into(),
            "agent.execute".into(),
            "objective",
            PrivacyClass::Internal,
            "idem-1".into(),
            1,
            10,
        )
        .unwrap();
        let capsule = build_capsule(
            &request,
            "cap-1".into(),
            vec!["loadout.default".into()],
            "a".repeat(64),
            "b".repeat(64),
            PrivacyClass::Sensitive,
        )
        .unwrap();
        let mut binding =
            build_binding(&request, &capsule, "binding-1".into(), "c".repeat(64), 10).unwrap();
        apply_transition(&mut binding, RemoteTaskState::Submitting, 1, 11).unwrap();
        assert_eq!(
            apply_transition(&mut binding, RemoteTaskState::Submitted, 1, 12),
            Err(A2aError::StaleRevision)
        );
        apply_transition(&mut binding, RemoteTaskState::Submitted, 2, 12).unwrap();
        assert!(binding.remote_task_id.is_none());
        let revision = binding.revision;
        apply_transition(&mut binding, RemoteTaskState::InputRequired, revision, 13).unwrap();
        assert_eq!(binding.state, RemoteTaskState::InputRequired);
        let revision = binding.revision;
        assert_eq!(
            apply_transition(&mut binding, RemoteTaskState::AuthRequired, revision, 14),
            Err(A2aError::InvalidTransition)
        );

        let mut auth =
            build_binding(&request, &capsule, "binding-2".into(), "c".repeat(64), 10).unwrap();
        apply_transition(&mut auth, RemoteTaskState::Submitting, 1, 11).unwrap();
        apply_transition(&mut auth, RemoteTaskState::Submitted, 2, 12).unwrap();
        apply_transition(&mut auth, RemoteTaskState::AuthRequired, 3, 13).unwrap();
        assert_eq!(auth.state, RemoteTaskState::AuthRequired);
        assert_eq!(
            transition_remote_task(RemoteTaskState::Completed, RemoteTaskState::Running),
            Err(A2aError::InvalidTransition)
        );
    }
}

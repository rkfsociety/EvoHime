//! Provider contract: immutable snapshot, health overlay, circuit breaker.
//!
//! Implements the full contract from plan 02.1:
//! - `RoutePolicySnapshot`: frozen at run start, contains candidates, policy hashes,
//!   capability epochs, initial health, preference, budget_id.
//! - `RunHealthOverlay`: mutable per-run state with circuit states, failure counters,
//!   cooldowns, excluded routes. Updated atomically under Core-owned lock.
//! - Circuit breaker with error categories (timeout, connection, 5xx, malformed, 429).
//! - Capability probe with bounded limits (2s connect, 10s total, 4KiB req, 64KiB resp).
//! - Policy hashes (SHA-256 of canonical policy sections).
//! - Deterministic retry with exponential backoff and jitter.
//! - Trace serialization without secrets/prompt/raw output.

use crate::routing_catalog::EvaluationCatalog;
pub use crate::routing_policy::PrivacyClass;
use crate::routing_policy::RoutingRequest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Maximum number of candidates in a snapshot.
pub const MAX_SNAPSHOT_CANDIDATES: usize = 64;
/// Maximum number of capabilities per candidate.
pub const MAX_CAPABILITIES_PER_CANDIDATE: usize = 32;
/// Maximum length for route IDs and model names.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum length for reason strings.
pub const MAX_REASON_BYTES: usize = 256;
/// Maximum number of telemetry fields in trace.
pub const MAX_TRACE_FIELDS: usize = 32;
/// Maximum bytes per telemetry value.
pub const MAX_TRACE_VALUE_BYTES: usize = 512;

/// Schema version for RoutePolicySnapshot serialization.
pub const SNAPSHOT_SCHEMA_VERSION: &str = "route-policy-snapshot-v1";
/// Schema version for RunHealthOverlay serialization.
pub const OVERLAY_SCHEMA_VERSION: &str = "run-health-overlay-v1";
/// Schema version for capability metadata.
pub const CAPABILITY_SCHEMA_VERSION: &str = "capability-metadata-v1";

/// Capability metadata for a provider/route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityMetadata {
    /// Major.minor schema version; major changes break compatibility.
    pub schema_version: String,
    /// Provider implementation version (e.g., "llama.cpp-0.2.1").
    pub provider_version: String,
    /// Monotonic epoch; increments when capabilities change.
    pub capability_epoch: u64,
    /// Supports tool calling with structured arguments.
    #[serde(default)]
    pub tool_calling: bool,
    /// Supports structured JSON output mode.
    #[serde(default)]
    pub structured_output: bool,
    /// Maximum context tokens (None = unknown/unlimited).
    pub context_limit: Option<u32>,
    /// Supports streaming token responses.
    #[serde(default)]
    pub streaming: bool,
    /// Supports vision/image input.
    #[serde(default)]
    pub vision: bool,
    /// Execution class: local (loopback) or cloud.
    pub execution_class: ExecutionClass,
    /// Maximum privacy level this provider can handle.
    pub privacy_boundary: PrivacyClass,
}

/// Execution class distinguishes local vs cloud providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionClass {
    Local,
    Cloud,
}

/// Health status from probe or observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ready,
    Unavailable,
    Stale,
    Degraded,
}

/// Initial health snapshot for a candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateHealthSnapshot {
    pub status: HealthStatus,
    /// Wall-clock timestamp when observed.
    pub observed_at: u64,
    /// TTL in milliseconds; health is stale after this.
    pub ttl_ms: u64,
    /// Circuit state at snapshot time.
    pub circuit_state: CircuitState,
    /// Last failure category if any.
    pub last_failure_category: Option<FailureCategory>,
}

impl CandidateHealthSnapshot {
    /// Creates a ready health snapshot with current timestamp.
    pub fn ready(ttl_ms: u64) -> Self {
        let now = current_time_ms();
        Self::ready_at(ttl_ms, now)
    }

    pub fn ready_at(ttl_ms: u64, now: u64) -> Self {
        Self {
            status: HealthStatus::Ready,
            observed_at: now,
            ttl_ms,
            circuit_state: CircuitState::Closed,
            last_failure_category: None,
        }
    }

    /// Checks if this health observation is still valid (not stale).
    pub fn is_fresh(&self) -> bool {
        self.is_fresh_at(current_time_ms())
    }

    pub fn is_fresh_at(&self, now: u64) -> bool {
        now < self.observed_at + self.ttl_ms
    }
}

/// Circuit breaker state for a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitState {
    /// Normal operation; requests allowed.
    Closed,
    /// Circuit opened due to failures; requests blocked.
    Open,
    /// Cooldown after rate limiting; requests blocked temporarily.
    Cooldown,
}

/// Failure categories for circuit breaker logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    /// Request timeout.
    Timeout,
    /// Connection refused/unreachable.
    ConnectionRefused,
    /// HTTP 5xx server error.
    ServerError,
    /// Malformed response from provider.
    MalformedResponse,
    /// HTTP 429 rate limit.
    RateLimited,
    /// Policy/approval denial (does NOT open circuit).
    PolicyDenied,
    /// Invalid request from client (does NOT open circuit).
    InvalidRequest,
    /// User/system cancellation (does NOT open circuit).
    Cancelled,
}

impl FailureCategory {
    /// Returns true if this category should open the circuit breaker.
    pub fn opens_circuit(self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::ConnectionRefused | Self::ServerError | Self::MalformedResponse
        )
    }

    /// Returns true if this category triggers cooldown (rate limit).
    pub fn triggers_cooldown(self) -> bool {
        matches!(self, Self::RateLimited)
    }
}

/// Hashes of policy sections for audit/verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyHashes {
    /// SHA-256 hex of canonical privacy policy section.
    pub privacy: String,
    /// SHA-256 hex of approval policy section.
    pub approval: String,
    /// SHA-256 hex of tools policy section.
    pub tools: String,
    /// SHA-256 hex of sandbox policy section.
    pub sandbox: String,
    /// SHA-256 hex of retry policy section.
    pub retry: String,
}

impl PolicyHashes {
    /// Computes SHA-256 hash of canonical JSON bytes.
    fn hash_canonical(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Creates policy hashes from canonical JSON sections.
    pub fn from_canonical_json(
        privacy_json: &[u8],
        approval_json: &[u8],
        tools_json: &[u8],
        sandbox_json: &[u8],
        retry_json: &[u8],
    ) -> Self {
        Self {
            privacy: Self::hash_canonical(privacy_json),
            approval: Self::hash_canonical(approval_json),
            tools: Self::hash_canonical(tools_json),
            sandbox: Self::hash_canonical(sandbox_json),
            retry: Self::hash_canonical(retry_json),
        }
    }
}

/// User preference for route ordering (hints only).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserPreference {
    /// Preferred route IDs in order (first available wins).
    pub preferred_order: Vec<String>,
    /// Routes to avoid if possible (not forbidden).
    pub avoid: Vec<String>,
}

/// Immutable snapshot created at run start.
///
/// Contains all candidates, their capability epochs, initial health,
/// policy hashes, user preference, and budget ID. Frozen until run ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutePolicySnapshot {
    /// Schema version for deserialization/migration.
    pub schema_version: String,
    /// Policy version identifier.
    pub policy_version: String,
    /// Unique run ID (UUIDv7 or similar).
    pub run_id: String,
    /// Candidates with their capability epochs and initial health.
    pub candidates: Vec<CandidateEntry>,
    /// Hashes of policy sections.
    pub policy_hashes: PolicyHashes,
    /// User preference hints.
    pub preference: UserPreference,
    /// Budget snapshot ID from Context Budget Manager.
    pub budget_id: Option<String>,
    /// Timestamp when snapshot was created (Unix millis).
    pub created_at: u64,
}

/// A candidate entry in the snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateEntry {
    /// Route identifier.
    pub route_id: String,
    /// Model name.
    pub model: String,
    /// Capability metadata with epoch.
    pub capabilities: CapabilityMetadata,
    /// Initial health at snapshot time.
    pub initial_health: CandidateHealthSnapshot,
    /// Cost in micros per 1K tokens.
    pub cost_micros_per_1k_tokens: u64,
    /// P95 latency in milliseconds.
    pub p95_latency_ms: u32,
    /// Privacy class.
    pub privacy: PrivacyClass,
    /// Fallback rank (lower = preferred on ties).
    pub fallback_rank: u16,
}

impl RoutePolicySnapshot {
    /// Creates a new snapshot with validated candidates.
    pub fn new(
        run_id: String,
        candidates: Vec<CandidateEntry>,
        policy_hashes: PolicyHashes,
        preference: UserPreference,
        budget_id: Option<String>,
    ) -> Result<Self, SnapshotError> {
        validate_candidates(&candidates)?;

        Self::new_at(
            run_id,
            candidates,
            policy_hashes,
            preference,
            budget_id,
            current_time_ms(),
        )
    }

    pub fn new_at(
        run_id: String,
        candidates: Vec<CandidateEntry>,
        policy_hashes: PolicyHashes,
        preference: UserPreference,
        budget_id: Option<String>,
        created_at: u64,
    ) -> Result<Self, SnapshotError> {
        validate_candidates(&candidates)?;

        Ok(Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION.to_string(),
            policy_version: "v1".to_string(),
            run_id,
            candidates,
            policy_hashes,
            preference,
            budget_id,
            created_at,
        })
    }

    /// Computes round-trip hash: serialize → deserialize → re-serialize.
    pub fn round_trip_hash(&self) -> Result<String, SnapshotError> {
        let json =
            serde_json::to_vec(self).map_err(|e| SnapshotError::Serialization(e.to_string()))?;
        let parsed: Self = serde_json::from_slice(&json)
            .map_err(|e| SnapshotError::Deserialization(e.to_string()))?;
        let json2 =
            serde_json::to_vec(&parsed).map_err(|e| SnapshotError::Serialization(e.to_string()))?;

        let mut hasher = Sha256::new();
        hasher.update(&json2);
        Ok(hex::encode(hasher.finalize()))
    }

    /// Validates that this snapshot's schema version is supported.
    pub fn validate_schema(&self) -> Result<(), SnapshotError> {
        if self.schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(SnapshotError::UnsupportedSchemaVersion(
                self.schema_version.clone(),
            ));
        }
        Ok(())
    }
}

/// Mutable health overlay for a single run.
///
/// Tracks circuit states, failure counters, cooldowns, and excluded routes
/// for the current run only. Updated atomically under Core-owned lock.
#[derive(Debug)]
pub struct RunHealthOverlay {
    /// Schema version.
    pub schema_version: String,
    /// Associated run ID for validation.
    pub run_id: String,
    /// Current generation (monotonic, incremented on each update).
    pub generation: AtomicU64,
    /// Circuit state per route.
    pub circuits: Arc<parking_lot::RwLock<BTreeMap<String, CircuitEntry>>>,
    /// Failure counters per route.
    pub failure_counters: Arc<parking_lot::RwLock<BTreeMap<String, RouteFailures>>>,
    /// Excluded routes for this run (cannot be re-selected).
    pub excluded_routes: Arc<parking_lot::RwLock<BTreeMap<String, ExclusionReason>>>,
    /// Timestamp when circuit was last opened during this run.
    pub circuit_opened_during_run: Arc<parking_lot::RwLock<Option<u64>>>,
}

// Manual Clone implementation for RunHealthOverlay since AtomicU64 doesn't implement Clone
impl Clone for RunHealthOverlay {
    fn clone(&self) -> Self {
        Self {
            schema_version: self.schema_version.clone(),
            run_id: self.run_id.clone(),
            generation: AtomicU64::new(self.generation.load(Ordering::SeqCst)),
            circuits: self.circuits.clone(),
            failure_counters: self.failure_counters.clone(),
            excluded_routes: self.excluded_routes.clone(),
            circuit_opened_during_run: self.circuit_opened_during_run.clone(),
        }
    }
}

/// Circuit entry with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitEntry {
    pub state: CircuitState,
    pub opened_at: Option<u64>,
    pub failure_count: u32,
    pub last_failure_category: Option<FailureCategory>,
    pub cooldown_until_ms: Option<u64>,
}

impl CircuitEntry {
    pub fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            opened_at: None,
            failure_count: 0,
            last_failure_category: None,
            cooldown_until_ms: None,
        }
    }
}

impl Default for CircuitEntry {
    fn default() -> Self {
        Self::new()
    }
}

/// Failure counters for a route.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RouteFailures {
    /// Total failures by category.
    pub by_category: BTreeMap<FailureCategory, u32>,
    /// Consecutive failures (for circuit threshold).
    pub consecutive: u32,
    /// Last failure timestamp (Unix millis).
    pub last_failure_at: Option<u64>,
}

/// Reason why a route was excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExclusionReason {
    pub reason: String,
    pub attempt_id: u32,
    pub generation: u64,
}

impl RunHealthOverlay {
    /// Creates a new overlay for a run.
    pub fn new(run_id: &str) -> Self {
        Self {
            schema_version: OVERLAY_SCHEMA_VERSION.to_string(),
            run_id: run_id.to_string(),
            generation: AtomicU64::new(0),
            circuits: Arc::new(parking_lot::RwLock::new(BTreeMap::new())),
            failure_counters: Arc::new(parking_lot::RwLock::new(BTreeMap::new())),
            excluded_routes: Arc::new(parking_lot::RwLock::new(BTreeMap::new())),
            circuit_opened_during_run: Arc::new(parking_lot::RwLock::new(None)),
        }
    }

    /// Records a failure and potentially opens circuit.
    ///
    /// Returns the new generation number if successful.
    pub fn record_failure(
        &self,
        route_id: &str,
        attempt_id: u32,
        category: FailureCategory,
        config: &RetryConfig,
    ) -> Result<u64, OverlayError> {
        self.record_failure_at(route_id, attempt_id, category, config, current_time_ms())
    }

    pub fn record_failure_at(
        &self,
        route_id: &str,
        attempt_id: u32,
        category: FailureCategory,
        config: &RetryConfig,
        now: u64,
    ) -> Result<u64, OverlayError> {
        // Update failure counters
        {
            let mut counters = self.failure_counters.write();
            let entry = counters.entry(route_id.to_string()).or_default();
            *entry.by_category.entry(category).or_insert(0) += 1;
            entry.consecutive = entry.consecutive.saturating_add(1);
            entry.last_failure_at = Some(now);
        }

        // Handle cooldown for rate limits
        if category.triggers_cooldown() {
            let counter = self
                .failure_counters
                .read()
                .get(route_id)
                .map(|f| {
                    f.by_category
                        .get(&FailureCategory::RateLimited)
                        .copied()
                        .unwrap_or(0)
                })
                .unwrap_or(0);

            if counter >= config.rate_limit_threshold {
                let mut circuits = self.circuits.write();
                let entry = circuits.entry(route_id.to_string()).or_default();
                entry.state = CircuitState::Cooldown;
                entry.cooldown_until_ms = Some(now + config.cooldown_ms);
                entry.last_failure_category = Some(category);

                let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
                return Ok(gen);
            }
        }

        // Handle circuit breaker for other failures
        if category.opens_circuit() {
            let counter = self
                .failure_counters
                .read()
                .get(route_id)
                .map(|f| f.consecutive)
                .unwrap_or(0);

            if counter >= config.failure_threshold {
                let mut circuits = self.circuits.write();
                let entry = circuits.entry(route_id.to_string()).or_default();
                entry.state = CircuitState::Open;
                entry.opened_at = Some(now);
                entry.failure_count = counter;
                entry.last_failure_category = Some(category);

                // Mark that circuit was opened during this run
                *self.circuit_opened_during_run.write() = Some(now);

                // Exclude this route
                self.exclude_route(
                    route_id,
                    attempt_id,
                    format!("circuit_opened:{:?}", category),
                )?;

                let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
                return Ok(gen);
            }
        }

        Ok(self.generation.load(Ordering::SeqCst))
    }

    /// Excludes a route from further selection in this run.
    pub fn exclude_route(
        &self,
        route_id: &str,
        attempt_id: u32,
        reason: impl Into<String>,
    ) -> Result<u64, OverlayError> {
        let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let mut excluded = self.excluded_routes.write();
        excluded.insert(
            route_id.to_string(),
            ExclusionReason {
                reason: bound_string(reason.into()),
                attempt_id,
                generation: gen,
            },
        );
        Ok(gen)
    }

    /// Checks if a route is excluded.
    pub fn is_excluded(&self, route_id: &str) -> bool {
        self.excluded_routes.read().contains_key(route_id)
    }

    /// Gets circuit state for a route.
    pub fn get_circuit_state(&self, route_id: &str) -> CircuitState {
        self.circuits
            .read()
            .get(route_id)
            .map(|e| e.state)
            .unwrap_or(CircuitState::Closed)
    }

    /// Checks if cooldown has expired for a route.
    pub fn is_cooldown_expired(&self, route_id: &str) -> bool {
        self.is_cooldown_expired_at(route_id, current_time_ms())
    }

    pub fn is_cooldown_expired_at(&self, route_id: &str, now: u64) -> bool {
        let circuits = self.circuits.read();
        match circuits.get(route_id) {
            Some(entry) if entry.state == CircuitState::Cooldown => {
                entry.cooldown_until_ms.is_none_or(|until| now >= until)
            }
            _ => true,
        }
    }

    /// Resets consecutive failure counter on success.
    pub fn record_success(&self, route_id: &str) {
        if let Some(entry) = self.failure_counters.write().get_mut(route_id) {
            entry.consecutive = 0;
        }
    }

    /// Gets current generation.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Checks if circuit was opened during this run.
    pub fn circuit_opened_during_run(&self) -> bool {
        self.circuit_opened_during_run.read().is_some()
    }
}

/// Retry configuration with bounded limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum attempts per run.
    pub max_attempts: u32,
    /// Maximum attempts per individual route.
    pub max_attempts_per_route: u32,
    /// Initial backoff in milliseconds.
    pub initial_backoff_ms: u64,
    /// Maximum backoff cap in milliseconds.
    pub max_backoff_ms: u64,
    /// Jitter ratio (0.0-1.0).
    pub jitter_ratio: f64,
    /// Maximum total elapsed time in milliseconds.
    pub max_elapsed_ms: u64,
    /// Failure threshold to open circuit.
    pub failure_threshold: u32,
    /// Rate limit threshold to trigger cooldown.
    pub rate_limit_threshold: u32,
    /// Cooldown duration in milliseconds.
    pub cooldown_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            max_attempts_per_route: 2,
            initial_backoff_ms: 250,
            max_backoff_ms: 4000,
            jitter_ratio: 0.20,
            max_elapsed_ms: 15000,
            failure_threshold: 2,
            rate_limit_threshold: 3,
            cooldown_ms: 30000,
        }
    }
}

impl RetryConfig {
    /// Validates configuration bounds.
    pub fn validate(&self) -> Result<(), RetryConfigError> {
        if self.max_attempts == 0 || self.max_attempts > 128 {
            return Err(RetryConfigError::InvalidMaxAttempts);
        }
        if self.max_attempts_per_route == 0 || self.max_attempts_per_route > 64 {
            return Err(RetryConfigError::InvalidMaxAttemptsPerRoute);
        }
        if self.initial_backoff_ms == 0 || self.initial_backoff_ms > 10000 {
            return Err(RetryConfigError::InvalidBackoff);
        }
        if self.max_backoff_ms < self.initial_backoff_ms || self.max_backoff_ms > 60000 {
            return Err(RetryConfigError::InvalidBackoff);
        }
        if self.jitter_ratio < 0.0 || self.jitter_ratio > 1.0 {
            return Err(RetryConfigError::InvalidJitter);
        }
        if self.max_elapsed_ms == 0 || self.max_elapsed_ms > 300000 {
            return Err(RetryConfigError::InvalidMaxElapsed);
        }
        if self.failure_threshold == 0 || self.failure_threshold > 10 {
            return Err(RetryConfigError::InvalidThreshold);
        }
        if self.rate_limit_threshold == 0 || self.rate_limit_threshold > 10 {
            return Err(RetryConfigError::InvalidThreshold);
        }
        if self.cooldown_ms < 1000 || self.cooldown_ms > 300000 {
            return Err(RetryConfigError::InvalidCooldown);
        }
        Ok(())
    }

    /// Computes deterministic backoff with jitter.
    pub fn compute_backoff(&self, attempt: u32, run_id: &str, route_id: &str) -> Duration {
        let exponent = attempt.saturating_sub(1).min(63);
        let base = self
            .initial_backoff_ms
            .saturating_mul(2_u64.saturating_pow(exponent));
        let capped = base.min(self.max_backoff_ms);

        // Deterministic jitter from hash
        let seed = format!("{}|{}|{}", run_id, route_id, attempt);
        let hash = {
            let mut h = Sha256::new();
            h.update(seed.as_bytes());
            let digest = h.finalize();
            u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
        };

        let jitter_range = (capped as f64 * self.jitter_ratio) as u64;
        let jitter = if jitter_range > 0 {
            hash as u64 % jitter_range
        } else {
            0
        };

        let backoff_ms = capped.saturating_add(jitter).min(self.max_backoff_ms);
        Duration::from_millis(backoff_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotCandidateDecision {
    pub route_id: String,
    pub capability_epoch: u64,
    pub health_status: HealthStatus,
    pub circuit_state: CircuitState,
    pub reject_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotRouteDecision {
    pub selected_route: Option<String>,
    pub fallback_chain: Vec<String>,
    pub candidates: Vec<SnapshotCandidateDecision>,
    pub reason_code: String,
}

/// Canonical Core-owned selection over an immutable snapshot and a per-run
/// overlay. It has no clock, filesystem, provider or budget-manager access.
pub fn select_route_snapshot(
    request: &RoutingRequest,
    snapshot: &RoutePolicySnapshot,
    overlay: &RunHealthOverlay,
    catalog: Option<&EvaluationCatalog>,
    attempt_id: u32,
    now_ms: u64,
) -> Result<SnapshotRouteDecision, SnapshotError> {
    snapshot.validate_schema()?;
    if snapshot.run_id.is_empty() || attempt_id > 1_000_000 {
        return Err(SnapshotError::MissingField("run_id/attempt_id".into()));
    }
    let mut decisions = Vec::with_capacity(snapshot.candidates.len());
    let mut eligible: Vec<&CandidateEntry> = Vec::new();
    let unknown_class = request
        .task_class
        .as_deref()
        .is_none_or(|class| class != "simple" && class != "complex");
    for candidate in &snapshot.candidates {
        let mut reject = None;
        let health = &candidate.initial_health;
        let mut status = health.status;
        let circuit = overlay.get_circuit_state(&candidate.route_id);
        if !health.is_fresh_at(now_ms) {
            status = HealthStatus::Stale;
        }
        if candidate.privacy < request.required_privacy {
            reject = Some("privacy_violation");
        } else if request.offline && candidate.capabilities.execution_class != ExecutionClass::Local
        {
            reject = Some("offline_mode");
        } else if (unknown_class && candidate.capabilities.execution_class == ExecutionClass::Cloud)
            || (!request.allow_cloud
                && candidate.capabilities.execution_class != ExecutionClass::Local)
        {
            // Незавершённая классификация и запрет облака дают один и тот же
            // отказ: кандидат не доказал, что исполняется локально.
            reject = Some("classification_incomplete");
        } else if request
            .required_capabilities
            .iter()
            .any(|cap| !candidate_supports_capability(candidate, cap))
        {
            reject = Some("capability_missing");
        } else if candidate
            .capabilities
            .context_limit
            .is_some_and(|limit| request.estimated_input_tokens > limit)
        {
            reject = Some("context_limit_exceeded");
        } else if request
            .max_cost_micros_per_1k_tokens
            .is_some_and(|limit| candidate.cost_micros_per_1k_tokens > limit)
        {
            reject = Some("budget_exceeded");
        } else if request
            .max_latency_ms
            .is_some_and(|limit| candidate.p95_latency_ms > limit)
        {
            reject = Some("latency_exceeded");
        } else if candidate.capabilities.execution_class == ExecutionClass::Local
            && request.task_class.as_deref() == Some("simple")
        {
            let large = snapshot
                .candidates
                .iter()
                .find(|other| other.capabilities.execution_class == ExecutionClass::Cloud);
            let gate_ok = large
                .and_then(|large| {
                    catalog.map(|cat| {
                        cat.small_route_allowed(
                            "simple",
                            &large.route_id,
                            &candidate.route_id,
                            request.quality_delta,
                            now_ms,
                        )
                    })
                })
                .unwrap_or(false);
            if !gate_ok {
                reject = Some("gate_unavailable");
            }
        }
        if reject.is_none() && overlay.is_excluded(&candidate.route_id) {
            reject = Some("route_attempts_exhausted");
        } else if reject.is_none() && circuit != CircuitState::Closed {
            reject = Some("circuit_open");
        } else if reject.is_none() && status == HealthStatus::Unavailable {
            reject = Some("health_unavailable");
        } else if reject.is_none() && status == HealthStatus::Stale {
            reject = Some("health_stale");
        }
        decisions.push(SnapshotCandidateDecision {
            route_id: candidate.route_id.clone(),
            capability_epoch: candidate.capabilities.capability_epoch,
            health_status: status,
            circuit_state: circuit,
            reject_reason: reject.map(str::to_owned),
        });
        if reject.is_none() {
            eligible.push(candidate);
        }
    }
    eligible.sort_by(|left, right| {
        let preferred = |candidate: &CandidateEntry| request.preference_rank(&candidate.route_id);
        health_rank(left, now_ms)
            .cmp(&health_rank(right, now_ms))
            .then(left.p95_latency_ms.cmp(&right.p95_latency_ms))
            .then(
                left.cost_micros_per_1k_tokens
                    .cmp(&right.cost_micros_per_1k_tokens),
            )
            .then(preferred(left).cmp(&preferred(right)))
            .then(left.route_id.cmp(&right.route_id))
    });
    let selected_route = eligible.first().map(|candidate| candidate.route_id.clone());
    let fallback_chain = eligible
        .iter()
        .skip(1)
        .take(16)
        .map(|candidate| candidate.route_id.clone())
        .collect::<Vec<_>>();
    let reason_code = if snapshot.candidates.is_empty() {
        "no_routes_configured"
    } else if selected_route.is_none() {
        "all_routes_excluded"
    } else if attempt_id > 0 {
        "fallback_selection"
    } else {
        "policy_selection"
    };
    Ok(SnapshotRouteDecision {
        selected_route,
        fallback_chain,
        candidates: decisions,
        reason_code: reason_code.into(),
    })
}

fn candidate_supports_capability(candidate: &CandidateEntry, capability: &str) -> bool {
    match capability {
        "chat" => true,
        "tools" | "tool_calling" => candidate.capabilities.tool_calling,
        "structured_output" => candidate.capabilities.structured_output,
        "streaming" => candidate.capabilities.streaming,
        "vision" => candidate.capabilities.vision,
        _ => false,
    }
}

fn health_rank(candidate: &CandidateEntry, now_ms: u64) -> u8 {
    if !candidate.initial_health.is_fresh_at(now_ms) {
        return 3;
    }
    match candidate.initial_health.status {
        HealthStatus::Ready => 0,
        HealthStatus::Degraded => 1,
        HealthStatus::Stale => 2,
        HealthStatus::Unavailable => 3,
    }
}

trait PreferenceRank {
    fn preference_rank(&self, route_id: &str) -> usize;
}
impl PreferenceRank for RoutingRequest {
    fn preference_rank(&self, route_id: &str) -> usize {
        if self.preferred_route.as_deref() == Some(route_id) {
            0
        } else {
            1
        }
    }
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Startup probe configuration and result.
#[derive(Debug, Clone)]
pub struct ProbeConfig {
    /// Connect timeout (default 2s).
    pub connect_timeout: Duration,
    /// Total request timeout (default 10s).
    pub total_timeout: Duration,
    /// Maximum request size in bytes (default 4 KiB).
    pub max_request_bytes: usize,
    /// Maximum response size in bytes (default 64 KiB).
    pub max_response_bytes: usize,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(2),
            total_timeout: Duration::from_secs(10),
            max_request_bytes: 4 * 1024,
            max_response_bytes: 64 * 1024,
        }
    }
}

/// Result of startup probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeResult {
    /// Probe succeeded, provider is ready.
    Ready(CapabilityMetadata),
    /// Probe failed, provider unavailable.
    Unavailable(ProbeFailure),
    /// Partial capabilities (some features missing).
    Partial(CapabilityMetadata, Vec<String>),
}

/// Probe failure reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeFailure {
    Timeout,
    ConnectionRefused,
    MalformedResponse,
    SchemaMismatch(String),
    SizeLimitExceeded,
    Cancellation,
}

/// Errors for snapshot operations.
#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("too many candidates: {0} > {MAX_SNAPSHOT_CANDIDATES}")]
    TooManyCandidates(usize),
    #[error("invalid route ID: {0}")]
    InvalidRouteId(String),
    #[error("invalid model name: {0}")]
    InvalidModelName(String),
    #[error("duplicate route ID: {0}")]
    DuplicateRouteId(String),
    #[error("unsupported schema version: {0}")]
    UnsupportedSchemaVersion(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("deserialization error: {0}")]
    Deserialization(String),
    #[error("round-trip hash mismatch")]
    RoundTripHashMismatch,
    #[error("missing required field: {0}")]
    MissingField(String),
}

/// Errors for overlay operations.
#[derive(Debug, Error)]
pub enum OverlayError {
    #[error("run ID mismatch")]
    RunIdMismatch,
    #[error("stale generation: expected >= {0}, got {1}")]
    StaleGeneration(u64, u64),
    #[error("invalid attempt ID: must be monotonic")]
    InvalidAttemptId,
}

/// Errors for retry configuration.
#[derive(Debug, Error)]
pub enum RetryConfigError {
    #[error("invalid max_attempts")]
    InvalidMaxAttempts,
    #[error("invalid max_attempts_per_route")]
    InvalidMaxAttemptsPerRoute,
    #[error("invalid backoff configuration")]
    InvalidBackoff,
    #[error("invalid jitter ratio")]
    InvalidJitter,
    #[error("invalid max_elapsed_ms")]
    InvalidMaxElapsed,
    #[error("invalid threshold")]
    InvalidThreshold,
    #[error("invalid cooldown_ms")]
    InvalidCooldown,
}

/// Trace event for a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunTrace {
    /// Run ID.
    pub run_id: String,
    /// Snapshot schema version.
    pub snapshot_schema: String,
    /// Policy hash.
    pub policy_hash: String,
    /// Ordered list of attempts.
    pub attempts: Vec<AttemptTrace>,
    /// Whether circuit was opened during run.
    pub circuit_opened_during_run: bool,
    /// Final result.
    pub result: RunResult,
    /// Additional telemetry fields (bounded, redacted).
    pub fields: BTreeMap<String, String>,
}

/// Attempt trace within a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptTrace {
    /// Attempt index (0-based).
    pub attempt_id: u32,
    /// The clock value supplied to selection for deterministic replay.
    pub now_ms: u64,
    /// Selected route ID.
    pub route_id: String,
    /// Capability epoch at attempt time.
    pub capability_epoch: u64,
    /// Selection reason.
    pub selection_reason: String,
    /// Failure category if failed.
    pub failure_category: Option<FailureCategory>,
    /// Backoff duration in milliseconds.
    pub backoff_ms: u64,
    /// Overlay generation at attempt time.
    pub overlay_generation: u64,
}

/// Final result of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunResult {
    Success,
    Failed,
    Cancelled,
    RouteExhausted,
}

impl RunTrace {
    /// Creates a new trace for a run.
    pub fn new(run_id: String, policy_hash: String, snapshot_schema: String) -> Self {
        Self {
            run_id,
            snapshot_schema,
            policy_hash,
            attempts: Vec::new(),
            circuit_opened_during_run: false,
            result: RunResult::Failed,
            fields: BTreeMap::new(),
        }
    }

    /// Adds an attempt to the trace.
    pub fn add_attempt(&mut self, attempt: AttemptTrace) {
        self.attempts.push(attempt);
    }

    /// Sets the final result.
    pub fn set_result(&mut self, result: RunResult) {
        self.result = result;
    }

    /// Adds a telemetry field (bounded, redacted).
    pub fn add_field(&mut self, name: impl Into<String>, value: impl Into<String>) {
        if self.fields.len() >= MAX_TRACE_FIELDS {
            return;
        }
        let name = redact_field_name(name.into());
        if name.is_empty() {
            return;
        }
        self.fields.insert(name, bound_string(value.into()));
    }

    /// Serializes to deterministic JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("RunTrace is serializable")
    }
}

/// Validates candidate entries for snapshot.
fn validate_candidates(candidates: &[CandidateEntry]) -> Result<(), SnapshotError> {
    if candidates.len() > MAX_SNAPSHOT_CANDIDATES {
        return Err(SnapshotError::TooManyCandidates(candidates.len()));
    }

    let mut seen_ids = std::collections::HashSet::new();
    for c in candidates {
        validate_identifier(&c.route_id, "route_id")?;
        validate_identifier(&c.model, "model")?;

        if !seen_ids.insert(&c.route_id) {
            return Err(SnapshotError::DuplicateRouteId(c.route_id.clone()));
        }

        if c.capabilities.schema_version.is_empty() {
            return Err(SnapshotError::MissingField(
                "capabilities.schema_version".into(),
            ));
        }
    }

    Ok(())
}

/// Validates an identifier (route ID, model name, etc.).
fn validate_identifier(value: &str, field: &str) -> Result<(), SnapshotError> {
    if value.is_empty() || value.len() > MAX_ID_BYTES {
        return Err(SnapshotError::InvalidRouteId(format!(
            "{} is empty or too long",
            field
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(SnapshotError::InvalidRouteId(format!(
            "{} contains control characters",
            field
        )));
    }
    let lower = value.to_ascii_lowercase();
    if [
        "api_key",
        "apikey",
        "authorization",
        "bearer",
        "token",
        "secret",
    ]
    .iter()
    .any(|m| lower.contains(m))
    {
        return Err(SnapshotError::InvalidRouteId(format!(
            "{} contains secret-like marker",
            field
        )));
    }
    Ok(())
}

/// Bounds a string to maximum trace value size.
fn bound_string(value: String) -> String {
    value.chars().take(MAX_TRACE_VALUE_BYTES).collect()
}

/// Redacts field names that look like secrets.
fn redact_field_name(value: String) -> String {
    let lower = value.to_ascii_lowercase();
    if [
        "api_key",
        "apikey",
        "authorization",
        "bearer",
        "password",
        "secret",
        "token",
    ]
    .iter()
    .any(|m| lower.contains(m))
    {
        String::new()
    } else {
        bound_string(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_candidate(route_id: &str, epoch: u64) -> CandidateEntry {
        CandidateEntry {
            route_id: route_id.to_string(),
            model: format!("{}-model", route_id),
            capabilities: CapabilityMetadata {
                schema_version: CAPABILITY_SCHEMA_VERSION.to_string(),
                provider_version: "test-1.0".to_string(),
                capability_epoch: epoch,
                tool_calling: true,
                structured_output: true,
                context_limit: Some(32000),
                streaming: true,
                vision: false,
                execution_class: ExecutionClass::Local,
                privacy_boundary: PrivacyClass::Internal,
            },
            initial_health: CandidateHealthSnapshot::ready(60000),
            cost_micros_per_1k_tokens: 1,
            p95_latency_ms: 100,
            privacy: PrivacyClass::Internal,
            fallback_rank: 0,
        }
    }

    #[test]
    fn snapshot_creation_validates_candidates() {
        let candidates = vec![make_candidate("local-1", 1), make_candidate("cloud-1", 2)];
        let hashes = PolicyHashes::from_canonical_json(b"{}", b"{}", b"{}", b"{}", b"{}");
        let snapshot = RoutePolicySnapshot::new(
            "run-123".to_string(),
            candidates,
            hashes,
            UserPreference::default(),
            Some("budget-456".to_string()),
        )
        .expect("valid snapshot");

        assert_eq!(snapshot.schema_version, SNAPSHOT_SCHEMA_VERSION);
        assert_eq!(snapshot.run_id, "run-123");
        assert_eq!(snapshot.candidates.len(), 2);
    }

    #[test]
    fn snapshot_rejects_too_many_candidates() {
        let candidates: Vec<_> = (0..MAX_SNAPSHOT_CANDIDATES + 1)
            .map(|i| make_candidate(&format!("route-{}", i), i as u64))
            .collect();
        let hashes = PolicyHashes::from_canonical_json(b"{}", b"{}", b"{}", b"{}", b"{}");
        let result = RoutePolicySnapshot::new(
            "run-123".to_string(),
            candidates,
            hashes,
            UserPreference::default(),
            Some("budget-456".to_string()),
        );
        assert!(matches!(result, Err(SnapshotError::TooManyCandidates(_))));
    }

    #[test]
    fn snapshot_rejects_secret_like_route_ids() {
        let candidates = vec![make_candidate("api_key_route", 1)];
        let hashes = PolicyHashes::from_canonical_json(b"{}", b"{}", b"{}", b"{}", b"{}");
        let result = RoutePolicySnapshot::new(
            "run-123".to_string(),
            candidates,
            hashes,
            UserPreference::default(),
            Some("budget-456".to_string()),
        );
        assert!(matches!(result, Err(SnapshotError::InvalidRouteId(_))));
    }

    #[test]
    fn snapshot_selector_applies_offline_health_and_preference_without_clock_reads() {
        let mut local = make_candidate("local-1", 1);
        let mut cloud = make_candidate("cloud-1", 1);
        cloud.capabilities.execution_class = ExecutionClass::Cloud;
        local.initial_health = CandidateHealthSnapshot::ready_at(1_000, 1_000);
        cloud.initial_health = CandidateHealthSnapshot::ready_at(1_000, 1_000);
        let snapshot = RoutePolicySnapshot::new_at(
            "run-select".into(),
            vec![local, cloud],
            PolicyHashes::from_canonical_json(b"p", b"a", b"t", b"s", b"r"),
            UserPreference {
                preferred_order: vec!["cloud-1".into()],
                avoid: Vec::new(),
            },
            None,
            1_000,
        )
        .expect("snapshot");
        let overlay = RunHealthOverlay::new("run-select");
        let request = RoutingRequest {
            required_capabilities: vec!["chat".into()],
            max_cost_micros_per_1k_tokens: None,
            max_latency_ms: None,
            required_privacy: PrivacyClass::Internal,
            allow_fallback: true,
            preferred_route: Some("cloud-1".into()),
            task_class: Some("complex".into()),
            offline: true,
            allow_cloud: true,
            estimated_input_tokens: 10,
            quality_delta: 0.05,
        };
        let decision =
            select_route_snapshot(&request, &snapshot, &overlay, None, 0, 1_050).expect("decision");
        assert_eq!(decision.selected_route.as_deref(), Some("local-1"));
        assert_eq!(
            decision
                .candidates
                .iter()
                .find(|c| c.route_id == "cloud-1")
                .and_then(|c| c.reject_reason.as_deref()),
            Some("offline_mode")
        );
    }

    #[test]
    fn health_overlay_records_failures_and_opens_circuit() {
        let overlay = RunHealthOverlay::new("run-123");
        let config = RetryConfig::default();

        // First failure
        let gen = overlay
            .record_failure("route-1", 1, FailureCategory::Timeout, &config)
            .expect("recorded");
        assert_eq!(overlay.get_circuit_state("route-1"), CircuitState::Closed);

        // Second failure should open circuit (threshold=2)
        let gen2 = overlay
            .record_failure("route-1", 2, FailureCategory::Timeout, &config)
            .expect("recorded");
        assert!(gen2 > gen);
        assert_eq!(overlay.get_circuit_state("route-1"), CircuitState::Open);
        assert!(overlay.is_excluded("route-1"));
        assert!(overlay.circuit_opened_during_run());
    }

    #[test]
    fn health_overlay_handles_rate_limit_cooldown() {
        let overlay = RunHealthOverlay::new("run-123");
        let config = RetryConfig::default();

        // Three rate limit failures should trigger cooldown
        for i in 1..=3 {
            overlay
                .record_failure("route-1", i, FailureCategory::RateLimited, &config)
                .expect("recorded");
        }

        assert_eq!(overlay.get_circuit_state("route-1"), CircuitState::Cooldown);
        assert!(!overlay.is_cooldown_expired("route-1"));
    }

    #[test]
    fn retry_config_computes_deterministic_backoff() {
        let config = RetryConfig::default();
        let backoff1 = config.compute_backoff(1, "run-a", "route-1");
        let backoff2 = config.compute_backoff(1, "run-a", "route-1");
        let backoff3 = config.compute_backoff(1, "run-b", "route-1");

        // Same inputs give same backoff
        assert_eq!(backoff1, backoff2);
        // Different run gives different backoff (due to hash)
        assert_ne!(backoff1, backoff3);
    }

    #[test]
    fn trace_serialization_excludes_secrets() {
        let mut trace = RunTrace::new(
            "run-123".to_string(),
            "hash-abc".to_string(),
            SNAPSHOT_SCHEMA_VERSION.to_string(),
        );
        trace.add_field("safe_field", "safe_value");
        trace.add_field("api_key", "secret123");
        trace.add_field("another_token", "tok_abc");

        let json = trace.to_json();
        assert!(json.contains("safe_field"));
        assert!(!json.contains("api_key"));
        assert!(!json.contains("another_token"));
    }

    #[test]
    fn round_trip_hash_matches() {
        let candidates = vec![make_candidate("local-1", 1)];
        let hashes = PolicyHashes::from_canonical_json(b"{}", b"{}", b"{}", b"{}", b"{}");
        let snapshot = RoutePolicySnapshot::new(
            "run-123".to_string(),
            candidates,
            hashes,
            UserPreference::default(),
            Some("budget-456".to_string()),
        )
        .expect("valid snapshot");

        let hash1 = snapshot.round_trip_hash().expect("hash computed");
        let hash2 = snapshot.round_trip_hash().expect("hash computed again");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn health_is_fresh_within_ttl() {
        let health = CandidateHealthSnapshot::ready(60000); // 60 second TTL
        assert!(health.is_fresh());
    }

    #[test]
    fn failure_categories_correctly_classified() {
        assert!(FailureCategory::Timeout.opens_circuit());
        assert!(FailureCategory::ConnectionRefused.opens_circuit());
        assert!(FailureCategory::ServerError.opens_circuit());
        assert!(FailureCategory::MalformedResponse.opens_circuit());
        assert!(FailureCategory::RateLimited.triggers_cooldown());
        assert!(!FailureCategory::PolicyDenied.opens_circuit());
        assert!(!FailureCategory::InvalidRequest.opens_circuit());
        assert!(!FailureCategory::Cancelled.opens_circuit());
    }
}

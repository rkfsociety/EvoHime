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

use crate::routing_policy::PrivacyClass;
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
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
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
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
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
            Self::Timeout
                | Self::ConnectionRefused
                | Self::ServerError
                | Self::MalformedResponse
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
    pub budget_id: String,
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
        budget_id: String,
    ) -> Result<Self, SnapshotError> {
        validate_candidates(&candidates)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Ok(Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION.to_string(),
            policy_version: "v1".to_string(),
            run_id,
            candidates,
            policy_hashes,
            preference,
            budget_id,
            created_at: now,
        })
    }

    /// Computes round-trip hash: serialize → deserialize → re-serialize.
    pub fn round_trip_hash(&self) -> Result<String, SnapshotError> {
        let json = serde_json::to_vec(self).map_err(|e| SnapshotError::Serialization(e.to_string()))?;
        let parsed: Self = serde_json::from_slice(&json)
            .map_err(|e| SnapshotError::Deserialization(e.to_string()))?;
        let json2 = serde_json::to_vec(&parsed).map_err(|e| SnapshotError::Serialization(e.to_string()))?;
        
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
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

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
                .map(|f| f.by_category.get(&FailureCategory::RateLimited).copied().unwrap_or(0))
                .unwrap_or(0);

            if counter >= config.rate_limit_threshold {
                let mut circuits = self.circuits.write();
                let entry = circuits.entry(route_id.to_string()).or_insert_with(CircuitEntry::new);
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
                let entry = circuits.entry(route_id.to_string()).or_insert_with(CircuitEntry::new);
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
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let circuits = self.circuits.read();
        match circuits.get(route_id) {
            Some(entry) if entry.state == CircuitState::Cooldown => {
                entry.cooldown_until_ms.map_or(true, |until| now >= until)
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
        let base = self.initial_backoff_ms * 2_u64.pow(attempt.saturating_sub(1));
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

        let backoff_ms = capped + jitter;
        Duration::from_millis(backoff_ms)
    }
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
            return Err(SnapshotError::MissingField("capabilities.schema_version".into()));
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
    if ["api_key", "apikey", "authorization", "bearer", "token", "secret"]
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
    if ["api_key", "apikey", "authorization", "bearer", "password", "secret", "token"]
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
        let candidates = vec![
            make_candidate("local-1", 1),
            make_candidate("cloud-1", 2),
        ];
        let hashes = PolicyHashes::from_canonical_json(b"{}", b"{}", b"{}", b"{}", b"{}");
        let snapshot = RoutePolicySnapshot::new(
            "run-123".to_string(),
            candidates,
            hashes,
            UserPreference::default(),
            "budget-456".to_string(),
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
            "budget-456".to_string(),
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
            "budget-456".to_string(),
        );
        assert!(matches!(result, Err(SnapshotError::InvalidRouteId(_))));
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
        let mut trace = RunTrace::new("run-123".to_string(), "hash-abc".to_string(), SNAPSHOT_SCHEMA_VERSION.to_string());
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
            "budget-456".to_string(),
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

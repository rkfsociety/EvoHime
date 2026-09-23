//! Bounded runtime contract for model routing.
//!
//! This module plans and records a run. It does not call a provider, perform
//! network I/O, or store credentials. Provider execution remains outside the
//! contract and must consume the explicit route and budget decisions here.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

use crate::routing_policy::{select_route, RouteCandidate, RoutingDecision, RoutingRequest};

/// Hard upper bound for planned run iterations.
pub const MAX_ITERATIONS: u32 = 128;
/// Hard upper bound for tool calls in one run.
pub const MAX_TOOL_CALLS: u32 = 512;
/// Hard upper bound for accounted model tokens in one run.
pub const MAX_TOKENS: u64 = 2_000_000;
/// Hard upper bound for run wall-clock duration in milliseconds.
pub const MAX_WALL_CLOCK_MS: u64 = 3_600_000;
/// Maximum number of extra telemetry fields retained per run.
pub const MAX_TELEMETRY_FIELDS: usize = 24;
/// Maximum number of characters retained in a telemetry field value.
pub const MAX_TELEMETRY_VALUE_BYTES: usize = 512;

/// Route population policy applied when building a run plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingMode {
    /// Prefer local routes and fall back when none are available.
    LocalFirst,
    /// Consider all eligible local and cloud routes under policy ranking.
    Balanced,
    /// Restrict route candidates to cloud research providers.
    CloudResearch,
    /// Restrict routing to local/offline candidates.
    Offline,
}

/// Lifecycle states supported by [`RoutingRuntime`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    /// A route and limits have been planned but execution has not started.
    Planned,
    /// The run is currently consuming its route and resource budget.
    Running,
    /// Execution is temporarily suspended and can be resumed.
    Paused,
    /// Execution was stopped before completion.
    Stopped,
    /// Execution completed successfully.
    Completed,
    /// Execution failed, including when a budget was exceeded.
    Failed,
}

/// Caller-selected limits that must stay within gateway hard bounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeLimits {
    /// Maximum iterations permitted for the run.
    pub max_iterations: u32,
    /// Maximum tool calls permitted for the run.
    pub max_tool_calls: u32,
    /// Maximum accounted model tokens.
    pub max_tokens: u64,
    /// Maximum elapsed time in milliseconds.
    pub wall_clock_ms: u64,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_iterations: 16,
            max_tool_calls: 64,
            max_tokens: 100_000,
            wall_clock_ms: 15 * 60 * 1000,
        }
    }
}

impl RuntimeLimits {
    /// Rejects zero limits and values above the gateway hard caps.
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.max_iterations == 0 || self.max_iterations > MAX_ITERATIONS {
            return Err(RuntimeError::LimitOutOfBounds("max_iterations"));
        }
        if self.max_tool_calls == 0 || self.max_tool_calls > MAX_TOOL_CALLS {
            return Err(RuntimeError::LimitOutOfBounds("max_tool_calls"));
        }
        if self.max_tokens == 0 || self.max_tokens > MAX_TOKENS {
            return Err(RuntimeError::LimitOutOfBounds("max_tokens"));
        }
        if self.wall_clock_ms == 0 || self.wall_clock_ms > MAX_WALL_CLOCK_MS {
            return Err(RuntimeError::LimitOutOfBounds("wall_clock_ms"));
        }
        Ok(())
    }
}

/// Accumulated resource usage for a routing run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeUsage {
    /// Iterations recorded so far.
    pub iterations: u32,
    /// Tool calls recorded so far.
    pub tool_calls: u32,
    /// Model tokens accounted so far.
    pub tokens: u64,
    /// Elapsed run time in milliseconds.
    pub elapsed_ms: u64,
}

/// Observable route transition or available fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FallbackNotice {
    /// Route that was abandoned or preferred before fallback.
    pub from_route: String,
    /// Route selected as the fallback destination.
    pub to_route: String,
    /// Stable reason explaining why fallback was used or exposed.
    pub reason: String,
}

/// Bounded, redacted runtime telemetry for one routed execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingTelemetry {
    /// Mode used to filter and rank routes.
    pub mode: RoutingMode,
    /// Current lifecycle state.
    pub state: RuntimeState,
    /// Selected route identifier, when planned.
    pub route: Option<String>,
    /// Selected model identifier, when planned.
    pub model: Option<String>,
    /// Stable reason for the current runtime decision.
    pub reason: String,
    /// Fallback information, if applicable.
    pub fallback: Option<FallbackNotice>,
    /// Accumulated run resource usage.
    pub usage: RuntimeUsage,
    /// Additional bounded fields, with secret-like names or values rejected.
    pub fields: BTreeMap<String, String>,
}

impl RoutingTelemetry {
    fn new(mode: RoutingMode, reason: impl Into<String>) -> Self {
        Self {
            mode,
            state: RuntimeState::Planned,
            route: None,
            model: None,
            reason: bound_value(reason.into()),
            fallback: None,
            usage: RuntimeUsage::default(),
            fields: BTreeMap::new(),
        }
    }

    /// Serializes telemetry deterministically using its ordered field map.
    pub fn to_deterministic_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Inserts a bounded redacted field unless the telemetry field cap is reached.
    pub fn insert_field(&mut self, name: impl Into<String>, value: impl Into<String>) {
        if self.fields.len() >= MAX_TELEMETRY_FIELDS {
            return;
        }
        let name = redact_name(name.into());
        if name.is_empty() {
            return;
        }
        self.fields.insert(name, bound_value(value.into()));
    }
}

/// Failure to plan a route, transition state, or remain within runtime bounds.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// A requested resource limit is zero or exceeds a hard cap.
    #[error("runtime limit is out of bounds: {0}")]
    LimitOutOfBounds(&'static str),
    /// No candidate satisfied the selected routing mode and request policy.
    #[error("no route is eligible for the selected routing mode")]
    NoRoute,
    /// An operation is not valid in the runtime's current state.
    #[error("invalid state transition from {0:?}")]
    InvalidTransition(RuntimeState),
    /// A tracked resource exceeded its configured run limit.
    #[error("runtime budget exceeded: {0}")]
    BudgetExceeded(&'static str),
    /// Telemetry input contained a secret-like name or value.
    #[error("runtime telemetry field is secret-like")]
    SecretLikeTelemetry,
}

/// Plans a route and enforces lifecycle transitions and per-run resource bounds.
#[derive(Debug, Clone)]
pub struct RoutingRuntime {
    mode: RoutingMode,
    limits: RuntimeLimits,
    usage: RuntimeUsage,
    state: RuntimeState,
    decision: RoutingDecision,
    telemetry: RoutingTelemetry,
}

impl RoutingRuntime {
    /// Selects an eligible route and constructs a planned runtime with validated limits.
    pub fn plan(
        mode: RoutingMode,
        request: &RoutingRequest,
        candidates: &[RouteCandidate],
        limits: RuntimeLimits,
    ) -> Result<Self, RuntimeError> {
        limits.validate()?;
        let (decision, fallback_reason) = choose_mode(mode, request, candidates)?;
        let mut telemetry = RoutingTelemetry::new(mode, "route_planned");
        telemetry.route = decision.selected_route.clone();
        telemetry.model = decision.selected_model.clone();
        if let Some(reason) = fallback_reason {
            if let Some(to) = decision.selected_route.clone() {
                telemetry.fallback = Some(FallbackNotice {
                    from_route: "local".into(),
                    to_route: to,
                    reason,
                });
            }
        } else if let (Some(from), Some(to)) = (
            decision.selected_route.clone(),
            decision.fallback_chain.first().cloned(),
        ) {
            telemetry.fallback = Some(FallbackNotice {
                from_route: from,
                to_route: to,
                reason: "visible_fallback_available".into(),
            });
        }
        Ok(Self {
            mode,
            limits,
            usage: RuntimeUsage::default(),
            state: RuntimeState::Planned,
            decision,
            telemetry,
        })
    }

    /// Starts a planned run and returns its immutable route decision.
    pub fn start(&mut self) -> Result<&RoutingDecision, RuntimeError> {
        if self.state != RuntimeState::Planned {
            return Err(RuntimeError::InvalidTransition(self.state));
        }
        self.state = RuntimeState::Running;
        self.telemetry.state = self.state;
        Ok(&self.decision)
    }

    /// Pauses a running runtime.
    pub fn pause(&mut self) -> Result<(), RuntimeError> {
        self.transition(RuntimeState::Running, RuntimeState::Paused)
    }

    /// Resumes a paused runtime.
    pub fn resume(&mut self) -> Result<(), RuntimeError> {
        self.transition(RuntimeState::Paused, RuntimeState::Running)
    }

    /// Stops a running or paused runtime.
    pub fn stop(&mut self) -> Result<(), RuntimeError> {
        match self.state {
            RuntimeState::Running | RuntimeState::Paused => {
                self.state = RuntimeState::Stopped;
                self.telemetry.state = self.state;
                Ok(())
            }
            state => Err(RuntimeError::InvalidTransition(state)),
        }
    }

    /// Marks a running runtime as successfully completed.
    pub fn complete(&mut self) -> Result<(), RuntimeError> {
        self.transition(RuntimeState::Running, RuntimeState::Completed)
    }

    /// Accounts one iteration and fails the runtime when its iteration budget is exceeded.
    pub fn record_iteration(&mut self) -> Result<(), RuntimeError> {
        self.require_running()?;
        self.usage.iterations = self.usage.iterations.saturating_add(1);
        if self.usage.iterations > self.limits.max_iterations {
            self.fail_budget("max_iterations")
        } else {
            self.sync_usage();
            Ok(())
        }
    }

    /// Accounts one tool call and its token usage against configured limits.
    pub fn record_tool_call(&mut self, tokens: u64) -> Result<(), RuntimeError> {
        self.require_running()?;
        self.usage.tool_calls = self.usage.tool_calls.saturating_add(1);
        self.usage.tokens = self.usage.tokens.saturating_add(tokens);
        if self.usage.tool_calls > self.limits.max_tool_calls {
            return self.fail_budget("max_tool_calls");
        }
        if self.usage.tokens > self.limits.max_tokens {
            return self.fail_budget("max_tokens");
        }
        self.sync_usage();
        Ok(())
    }

    /// Updates elapsed time and fails the runtime when its wall-clock budget is exceeded.
    pub fn record_elapsed(&mut self, elapsed_ms: u64) -> Result<(), RuntimeError> {
        self.require_running()?;
        self.usage.elapsed_ms = elapsed_ms;
        if elapsed_ms > self.limits.wall_clock_ms {
            return self.fail_budget("wall_clock_ms");
        }
        self.sync_usage();
        Ok(())
    }

    /// Adds a bounded field after rejecting secret-like names and values.
    pub fn add_telemetry_field(
        &mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        let name = name.into();
        let value = value.into();
        if is_secret_like(&name) || is_secret_like(&value) {
            return Err(RuntimeError::SecretLikeTelemetry);
        }
        self.telemetry.insert_field(name, value);
        Ok(())
    }

    /// Returns the current lifecycle state.
    pub fn state(&self) -> RuntimeState {
        self.state
    }

    /// Режим, в котором план был построен: снимок решения обязан объяснять
    /// сам себя, не полагаясь на то, что вызывающий помнит аргумент `plan`.
    pub fn mode(&self) -> RoutingMode {
        self.mode
    }

    /// Returns the immutable route decision selected during planning.
    pub fn decision(&self) -> &RoutingDecision {
        &self.decision
    }

    /// Returns the current redacted telemetry snapshot.
    pub fn telemetry(&self) -> &RoutingTelemetry {
        &self.telemetry
    }

    fn transition(&mut self, from: RuntimeState, to: RuntimeState) -> Result<(), RuntimeError> {
        if self.state != from {
            return Err(RuntimeError::InvalidTransition(self.state));
        }
        self.state = to;
        self.telemetry.state = to;
        Ok(())
    }

    fn require_running(&self) -> Result<(), RuntimeError> {
        if self.state == RuntimeState::Running {
            Ok(())
        } else {
            Err(RuntimeError::InvalidTransition(self.state))
        }
    }

    fn fail_budget<T>(&mut self, budget: &'static str) -> Result<T, RuntimeError> {
        self.state = RuntimeState::Failed;
        self.telemetry.state = self.state;
        self.sync_usage();
        Err(RuntimeError::BudgetExceeded(budget))
    }

    fn sync_usage(&mut self) {
        self.telemetry.usage = self.usage.clone();
    }
}

fn choose_mode(
    mode: RoutingMode,
    request: &RoutingRequest,
    candidates: &[RouteCandidate],
) -> Result<(RoutingDecision, Option<String>), RuntimeError> {
    let mut filtered = candidates
        .iter()
        .filter(|candidate| match mode {
            RoutingMode::Balanced => true,
            RoutingMode::Offline => candidate.available && is_local_route(candidate),
            RoutingMode::CloudResearch => candidate.available && is_cloud_route(candidate),
            RoutingMode::LocalFirst => candidate.available && is_local_route(candidate),
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut fallback_reason = None;
    if filtered.is_empty() && mode == RoutingMode::LocalFirst {
        filtered = candidates.to_vec();
        fallback_reason = Some("local_route_unavailable".into());
    }
    if filtered.is_empty() {
        return Err(RuntimeError::NoRoute);
    }
    select_route(request, &filtered)
        .map(|decision| (decision, fallback_reason))
        .map_err(|_| RuntimeError::NoRoute)
}

fn is_local_route(candidate: &RouteCandidate) -> bool {
    let route = format!("{} {}", candidate.route_id, candidate.model).to_ascii_lowercase();
    route.contains("local") || route.contains("offline")
}

fn is_cloud_route(candidate: &RouteCandidate) -> bool {
    let route = format!("{} {}", candidate.route_id, candidate.model).to_ascii_lowercase();
    route.contains("cloud") || route.contains("research")
}

fn is_secret_like(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "api_key",
        "apikey",
        "authorization",
        "bearer",
        "password",
        "secret",
        "token",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn redact_name(value: String) -> String {
    if is_secret_like(&value) {
        String::new()
    } else {
        bound_value(value)
    }
}

fn bound_value(value: String) -> String {
    value.chars().take(MAX_TELEMETRY_VALUE_BYTES).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing_policy::PrivacyClass;

    fn route(id: &str, cost: u64) -> RouteCandidate {
        RouteCandidate {
            route_id: id.into(),
            model: format!("{id}-model"),
            capabilities: vec!["chat".into()],
            cost_micros_per_1k_tokens: cost,
            p95_latency_ms: 100,
            privacy: PrivacyClass::Internal,
            available: true,
            fallback_rank: 0,
        }
    }

    fn request() -> RoutingRequest {
        RoutingRequest {
            required_capabilities: vec!["chat".into()],
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

    #[test]
    fn mode_selection_is_visible_and_offline_never_uses_cloud() {
        let mut runtime = RoutingRuntime::plan(
            RoutingMode::Offline,
            &request(),
            &[route("cloud-research", 1), route("local", 5)],
            RuntimeLimits::default(),
        )
        .expect("offline route");
        runtime.start().expect("start");
        assert_eq!(runtime.decision().selected_route.as_deref(), Some("local"));
        assert!(!runtime
            .telemetry()
            .to_deterministic_json()
            .expect("telemetry serializes")
            .contains("api_key"));
    }

    #[test]
    fn local_first_emits_fallback_when_local_is_unavailable() {
        let mut local = route("local", 1);
        local.available = false;
        let mut runtime = RoutingRuntime::plan(
            RoutingMode::LocalFirst,
            &request(),
            &[local, route("cloud", 2)],
            RuntimeLimits::default(),
        )
        .expect("fallback route");
        runtime.start().expect("start");
        assert_eq!(runtime.decision().selected_route.as_deref(), Some("cloud"));
        assert_eq!(
            runtime
                .telemetry()
                .fallback
                .as_ref()
                .map(|item| item.reason.as_str()),
            Some("local_route_unavailable")
        );
    }

    #[test]
    fn lifecycle_supports_pause_resume_stop_and_rejects_invalid_transitions() {
        let mut runtime = RoutingRuntime::plan(
            RoutingMode::Balanced,
            &request(),
            &[route("local", 1)],
            RuntimeLimits::default(),
        )
        .expect("plan");
        assert_eq!(
            runtime.pause(),
            Err(RuntimeError::InvalidTransition(RuntimeState::Planned))
        );
        runtime.start().expect("start");
        runtime.pause().expect("pause");
        runtime.resume().expect("resume");
        runtime.stop().expect("stop");
        assert_eq!(runtime.state(), RuntimeState::Stopped);
        assert_eq!(
            runtime.resume(),
            Err(RuntimeError::InvalidTransition(RuntimeState::Stopped))
        );
    }

    #[test]
    fn budgets_fail_closed_and_telemetry_rejects_secrets() {
        let limits = RuntimeLimits {
            max_iterations: 1,
            max_tool_calls: 1,
            max_tokens: 10,
            wall_clock_ms: 100,
        };
        let mut runtime = RoutingRuntime::plan(
            RoutingMode::Balanced,
            &request(),
            &[route("local", 1)],
            limits,
        )
        .expect("plan");
        runtime.start().expect("start");
        runtime.record_tool_call(11).expect_err("token budget");
        assert_eq!(runtime.state(), RuntimeState::Failed);
        assert_eq!(
            runtime.add_telemetry_field("api_key", "hidden"),
            Err(RuntimeError::SecretLikeTelemetry)
        );
    }

    #[test]
    fn telemetry_json_is_deterministic_and_bounded() {
        let mut runtime = RoutingRuntime::plan(
            RoutingMode::Balanced,
            &request(),
            &[route("local", 1)],
            RuntimeLimits::default(),
        )
        .expect("plan");
        runtime.add_telemetry_field("zeta", "last").expect("field");
        runtime
            .add_telemetry_field("alpha", "first")
            .expect("field");
        let json = runtime
            .telemetry()
            .to_deterministic_json()
            .expect("telemetry serializes");
        assert!(json.find("alpha").unwrap() < json.find("zeta").unwrap());
        assert!(json.len() < 32 * 1024);
    }

    #[test]
    fn invalid_limits_and_unavailable_modes_are_rejected() {
        let invalid = RuntimeLimits {
            max_iterations: 0,
            ..RuntimeLimits::default()
        };
        assert_eq!(
            RoutingRuntime::plan(
                RoutingMode::Balanced,
                &request(),
                &[route("local", 1)],
                invalid
            )
            .expect_err("invalid limits"),
            RuntimeError::LimitOutOfBounds("max_iterations")
        );
        assert_eq!(
            RoutingRuntime::plan(
                RoutingMode::CloudResearch,
                &request(),
                &[route("local", 1)],
                RuntimeLimits::default()
            )
            .expect_err("no cloud"),
            RuntimeError::NoRoute
        );
    }
}

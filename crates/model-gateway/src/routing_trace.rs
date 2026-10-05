//! Versioned redacted routing trace shared by Core and desktop IPC.

use serde::{Deserialize, Serialize};

/// Schema version serialized by [`RoutingTrace`].
pub const ROUTING_TRACE_SCHEMA_VERSION: u32 = 2;

/// Stable terminal outcomes emitted by the routing runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalStatus {
    /// The run completed with a selected route.
    Success,
    /// The run was cancelled before completion.
    Cancelled,
    /// No provider route was configured.
    NoRoutesConfigured,
    /// Every configured route was unavailable.
    BothRoutesUnavailable,
    /// Required request classification could not be completed.
    ClassificationIncomplete,
    /// The assembled context exceeded the route's supported limit.
    ContextLimitExceeded,
    /// Routing policy rejected the request.
    PolicyViolation,
    /// The required execution budget was unavailable.
    BudgetUnavailable,
    /// Context construction failed.
    ContextAssemblyFailed,
    /// The configured fallback limit was reached.
    FallbackLimitReached,
    /// The run exceeded its deadline.
    RunDeadlineExceeded,
    /// Required approval for rerouting was declined.
    RerouteApprovalDeclined,
    /// An internal failure prevented a terminal result.
    InternalError,
}

/// User-safe actions that can follow a routing refusal or failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeNextAction {
    /// Retry the request after a delay.
    RetryLater,
    /// Ask the caller to clarify the request.
    ClarifyRequest,
    /// Contact the product support channel.
    ContactSupport,
    /// Have an operator review the request or trace.
    ManualReview,
}

/// Coarse availability classification for one route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    /// No probe or provider observation is available.
    Unknown,
    /// Route health permits normal use.
    Healthy,
    /// Route remains usable with reduced confidence or capability.
    Degraded,
    /// Route is not eligible for dispatch.
    Unavailable,
}

/// Data-sensitivity label carried in a redacted route trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLabel {
    /// Request content requires sensitive-data handling.
    Sensitive,
    /// Request content is classified as non-sensitive.
    NonSensitive,
    /// Privacy classification is not available.
    Unknown,
}

/// Redacted status of one route considered during routing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceCandidate {
    /// Stable route identifier.
    pub route_id: String,
    /// Capability revision observed for the route.
    pub capability_epoch: u64,
    /// Provider health status observed during selection.
    pub health_status: HealthStatus,
    /// Circuit-breaker state observed during selection.
    pub circuit_state: CircuitState,
    /// Combined route-health classification.
    pub health_state: HealthState,
    /// Stable reason code when this candidate was rejected.
    pub reject_reason: Option<String>,
}

pub use crate::provider_contract::{CircuitState, HealthStatus};

/// Versioned, redacted explanation of one routing decision or terminal outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingTrace {
    /// Serialized trace schema version.
    pub schema_version: u32,
    /// Unique trace identifier.
    pub trace_id: String,
    /// Run identifier that owns the trace.
    pub run_id: String,
    /// Monotonic trace sequence within the run.
    pub sequence: u64,
    /// Zero-based routing attempt number.
    pub attempt_id: u32,
    /// Event timestamp in Unix milliseconds.
    pub now_ms: u64,
    /// Version of the routing policy applied.
    pub policy_version: String,
    /// Version of the provider catalog consulted.
    pub catalog_version: String,
    /// Digest of the immutable routing snapshot.
    pub snapshot_hash: String,
    /// Coarse request classification used for routing.
    pub classification: String,
    /// Privacy category assigned to the request.
    pub privacy_label: PrivacyLabel,
    /// Bounded redacted summaries of considered routes.
    pub candidates: Vec<TraceCandidate>,
    /// Selected route for a successful decision.
    pub selected_route: Option<String>,
    /// Stable reason code for the decision or terminal outcome.
    pub reason_code: String,
    /// Number of route fallbacks performed.
    pub fallback_count: u32,
    /// Event category represented by this trace.
    pub event: String,
    /// Elapsed routing time in milliseconds.
    pub latency_ms: u64,
    /// Terminal outcome when this trace closes the run.
    pub terminal_status: Option<TerminalStatus>,
    /// Suggested safe action for a non-success terminal outcome.
    pub safe_next_action: Option<SafeNextAction>,
    /// Identifier of the budget applied, when present.
    pub budget_id: Option<String>,
    /// Explicitly distinguishes an absent budget from a missing field.
    pub budget_absent: bool,
    /// Estimated input size in model tokens.
    pub estimated_input_tokens: u32,
    /// Version of the selected routing profile, when available.
    pub profile_version: Option<String>,
    /// Digest of the context ledger used for the request.
    pub context_ledger_hash: Option<String>,
}

impl RoutingTrace {
    /// Checks schema identity and relationships between route, status, and budget fields.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != ROUTING_TRACE_SCHEMA_VERSION {
            return Err("unsupported_schema_version");
        }
        if self.trace_id.is_empty() || self.run_id.is_empty() || self.policy_version.is_empty() {
            return Err("missing_identity");
        }
        if self.terminal_status == Some(TerminalStatus::Success) && self.selected_route.is_none() {
            return Err("success_requires_route");
        }
        if self.terminal_status != Some(TerminalStatus::Success) && self.selected_route.is_some() {
            return Err("refusal_forbids_route");
        }
        if self.budget_id.is_none() && !self.budget_absent {
            return Err("budget_presence_ambiguous");
        }
        if self
            .terminal_status
            .is_some_and(TerminalStatus::requires_safe_action)
            && self.safe_next_action.is_none()
        {
            return Err("refusal_requires_safe_action");
        }
        if self.terminal_status == Some(TerminalStatus::Success) && self.safe_next_action.is_some()
        {
            return Err("success_forbids_safe_action");
        }
        Ok(())
    }
    /// Serializes this trace as one compact JSON line.
    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

impl TerminalStatus {
    /// Returns whether a terminal outcome requires an accompanying safe action.
    pub fn requires_safe_action(self) -> bool {
        !matches!(self, Self::Success | Self::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trace_requires_explicit_budget_absence() {
        let trace = RoutingTrace {
            schema_version: ROUTING_TRACE_SCHEMA_VERSION,
            trace_id: "t".into(),
            run_id: "r".into(),
            sequence: 1,
            attempt_id: 0,
            now_ms: 10,
            policy_version: "p".into(),
            catalog_version: "c".into(),
            snapshot_hash: "h".into(),
            classification: "simple".into(),
            privacy_label: PrivacyLabel::NonSensitive,
            candidates: vec![],
            selected_route: None,
            reason_code: "internal_error".into(),
            fallback_count: 0,
            event: "terminal".into(),
            latency_ms: 0,
            terminal_status: Some(TerminalStatus::InternalError),
            safe_next_action: Some(SafeNextAction::ContactSupport),
            budget_id: None,
            budget_absent: true,
            estimated_input_tokens: 0,
            profile_version: None,
            context_ledger_hash: None,
        };
        assert!(trace.validate().is_ok());
    }

    #[test]
    fn refusal_requires_a_safe_next_action() {
        let mut trace = RoutingTrace {
            schema_version: ROUTING_TRACE_SCHEMA_VERSION,
            trace_id: "t".into(),
            run_id: "r".into(),
            sequence: 1,
            now_ms: 10,
            attempt_id: 0,
            policy_version: "p".into(),
            catalog_version: "c".into(),
            snapshot_hash: "h".into(),
            classification: "simple".into(),
            privacy_label: PrivacyLabel::Unknown,
            candidates: vec![],
            selected_route: None,
            reason_code: "internal_error".into(),
            fallback_count: 0,
            event: "terminal".into(),
            latency_ms: 0,
            terminal_status: Some(TerminalStatus::InternalError),
            safe_next_action: None,
            budget_id: None,
            budget_absent: true,
            estimated_input_tokens: 0,
            profile_version: None,
            context_ledger_hash: None,
        };
        assert_eq!(trace.validate(), Err("refusal_requires_safe_action"));
        trace.safe_next_action = Some(SafeNextAction::ContactSupport);
        assert!(trace.validate().is_ok());
    }
}

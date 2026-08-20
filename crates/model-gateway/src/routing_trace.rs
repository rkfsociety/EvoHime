//! Versioned redacted routing trace shared by Core and desktop IPC.

use serde::{Deserialize, Serialize};

pub const ROUTING_TRACE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalStatus {
    Success, Cancelled, NoRoutesConfigured, BothRoutesUnavailable,
    ClassificationIncomplete, ContextLimitExceeded, PolicyViolation,
    BudgetUnavailable, ContextAssemblyFailed, FallbackLimitReached,
    RunDeadlineExceeded, RerouteApprovalDeclined, InternalError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeNextAction { RetryLater, ClarifyRequest, ContactSupport, ManualReview }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState { Healthy, Degraded, Unavailable }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLabel { Sensitive, NonSensitive, Unknown }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceCandidate {
    pub route_id: String,
    pub capability_epoch: u64,
    pub health_status: HealthStatus,
    pub circuit_state: CircuitState,
    pub health_state: HealthState,
    pub reject_reason: Option<String>,
}

pub use crate::provider_contract::{CircuitState, HealthStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingTrace {
    pub schema_version: u32,
    pub trace_id: String,
    pub run_id: String,
    pub sequence: u64,
    pub attempt_id: u32,
    pub now_ms: u64,
    pub policy_version: String,
    pub catalog_version: String,
    pub snapshot_hash: String,
    pub classification: String,
    pub privacy_label: PrivacyLabel,
    pub candidates: Vec<TraceCandidate>,
    pub selected_route: Option<String>,
    pub reason_code: String,
    pub fallback_count: u32,
    pub event: String,
    pub latency_ms: u64,
    pub terminal_status: Option<TerminalStatus>,
    pub safe_next_action: Option<SafeNextAction>,
    pub budget_id: Option<String>,
    pub budget_absent: bool,
    pub estimated_input_tokens: u32,
    pub profile_version: Option<String>,
    pub context_ledger_hash: Option<String>,
}

impl RoutingTrace {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != ROUTING_TRACE_SCHEMA_VERSION { return Err("unsupported_schema_version"); }
        if self.trace_id.is_empty() || self.run_id.is_empty() || self.policy_version.is_empty() { return Err("missing_identity"); }
        if self.terminal_status == Some(TerminalStatus::Success) && self.selected_route.is_none() { return Err("success_requires_route"); }
        if self.terminal_status != Some(TerminalStatus::Success) && self.selected_route.is_some() { return Err("refusal_forbids_route"); }
        if self.budget_id.is_none() && !self.budget_absent { return Err("budget_presence_ambiguous"); }
        if self.terminal_status.is_some_and(TerminalStatus::requires_safe_action) && self.safe_next_action.is_none() { return Err("refusal_requires_safe_action"); }
        if self.terminal_status == Some(TerminalStatus::Success) && self.safe_next_action.is_some() { return Err("success_forbids_safe_action"); }
        Ok(())
    }
    pub fn to_json_line(&self) -> Result<String, serde_json::Error> { serde_json::to_string(self) }
}

impl TerminalStatus {
    pub fn requires_safe_action(self) -> bool {
        !matches!(self, Self::Success | Self::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trace_requires_explicit_budget_absence() {
        let trace = RoutingTrace { schema_version: 1, trace_id: "t".into(), run_id: "r".into(), sequence: 1, attempt_id: 0, now_ms: 10, policy_version: "p".into(), catalog_version: "c".into(), snapshot_hash: "h".into(), classification: "simple".into(), privacy_label: PrivacyLabel::NonSensitive, candidates: vec![], selected_route: None, reason_code: "internal_error".into(), fallback_count: 0, event: "terminal".into(), latency_ms: 0, terminal_status: Some(TerminalStatus::InternalError), safe_next_action: Some(SafeNextAction::ContactSupport), budget_id: None, budget_absent: true, estimated_input_tokens: 0, profile_version: None, context_ledger_hash: None };
        assert!(trace.validate().is_ok());
    }

    #[test]
    fn refusal_requires_a_safe_next_action() {
        let mut trace = RoutingTrace { schema_version: 1, trace_id: "t".into(), run_id: "r".into(), sequence: 1, now_ms: 10, attempt_id: 0, policy_version: "p".into(), catalog_version: "c".into(), snapshot_hash: "h".into(), classification: "simple".into(), privacy_label: PrivacyLabel::Unknown, candidates: vec![], selected_route: None, reason_code: "internal_error".into(), fallback_count: 0, event: "terminal".into(), latency_ms: 0, terminal_status: Some(TerminalStatus::InternalError), safe_next_action: None, budget_id: None, budget_absent: true, estimated_input_tokens: 0, profile_version: None, context_ledger_hash: None };
        assert_eq!(trace.validate(), Err("refusal_requires_safe_action"));
        trace.safe_next_action = Some(SafeNextAction::ContactSupport);
        assert!(trace.validate().is_ok());
    }
}

use evohime_model_gateway::ToolSpec;

use crate::AgentRunError;

/// Версия контракта политики выбора маршрута.
pub(crate) const ROUTING_POLICY_VERSION: &str = "routing-policy-v1";
/// Версия встроенного каталога маршрутов.
pub(crate) const ROUTING_CATALOG_VERSION: &str = "builtin-v1";
/// Имя терминального события в трассировке маршрутизации.
pub(crate) const ROUTING_EVENT_TERMINAL: &str = "terminal";
/// Причина отката к локальному выбору runtime.
pub(crate) const ROUTING_SNAPSHOT_FALLBACK: &str = "runtime-selection";

pub(crate) struct RoutingSuccessInput<'a> {
    pub run_id: &'a str,
    pub selected_route: &'a str,
    pub fallback_count: usize,
    pub estimated_input_tokens: u32,
    pub profile_version: &'a str,
    pub context_ledger_hash: &'a str,
    pub classification: &'a str,
    pub decision: Option<&'a evohime_model_gateway::SnapshotRouteDecision>,
    pub snapshot_hash: Option<&'a str>,
    pub attempt_id: u32,
    pub now_ms: u64,
}

pub(crate) fn routing_success_trace(
    input: RoutingSuccessInput<'_>,
) -> evohime_model_gateway::RoutingTrace {
    let candidates = input
        .decision
        .map(|decision| {
            decision
                .candidates
                .iter()
                .map(|candidate| {
                    let health_state = match candidate.health_status {
                        evohime_model_gateway::HealthStatus::Ready => {
                            evohime_model_gateway::HealthState::Healthy
                        }
                        evohime_model_gateway::HealthStatus::Degraded => {
                            evohime_model_gateway::HealthState::Degraded
                        }
                        evohime_model_gateway::HealthStatus::Stale
                        | evohime_model_gateway::HealthStatus::Unavailable => {
                            evohime_model_gateway::HealthState::Unavailable
                        }
                    };
                    evohime_model_gateway::TraceCandidate {
                        route_id: candidate.route_id.clone(),
                        capability_epoch: candidate.capability_epoch,
                        health_status: candidate.health_status,
                        circuit_state: candidate.circuit_state,
                        health_state,
                        reject_reason: candidate.reject_reason.clone(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    evohime_model_gateway::RoutingTrace {
        schema_version: 1,
        trace_id: input.run_id.to_owned(),
        run_id: input.run_id.to_owned(),
        sequence: 1,
        attempt_id: input.attempt_id,
        now_ms: input.now_ms,
        policy_version: ROUTING_POLICY_VERSION.into(),
        catalog_version: ROUTING_CATALOG_VERSION.into(),
        snapshot_hash: input
            .snapshot_hash
            .unwrap_or(ROUTING_SNAPSHOT_FALLBACK)
            .into(),
        classification: input.classification.into(),
        privacy_label: evohime_model_gateway::PrivacyLabel::NonSensitive,
        candidates,
        selected_route: Some(input.selected_route.to_owned()),
        reason_code: input
            .decision
            .map(|decision| decision.reason_code.clone())
            .unwrap_or_else(|| {
                if input.fallback_count > 0 {
                    "fallback_rank_preferred".into()
                } else {
                    "only_candidate".into()
                }
            }),
        fallback_count: input.fallback_count as u32,
        event: ROUTING_EVENT_TERMINAL.into(),
        latency_ms: 0,
        terminal_status: Some(evohime_model_gateway::TerminalStatus::Success),
        safe_next_action: None,
        budget_id: None,
        budget_absent: true,
        estimated_input_tokens: input.estimated_input_tokens,
        profile_version: Some(input.profile_version.to_owned()),
        context_ledger_hash: Some(input.context_ledger_hash.to_owned()),
    }
}

pub(crate) fn routing_failure_trace(
    run_id: &str,
    error: &AgentRunError,
) -> evohime_model_gateway::RoutingTrace {
    let (status, reason, action) = match error {
        AgentRunError::Cancelled => (
            evohime_model_gateway::TerminalStatus::Cancelled,
            "cancelled",
            None,
        ),
        AgentRunError::Timeout(_) => (
            evohime_model_gateway::TerminalStatus::RunDeadlineExceeded,
            "run_deadline_exceeded",
            Some(evohime_model_gateway::SafeNextAction::RetryLater),
        ),
        AgentRunError::BudgetUnavailable { .. } => (
            evohime_model_gateway::TerminalStatus::BudgetUnavailable,
            "budget_unavailable",
            Some(evohime_model_gateway::SafeNextAction::ClarifyRequest),
        ),
        AgentRunError::Provider(_) => (
            evohime_model_gateway::TerminalStatus::BothRoutesUnavailable,
            "provider_unavailable",
            Some(evohime_model_gateway::SafeNextAction::RetryLater),
        ),
        AgentRunError::RoutingApprovalDeclined => (
            evohime_model_gateway::TerminalStatus::RerouteApprovalDeclined,
            "reroute_approval_declined",
            Some(evohime_model_gateway::SafeNextAction::ManualReview),
        ),
        AgentRunError::Internal(_) => (
            evohime_model_gateway::TerminalStatus::InternalError,
            "internal_error",
            Some(evohime_model_gateway::SafeNextAction::ContactSupport),
        ),
    };
    evohime_model_gateway::RoutingTrace {
        schema_version: 1,
        trace_id: run_id.to_owned(),
        run_id: run_id.to_owned(),
        sequence: 1,
        attempt_id: 0,
        now_ms: crate::task_memory::now_millis(),
        policy_version: ROUTING_POLICY_VERSION.into(),
        catalog_version: ROUTING_CATALOG_VERSION.into(),
        snapshot_hash: ROUTING_SNAPSHOT_FALLBACK.into(),
        classification: "complex".into(),
        privacy_label: evohime_model_gateway::PrivacyLabel::Unknown,
        candidates: Vec::new(),
        selected_route: None,
        reason_code: reason.into(),
        fallback_count: 0,
        event: ROUTING_EVENT_TERMINAL.into(),
        latency_ms: 0,
        terminal_status: Some(status),
        safe_next_action: action,
        budget_id: None,
        budget_absent: true,
        estimated_input_tokens: 0,
        profile_version: None,
        context_ledger_hash: None,
    }
}

pub(crate) fn classify_routing_task(prompt: &str, tools: &[ToolSpec]) -> &'static str {
    let lower = prompt.to_ascii_lowercase();
    let mutation_markers = [
        "запиши",
        "измени",
        "удали",
        "создай",
        "commit",
        "push",
        "write",
        "patch",
        "execute",
    ];
    let read_only = !mutation_markers.iter().any(|marker| lower.contains(marker))
        && tools.len() <= 8
        && !lower.contains("multi-hop");
    if read_only {
        "simple"
    } else {
        "complex"
    }
}

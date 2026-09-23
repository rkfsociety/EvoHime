//! Core-owned, replay-safe termination condition algebra.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current schema version for composable termination policies.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum number of leaf and composite nodes in one expression tree.
pub const MAX_NODES: usize = 64;
/// Maximum byte length of policy, condition, and event text identifiers.
pub const MAX_TEXT: usize = 256;

/// Measurable or event-driven condition that may end a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionKind {
    /// Stop after a maximum number of messages.
    MaxMessages,
    /// Stop after a maximum number of agent turns.
    MaxTurns,
    /// Stop after a maximum number of tool calls.
    MaxToolCalls,
    /// Stop after combined input and output tokens reach a budget.
    TokenBudget,
    /// Stop after accumulated cost reaches a budget.
    CostBudget,
    /// Stop after elapsed wall-clock time reaches a deadline.
    WallClockTimeout,
    /// Stop after an inactivity interval.
    IdleTimeout,
    /// Stop when a stop event is received.
    StopEvent,
    /// Stop when the event source contains the configured text.
    SourceMatch,
    /// Stop when a handoff has been reached.
    HandoffReached,
    /// Stop when the event carries a matching external signal.
    ExternalSignal,
    /// Stop when the goal enters a configured state.
    GoalStateReached,
    /// Stop when the workflow enters a configured state.
    WorkflowStateReached,
}
/// Boolean composition used to combine child termination conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Composition {
    /// Trigger when any child condition matches.
    Any,
    /// Trigger only when all child conditions match the same event.
    All,
}
/// Run outcome produced after a terminal condition fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalOutcome {
    /// No terminal condition has fired; execution may continue.
    Continue,
    /// Run completed according to its termination policy.
    Completed,
    /// Run was paused for later continuation.
    Paused,
    /// A configured resource budget was exhausted.
    BudgetExhausted,
    /// A wall-clock deadline was reached.
    Timeout,
    /// Run terminated after an execution failure.
    Failed,
}
/// Event data evaluated against a termination expression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminationEvent {
    /// Stable event identifier used for replay detection.
    pub event_id: String,
    /// Event category, including `stop` for explicit stop events.
    pub kind: String,
    /// Source associated with the event.
    pub source: String,
    /// Message count observed by the run.
    pub messages: u64,
    /// Turn count observed by the run.
    pub turns: u64,
    /// Tool-call count observed by the run.
    pub tool_calls: u64,
    /// Input token count observed by the run.
    pub input_tokens: u64,
    /// Output token count observed by the run.
    pub output_tokens: u64,
    /// Accumulated cost in micro-units.
    pub cost_micros: u64,
    /// Elapsed wall-clock time in milliseconds.
    pub elapsed_ms: u64,
    /// Idle time in milliseconds.
    pub idle_ms: u64,
    /// Optional current goal state.
    pub goal_state: Option<String>,
    /// Optional current workflow state.
    pub workflow_state: Option<String>,
    /// Optional external signal identifier.
    pub signal: Option<String>,
    /// Whether a handoff boundary has been reached.
    pub handoff_reached: bool,
}
/// Recursive leaf or boolean combination in a termination policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminationExpression {
    /// Leaf condition evaluated directly against one event.
    Condition {
        /// Stable identifier returned when this leaf fires.
        id: String,
        /// Event property or signal evaluated by this condition.
        kind: ConditionKind,
        /// Positive threshold for numeric conditions.
        threshold: u64,
        /// Optional value used by source, state, or signal matching.
        text: Option<String>,
    },
    /// Boolean composition over one or more child expressions.
    Composite {
        /// Rule used to combine all child expressions.
        mode: Composition,
        /// Non-empty bounded child expression set.
        children: Vec<TerminationExpression>,
    },
}
/// Integrity-bound policy controlling when a run should stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminationPolicy {
    /// Policy schema version.
    pub schema_version: u32,
    /// Stable policy identifier.
    pub id: String,
    /// Monotonic policy revision.
    pub version: u64,
    /// Expression evaluated against each new event.
    pub expression: TerminationExpression,
    /// Whether a fired condition should halt the run immediately.
    pub hard_stop: bool,
    /// Digest of the policy with this field cleared.
    pub content_hash: String,
}
/// Replay cursor and durable outcome for a termination policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminationState {
    /// State schema version.
    pub schema_version: u32,
    /// Policy revision currently evaluated.
    pub policy_version: u64,
    /// Last event identifier already evaluated.
    pub event_cursor: String,
    /// Current terminal or continuing outcome.
    pub outcome: TerminalOutcome,
    /// Condition that most recently triggered, if any.
    pub triggered_condition_id: Option<String>,
    /// Event that caused the latest decision, if any.
    pub triggered_event_id: Option<String>,
    /// Stable reason code for the current outcome.
    pub reason_code: Option<String>,
    /// Evidence references supporting the termination decision.
    pub evidence_refs: Vec<String>,
    /// Monotonic state revision.
    pub version: u64,
}
/// Terminal decision returned when a policy condition matches an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminationDecision {
    /// Outcome the caller should apply to the run.
    pub outcome: TerminalOutcome,
    /// Stable reason code derived from the matching condition.
    pub reason_code: String,
    /// Identifier of the condition that matched.
    pub condition_id: String,
    /// Event identifier that triggered the decision.
    pub event_id: String,
}
/// Invalid policy, replayed event, unsupported version, or already-terminal state.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TerminationError {
    /// Policy or state uses an unsupported schema version.
    #[error("unsupported termination schema")]
    Version,
    /// Policy identity, expression, or integrity digest is invalid.
    #[error("invalid termination policy")]
    Invalid,
    /// Expression contains too many nodes.
    #[error("termination policy is too large")]
    TooLarge,
    /// State already records a terminal outcome.
    #[error("termination state is already terminal")]
    Terminal,
    /// Event identifier has already been evaluated.
    #[error("termination event replayed")]
    Replay,
}

fn check_leaf(
    kind: ConditionKind,
    threshold: u64,
    event: &TerminationEvent,
    text: Option<&str>,
) -> bool {
    match kind {
        ConditionKind::MaxMessages => event.messages >= threshold,
        ConditionKind::MaxTurns => event.turns >= threshold,
        ConditionKind::MaxToolCalls => event.tool_calls >= threshold,
        ConditionKind::TokenBudget => {
            event.input_tokens.saturating_add(event.output_tokens) >= threshold
        }
        ConditionKind::CostBudget => event.cost_micros >= threshold,
        ConditionKind::WallClockTimeout => event.elapsed_ms >= threshold,
        ConditionKind::IdleTimeout => event.idle_ms >= threshold,
        ConditionKind::StopEvent => event.kind == "stop",
        ConditionKind::SourceMatch => {
            text.is_some_and(|needle| !needle.is_empty() && event.source.contains(needle))
        }
        ConditionKind::HandoffReached => event.handoff_reached,
        ConditionKind::ExternalSignal => {
            text.is_some_and(|signal| event.signal.as_deref() == Some(signal))
        }
        ConditionKind::GoalStateReached => {
            text.is_some_and(|state| event.goal_state.as_deref() == Some(state))
        }
        ConditionKind::WorkflowStateReached => {
            text.is_some_and(|state| event.workflow_state.as_deref() == Some(state))
        }
    }
}
fn validate_expression(
    e: &TerminationExpression,
    count: &mut usize,
) -> Result<(), TerminationError> {
    *count += 1;
    if *count > MAX_NODES {
        return Err(TerminationError::TooLarge);
    }
    match e {
        TerminationExpression::Condition {
            id,
            threshold,
            text,
            ..
        } => {
            if id.is_empty()
                || id.len() > MAX_TEXT
                || *threshold == 0
                || text.as_ref().is_some_and(|v| v.len() > MAX_TEXT)
            {
                return Err(TerminationError::Invalid);
            }
        }
        TerminationExpression::Composite { children, .. } => {
            if children.is_empty() {
                return Err(TerminationError::Invalid);
            }
            for child in children {
                validate_expression(child, count)?;
            }
        }
    }
    Ok(())
}
/// Validates policy identity, schema, expression depth/count, and bounds.
pub fn validate_policy(p: &TerminationPolicy) -> Result<(), TerminationError> {
    if p.schema_version != SCHEMA_VERSION
        || p.id.is_empty()
        || p.id.len() > MAX_TEXT
        || p.version == 0
        || p.content_hash.len() != 64
    {
        return Err(if p.schema_version != SCHEMA_VERSION {
            TerminationError::Version
        } else {
            TerminationError::Invalid
        });
    }
    let mut count = 0;
    validate_expression(&p.expression, &mut count)
}
/// Computes the policy digest with the stored hash field cleared.
pub fn canonical_hash(p: &TerminationPolicy) -> Result<String, TerminationError> {
    let mut c = p.clone();
    c.content_hash.clear();
    let bytes = serde_json::to_vec(&c).map_err(|_| TerminationError::Invalid)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
/// Validates the policy and verifies its canonical content hash.
pub fn validate_hash(p: &TerminationPolicy) -> Result<(), TerminationError> {
    validate_policy(p)?;
    if canonical_hash(p)? != p.content_hash {
        return Err(TerminationError::Invalid);
    }
    Ok(())
}
fn evaluate(e: &TerminationExpression, event: &TerminationEvent) -> Option<String> {
    match e {
        TerminationExpression::Condition {
            id,
            kind,
            threshold,
            text,
        } => check_leaf(*kind, *threshold, event, text.as_deref()).then(|| id.clone()),
        TerminationExpression::Composite { mode, children } => {
            let hits = children
                .iter()
                .filter_map(|c| evaluate(c, event))
                .collect::<Vec<_>>();
            match mode {
                Composition::Any => hits.into_iter().next(),
                Composition::All => (hits.len() == children.len())
                    .then(|| hits.into_iter().next().unwrap_or_default()),
            }
        }
    }
}
/// Evaluates a new event and returns a decision when a condition is satisfied.
pub fn evaluate_policy(
    p: &TerminationPolicy,
    state: &TerminationState,
    event: &TerminationEvent,
) -> Result<Option<TerminationDecision>, TerminationError> {
    validate_hash(p)?;
    if state.outcome != TerminalOutcome::Continue {
        return Err(TerminationError::Terminal);
    }
    if state.event_cursor == event.event_id {
        return Err(TerminationError::Replay);
    }
    if let Some(id) = evaluate(&p.expression, event) {
        let outcome = if matches!(
            p.expression,
            TerminationExpression::Condition {
                kind: ConditionKind::WallClockTimeout,
                ..
            }
        ) {
            TerminalOutcome::Timeout
        } else {
            TerminalOutcome::Completed
        };
        return Ok(Some(TerminationDecision {
            outcome,
            reason_code: format!("termination.{id}"),
            condition_id: id,
            event_id: event.event_id.clone(),
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> TerminationPolicy {
        let mut p = TerminationPolicy {
            schema_version: 1,
            id: "p".into(),
            version: 1,
            expression: TerminationExpression::Composite {
                mode: Composition::Any,
                children: (0..13)
                    .map(|i| TerminationExpression::Condition {
                        id: format!("c{i}"),
                        kind: [
                            ConditionKind::MaxMessages,
                            ConditionKind::MaxTurns,
                            ConditionKind::MaxToolCalls,
                            ConditionKind::TokenBudget,
                            ConditionKind::CostBudget,
                            ConditionKind::WallClockTimeout,
                            ConditionKind::IdleTimeout,
                            ConditionKind::StopEvent,
                            ConditionKind::SourceMatch,
                            ConditionKind::HandoffReached,
                            ConditionKind::ExternalSignal,
                            ConditionKind::GoalStateReached,
                            ConditionKind::WorkflowStateReached,
                        ][i],
                        threshold: 1,
                        text: Some("x".into()),
                    })
                    .collect(),
            },
            hard_stop: true,
            content_hash: String::new(),
        };
        p.content_hash = canonical_hash(&p).unwrap();
        p
    }
    fn event() -> TerminationEvent {
        TerminationEvent {
            event_id: "e1".into(),
            kind: "stop".into(),
            source: "x".into(),
            messages: 1,
            turns: 0,
            tool_calls: 0,
            input_tokens: 0,
            output_tokens: 0,
            cost_micros: 0,
            elapsed_ms: 0,
            idle_ms: 0,
            goal_state: None,
            workflow_state: None,
            signal: None,
            handoff_reached: false,
        }
    }
    #[test]
    fn all_builtins_are_bounded_and_hashable() {
        assert!(validate_hash(&policy()).is_ok());
    }
    #[test]
    fn first_trigger_is_deterministic_and_replay_is_rejected() {
        let p = policy();
        let s = TerminationState {
            schema_version: 1,
            policy_version: 1,
            event_cursor: "".into(),
            outcome: TerminalOutcome::Continue,
            triggered_condition_id: None,
            triggered_event_id: None,
            reason_code: None,
            evidence_refs: vec![],
            version: 1,
        };
        assert_eq!(
            evaluate_policy(&p, &s, &event())
                .unwrap()
                .unwrap()
                .condition_id,
            "c0"
        );
        let replay = TerminationState {
            event_cursor: "e1".into(),
            ..s
        };
        assert_eq!(
            evaluate_policy(&p, &replay, &event()),
            Err(TerminationError::Replay)
        );
    }
}

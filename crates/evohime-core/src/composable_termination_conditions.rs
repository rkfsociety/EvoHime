//! Core-owned, replay-safe termination condition algebra.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_NODES: usize = 64;
pub const MAX_TEXT: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionKind {
    MaxMessages,
    MaxTurns,
    MaxToolCalls,
    TokenBudget,
    CostBudget,
    WallClockTimeout,
    IdleTimeout,
    StopEvent,
    SourceMatch,
    HandoffReached,
    ExternalSignal,
    GoalStateReached,
    WorkflowStateReached,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Composition {
    Any,
    All,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalOutcome {
    Continue,
    Completed,
    Paused,
    BudgetExhausted,
    Timeout,
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminationEvent {
    pub event_id: String,
    pub kind: String,
    pub source: String,
    pub messages: u64,
    pub turns: u64,
    pub tool_calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_micros: u64,
    pub elapsed_ms: u64,
    pub idle_ms: u64,
    pub goal_state: Option<String>,
    pub workflow_state: Option<String>,
    pub signal: Option<String>,
    pub handoff_reached: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminationExpression {
    Condition {
        id: String,
        kind: ConditionKind,
        threshold: u64,
        text: Option<String>,
    },
    Composite {
        mode: Composition,
        children: Vec<TerminationExpression>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminationPolicy {
    pub schema_version: u32,
    pub id: String,
    pub version: u64,
    pub expression: TerminationExpression,
    pub hard_stop: bool,
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminationState {
    pub schema_version: u32,
    pub policy_version: u64,
    pub event_cursor: String,
    pub outcome: TerminalOutcome,
    pub triggered_condition_id: Option<String>,
    pub triggered_event_id: Option<String>,
    pub reason_code: Option<String>,
    pub evidence_refs: Vec<String>,
    pub version: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminationDecision {
    pub outcome: TerminalOutcome,
    pub reason_code: String,
    pub condition_id: String,
    pub event_id: String,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TerminationError {
    #[error("unsupported termination schema")]
    Version,
    #[error("invalid termination policy")]
    Invalid,
    #[error("termination policy is too large")]
    TooLarge,
    #[error("termination state is already terminal")]
    Terminal,
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
pub fn canonical_hash(p: &TerminationPolicy) -> Result<String, TerminationError> {
    let mut c = p.clone();
    c.content_hash.clear();
    let bytes = serde_json::to_vec(&c).map_err(|_| TerminationError::Invalid)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
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

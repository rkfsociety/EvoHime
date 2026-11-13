//! Versioned, redacted execution-trace contracts and deterministic assertions.
//!
//! This module deliberately accepts only metadata and digests. Raw prompts,
//! tool arguments, provider output, and reasoning are not part of the trace
//! schema and therefore cannot enter its canonical hash.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Current serialized execution-trace schema version.
pub const TRACE_SCHEMA_VERSION: u32 = 1;
/// Current serialized evaluation-contract version.
pub const EVALUATION_CONTRACT_VERSION: u32 = 1;
/// Maximum number of events accepted in one trace.
pub const MAX_TRACE_EVENTS: usize = 4_096;
/// Maximum number of assertions accepted in one evaluation contract.
pub const MAX_TRACE_ASSERTIONS: usize = 128;
/// Maximum number of identifiers accepted in a list assertion.
pub const MAX_ASSERTION_ITEMS: usize = 256;
/// Maximum length of an identifier or reason-code context.
pub const MAX_IDENTIFIER_CHARS: usize = 128;

/// Terminal state observed for one benchmark attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceTerminalState {
    /// Runtime emitted a terminal success event and closed the attempt.
    Completed,
    /// Runtime emitted a terminal failure event and closed the attempt.
    Failed,
    /// Runtime was interrupted before a terminal event was observed.
    Interrupted,
    /// Capture ended while the runtime was still active.
    Partial,
    /// The capture source could not establish the terminal state.
    Unknown,
}

/// Normalized event category; arbitrary provider or tool payloads are excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceEventKind {
    /// A tool invocation was observed.
    ToolCall,
    /// Core granted an approval for a protected action.
    ApprovalGranted,
    /// A protected effect was committed.
    EffectCommitted,
    /// A bounded evidence reference was attached.
    EvidenceAttached,
    /// A retry attempt was started.
    RetryStarted,
}

/// Redacted event outcome observed from the authoritative runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceEventOutcome {
    /// The event completed successfully.
    Succeeded,
    /// The event completed with a failure.
    Failed,
    /// The event was denied by policy.
    Denied,
    /// The event did not reach a known terminal outcome.
    Unknown,
}

/// One immutable, redacted event in observed sequence order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionTraceEvent {
    /// Monotonic, one-based sequence assigned by the capture owner.
    pub sequence: u64,
    /// Stable event identifier within the attempt.
    pub event_id: String,
    /// Normalized event category.
    pub kind: TraceEventKind,
    /// Stable tool identifier for tool-call events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    /// SHA-256 digest of canonical, redacted tool arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_digest: Option<String>,
    /// Approval identifier bound to a granted approval or protected effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    /// Stable identifier of a committed effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_id: Option<String>,
    /// Bounded opaque references to evidence owned by the existing evidence ledger.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<String>,
    /// Original event identifier for a retry, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_of: Option<String>,
    /// Normalized outcome observed by the capture owner.
    pub outcome: TraceEventOutcome,
}

/// Immutable redacted execution trace for one run and benchmark attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionTrace {
    /// Version of this serialized trace schema.
    pub schema_version: u32,
    /// Stable Core run identifier.
    pub run_id: String,
    /// Stable benchmark attempt identifier.
    pub attempt_id: String,
    /// Observed terminal state of the attempt.
    pub terminal_state: TraceTerminalState,
    /// Events in their original observed order.
    pub events: Vec<ExecutionTraceEvent>,
}

/// Deterministic assertion evaluated against an observed trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TraceAssertionRule {
    /// Require the complete observed tool-call sequence to match exactly.
    ExactToolSequence {
        /// Expected stable tool identifiers in observed order.
        tool_ids: Vec<String>,
    },
    /// Require the listed tool calls to occur in order; other calls may intervene.
    ToolSubsequence {
        /// Expected stable tool identifiers in observed order.
        tool_ids: Vec<String>,
    },
    /// Fail if any listed tool is observed.
    ForbiddenTools {
        /// Stable tool identifiers that must not appear.
        tool_ids: Vec<String>,
    },
    /// Require every committed effect to follow its matching approval event.
    ApprovalBeforeEffect,
    /// Require at least one attached reference from the existing evidence ledger.
    RequiresEvidence,
    /// Require the observed event count not to exceed this bound.
    MaxEvents {
        /// Maximum permitted number of events.
        maximum: usize,
    },
    /// Require a retry to preserve the original tool identity and argument digest.
    RetryEquivalent {
        /// Event identifier of the original tool call.
        original_event_id: String,
        /// Event identifier of the retry tool call.
        retry_event_id: String,
    },
}

/// One uniquely identified assertion in an evaluation contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceAssertion {
    /// Stable assertion identifier.
    pub id: String,
    /// Deterministic rule to evaluate.
    pub rule: TraceAssertionRule,
}

/// Versioned set of deterministic assertions for one trace schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceEvaluationContract {
    /// Version of this evaluation contract.
    pub version: u32,
    /// Trace schema version required by this contract.
    pub trace_schema_version: u32,
    /// Stable contract identifier.
    pub contract_id: String,
    /// Ordered assertions; order is preserved in the report.
    pub assertions: Vec<TraceAssertion>,
}

/// Stable, machine-readable reason code for an assertion result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceAssertionReason {
    /// The assertion condition was satisfied.
    Passed,
    /// The assertion condition was not satisfied.
    ConditionFailed,
    /// The trace exceeded a configured event bound.
    EventLimitExceeded,
    /// A protected effect did not follow its matching approval.
    ApprovalEffectOrderViolation,
    /// A retry changed the tool identity or canonical argument digest.
    RetryNotEquivalent,
    /// Required evidence was not attached.
    EvidenceMissing,
}

/// Bounded result for one assertion; it contains no raw trace payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceAssertionResult {
    /// Stable assertion identifier.
    pub assertion_id: String,
    /// Whether the assertion passed.
    pub passed: bool,
    /// Stable reason code suitable for CI diagnostics.
    pub reason: TraceAssertionReason,
}

/// Overall result of deterministic trace evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceEvaluationOutcome {
    /// Every assertion passed against a complete observed trace.
    Passed,
    /// At least one assertion failed.
    Failed,
    /// Trace or contract failed integrity/compatibility validation.
    Invalid,
    /// Trace capture was interrupted or did not reach a known terminal state.
    Interrupted,
}

/// Redacted evaluation report; raw events and tool arguments are never included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceEvaluationReport {
    /// Stable contract identifier.
    pub contract_id: String,
    /// Version of the evaluation contract.
    pub contract_version: u32,
    /// SHA-256 digest of the canonical redacted trace.
    pub trace_hash: Option<String>,
    /// SHA-256 digest of the canonical evaluation contract.
    pub contract_hash: String,
    /// Overall evaluation outcome.
    pub outcome: TraceEvaluationOutcome,
    /// Bounded per-assertion results.
    pub assertions: Vec<TraceAssertionResult>,
}

/// Stable validation failure; the value is safe to expose in bounded diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TraceEvaluationError {
    /// Trace schema version is not supported.
    #[error("unsupported trace schema")]
    UnsupportedTraceSchema,
    /// Evaluation contract version is not supported.
    #[error("unsupported evaluation contract")]
    UnsupportedContract,
    /// Trace or contract identity is malformed.
    #[error("invalid identity")]
    InvalidIdentity,
    /// Trace event count exceeds its bound.
    #[error("trace event limit exceeded")]
    EventLimitExceeded,
    /// Event sequence or identity is duplicated or non-contiguous.
    #[error("invalid event sequence")]
    InvalidEventSequence,
    /// Event fields do not match their normalized category.
    #[error("invalid event shape")]
    InvalidEventShape,
    /// A protected effect has no preceding matching approval.
    #[error("approval/effect order violation")]
    ApprovalEffectOrderViolation,
    /// Contract assertion count or assertion values exceed their bounds.
    #[error("invalid assertion contract")]
    InvalidAssertions,
    /// The trace is incomplete or its terminal state is not known.
    #[error("trace is incomplete")]
    IncompleteTrace,
    /// Canonical serialization failed.
    #[error("canonical serialization failed")]
    Serialization,
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_IDENTIFIER_CHARS
        && !value.chars().any(char::is_control)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl ExecutionTrace {
    /// Validates the redacted trace before it can be hashed or evaluated.
    pub fn validate(&self) -> Result<(), TraceEvaluationError> {
        if self.schema_version != TRACE_SCHEMA_VERSION {
            return Err(TraceEvaluationError::UnsupportedTraceSchema);
        }
        if !valid_identifier(&self.run_id) || !valid_identifier(&self.attempt_id) {
            return Err(TraceEvaluationError::InvalidIdentity);
        }
        if self.events.is_empty() || self.events.len() > MAX_TRACE_EVENTS {
            return Err(TraceEvaluationError::EventLimitExceeded);
        }

        let mut event_ids = BTreeSet::new();
        let mut approvals = BTreeMap::<String, u64>::new();
        let mut tool_call_ids = BTreeSet::new();
        for (index, event) in self.events.iter().enumerate() {
            if event.sequence != index as u64 + 1
                || !valid_identifier(&event.event_id)
                || !event_ids.insert(event.event_id.as_str())
            {
                return Err(TraceEvaluationError::InvalidEventSequence);
            }
            for value in [
                event.tool_id.as_deref(),
                event.approval_id.as_deref(),
                event.effect_id.as_deref(),
                event.retry_of.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                if !valid_identifier(value) {
                    return Err(TraceEvaluationError::InvalidEventShape);
                }
            }
            if event.evidence_refs.len() > MAX_ASSERTION_ITEMS
                || event
                    .evidence_refs
                    .iter()
                    .any(|value| !valid_identifier(value))
            {
                return Err(TraceEvaluationError::InvalidEventShape);
            }
            if let Some(digest) = event.args_digest.as_deref() {
                if !valid_digest(digest) {
                    return Err(TraceEvaluationError::InvalidEventShape);
                }
            }

            match event.kind {
                TraceEventKind::ToolCall => {
                    if event.tool_id.is_none()
                        || event.args_digest.is_none()
                        || event.approval_id.is_some()
                        || event.effect_id.is_some()
                        || !event.evidence_refs.is_empty()
                        || event.retry_of.is_some()
                        || event.outcome == TraceEventOutcome::Unknown
                    {
                        return Err(TraceEvaluationError::InvalidEventShape);
                    }
                    tool_call_ids.insert(event.event_id.as_str());
                }
                TraceEventKind::ApprovalGranted => {
                    let id = event
                        .approval_id
                        .as_deref()
                        .ok_or(TraceEvaluationError::InvalidEventShape)?;
                    if event.tool_id.is_some()
                        || event.args_digest.is_some()
                        || event.effect_id.is_some()
                        || !event.evidence_refs.is_empty()
                        || event.retry_of.is_some()
                        || event.outcome != TraceEventOutcome::Succeeded
                        || approvals.insert(id.to_owned(), event.sequence).is_some()
                    {
                        return Err(TraceEvaluationError::InvalidEventShape);
                    }
                }
                TraceEventKind::EffectCommitted => {
                    let approval_id = event
                        .approval_id
                        .as_deref()
                        .ok_or(TraceEvaluationError::InvalidEventShape)?;
                    if event.effect_id.is_none()
                        || event.tool_id.is_some()
                        || event.args_digest.is_some()
                        || !event.evidence_refs.is_empty()
                        || event.retry_of.is_some()
                        || event.outcome != TraceEventOutcome::Succeeded
                    {
                        return Err(TraceEvaluationError::InvalidEventShape);
                    }
                    if approvals
                        .get(approval_id)
                        .is_none_or(|sequence| *sequence >= event.sequence)
                    {
                        return Err(TraceEvaluationError::ApprovalEffectOrderViolation);
                    }
                }
                TraceEventKind::EvidenceAttached => {
                    if event.evidence_refs.is_empty()
                        || event.tool_id.is_some()
                        || event.args_digest.is_some()
                        || event.approval_id.is_some()
                        || event.effect_id.is_some()
                        || event.retry_of.is_some()
                        || event.outcome != TraceEventOutcome::Succeeded
                    {
                        return Err(TraceEvaluationError::InvalidEventShape);
                    }
                }
                TraceEventKind::RetryStarted => {
                    if event.retry_of.is_none()
                        || event.tool_id.is_none()
                        || event.args_digest.is_none()
                        || event.approval_id.is_some()
                        || event.effect_id.is_some()
                        || !event.evidence_refs.is_empty()
                        || event.outcome != TraceEventOutcome::Unknown
                    {
                        return Err(TraceEvaluationError::InvalidEventShape);
                    }
                    let original_id = event
                        .retry_of
                        .as_deref()
                        .ok_or(TraceEvaluationError::InvalidEventShape)?;
                    if !tool_call_ids.contains(original_id) {
                        return Err(TraceEvaluationError::InvalidEventShape);
                    }
                }
            }
        }
        Ok(())
    }

    /// Returns the legacy 64-character digest accepted by
    /// [`crate::agent_benchmark_matrix::AttemptResult::tool_trace_digest`].
    ///
    /// The digest bytes are identical to [`Self::canonical_hash`]; this method
    /// omits its `sha256:` prefix to preserve the existing benchmark field
    /// format. It does not create or persist a second trace.
    pub fn legacy_tool_trace_digest(&self) -> Result<String, TraceEvaluationError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| TraceEvaluationError::Serialization)?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }

    /// Returns the canonical SHA-256 digest of this validated redacted trace.
    pub fn canonical_hash(&self) -> Result<String, TraceEvaluationError> {
        Ok(format!("sha256:{}", self.legacy_tool_trace_digest()?))
    }
}

impl TraceEvaluationContract {
    /// Validates contract identity, compatibility, bounds, and unique assertion IDs.
    pub fn validate(&self) -> Result<(), TraceEvaluationError> {
        if self.version != EVALUATION_CONTRACT_VERSION
            || self.trace_schema_version != TRACE_SCHEMA_VERSION
        {
            return Err(TraceEvaluationError::UnsupportedContract);
        }
        if !valid_identifier(&self.contract_id)
            || self.assertions.is_empty()
            || self.assertions.len() > MAX_TRACE_ASSERTIONS
        {
            return Err(TraceEvaluationError::InvalidAssertions);
        }
        let mut ids = BTreeSet::new();
        for assertion in &self.assertions {
            if !valid_identifier(&assertion.id) || !ids.insert(assertion.id.as_str()) {
                return Err(TraceEvaluationError::InvalidAssertions);
            }
            match &assertion.rule {
                TraceAssertionRule::ExactToolSequence { tool_ids }
                | TraceAssertionRule::ToolSubsequence { tool_ids }
                | TraceAssertionRule::ForbiddenTools { tool_ids } => {
                    if tool_ids.len() > MAX_ASSERTION_ITEMS
                        || tool_ids.iter().any(|value| !valid_identifier(value))
                    {
                        return Err(TraceEvaluationError::InvalidAssertions);
                    }
                }
                TraceAssertionRule::MaxEvents { maximum } => {
                    if *maximum == 0 || *maximum > MAX_TRACE_EVENTS {
                        return Err(TraceEvaluationError::InvalidAssertions);
                    }
                }
                TraceAssertionRule::RetryEquivalent {
                    original_event_id,
                    retry_event_id,
                } => {
                    if !valid_identifier(original_event_id)
                        || !valid_identifier(retry_event_id)
                        || original_event_id == retry_event_id
                    {
                        return Err(TraceEvaluationError::InvalidAssertions);
                    }
                }
                TraceAssertionRule::ApprovalBeforeEffect | TraceAssertionRule::RequiresEvidence => {
                }
            }
        }
        Ok(())
    }

    /// Returns the canonical SHA-256 digest of this validated evaluation contract.
    pub fn canonical_hash(&self) -> Result<String, TraceEvaluationError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| TraceEvaluationError::Serialization)?;
        Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
    }
}

fn tool_calls(trace: &ExecutionTrace) -> Vec<&ExecutionTraceEvent> {
    trace
        .events
        .iter()
        .filter(|event| event.kind == TraceEventKind::ToolCall)
        .collect()
}

fn evaluate_assertion(trace: &ExecutionTrace, assertion: &TraceAssertion) -> TraceAssertionResult {
    let reason = match &assertion.rule {
        TraceAssertionRule::ExactToolSequence { tool_ids } => {
            let actual = tool_calls(trace)
                .into_iter()
                .filter_map(|event| event.tool_id.as_deref())
                .collect::<Vec<_>>();
            if actual == tool_ids.iter().map(String::as_str).collect::<Vec<_>>() {
                TraceAssertionReason::Passed
            } else {
                TraceAssertionReason::ConditionFailed
            }
        }
        TraceAssertionRule::ToolSubsequence { tool_ids } => {
            let actual = tool_calls(trace);
            let mut expected = tool_ids.iter();
            let mut next = expected.next();
            for event in actual {
                if next.is_some_and(|value| event.tool_id.as_deref() == Some(value.as_str())) {
                    next = expected.next();
                }
            }
            if next.is_none() {
                TraceAssertionReason::Passed
            } else {
                TraceAssertionReason::ConditionFailed
            }
        }
        TraceAssertionRule::ForbiddenTools { tool_ids } => {
            if tool_calls(trace).iter().any(|event| {
                event
                    .tool_id
                    .as_ref()
                    .is_some_and(|tool| tool_ids.contains(tool))
            }) {
                TraceAssertionReason::ConditionFailed
            } else {
                TraceAssertionReason::Passed
            }
        }
        TraceAssertionRule::ApprovalBeforeEffect => TraceAssertionReason::Passed,
        TraceAssertionRule::RequiresEvidence => {
            if trace
                .events
                .iter()
                .any(|event| !event.evidence_refs.is_empty())
            {
                TraceAssertionReason::Passed
            } else {
                TraceAssertionReason::EvidenceMissing
            }
        }
        TraceAssertionRule::MaxEvents { maximum } => {
            if trace.events.len() <= *maximum {
                TraceAssertionReason::Passed
            } else {
                TraceAssertionReason::EventLimitExceeded
            }
        }
        TraceAssertionRule::RetryEquivalent {
            original_event_id,
            retry_event_id,
        } => {
            let original = trace
                .events
                .iter()
                .find(|event| event.event_id == *original_event_id);
            let retry = trace
                .events
                .iter()
                .find(|event| event.event_id == *retry_event_id);
            match (original, retry) {
                (Some(original), Some(retry))
                    if retry.kind == TraceEventKind::RetryStarted
                        && retry.retry_of.as_deref() == Some(original_event_id.as_str())
                        && retry.tool_id == original.tool_id
                        && retry.args_digest == original.args_digest =>
                {
                    TraceAssertionReason::Passed
                }
                _ => TraceAssertionReason::RetryNotEquivalent,
            }
        }
    };
    TraceAssertionResult {
        assertion_id: assertion.id.clone(),
        passed: reason == TraceAssertionReason::Passed,
        reason,
    }
}

/// Evaluates a complete observed trace without exposing raw events in the report.
///
/// Invalid, partial, interrupted, and unknown-terminal traces never receive a
/// passing report. A protected effect without its matching preceding approval
/// is rejected during trace validation.
pub fn evaluate_trace(
    trace: &ExecutionTrace,
    contract: &TraceEvaluationContract,
) -> Result<TraceEvaluationReport, TraceEvaluationError> {
    contract.validate()?;
    let contract_hash = contract.canonical_hash()?;
    if trace.terminal_state != TraceTerminalState::Completed {
        return Err(TraceEvaluationError::IncompleteTrace);
    }
    trace.validate()?;
    let trace_hash = trace.canonical_hash()?;
    let assertions = contract
        .assertions
        .iter()
        .map(|assertion| evaluate_assertion(trace, assertion))
        .collect::<Vec<_>>();
    let outcome = if assertions.iter().all(|result| result.passed) {
        TraceEvaluationOutcome::Passed
    } else {
        TraceEvaluationOutcome::Failed
    };
    Ok(TraceEvaluationReport {
        contract_id: contract.contract_id.clone(),
        contract_version: contract.version,
        trace_hash: Some(trace_hash),
        contract_hash,
        outcome,
        assertions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace() -> ExecutionTrace {
        ExecutionTrace {
            schema_version: TRACE_SCHEMA_VERSION,
            run_id: "run-1".into(),
            attempt_id: "attempt-1".into(),
            terminal_state: TraceTerminalState::Completed,
            events: vec![
                ExecutionTraceEvent {
                    sequence: 1,
                    event_id: "approval-1".into(),
                    kind: TraceEventKind::ApprovalGranted,
                    tool_id: None,
                    args_digest: None,
                    approval_id: Some("approval-id-1".into()),
                    effect_id: None,
                    evidence_refs: vec![],
                    retry_of: None,
                    outcome: TraceEventOutcome::Succeeded,
                },
                ExecutionTraceEvent {
                    sequence: 2,
                    event_id: "tool-1".into(),
                    kind: TraceEventKind::ToolCall,
                    tool_id: Some("workspace.write".into()),
                    args_digest: Some("a".repeat(64)),
                    approval_id: None,
                    effect_id: None,
                    evidence_refs: vec![],
                    retry_of: None,
                    outcome: TraceEventOutcome::Succeeded,
                },
                ExecutionTraceEvent {
                    sequence: 3,
                    event_id: "effect-1".into(),
                    kind: TraceEventKind::EffectCommitted,
                    tool_id: None,
                    args_digest: None,
                    approval_id: Some("approval-id-1".into()),
                    effect_id: Some("effect-id-1".into()),
                    evidence_refs: vec![],
                    retry_of: None,
                    outcome: TraceEventOutcome::Succeeded,
                },
                ExecutionTraceEvent {
                    sequence: 4,
                    event_id: "evidence-1".into(),
                    kind: TraceEventKind::EvidenceAttached,
                    tool_id: None,
                    args_digest: None,
                    approval_id: None,
                    effect_id: None,
                    evidence_refs: vec!["evidence-ref-1".into()],
                    retry_of: None,
                    outcome: TraceEventOutcome::Succeeded,
                },
            ],
        }
    }

    fn retry_event(retry_of: &str) -> ExecutionTraceEvent {
        ExecutionTraceEvent {
            sequence: 5,
            event_id: "retry-1".into(),
            kind: TraceEventKind::RetryStarted,
            tool_id: Some("workspace.write".into()),
            args_digest: Some("a".repeat(64)),
            approval_id: None,
            effect_id: None,
            evidence_refs: vec![],
            retry_of: Some(retry_of.into()),
            outcome: TraceEventOutcome::Unknown,
        }
    }

    fn contract() -> TraceEvaluationContract {
        TraceEvaluationContract {
            version: EVALUATION_CONTRACT_VERSION,
            trace_schema_version: TRACE_SCHEMA_VERSION,
            contract_id: "contract-1".into(),
            assertions: vec![
                TraceAssertion {
                    id: "tools".into(),
                    rule: TraceAssertionRule::ExactToolSequence {
                        tool_ids: vec!["workspace.write".into()],
                    },
                },
                TraceAssertion {
                    id: "approval".into(),
                    rule: TraceAssertionRule::ApprovalBeforeEffect,
                },
                TraceAssertion {
                    id: "evidence".into(),
                    rule: TraceAssertionRule::RequiresEvidence,
                },
            ],
        }
    }

    #[test]
    fn canonical_hash_is_stable_for_same_redacted_trace() {
        let value = trace();
        assert_eq!(
            value.canonical_hash().unwrap(),
            value.canonical_hash().unwrap()
        );
        assert!(value.canonical_hash().unwrap().starts_with("sha256:"));
    }

    #[test]
    fn legacy_tool_trace_digest_preserves_benchmark_field_compatibility() {
        let value = trace();
        let legacy = value.legacy_tool_trace_digest().unwrap();
        assert_eq!(legacy.len(), 64);
        assert!(legacy
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        assert_eq!(value.canonical_hash().unwrap(), format!("sha256:{legacy}"));
    }

    #[test]
    fn assertion_reason_codes_have_stable_serialized_names() {
        let cases = [
            (TraceAssertionReason::Passed, "passed"),
            (TraceAssertionReason::ConditionFailed, "condition_failed"),
            (TraceAssertionReason::EventLimitExceeded, "event_limit_exceeded"),
            (
                TraceAssertionReason::ApprovalEffectOrderViolation,
                "approval_effect_order_violation",
            ),
            (TraceAssertionReason::RetryNotEquivalent, "retry_not_equivalent"),
            (TraceAssertionReason::EvidenceMissing, "evidence_missing"),
        ];
        for (reason, expected) in cases {
            assert_eq!(
                serde_json::to_string(&reason).unwrap(),
                format!("\"{expected}\"")
            );
        }
    }

    #[test]
    fn complete_observed_trace_passes_deterministic_assertions() {
        let report = evaluate_trace(&trace(), &contract()).unwrap();
        assert_eq!(report.outcome, TraceEvaluationOutcome::Passed);
        assert_eq!(report.assertions.len(), 3);
        assert!(report.assertions.iter().all(|result| result.passed));
        let encoded = serde_json::to_string(&report).unwrap();
        assert!(!encoded.contains("workspace.write"));
        assert!(!encoded.contains("evidence-ref-1"));
    }

    #[test]
    fn partial_or_unknown_terminal_trace_never_passes() {
        for terminal_state in [
            TraceTerminalState::Interrupted,
            TraceTerminalState::Partial,
            TraceTerminalState::Unknown,
            TraceTerminalState::Failed,
        ] {
            let mut value = trace();
            value.terminal_state = terminal_state;
            assert_eq!(
                evaluate_trace(&value, &contract()).unwrap_err(),
                TraceEvaluationError::IncompleteTrace
            );
        }
    }

    #[test]
    fn effect_without_preceding_matching_approval_is_rejected() {
        let mut value = trace();
        value.events.swap(0, 2);
        for (index, event) in value.events.iter_mut().enumerate() {
            event.sequence = index as u64 + 1;
        }
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::ApprovalEffectOrderViolation
        );
    }

    #[test]
    fn unknown_event_kinds_and_extra_fields_fail_deserialization() {
        let mut value = serde_json::to_value(trace()).unwrap();
        value["events"][0]["kind"] = serde_json::json!("future_event");
        assert!(serde_json::from_value::<ExecutionTrace>(value).is_err());

        let mut value = serde_json::to_value(trace()).unwrap();
        value["events"][0]["raw_payload"] = serde_json::json!("must not enter a trace");
        assert!(serde_json::from_value::<ExecutionTrace>(value).is_err());
    }

    #[test]
    fn malformed_digest_and_non_contiguous_sequence_are_rejected() {
        let mut value = trace();
        value.events[1].args_digest = Some("not-a-digest".into());
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::InvalidEventShape
        );
        let mut value = trace();
        value.events[1].args_digest = Some("A".repeat(64));
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::InvalidEventShape
        );
        let mut value = trace();
        value.events[1].evidence_refs.push("orphan-evidence".into());
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::InvalidEventShape
        );
        let mut value = trace();
        value.events[1].sequence = 9;
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::InvalidEventSequence
        );
    }

    #[test]
    fn incompatible_contract_and_duplicate_assertion_ids_are_rejected() {
        let mut value = contract();
        value.version = 99;
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::UnsupportedContract
        );
        let mut value = contract();
        value.assertions[1].id = value.assertions[0].id.clone();
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::InvalidAssertions
        );
    }

    #[test]
    fn failed_assertion_produces_failed_report_with_stable_reason() {
        let mut value = contract();
        value.assertions[0].rule = TraceAssertionRule::ForbiddenTools {
            tool_ids: vec!["workspace.write".into()],
        };
        let report = evaluate_trace(&trace(), &value).unwrap();
        assert_eq!(report.outcome, TraceEvaluationOutcome::Failed);
        assert_eq!(
            report.assertions[0].reason,
            TraceAssertionReason::ConditionFailed
        );
    }

    #[test]
    fn retry_must_match_original_tool_and_argument_digest() {
        let mut value = trace();
        value.events.push(retry_event("tool-1"));
        let mut policy = contract();
        policy.assertions.push(TraceAssertion {
            id: "retry".into(),
            rule: TraceAssertionRule::RetryEquivalent {
                original_event_id: "tool-1".into(),
                retry_event_id: "retry-1".into(),
            },
        });
        assert!(evaluate_trace(&value, &policy).unwrap().assertions[3].passed);
        value.events[4].args_digest = Some("b".repeat(64));
        assert_eq!(
            evaluate_trace(&value, &policy).unwrap().assertions[3].reason,
            TraceAssertionReason::RetryNotEquivalent
        );
    }

    #[test]
    fn retry_must_reference_a_preceding_tool_call() {
        let mut value = trace();
        value.events.push(retry_event("approval-1"));
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::InvalidEventShape
        );

        let mut value = trace();
        value.events.push(retry_event("missing-tool-call"));
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::InvalidEventShape
        );
    }

    #[test]
    fn completed_trace_rejects_unknown_or_inconsistent_event_outcomes() {
        let mut value = trace();
        value.events[1].outcome = TraceEventOutcome::Unknown;
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::InvalidEventShape
        );

        let mut value = trace();
        value.events[2].outcome = TraceEventOutcome::Failed;
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::InvalidEventShape
        );

        let mut value = trace();
        value.events[3].outcome = TraceEventOutcome::Denied;
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::InvalidEventShape
        );

        let mut value = trace();
        value.events.push(retry_event("tool-1"));
        value.events[4].outcome = TraceEventOutcome::Succeeded;
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::InvalidEventShape
        );
    }

    #[test]
    fn oversized_trace_and_contract_are_rejected() {
        let mut value = trace();
        value.events = (0..=MAX_TRACE_EVENTS)
            .map(|index| ExecutionTraceEvent {
                sequence: index as u64 + 1,
                event_id: format!("event-{index}"),
                kind: TraceEventKind::EvidenceAttached,
                tool_id: None,
                args_digest: None,
                approval_id: None,
                effect_id: None,
                evidence_refs: vec!["evidence".into()],
                retry_of: None,
                outcome: TraceEventOutcome::Succeeded,
            })
            .collect();
        assert_eq!(
            value.validate().unwrap_err(),
            TraceEvaluationError::EventLimitExceeded
        );
        let mut policy = contract();
        policy.assertions = (0..=MAX_TRACE_ASSERTIONS)
            .map(|index| TraceAssertion {
                id: format!("assertion-{index}"),
                rule: TraceAssertionRule::RequiresEvidence,
            })
            .collect();
        assert_eq!(
            policy.validate().unwrap_err(),
            TraceEvaluationError::InvalidAssertions
        );
    }
}

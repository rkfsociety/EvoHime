//! Core-owned typed ownership transfer. A handoff carries references and
//! bounded context metadata, never capabilities, credentials or raw transcript.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Current wire-contract version for agent handoffs.
pub const CONTRACT_VERSION: u32 = 1;
/// Maximum byte length for handoff identifiers and text fields.
pub const MAX_TEXT: usize = 512;
/// Maximum number of references, questions, or blockers in a packet.
pub const MAX_REFS: usize = 32;
/// Maximum context volume requested by one handoff, in bytes.
pub const MAX_CONTEXT_BYTES: u32 = 256 * 1024;

/// Bounded selection of context categories to include in a handoff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextTransferSpec {
    /// Maximum context size in bytes.
    pub max_bytes: u32,
    /// Whether to include a checkpoint reference.
    pub include_checkpoint: bool,
    /// Whether to include artifact references.
    pub include_artifacts: bool,
    /// Whether to include evidence references.
    pub include_evidence: bool,
    /// Whether to include bounded message context.
    pub include_messages: bool,
}

/// Typed request to transfer task ownership and bounded context to another agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffPacket {
    /// Wire-contract version.
    pub version: u32,
    /// Stable handoff identifier.
    pub handoff_id: String,
    /// Agent or role relinquishing ownership.
    pub from: String,
    /// Intended receiving agent or role.
    pub target: String,
    /// Objective the receiving agent is expected to continue.
    pub objective: String,
    /// Stable reason code explaining the transfer.
    pub reason_code: String,
    /// Bounded summary of current progress and state.
    pub summary: String,
    /// Optional reference to the latest checkpoint.
    pub checkpoint_ref: Option<String>,
    /// Artifact references relevant to the handoff.
    pub artifact_refs: Vec<String>,
    /// Verification or other evidence references.
    pub evidence_refs: Vec<String>,
    /// Questions the receiving agent should resolve.
    pub open_questions: Vec<String>,
    /// Known blockers the receiving agent should consider.
    pub blockers: Vec<String>,
    /// Optional parent goal identifier.
    pub goal_id: Option<String>,
    /// Workflow run created or continued by the handoff.
    pub workflow_run_id: String,
    /// Optional parent workflow run identifier.
    pub parent_run_id: Option<String>,
    /// Categories and size bound for transferred context.
    pub requested_context: ContextTransferSpec,
    /// Unix timestamp in milliseconds when the packet was created.
    pub created_at_ms: i64,
    /// Optional deadline after which the handoff cannot be accepted.
    pub expires_at_ms: Option<i64>,
}

/// Lifecycle state of an ownership transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffState {
    /// Proposed but not yet acknowledged by the receiver.
    Proposed,
    /// Receiver accepted the handoff.
    Accepted,
    /// Receiver began active work.
    Active,
    /// Work was completed by the receiver.
    Completed,
    /// Receiver declined the handoff.
    Rejected,
    /// Handoff deadline elapsed before completion.
    Expired,
    /// Transfer failed after acceptance.
    Failed,
    /// Receiver returned ownership to the sender.
    Returned,
}

/// One versioned actor action in the handoff lifecycle history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffTransition {
    /// State reached by this transition.
    pub state: HandoffState,
    /// Actor responsible for the transition.
    pub actor: String,
    /// Bounded reason for the state change.
    pub reason: String,
    /// Record version produced by this transition.
    pub version: u64,
    /// Unix timestamp in milliseconds for the transition.
    pub at_ms: i64,
}

/// Handoff packet, current state, transition history, and provenance references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffRecord {
    /// Immutable ownership-transfer request.
    pub packet: HandoffPacket,
    /// Current lifecycle state.
    pub state: HandoffState,
    /// Current optimistic-concurrency revision.
    pub version: u64,
    /// Ordered lifecycle history.
    pub transitions: Vec<HandoffTransition>,
    /// Stable provenance keys and source references.
    pub provenance: BTreeMap<String, String>,
}

/// Invalid handoff, stale revision, expiry, or target conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandoffError {
    /// A packet or provenance field violates the contract.
    Invalid(&'static str),
    /// Packet uses an unsupported wire version.
    UnsupportedVersion(u32),
    /// Requested lifecycle transition is not permitted.
    InvalidTransition,
    /// Handoff expired before the requested transition.
    Expired,
    /// Expected record version does not match.
    Stale,
    /// Another handoff already uses this identifier.
    Duplicate,
    /// Receiving target does not exist.
    UnknownTarget,
}
impl std::fmt::Display for HandoffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(v) => f.write_str(v),
            Self::UnsupportedVersion(_) => f.write_str("unsupported_version"),
            Self::InvalidTransition => f.write_str("invalid_transition"),
            Self::Expired => f.write_str("expired"),
            Self::Stale => f.write_str("stale"),
            Self::Duplicate => f.write_str("duplicate"),
            Self::UnknownTarget => f.write_str("unknown_target"),
        }
    }
}
impl std::error::Error for HandoffError {}

fn bounded(v: &str) -> bool {
    !v.is_empty() && v.len() <= MAX_TEXT && !v.chars().any(char::is_control)
}
fn refs(values: &[String]) -> bool {
    values.len() <= MAX_REFS && values.iter().all(|v| bounded(v))
}
/// Validates packet identity, bounded references, and context limits.
pub fn validate_packet(packet: &HandoffPacket) -> Result<(), HandoffError> {
    if packet.version != CONTRACT_VERSION {
        return Err(HandoffError::UnsupportedVersion(packet.version));
    }
    if !bounded(&packet.handoff_id)
        || !bounded(&packet.from)
        || !bounded(&packet.target)
        || packet.from == packet.target
        || !bounded(&packet.objective)
        || !bounded(&packet.reason_code)
        || !bounded(&packet.summary)
        || !bounded(&packet.workflow_run_id)
        || packet.created_at_ms < 0
        || packet
            .expires_at_ms
            .is_some_and(|v| v < packet.created_at_ms)
        || packet.requested_context.max_bytes == 0
        || packet.requested_context.max_bytes > MAX_CONTEXT_BYTES
        || !refs(&packet.artifact_refs)
        || !refs(&packet.evidence_refs)
        || !refs(&packet.open_questions)
        || !refs(&packet.blockers)
        || packet
            .checkpoint_ref
            .as_deref()
            .is_some_and(|v| !bounded(v))
        || packet.goal_id.as_deref().is_some_and(|v| !bounded(v))
        || packet.parent_run_id.as_deref().is_some_and(|v| !bounded(v))
    {
        return Err(HandoffError::Invalid("packet"));
    }
    Ok(())
}

/// Creates a proposed handoff record and binds its initial provenance event.
pub fn propose(
    packet: HandoffPacket,
    source_event_id: &str,
) -> Result<HandoffRecord, HandoffError> {
    validate_packet(&packet)?;
    if !bounded(source_event_id) {
        return Err(HandoffError::Invalid("provenance"));
    }
    Ok(HandoffRecord {
        packet: packet.clone(),
        state: HandoffState::Proposed,
        version: 1,
        transitions: vec![HandoffTransition {
            state: HandoffState::Proposed,
            actor: packet.from.clone(),
            reason: packet.reason_code.clone(),
            version: 1,
            at_ms: packet.created_at_ms,
        }],
        provenance: BTreeMap::from([
            (String::from("source_event"), source_event_id.to_owned()),
            (String::from("target_run"), packet.workflow_run_id),
        ]),
    })
}

/// Applies an optimistic-concurrency-checked handoff lifecycle transition.
pub fn transition(
    record: &mut HandoffRecord,
    next: HandoffState,
    actor: &str,
    reason: &str,
    expected_version: u64,
    now_ms: i64,
) -> Result<(), HandoffError> {
    if record.version != expected_version {
        return Err(HandoffError::Stale);
    }
    if record
        .packet
        .expires_at_ms
        .is_some_and(|expires| now_ms > expires)
        && !matches!(
            record.state,
            HandoffState::Completed | HandoffState::Rejected | HandoffState::Expired
        )
    {
        record.state = HandoffState::Expired;
        return Err(HandoffError::Expired);
    }
    let allowed = matches!(
        (record.state, next),
        (
            HandoffState::Proposed,
            HandoffState::Accepted | HandoffState::Rejected | HandoffState::Expired
        ) | (
            HandoffState::Accepted,
            HandoffState::Active | HandoffState::Failed
        ) | (
            HandoffState::Active,
            HandoffState::Completed | HandoffState::Returned | HandoffState::Failed
        )
    );
    if !allowed || !bounded(actor) || !bounded(reason) {
        return Err(HandoffError::InvalidTransition);
    }
    record.version += 1;
    record.state = next;
    record.transitions.push(HandoffTransition {
        state: next,
        actor: actor.to_owned(),
        reason: reason.to_owned(),
        version: record.version,
        at_ms: now_ms,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet() -> HandoffPacket {
        HandoffPacket {
            version: 1,
            handoff_id: "h".into(),
            from: "coder".into(),
            target: "reviewer".into(),
            objective: "review".into(),
            reason_code: "security".into(),
            summary: "bounded".into(),
            checkpoint_ref: None,
            artifact_refs: vec![],
            evidence_refs: vec![],
            open_questions: vec![],
            blockers: vec![],
            goal_id: None,
            workflow_run_id: "run".into(),
            parent_run_id: None,
            requested_context: ContextTransferSpec {
                max_bytes: 1024,
                include_checkpoint: true,
                include_artifacts: true,
                include_evidence: true,
                include_messages: false,
            },
            created_at_ms: 1,
            expires_at_ms: Some(100),
        }
    }
    #[test]
    fn lifecycle_has_ack_and_active() {
        let mut value = propose(packet(), "event").unwrap();
        transition(&mut value, HandoffState::Accepted, "reviewer", "ack", 1, 2).unwrap();
        transition(&mut value, HandoffState::Active, "reviewer", "start", 2, 3).unwrap();
        assert_eq!(value.version, 3);
    }
    #[test]
    fn stale_and_expired_are_safe() {
        let mut value = propose(packet(), "event").unwrap();
        assert_eq!(
            transition(&mut value, HandoffState::Accepted, "reviewer", "ack", 0, 2),
            Err(HandoffError::Stale)
        );
        assert_eq!(
            transition(
                &mut value,
                HandoffState::Accepted,
                "reviewer",
                "ack",
                1,
                101
            ),
            Err(HandoffError::Expired)
        );
    }
}

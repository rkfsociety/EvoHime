//! Core-owned typed ownership transfer. A handoff carries references and
//! bounded context metadata, never capabilities, credentials or raw transcript.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_TEXT: usize = 512;
pub const MAX_REFS: usize = 32;
pub const MAX_CONTEXT_BYTES: u32 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextTransferSpec {
    pub max_bytes: u32,
    pub include_checkpoint: bool,
    pub include_artifacts: bool,
    pub include_evidence: bool,
    pub include_messages: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffPacket {
    pub version: u32,
    pub handoff_id: String,
    pub from: String,
    pub target: String,
    pub objective: String,
    pub reason_code: String,
    pub summary: String,
    pub checkpoint_ref: Option<String>,
    pub artifact_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub open_questions: Vec<String>,
    pub blockers: Vec<String>,
    pub goal_id: Option<String>,
    pub workflow_run_id: String,
    pub parent_run_id: Option<String>,
    pub requested_context: ContextTransferSpec,
    pub created_at_ms: i64,
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffState {
    Proposed,
    Accepted,
    Active,
    Completed,
    Rejected,
    Expired,
    Failed,
    Returned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffTransition {
    pub state: HandoffState,
    pub actor: String,
    pub reason: String,
    pub version: u64,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffRecord {
    pub packet: HandoffPacket,
    pub state: HandoffState,
    pub version: u64,
    pub transitions: Vec<HandoffTransition>,
    pub provenance: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandoffError {
    Invalid(&'static str),
    UnsupportedVersion(u32),
    InvalidTransition,
    Expired,
    Stale,
    Duplicate,
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
            (String::from("target_run"), packet.workflow_run_id.clone()),
        ]),
    })
}

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

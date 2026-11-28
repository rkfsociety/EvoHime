//! Core-owned Team SOP contract and bounded session state machine.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Schema version accepted by team SOP protocol contracts.
pub const CONTRACT_VERSION: u32 = 1;
/// Maximum number of identifier entries in a list field.
pub const MAX_ITEMS: usize = 64;
/// Maximum number of participants in a team SOP protocol.
pub const MAX_PARTICIPANTS: usize = 32;
/// Maximum number of phases in a team SOP protocol.
pub const MAX_PHASES: usize = 32;
/// Maximum number of handoffs in a team SOP protocol.
pub const MAX_HANDOFFS: usize = 64;
/// Maximum review iterations allowed in a session.
pub const MAX_REVIEW_ITERATIONS: u32 = 8;
/// Maximum serialized byte size for canonical hashing.
pub const MAX_CANONICAL_BYTES: usize = 64 * 1024;

/// Versioned reference to a role profile and its content hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleProfileRef {
    /// Stable role profile identifier.
    pub id: String,
    /// Positive profile revision.
    pub revision: u64,
    /// SHA-256 hex digest of the role profile content.
    pub content_hash: String,
}
/// Participant slot definition inside a team SOP protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamParticipantSlot {
    /// Stable slot identifier.
    pub slot_id: String,
    /// Bound role profile reference.
    pub role_profile_ref: RoleProfileRef,
    /// Minimum number of participants in this slot.
    pub cardinality_min: u32,
    /// Maximum number of participants in this slot.
    pub cardinality_max: u32,
    /// Whether this slot is required for protocol execution.
    pub required: bool,
    /// Maximum capabilities granted to participants in this slot.
    pub grants_ceiling: Vec<String>,
    /// Allowed peer routing targets from this slot.
    pub allowed_peer_routes: Vec<String>,
}
/// Execution mode for a team SOP phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Steps run one after another.
    Sequential,
    /// Steps run concurrently.
    Parallel,
    /// Fan-out to workers followed by fan-in aggregation.
    FanOutFanIn,
    /// Iterative review loop.
    ReviewLoop,
    /// Explicit human approval gate.
    HumanGate,
}
/// Single phase inside a team SOP protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamPhase {
    /// Stable phase identifier.
    pub id: String,
    /// Human-readable phase name.
    pub name: String,
    /// Participant slot IDs owning this phase.
    pub owners: Vec<String>,
    /// Trigger condition for entering this phase.
    pub trigger: String,
    /// Required input artifact or signal identifiers.
    pub required_inputs: Vec<String>,
    /// Expected output artifact or signal identifiers.
    pub expected_outputs: Vec<String>,
    /// Execution mode of the phase.
    pub execution_mode: ExecutionMode,
    /// Exit criteria required to finish the phase.
    pub exit_criteria: Vec<String>,
    /// Maximum review iterations allowed for this phase.
    pub max_review_iterations: u32,
}
/// Bounded handoff contract between producer and consumer slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamHandoff {
    /// Stable handoff identifier.
    pub id: String,
    /// Producer participant slot ID.
    pub producer_slot: String,
    /// Output contract description.
    pub output_contract: String,
    /// Consumer participant slot IDs.
    pub consumers: Vec<String>,
    /// Whether this handoff is mandatory.
    pub required: bool,
    /// Delivery mode identifier.
    pub delivery_mode: String,
}
/// Review policy bound to a protocol phase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewPolicy {
    /// Target phase identifier.
    pub phase_id: String,
    /// Reviewer participant slot IDs.
    pub reviewer_slots: Vec<String>,
    /// Maximum review iterations before escalation.
    pub max_iterations: u32,
}
/// Completion requirements for a team SOP protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionContract {
    /// Phases that must complete before session completion.
    pub required_phases: Vec<String>,
    /// Evidence identifiers required on completion.
    pub required_evidence: Vec<String>,
}
/// Versioned team SOP protocol definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamProtocol {
    /// Contract schema version.
    pub schema_version: u32,
    /// Stable protocol identifier.
    pub id: String,
    /// Positive protocol version.
    pub version: u64,
    /// Human-readable protocol name.
    pub name: String,
    /// Protocol description.
    pub description: String,
    /// Primary objective of the protocol.
    pub objective: String,
    /// Declared participant slots.
    pub participants: Vec<TeamParticipantSlot>,
    /// Ordered protocol phases.
    pub phases: Vec<TeamPhase>,
    /// Handoff contracts between slots.
    pub handoffs: Vec<TeamHandoff>,
    /// Per-phase review policies.
    pub review_policies: Vec<ReviewPolicy>,
    /// Completion requirements.
    pub completion_contract: CompletionContract,
    /// Optional budget policy identifier.
    pub budget_policy: Option<String>,
    /// Optional escalation policy identifier.
    pub escalation_policy: Option<String>,
    /// Canonical SHA-256 hex digest of the protocol.
    pub content_hash: String,
}
/// Pinned immutable protocol snapshot bound to a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolSnapshot {
    /// Protocol identifier.
    pub protocol_id: String,
    /// Pinned protocol version.
    pub version: u64,
    /// Canonical content hash of the pinned protocol.
    pub content_hash: String,
    /// Serialized canonical JSON bytes of the protocol.
    pub protocol_json: Vec<u8>,
}
/// Lifecycle status of a team SOP session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Protocol snapshot is pinned and ready to start.
    Pinned,
    /// Session is actively running.
    Running,
    /// Session is paused awaiting review or input.
    Paused,
    /// Session completed all required phases.
    Completed,
    /// Session was cancelled.
    Cancelled,
    /// Session is blocked on a dependency or gate.
    Blocked,
    /// Session status is unknown.
    Unknown,
}
/// Runtime state of a team SOP session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSession {
    /// Stable session identifier.
    pub id: String,
    /// Pinned protocol snapshot.
    pub snapshot: ProtocolSnapshot,
    /// Identifier of the active phase.
    pub current_phase: String,
    /// Identifiers of completed phases.
    pub completed_phases: Vec<String>,
    /// Count of review iterations executed so far.
    pub review_iterations: u32,
    /// Current session lifecycle status.
    pub status: SessionStatus,
    /// Optional associated workflow run identifier.
    pub workflow_run_id: Option<String>,
    /// Optimistic concurrency version.
    pub version: u64,
}
/// Validation and state-machine errors for team SOP protocols and sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamSopError {
    /// Invalid field or payload.
    Invalid(&'static str),
    /// Unsupported schema version.
    UnsupportedVersion(u32),
    /// Entity already exists.
    Duplicate,
    /// Entity was not found.
    NotFound,
    /// Optimistic concurrency version mismatch.
    Stale,
    /// Idempotency key reused with a different payload.
    IdempotencyConflict,
    /// Caller lacks required capability.
    CapabilityDenied,
    /// Configured size or iteration limit exceeded.
    Limit,
}
impl std::fmt::Display for TeamSopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid(v) => v,
            Self::UnsupportedVersion(_) => "unsupported_version",
            Self::Duplicate => "duplicate",
            Self::NotFound => "not_found",
            Self::Stale => "stale",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::CapabilityDenied => "capability_denied",
            Self::Limit => "limit",
        })
    }
}
impl std::error::Error for TeamSopError {}
fn id(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 128
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}
fn text(v: &str) -> bool {
    !v.is_empty() && v.len() <= 8192
}
fn ids(values: &[String]) -> bool {
    values.len() <= MAX_ITEMS && values.iter().all(|v| id(v))
}
/// Computes the canonical SHA-256 hex digest of a [`TeamProtocol`].
pub fn canonical_hash(protocol: &TeamProtocol) -> Result<String, TeamSopError> {
    let mut copy = protocol.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| TeamSopError::Invalid("serialization"))?;
    if bytes.len() > MAX_CANONICAL_BYTES {
        return Err(TeamSopError::Limit);
    }
    Ok(hex::encode(Sha256::digest(bytes)))
}
/// Validates a [`TeamProtocol`] against schema, size, and reference bounds.
pub fn validate_protocol(p: &TeamProtocol) -> Result<(), TeamSopError> {
    if p.schema_version != CONTRACT_VERSION {
        return Err(TeamSopError::UnsupportedVersion(p.schema_version));
    }
    if !id(&p.id)
        || p.version == 0
        || !text(&p.name)
        || !text(&p.description)
        || !text(&p.objective)
    {
        return Err(TeamSopError::Invalid("protocol"));
    }
    if p.participants.is_empty()
        || p.participants.len() > MAX_PARTICIPANTS
        || p.phases.is_empty()
        || p.phases.len() > MAX_PHASES
        || p.handoffs.len() > MAX_HANDOFFS
    {
        return Err(TeamSopError::Limit);
    }
    let mut participant_ids = BTreeSet::new();
    for slot in &p.participants {
        if !id(&slot.slot_id)
            || !participant_ids.insert(&slot.slot_id)
            || slot.cardinality_min == 0
            || slot.cardinality_min > slot.cardinality_max
            || slot.cardinality_max > 32
            || !id(&slot.role_profile_ref.id)
            || slot.role_profile_ref.revision == 0
            || slot.role_profile_ref.content_hash.len() != 64
            || !ids(&slot.grants_ceiling)
            || !ids(&slot.allowed_peer_routes)
        {
            return Err(TeamSopError::Invalid("participant"));
        }
    }
    let mut phase_ids = BTreeSet::new();
    for phase in &p.phases {
        if !id(&phase.id)
            || !phase_ids.insert(&phase.id)
            || !text(&phase.name)
            || !ids(&phase.owners)
            || !text(&phase.trigger)
            || !ids(&phase.required_inputs)
            || !ids(&phase.expected_outputs)
            || phase.exit_criteria.is_empty()
            || phase.exit_criteria.len() > MAX_ITEMS
            || phase.exit_criteria.iter().any(|v| !text(v))
            || phase.max_review_iterations > MAX_REVIEW_ITERATIONS
        {
            return Err(TeamSopError::Invalid("phase"));
        }
        if phase
            .owners
            .iter()
            .any(|owner| !participant_ids.contains(owner))
        {
            return Err(TeamSopError::Invalid("phase_owner"));
        }
    }
    let mut handoff_ids = BTreeSet::new();
    for h in &p.handoffs {
        if !id(&h.id)
            || !handoff_ids.insert(&h.id)
            || !participant_ids.contains(&h.producer_slot)
            || !text(&h.output_contract)
            || h.consumers.is_empty()
            || h.consumers.iter().any(|v| !participant_ids.contains(v))
            || !id(&h.delivery_mode)
        {
            return Err(TeamSopError::Invalid("handoff"));
        }
    }
    if p.review_policies.len() > p.phases.len()
        || p.review_policies.iter().any(|r| {
            !phase_ids.contains(&r.phase_id)
                || r.reviewer_slots.is_empty()
                || r.reviewer_slots
                    .iter()
                    .any(|v| !participant_ids.contains(v))
                || r.max_iterations > MAX_REVIEW_ITERATIONS
        })
    {
        return Err(TeamSopError::Invalid("review_policy"));
    }
    if p.completion_contract.required_phases.is_empty()
        || p.completion_contract
            .required_phases
            .iter()
            .any(|v| !phase_ids.contains(v))
        || !ids(&p.completion_contract.required_evidence)
    {
        return Err(TeamSopError::Invalid("completion"));
    }
    if !p.content_hash.is_empty() && p.content_hash != canonical_hash(p)? {
        return Err(TeamSopError::Invalid("content_hash"));
    }
    Ok(())
}
/// In-memory registry of team SOP protocols, sessions, and idempotency keys.
#[derive(Debug, Default)]
pub struct TeamSopRegistry {
    /// Registered protocols indexed by ID.
    pub protocols: BTreeMap<String, TeamProtocol>,
    /// Active and historical sessions indexed by ID.
    pub sessions: BTreeMap<String, TeamSession>,
    /// Idempotency keys mapped to canonical payload hashes.
    pub idempotency: BTreeMap<String, String>,
}
impl TeamSopRegistry {
    /// Lists all registered team protocols.
    pub fn list(&self) -> Vec<TeamProtocol> {
        self.protocols.values().cloned().collect()
    }
    /// Validates and registers a new team protocol idempotently.
    pub fn create(&mut self, mut p: TeamProtocol, key: &str) -> Result<TeamProtocol, TeamSopError> {
        validate_protocol(&p)?;
        if self.protocols.contains_key(&p.id) {
            return Err(TeamSopError::Duplicate);
        }
        let hash = canonical_hash(&p)?;
        if let Some(v) = self.idempotency.get(key) {
            return if v == &hash {
                Ok(p)
            } else {
                Err(TeamSopError::IdempotencyConflict)
            };
        }
        p.content_hash = hash.clone();
        self.idempotency.insert(key.into(), hash);
        self.protocols.insert(p.id.clone(), p.clone());
        Ok(p)
    }
    /// Revises an existing team protocol with optimistic concurrency checking.
    pub fn revise(
        &mut self,
        mut p: TeamProtocol,
        expected: u64,
        key: &str,
    ) -> Result<TeamProtocol, TeamSopError> {
        validate_protocol(&p)?;
        let current = self.protocols.get(&p.id).ok_or(TeamSopError::NotFound)?;
        if current.version != expected || p.version <= expected {
            return Err(TeamSopError::Stale);
        }
        let hash = canonical_hash(&p)?;
        if let Some(v) = self.idempotency.get(key) {
            return if v == &hash {
                Ok(p)
            } else {
                Err(TeamSopError::IdempotencyConflict)
            };
        }
        p.content_hash = hash.clone();
        self.idempotency.insert(key.into(), hash);
        self.protocols.insert(p.id.clone(), p.clone());
        Ok(p)
    }
    /// Pins a protocol version and starts a new team SOP session.
    pub fn start(
        &mut self,
        session_id: String,
        protocol_id: &str,
        version: u64,
        workflow_run_id: Option<String>,
    ) -> Result<TeamSession, TeamSopError> {
        if !id(&session_id) {
            return Err(TeamSopError::Invalid("session_id"));
        }
        if self.sessions.contains_key(&session_id) {
            return Err(TeamSopError::Duplicate);
        }
        let p = self
            .protocols
            .get(protocol_id)
            .ok_or(TeamSopError::NotFound)?;
        if p.version != version {
            return Err(TeamSopError::Stale);
        }
        let bytes = serde_json::to_vec(p).map_err(|_| TeamSopError::Invalid("serialization"))?;
        let s = TeamSession {
            id: session_id.clone(),
            snapshot: ProtocolSnapshot {
                protocol_id: p.id.clone(),
                version: p.version,
                content_hash: p.content_hash.clone(),
                protocol_json: bytes,
            },
            current_phase: p.phases[0].id.clone(),
            completed_phases: vec![],
            review_iterations: 0,
            status: SessionStatus::Pinned,
            workflow_run_id,
            version: 1,
        };
        self.sessions.insert(session_id, s.clone());
        Ok(s)
    }
    /// Advances an active session to the next phase or completes it.
    pub fn advance(&mut self, id: &str, expected: u64) -> Result<TeamSession, TeamSopError> {
        let s = self.sessions.get_mut(id).ok_or(TeamSopError::NotFound)?;
        if s.version != expected {
            return Err(TeamSopError::Stale);
        }
        let p = self
            .protocols
            .get(&s.snapshot.protocol_id)
            .ok_or(TeamSopError::NotFound)?;
        let index = p
            .phases
            .iter()
            .position(|x| x.id == s.current_phase)
            .ok_or(TeamSopError::Invalid("phase"))?;
        s.completed_phases.push(s.current_phase.clone());
        if index + 1 == p.phases.len() {
            s.status = SessionStatus::Completed;
        } else {
            s.current_phase = p.phases[index + 1].id.clone();
            s.status = SessionStatus::Running;
        }
        s.version += 1;
        Ok(s.clone())
    }
    /// Cancels an active or paused session.
    pub fn cancel(&mut self, id: &str) -> Result<TeamSession, TeamSopError> {
        let s = self.sessions.get_mut(id).ok_or(TeamSopError::NotFound)?;
        if !matches!(
            s.status,
            SessionStatus::Completed | SessionStatus::Cancelled
        ) {
            s.status = SessionStatus::Cancelled;
            s.version += 1;
        }
        Ok(s.clone())
    }

    /// Records a review step on a session, resuming or pausing within the limit.
    pub fn review(
        &mut self,
        id: &str,
        expected: u64,
        revise: bool,
    ) -> Result<TeamSession, TeamSopError> {
        let s = self.sessions.get_mut(id).ok_or(TeamSopError::NotFound)?;
        if s.version != expected {
            return Err(TeamSopError::Stale);
        }
        if s.review_iterations >= MAX_REVIEW_ITERATIONS {
            return Err(TeamSopError::Limit);
        }
        s.review_iterations += 1;
        s.status = if revise {
            SessionStatus::Running
        } else {
            SessionStatus::Paused
        };
        s.version += 1;
        Ok(s.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> TeamProtocol {
        TeamProtocol {
            schema_version: 1,
            id: "coding".into(),
            version: 1,
            name: "Coding team".into(),
            description: "bounded".into(),
            objective: "deliver".into(),
            participants: vec![TeamParticipantSlot {
                slot_id: "reviewer".into(),
                role_profile_ref: RoleProfileRef {
                    id: "reviewer".into(),
                    revision: 1,
                    content_hash: "a".repeat(64),
                },
                cardinality_min: 1,
                cardinality_max: 1,
                required: true,
                grants_ceiling: vec!["review".into()],
                allowed_peer_routes: vec![],
            }],
            phases: vec![TeamPhase {
                id: "review".into(),
                name: "Review".into(),
                owners: vec!["reviewer".into()],
                trigger: "start".into(),
                required_inputs: vec![],
                expected_outputs: vec!["report".into()],
                execution_mode: ExecutionMode::Sequential,
                exit_criteria: vec!["evidence".into()],
                max_review_iterations: 2,
            }],
            handoffs: vec![],
            review_policies: vec![],
            completion_contract: CompletionContract {
                required_phases: vec!["review".into()],
                required_evidence: vec!["report".into()],
            },
            budget_policy: None,
            escalation_policy: None,
            content_hash: String::new(),
        }
    }
    #[test]
    fn validates_hash_and_pins_snapshot() {
        let mut r = TeamSopRegistry::default();
        let x = r.create(p(), "k").unwrap();
        assert_eq!(x.content_hash.len(), 64);
        let s = r.start("s".into(), "coding", 1, None).unwrap();
        assert_eq!(s.snapshot.content_hash, x.content_hash);
    }
    #[test]
    fn stale_and_duplicate_are_typed() {
        let mut r = TeamSopRegistry::default();
        r.create(p(), "k").unwrap();
        assert!(matches!(
            r.create(p(), "other"),
            Err(TeamSopError::Duplicate)
        ));
        let s = r.start("s".into(), "coding", 1, None).unwrap();
        assert!(matches!(
            r.advance("s", s.version + 1),
            Err(TeamSopError::Stale)
        ));
    }
    #[test]
    fn review_loop_is_bounded_and_revise_is_typed() {
        let mut r = TeamSopRegistry::default();
        r.create(p(), "k").unwrap();
        let mut s = r.start("s".into(), "coding", 1, None).unwrap();
        for _ in 0..MAX_REVIEW_ITERATIONS {
            s = r.review("s", s.version, true).unwrap();
        }
        assert!(matches!(
            r.review("s", s.version, true),
            Err(TeamSopError::Limit)
        ));
    }
}

//! Core-owned Team SOP contract and bounded session state machine.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_ITEMS: usize = 64;
pub const MAX_PARTICIPANTS: usize = 32;
pub const MAX_PHASES: usize = 32;
pub const MAX_HANDOFFS: usize = 64;
pub const MAX_REVIEW_ITERATIONS: u32 = 8;
pub const MAX_CANONICAL_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleProfileRef {
    pub id: String,
    pub revision: u64,
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamParticipantSlot {
    pub slot_id: String,
    pub role_profile_ref: RoleProfileRef,
    pub cardinality_min: u32,
    pub cardinality_max: u32,
    pub required: bool,
    pub grants_ceiling: Vec<String>,
    pub allowed_peer_routes: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Sequential,
    Parallel,
    FanOutFanIn,
    ReviewLoop,
    HumanGate,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamPhase {
    pub id: String,
    pub name: String,
    pub owners: Vec<String>,
    pub trigger: String,
    pub required_inputs: Vec<String>,
    pub expected_outputs: Vec<String>,
    pub execution_mode: ExecutionMode,
    pub exit_criteria: Vec<String>,
    pub max_review_iterations: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamHandoff {
    pub id: String,
    pub producer_slot: String,
    pub output_contract: String,
    pub consumers: Vec<String>,
    pub required: bool,
    pub delivery_mode: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewPolicy {
    pub phase_id: String,
    pub reviewer_slots: Vec<String>,
    pub max_iterations: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionContract {
    pub required_phases: Vec<String>,
    pub required_evidence: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamProtocol {
    pub schema_version: u32,
    pub id: String,
    pub version: u64,
    pub name: String,
    pub description: String,
    pub objective: String,
    pub participants: Vec<TeamParticipantSlot>,
    pub phases: Vec<TeamPhase>,
    pub handoffs: Vec<TeamHandoff>,
    pub review_policies: Vec<ReviewPolicy>,
    pub completion_contract: CompletionContract,
    pub budget_policy: Option<String>,
    pub escalation_policy: Option<String>,
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolSnapshot {
    pub protocol_id: String,
    pub version: u64,
    pub content_hash: String,
    pub protocol_json: Vec<u8>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Pinned,
    Running,
    Paused,
    Completed,
    Cancelled,
    Blocked,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSession {
    pub id: String,
    pub snapshot: ProtocolSnapshot,
    pub current_phase: String,
    pub completed_phases: Vec<String>,
    pub review_iterations: u32,
    pub status: SessionStatus,
    pub workflow_run_id: Option<String>,
    pub version: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamSopError {
    Invalid(&'static str),
    UnsupportedVersion(u32),
    Duplicate,
    NotFound,
    Stale,
    IdempotencyConflict,
    CapabilityDenied,
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
pub fn canonical_hash(protocol: &TeamProtocol) -> Result<String, TeamSopError> {
    let mut copy = protocol.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| TeamSopError::Invalid("serialization"))?;
    if bytes.len() > MAX_CANONICAL_BYTES {
        return Err(TeamSopError::Limit);
    }
    Ok(hex::encode(Sha256::digest(bytes)))
}
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
#[derive(Debug, Default)]
pub struct TeamSopRegistry {
    pub protocols: BTreeMap<String, TeamProtocol>,
    pub sessions: BTreeMap<String, TeamSession>,
    pub idempotency: BTreeMap<String, String>,
}
impl TeamSopRegistry {
    pub fn list(&self) -> Vec<TeamProtocol> {
        self.protocols.values().cloned().collect()
    }
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

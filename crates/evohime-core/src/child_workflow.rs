//! Core-owned coordinator, leases, context isolation and safe projections.
//! This module deliberately contains no model transcript or renderer state.

use crate::child_contracts::{
    ContractError, CorrelationContext, Grant, TypedChildReport, TypedChildTaskRequest,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const DEFAULT_LEASE_MS: u64 = 30_000;
pub const DEFAULT_HEARTBEAT_MS: u64 = 5_000;
pub const MAX_TRANSPORT_RETRIES: u8 = 3;
pub const DEFAULT_INLINE_MAX_BYTES: usize = 32 * 1024;
pub const DEAD_LETTER_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinatorState {
    Created,
    Queued,
    Running,
    Validating,
    WaitingParentAcceptance,
    Accepted,
    Rejected,
    Failed,
    Cancelled,
    TimedOut,
    Aborted,
    RevisePlan,
}

impl CoordinatorState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created", Self::Queued => "queued", Self::Running => "running",
            Self::Validating => "validating", Self::WaitingParentAcceptance => "waiting_parent_acceptance",
            Self::Accepted => "accepted", Self::Rejected => "rejected", Self::Failed => "failed",
            Self::Cancelled => "cancelled", Self::TimedOut => "timed_out", Self::Aborted => "aborted",
            Self::RevisePlan => "revise_plan",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "created" => Self::Created, "queued" => Self::Queued, "running" => Self::Running,
            "validating" => Self::Validating, "waiting_parent_acceptance" => Self::WaitingParentAcceptance,
            "accepted" => Self::Accepted, "rejected" => Self::Rejected, "failed" => Self::Failed,
            "cancelled" => Self::Cancelled, "timed_out" => Self::TimedOut, "aborted" => Self::Aborted,
            "revise_plan" => Self::RevisePlan, _ => return None,
        })
    }
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Accepted
                | Self::Rejected
                | Self::Failed
                | Self::Cancelled
                | Self::TimedOut
                | Self::Aborted
                | Self::RevisePlan
        )
    }
    pub fn can_transition(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Queued)
                | (Self::Queued, Self::Running)
                | (
                    Self::Running,
                    Self::Validating
                        | Self::Cancelled
                        | Self::TimedOut
                        | Self::Aborted
                        | Self::Failed
                )
                | (
                    Self::Validating,
                    Self::WaitingParentAcceptance | Self::Failed | Self::Cancelled | Self::Aborted
                )
                | (
                    Self::WaitingParentAcceptance,
                    Self::Accepted
                        | Self::Rejected
                        | Self::Failed
                        | Self::Cancelled
                        | Self::RevisePlan
                )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildLease {
    pub child_task_id: String,
    pub revision: u32,
    pub issued_at_wall_ms: u64,
    pub deadline_wall_ms: u64,
    pub heartbeat_interval_ms: u64,
    pub last_heartbeat_wall_ms: u64,
    pub process_alive: bool,
    #[serde(default)]
    pub created_monotonic_ms: u64,
    #[serde(default)]
    pub deadline_monotonic_ms: u64,
    #[serde(default)]
    pub clock_boot_id: String,
    #[serde(default)]
    pub holder_process_id: String,
}

impl ChildLease {
    pub fn new(
        child_task_id: impl Into<String>,
        revision: u32,
        now_ms: u64,
        duration_ms: u64,
    ) -> Self {
        Self {
            child_task_id: child_task_id.into(),
            revision,
            issued_at_wall_ms: now_ms,
            deadline_wall_ms: now_ms.saturating_add(duration_ms),
            heartbeat_interval_ms: DEFAULT_HEARTBEAT_MS,
            last_heartbeat_wall_ms: now_ms,
            process_alive: true,
            created_monotonic_ms: now_ms,
            deadline_monotonic_ms: now_ms.saturating_add(duration_ms),
            clock_boot_id: "current".into(),
            holder_process_id: std::process::id().to_string(),
        }
    }
    pub fn heartbeat(&mut self, now_ms: u64, duration_ms: u64) -> bool {
        if !self.is_live(now_ms) {
            return false;
        }
        self.last_heartbeat_wall_ms = now_ms;
        self.deadline_wall_ms = now_ms.saturating_add(duration_ms);
        self.deadline_monotonic_ms = now_ms.saturating_add(duration_ms);
        true
    }
    pub fn is_live(&self, now_ms: u64) -> bool {
        self.process_alive && now_ms <= self.deadline_wall_ms
    }
    pub fn is_live_in_boot(&self, now_monotonic_ms: u64, boot_id: &str) -> bool {
        self.process_alive
            && self.clock_boot_id == boot_id
            && now_monotonic_ms <= self.deadline_monotonic_ms
    }
    pub fn expire(&mut self) {
        self.process_alive = false;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinatorCheckpoint {
    pub child_task_id: String,
    pub parent_task_id: String,
    pub state: CoordinatorState,
    pub revision: u32,
    pub parent_sequence: u64,
    pub lease: ChildLease,
    pub report_hash: Option<String>,
    pub retry_count: u8,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinatorError {
    InvalidTransition,
    TerminalState,
    LeaseLost,
    RetryExhausted,
    RevisionExhausted,
    ParentMismatch,
    InvalidCheckpoint,
}

#[derive(Debug, Clone)]
pub struct Coordinator {
    checkpoints: BTreeMap<String, CoordinatorCheckpoint>,
    next_sequence: BTreeMap<String, u64>,
}

impl Coordinator {
    pub fn new() -> Self {
        Self {
            checkpoints: BTreeMap::new(),
            next_sequence: BTreeMap::new(),
        }
    }
    pub fn next_parent_sequence(&mut self, parent: &str) -> u64 {
        let value = self.next_sequence.entry(parent.to_owned()).or_insert(0);
        *value += 1;
        *value
    }
    pub fn create(
        &mut self,
        request: &TypedChildTaskRequest,
        now_ms: u64,
    ) -> Result<CoordinatorCheckpoint, CoordinatorError> {
        if request.validate().is_err() {
            return Err(CoordinatorError::InvalidCheckpoint);
        }
        let checkpoint = CoordinatorCheckpoint {
            child_task_id: request.child_task_id.clone(),
            parent_task_id: request.parent_task_id.clone(),
            state: CoordinatorState::Created,
            revision: 0,
            parent_sequence: request.correlation.parent_sequence,
            lease: ChildLease::new(&request.child_task_id, 0, now_ms, DEFAULT_LEASE_MS),
            report_hash: None,
            retry_count: 0,
            reason_code: None,
        };
        self.checkpoints
            .insert(request.child_task_id.clone(), checkpoint.clone());
        Ok(checkpoint)
    }
    pub fn transition(
        &mut self,
        child: &str,
        next: CoordinatorState,
        reason: Option<String>,
    ) -> Result<CoordinatorCheckpoint, CoordinatorError> {
        let checkpoint = self
            .checkpoints
            .get_mut(child)
            .ok_or(CoordinatorError::InvalidCheckpoint)?;
        if checkpoint.state.terminal() {
            return Err(CoordinatorError::TerminalState);
        }
        if !checkpoint.state.can_transition(next) {
            return Err(CoordinatorError::InvalidTransition);
        }
        checkpoint.state = next;
        checkpoint.reason_code = reason;
        Ok(checkpoint.clone())
    }
    pub fn heartbeat(&mut self, child: &str, now_ms: u64) -> Result<(), CoordinatorError> {
        let c = self
            .checkpoints
            .get_mut(child)
            .ok_or(CoordinatorError::InvalidCheckpoint)?;
        if c.lease.heartbeat(now_ms, DEFAULT_LEASE_MS) {
            Ok(())
        } else {
            Err(CoordinatorError::LeaseLost)
        }
    }
    pub fn recover(
        &mut self,
        checkpoint: CoordinatorCheckpoint,
        now_ms: u64,
    ) -> Result<CoordinatorCheckpoint, CoordinatorError> {
        if checkpoint.lease.is_live(now_ms) {
            self.checkpoints
                .insert(checkpoint.child_task_id.clone(), checkpoint.clone());
            return Ok(checkpoint);
        }
        let mut failed = checkpoint;
        failed.lease.expire();
        failed.state = match failed.state {
            CoordinatorState::Running
            | CoordinatorState::Validating
            | CoordinatorState::WaitingParentAcceptance => CoordinatorState::Failed,
            other => other,
        };
        failed.reason_code = Some("restart_no_live_lease".into());
        self.checkpoints
            .insert(failed.child_task_id.clone(), failed.clone());
        Ok(failed)
    }
    pub fn checkpoint(&self, child: &str) -> Option<&CoordinatorCheckpoint> {
        self.checkpoints.get(child)
    }

    /// Starts a bounded revision while preserving the parent sequence and
    /// creating a fresh lease. Revision zero is the initial attempt; the
    /// configured limit counts additional attempts.
    pub fn begin_revision(
        &mut self,
        child: &str,
        max_revisions: u32,
        now_ms: u64,
    ) -> Result<CoordinatorCheckpoint, CoordinatorError> {
        let checkpoint = self
            .checkpoints
            .get_mut(child)
            .ok_or(CoordinatorError::InvalidCheckpoint)?;
        if checkpoint.state != CoordinatorState::RevisePlan
            && checkpoint.state != CoordinatorState::Rejected
        {
            return Err(CoordinatorError::InvalidTransition);
        }
        if checkpoint.revision >= max_revisions.min(3) {
            return Err(CoordinatorError::RevisionExhausted);
        }
        checkpoint.revision += 1;
        checkpoint.state = CoordinatorState::Created;
        checkpoint.retry_count = 0;
        checkpoint.reason_code = None;
        checkpoint.lease = ChildLease::new(
            checkpoint.child_task_id.clone(),
            checkpoint.revision,
            now_ms,
            DEFAULT_LEASE_MS,
        );
        Ok(checkpoint.clone())
    }

    pub fn retry_transport(&mut self, child: &str) -> Result<bool, CoordinatorError> {
        let checkpoint = self
            .checkpoints
            .get_mut(child)
            .ok_or(CoordinatorError::InvalidCheckpoint)?;
        if checkpoint.retry_count >= MAX_TRANSPORT_RETRIES {
            checkpoint.state = CoordinatorState::Failed;
            checkpoint.reason_code = Some("transport_retries_exhausted".into());
            checkpoint.lease.expire();
            return Ok(false);
        }
        checkpoint.retry_count += 1;
        Ok(true)
    }

    pub fn mark_dead_letter(&mut self, child: &str) -> Result<CoordinatorCheckpoint, CoordinatorError> {
        let checkpoint = self
            .checkpoints
            .get_mut(child)
            .ok_or(CoordinatorError::InvalidCheckpoint)?;
        checkpoint.state = CoordinatorState::Failed;
        checkpoint.reason_code = Some("dead_letter".into());
        checkpoint.lease.expire();
        Ok(checkpoint.clone())
    }

    pub fn recover_with_boot(
        &mut self,
        checkpoint: CoordinatorCheckpoint,
        now_monotonic_ms: u64,
        boot_id: &str,
    ) -> Result<CoordinatorCheckpoint, CoordinatorError> {
        if checkpoint.lease.is_live_in_boot(now_monotonic_ms, boot_id) {
            self.checkpoints
                .insert(checkpoint.child_task_id.clone(), checkpoint.clone());
            return Ok(checkpoint);
        }
        self.recover(checkpoint, now_monotonic_ms)
    }

    pub fn to_storage_record(
        &self,
        child: &str,
    ) -> Result<evohime_local_storage::child_store::CoordinatorCheckpointRecord, CoordinatorError> {
        let checkpoint = self
            .checkpoints
            .get(child)
            .ok_or(CoordinatorError::InvalidCheckpoint)?;
        Ok(evohime_local_storage::child_store::CoordinatorCheckpointRecord {
            schema_version: 1,
            child_task_id: checkpoint.child_task_id.clone(),
            parent_task_id: checkpoint.parent_task_id.clone(),
            revision: checkpoint.revision as i64,
            state: checkpoint.state.as_str().into(),
            failure_reason: checkpoint.reason_code.clone(),
            dead_letter: checkpoint.reason_code.as_deref() == Some("dead_letter"),
            report_json: None,
            evidence_locators_json: None,
            provenance_hashes_json: None,
            parent_sequence: checkpoint.parent_sequence as i64,
            lease_deadline_monotonic_ms: Some(checkpoint.lease.deadline_monotonic_ms as i64),
            lease_created_monotonic_ms: Some(checkpoint.lease.created_monotonic_ms as i64),
            lease_clock_boot_id: Some(checkpoint.lease.clock_boot_id.clone()),
            lease_holder_process_id: Some(checkpoint.lease.holder_process_id.clone()),
            last_transition_event: checkpoint
                .reason_code
                .clone()
                .unwrap_or_else(|| checkpoint.state.as_str().into()),
            last_transition_at_ms: checkpoint.lease.last_heartbeat_wall_ms as i64,
            created_at_ms: checkpoint.lease.issued_at_wall_ms as i64,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildProjection {
    pub event_id: String,
    pub parent_task_id: String,
    pub child_task_id: String,
    pub role: String,
    pub revision: u32,
    pub state: CoordinatorState,
    pub reason_code: Option<String>,
    pub parent_sequence: u64,
    pub budget: Option<crate::child_contracts::ChildBudget>,
    pub lease_live: bool,
    pub dead_letter: bool,
}

pub fn selected_context(
    request: &TypedChildTaskRequest,
    available: &BTreeSet<String>,
) -> Result<Vec<String>, ContractError> {
    for id in &request.input_context_ids {
        if !available.contains(id) {
            return Err(ContractError::ContextIdNotAccessible { id: id.clone() });
        }
    }
    Ok(request.input_context_ids.clone())
}

pub fn reviewer_grants(grants: &[Grant], role: &str) -> Vec<Grant> {
    if role != "reviewer" {
        return grants.to_vec();
    }
    let mut result: Vec<Grant> = grants
        .iter()
        .filter(|g| g.grant_type != "artifact_read_full")
        .cloned()
        .collect();
    if !result
        .iter()
        .any(|g| g.grant_type == "artifact_read_summary_only")
    {
        result.push(Grant {
            grant_type: "artifact_read_summary_only".into(),
            scope: None,
        });
    }
    result
}

pub fn deterministic_fan_in(reports: &mut [TypedChildReport]) {
    reports.sort_by(|a, b| {
        a.provenance
            .parent_sequence
            .cmp(&b.provenance.parent_sequence)
            .then_with(|| a.provenance.evidence_hash.cmp(&b.provenance.evidence_hash))
    });
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceCandidate {
    pub child_task_id: String,
    pub locator: String,
    pub path_scope: String,
    pub content_hash: String,
    pub published_at_ms: u64,
    pub parent_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupersededEvidence {
    pub locator: String,
    pub superseded_by: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanInResolution {
    pub selected: Vec<EvidenceCandidate>,
    pub superseded: Vec<SupersededEvidence>,
    pub unknowns: Vec<String>,
}

pub fn resolve_fan_in_conflicts(candidates: &[EvidenceCandidate]) -> FanInResolution {
    let mut ordered = candidates.to_vec();
    ordered.sort_by(|left, right| {
        right.published_at_ms.cmp(&left.published_at_ms)
            .then_with(|| right.path_scope.len().cmp(&left.path_scope.len()))
            .then_with(|| left.parent_sequence.cmp(&right.parent_sequence))
            .then_with(|| left.content_hash.cmp(&right.content_hash))
            .then_with(|| left.locator.cmp(&right.locator))
    });
    let mut selected = Vec::new();
    let mut superseded = Vec::new();
    let mut unknowns = Vec::new();
    for candidate in ordered {
        let Some(existing) = selected.iter().find(|item: &&EvidenceCandidate| {
            item.path_scope == candidate.path_scope
                || item.path_scope.starts_with(&format!("{}/", candidate.path_scope))
                || candidate.path_scope.starts_with(&format!("{}/", item.path_scope))
        }) else {
            selected.push(candidate);
            continue;
        };
        if existing.content_hash == candidate.content_hash {
            superseded.push(SupersededEvidence { locator: candidate.locator, superseded_by: existing.locator.clone(), reason: "duplicate_content".into() });
        } else if existing.published_at_ms == candidate.published_at_ms
            && existing.path_scope.len() == candidate.path_scope.len()
            && existing.parent_sequence == candidate.parent_sequence
            && existing.content_hash == candidate.content_hash
        {
            unknowns.push(format!("conflicting evidence scope {}", candidate.path_scope));
        } else {
            superseded.push(SupersededEvidence { locator: candidate.locator, superseded_by: existing.locator.clone(), reason: "deterministic_tiebreak".into() });
        }
    }
    FanInResolution { selected, superseded, unknowns }
}

pub fn correlation_parent_chain(correlation: &CorrelationContext) -> Vec<String> {
    vec![correlation.task_id.as_str().to_owned()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptanceDecision {
    Accepted,
    RevisePlan,
    WaitingParentAcceptance,
}

/// A partial tester result is acceptable only when required checks pass. An
/// optional failure is retained as evidence but never silently upgraded to a
/// success; the parent decides whether it needs a revision.
pub fn evaluate_test_acceptance(
    required_failed: bool,
    optional_failed: bool,
    parent_approved: bool,
) -> AcceptanceDecision {
    if required_failed {
        return AcceptanceDecision::RevisePlan;
    }
    if !parent_approved {
        return AcceptanceDecision::WaitingParentAcceptance;
    }
    let _optional_failure_is_evidence = optional_failed;
    AcceptanceDecision::Accepted
}

/// Re-checks the effective grants at every Core tool boundary. The caller
/// must provide the current parent grants, never a snapshot from creation.
pub fn validate_tool_call_grants(
    child_grants: &[Grant],
    current_parent_grants: &[Grant],
    role: crate::child_roles::ChildRole,
    capability: &str,
) -> Result<(), ContractError> {
    if !crate::child_roles::can_request_capability(role, capability) {
        return Err(ContractError::ForbiddenCapability(capability.to_owned()));
    }
    crate::child_contracts::validate_grant_subset(child_grants, current_parent_grants)
        .map_err(|_| ContractError::GrantDrift)
}

pub fn artifact_full_read_allowed(grants: &[Grant], locator: &str) -> bool {
    grants.iter().any(|grant| {
        grant.grant_type == "artifact_read_full"
            && grant.scope.as_deref().is_some_and(|scope| {
                scope == locator || (locator.starts_with(scope) && locator.as_bytes().get(scope.len()) == Some(&b'/'))
            })
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactSummaryProjection {
    pub locator: String,
    pub content_hash: String,
    pub summary: String,
    pub bytes: u64,
}

/// Performs the artifact policy check on every invocation. Callers may retain
/// only the returned summary; no full blob is exposed without a current,
/// locator-scoped full grant and explicit selected-context membership.
pub fn read_artifact_for_child(
    store: &evohime_local_storage::artifact_store::ArtifactStore<'_>,
    correlation: &CorrelationContext,
    selected_context_ids: &[String],
    grants: &[Grant],
    locator: &str,
    full: bool,
    kind: &str,
    now_ms: i64,
) -> Result<Result<String, ArtifactSummaryProjection>, ContractError> {
    if !selected_context_ids.iter().any(|id| id == locator) {
        return Err(ContractError::ContextIdNotAccessible { id: locator.to_owned() });
    }
    let reference = store
        .get_ref(locator)
        .map_err(|error| ContractError::ArtifactOffload(error.to_string()))?
        .ok_or_else(|| ContractError::ArtifactOffload("artifact was not found".into()))?;
    if full && !artifact_full_read_allowed(grants, locator) {
        return Err(ContractError::GrantDrift);
    }
    if full {
        let parent_chain = correlation_parent_chain(correlation);
        return store
            .read(locator, correlation.child_id.as_str(), &parent_chain, kind, now_ms)
            .map(Ok)
            .map_err(|error| ContractError::ArtifactOffload(error.to_string()));
    }
    Ok(Err(ArtifactSummaryProjection { locator: reference.locator, content_hash: reference.content_hash, summary: reference.summary, bytes: reference.bytes }))
}

/// Validates and, when explicitly enabled, offloads an oversized report before
/// it reaches persistence or the parent context. Sensitive/secret privacy is
/// rejected by the artifact store and never produces a parent-visible summary.
pub fn accept_report_with_offload(
    connection: &rusqlite::Connection,
    request: &TypedChildTaskRequest,
    report: &TypedChildReport,
    now_ms: i64,
) -> Result<TypedChildReport, ContractError> {
    use evohime_context_budget::item::Privacy;
    let Some(output) = report.output_data.as_deref() else {
        return crate::child_contracts::accept_typed_report(request, report);
    };
    let threshold = request.output_schema.as_ref().and_then(|schema| schema.max_bytes).unwrap_or(DEFAULT_INLINE_MAX_BYTES);
    if output.len() <= threshold || !request.allow_output_offload {
        return crate::child_contracts::accept_typed_report(request, report);
    }
    if let Some(schema) = request.output_schema.as_ref() {
        if let Some(json_schema) = &schema.json_schema {
            let schema_without_size = crate::child_contracts::Schema { json_schema: Some(json_schema.clone()), content_type: schema.content_type.clone(), max_bytes: None };
            schema_without_size.validate_content(output)?;
        }
    }
    let privacy = match request.output_privacy.as_deref().unwrap_or("workspace") {
        "workspace" => Privacy::Workspace,
        "sensitive" => Privacy::Sensitive,
        "secret" => Privacy::Secret,
        other => return Err(ContractError::ArtifactOffload(format!("unknown privacy label {other}"))),
    };
    let result = evohime_local_storage::artifact_store::ArtifactStore::new(connection)
        .offload("child-report", &request.child_task_id, &request.parent_task_id, output, privacy, now_ms)
        .map_err(|error| ContractError::ArtifactOffload(error.to_string()))?;
    let mut bounded = report.clone();
    bounded.output_data = None;
    bounded.output_artifact = Some(crate::child_contracts::ArtifactOutputRef { locator: result.reference.locator, content_hash: result.reference.content_hash, summary: result.reference.summary });
    crate::child_contracts::accept_typed_report(request, &bounded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::child_contracts::{CorrelationId, Provenance};
    fn request() -> TypedChildTaskRequest {
        TypedChildTaskRequest::new(
            "child",
            "parent",
            "researcher",
            "inspect",
            CorrelationContext::new(
                CorrelationId::new("parent").unwrap(),
                CorrelationId::new("child").unwrap(),
                1,
            ),
        )
        .unwrap()
    }
    #[test]
    fn lease_expiry_recovery_is_bounded() {
        let mut c = Coordinator::new();
        let r = request();
        let cp = c.create(&r, 0).unwrap();
        let recovered = c.recover(cp, DEFAULT_LEASE_MS + 1).unwrap();
        assert_eq!(recovered.state, CoordinatorState::Created);
    }
    #[test]
    fn state_machine_rejects_skips() {
        let mut c = Coordinator::new();
        let r = request();
        c.create(&r, 0).unwrap();
        assert_eq!(
            c.transition("child", CoordinatorState::Running, None),
            Err(CoordinatorError::InvalidTransition)
        );
    }
    #[test]
    fn fan_in_uses_parent_sequence() {
        let mut a = request();
        let mut b = request();
        a.correlation.parent_sequence = 2;
        b.correlation.parent_sequence = 1;
        let mut rs = vec![
            crate::child_contracts::TypedChildReport::new(
                "child",
                "parent",
                a.correlation.clone(),
                Provenance::new(2),
            )
            .unwrap(),
            crate::child_contracts::TypedChildReport::new(
                "child",
                "parent",
                b.correlation.clone(),
                Provenance::new(1),
            )
            .unwrap(),
        ];
        deterministic_fan_in(&mut rs);
        assert_eq!(rs[0].correlation.parent_sequence, 1);
    }

    #[test]
    fn transport_retries_and_revision_limit_are_bounded() {
        let mut coordinator = Coordinator::new();
        let request = request();
        coordinator.create(&request, 0).unwrap();
        assert!(coordinator.retry_transport("child").unwrap());
        assert!(coordinator.retry_transport("child").unwrap());
        assert!(coordinator.retry_transport("child").unwrap());
        assert!(!coordinator.retry_transport("child").unwrap());
        assert_eq!(coordinator.checkpoint("child").unwrap().state, CoordinatorState::Failed);

        let mut coordinator = Coordinator::new();
        coordinator.create(&request, 0).unwrap();
        coordinator.transition("child", CoordinatorState::Queued, None).unwrap();
        coordinator.transition("child", CoordinatorState::Running, None).unwrap();
        coordinator.transition("child", CoordinatorState::Validating, None).unwrap();
        coordinator.transition("child", CoordinatorState::WaitingParentAcceptance, None).unwrap();
        coordinator.transition("child", CoordinatorState::RevisePlan, None).unwrap();
        assert_eq!(coordinator.begin_revision("child", 2, 10).unwrap().revision, 1);
    }

    #[test]
    fn tester_acceptance_preserves_partial_semantics() {
        assert_eq!(evaluate_test_acceptance(true, false, true), AcceptanceDecision::RevisePlan);
        assert_eq!(evaluate_test_acceptance(false, true, false), AcceptanceDecision::WaitingParentAcceptance);
        assert_eq!(evaluate_test_acceptance(false, true, true), AcceptanceDecision::Accepted);
    }
}

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
        }
    }
    pub fn heartbeat(&mut self, now_ms: u64, duration_ms: u64) -> bool {
        if !self.is_live(now_ms) {
            return false;
        }
        self.last_heartbeat_wall_ms = now_ms;
        self.deadline_wall_ms = now_ms.saturating_add(duration_ms);
        true
    }
    pub fn is_live(&self, now_ms: u64) -> bool {
        self.process_alive && now_ms <= self.deadline_wall_ms
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

pub fn correlation_parent_chain(correlation: &CorrelationContext) -> Vec<String> {
    vec![correlation.task_id.as_str().to_owned()]
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
}

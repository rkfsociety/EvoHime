//! Durable, parent-scoped retained child context contract (plan 27).
//!
//! This module deliberately contains no transcript or runtime authority. It
//! validates the bounded metadata/mailbox envelope that Core may persist.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_ROLE_BYTES: usize = 64;
pub const MAX_NAME_BYTES: usize = 128;
pub const MAX_INSTRUCTION_BYTES: usize = 32 * 1024;
pub const MAX_REF_BYTES: usize = 512;
pub const MAX_REFS: usize = 32;
pub const MAX_INLINE_PAYLOAD_BYTES: usize = 32 * 1024;
pub const MAX_GRANTS: usize = 16;
pub const MAX_BUDGET_BYTES: usize = 1024;
pub const MAX_PENDING_PER_CHILD: usize = 32;
pub const MAX_FOLLOW_UPS_PER_CHILD: usize = 64;
pub const MAX_RETAINED_PER_PARENT: usize = 16;
pub const DEFAULT_TTL_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedLifecycle {
    Active,
    IdleRetained,
    QueuedFollowUp,
    RunningFollowUp,
    Expired,
    Deleted,
    Invalidated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FollowUpMode {
    FollowUp,
    Steer,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Pending,
    Dispatched,
    Delivered,
    Rejected,
    Blocked,
    Unknown,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainedChildV1 {
    pub version: u32,
    pub child_id: String,
    pub parent_id: String,
    pub family_root_id: String,
    pub role: String,
    pub stable_name: Option<String>,
    pub lifecycle: RetainedLifecycle,
    pub revision: u64,
    pub active_session_id: Option<String>,
    pub grant_snapshot_hash: String,
    pub context_scope_hash: String,
    pub workspace_state_ref: Option<String>,
    pub last_report_ref: Option<String>,
    pub retained_until_ms: u64,
    pub created_at_ms: u64,
    pub last_active_at_ms: u64,
    pub registry_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildFollowUpRequestV1 {
    pub version: u32,
    pub idempotency_key: String,
    pub parent_id: String,
    pub child_id: String,
    pub family_root_id: String,
    pub parent_sequence: u64,
    pub expected_child_revision: u64,
    pub instruction: String,
    pub context_refs: Vec<String>,
    pub requested_grants: Vec<String>,
    pub budget_json: String,
    pub mode: FollowUpMode,
    pub correlation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxEntryV1 {
    pub version: u32,
    pub message_id: String,
    pub sender_id: String,
    pub receiver_id: String,
    pub family_root_id: String,
    pub mode: FollowUpMode,
    pub kind: String,
    pub correlation_id: String,
    pub parent_sequence: u64,
    pub payload_ref: Option<String>,
    pub inline_payload: Option<Vec<u8>>,
    pub sensitivity: String,
    pub delivery: DeliveryState,
    pub delivered_at_ms: Option<u64>,
    pub idempotency_key: String,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetainedError {
    UnsupportedVersion,
    InvalidScope,
    StaleRevision,
    InvalidatedContext,
    LimitExceeded(&'static str),
    Duplicate,
    UnknownDelivery,
    InvalidTransition,
}
impl fmt::Display for RetainedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion => write!(f, "unsupported_version"),
            Self::InvalidScope => write!(f, "invalid_scope"),
            Self::StaleRevision => write!(f, "stale_revision"),
            Self::InvalidatedContext => write!(f, "invalidated_context"),
            Self::LimitExceeded(x) => write!(f, "limit_exceeded:{x}"),
            Self::Duplicate => write!(f, "duplicate"),
            Self::UnknownDelivery => write!(f, "unknown_delivery"),
            Self::InvalidTransition => write!(f, "invalid_transition"),
        }
    }
}
impl std::error::Error for RetainedError {}

fn bounded(
    name: &'static str,
    value: &str,
    max: usize,
    required: bool,
) -> Result<(), RetainedError> {
    if required && value.trim().is_empty() {
        return Err(RetainedError::InvalidScope);
    }
    if value.len() > max || value.chars().any(|c| c.is_control()) {
        return Err(RetainedError::LimitExceeded(name));
    }
    Ok(())
}
fn ids<T: AsRef<str>>(name: &'static str, values: &[T]) -> Result<(), RetainedError> {
    for value in values {
        bounded(name, value.as_ref(), MAX_ID_BYTES, true)?;
    }
    Ok(())
}

impl RetainedChildV1 {
    pub fn validate(&self, now_ms: u64) -> Result<(), RetainedError> {
        if self.version != CONTRACT_VERSION {
            return Err(RetainedError::UnsupportedVersion);
        }
        ids(
            "id",
            &[&self.child_id, &self.parent_id, &self.family_root_id],
        )?;
        bounded("role", &self.role, MAX_ROLE_BYTES, true)?;
        if let Some(x) = &self.stable_name {
            bounded("stable_name", x, MAX_NAME_BYTES, false)?;
        }
        if self.retained_until_ms < now_ms {
            return Err(RetainedError::InvalidatedContext);
        }
        Ok(())
    }
}
impl ChildFollowUpRequestV1 {
    pub fn validate(&self) -> Result<(), RetainedError> {
        if self.version != CONTRACT_VERSION {
            return Err(RetainedError::UnsupportedVersion);
        }
        ids(
            "id",
            &[
                &self.idempotency_key,
                &self.parent_id,
                &self.child_id,
                &self.family_root_id,
                &self.correlation_id,
            ],
        )?;
        bounded(
            "instruction",
            &self.instruction,
            MAX_INSTRUCTION_BYTES,
            true,
        )?;
        if self.context_refs.len() > MAX_REFS {
            return Err(RetainedError::LimitExceeded("context_refs"));
        }
        for x in &self.context_refs {
            bounded("context_ref", x, MAX_REF_BYTES, true)?;
        }
        if self.requested_grants.len() > MAX_GRANTS {
            return Err(RetainedError::LimitExceeded("requested_grants"));
        }
        if self.budget_json.len() > MAX_BUDGET_BYTES {
            return Err(RetainedError::LimitExceeded("budget"));
        }
        Ok(())
    }
}
impl MailboxEntryV1 {
    pub fn validate(&self) -> Result<(), RetainedError> {
        if self.version != CONTRACT_VERSION {
            return Err(RetainedError::UnsupportedVersion);
        }
        ids(
            "id",
            &[
                &self.message_id,
                &self.sender_id,
                &self.receiver_id,
                &self.family_root_id,
                &self.correlation_id,
                &self.idempotency_key,
            ],
        )?;
        bounded("kind", &self.kind, 64, true)?;
        bounded("sensitivity", &self.sensitivity, 32, true)?;
        if let Some(p) = &self.inline_payload {
            if p.len() > MAX_INLINE_PAYLOAD_BYTES {
                return Err(RetainedError::LimitExceeded("inline_payload"));
            }
            if self.sensitivity != "public" {
                return Err(RetainedError::InvalidScope);
            }
        }
        if self
            .payload_ref
            .as_ref()
            .is_some_and(|x| x.len() > MAX_REF_BYTES)
        {
            return Err(RetainedError::LimitExceeded("payload_ref"));
        }
        Ok(())
    }
}

pub fn canonical_hash<T: Serialize>(value: &T) -> Result<String, RetainedError> {
    let json = serde_json::to_value(value).map_err(|_| RetainedError::InvalidScope)?;
    let bytes = serde_json::to_vec(&json).map_err(|_| RetainedError::InvalidScope)?;
    let mut h = Sha256::new();
    h.update(bytes);
    Ok(hex::encode(h.finalize()))
}
pub fn can_transition(from: RetainedLifecycle, to: RetainedLifecycle) -> bool {
    matches!(
        (from, to),
        (RetainedLifecycle::Active, RetainedLifecycle::IdleRetained)
            | (
                RetainedLifecycle::IdleRetained,
                RetainedLifecycle::QueuedFollowUp
            )
            | (
                RetainedLifecycle::QueuedFollowUp,
                RetainedLifecycle::RunningFollowUp
            )
            | (
                RetainedLifecycle::RunningFollowUp,
                RetainedLifecycle::IdleRetained
            )
            | (_, RetainedLifecycle::Expired)
            | (_, RetainedLifecycle::Deleted)
            | (_, RetainedLifecycle::Invalidated)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowUpOutcome {
    Dispatched,
    Queued,
    Duplicate,
    Stale,
    Expired,
    Rejected,
}

/// Small Core-owned coordinator for the retained lifecycle. Persistence is
/// supplied by `evohime_local_storage::retained_child_store`; this type only
/// applies transitions and never stores a transcript.
#[derive(Debug, Default)]
pub struct RetainedRegistry {
    children: std::collections::BTreeMap<(String, String), RetainedChildV1>,
    follow_ups: std::collections::BTreeSet<String>,
}
impl RetainedRegistry {
    pub fn retain(
        &mut self,
        mut child: RetainedChildV1,
        now_ms: u64,
    ) -> Result<bool, RetainedError> {
        child.validate(now_ms)?;
        let key = (child.parent_id.clone(), child.child_id.clone());
        if let Some(old) = self.children.get(&key) {
            if old.registry_version >= child.registry_version {
                return Ok(false);
            }
        }
        child.lifecycle = RetainedLifecycle::IdleRetained;
        self.children.insert(key, child);
        Ok(true)
    }
    pub fn get(
        &self,
        parent_id: &str,
        child_id: &str,
        now_ms: u64,
    ) -> Result<&RetainedChildV1, RetainedError> {
        let child = self
            .children
            .get(&(parent_id.into(), child_id.into()))
            .ok_or(RetainedError::InvalidScope)?;
        if child.retained_until_ms < now_ms {
            return Err(RetainedError::InvalidatedContext);
        }
        Ok(child)
    }
    pub fn follow_up(
        &mut self,
        request: &ChildFollowUpRequestV1,
        now_ms: u64,
        busy: bool,
    ) -> Result<FollowUpOutcome, RetainedError> {
        request.validate()?;
        if !self.follow_ups.insert(request.idempotency_key.clone()) {
            return Ok(FollowUpOutcome::Duplicate);
        }
        let child = self
            .children
            .get_mut(&(request.parent_id.clone(), request.child_id.clone()))
            .ok_or(RetainedError::InvalidScope)?;
        if child.retained_until_ms < now_ms {
            child.lifecycle = RetainedLifecycle::Expired;
            return Ok(FollowUpOutcome::Expired);
        }
        if child.revision != request.expected_child_revision {
            return Ok(FollowUpOutcome::Stale);
        }
        match child.lifecycle {
            RetainedLifecycle::IdleRetained if !busy => {
                child.lifecycle = RetainedLifecycle::RunningFollowUp;
                child.revision += 1;
                Ok(FollowUpOutcome::Dispatched)
            }
            RetainedLifecycle::IdleRetained | RetainedLifecycle::RunningFollowUp
                if request.mode == FollowUpMode::Auto || busy =>
            {
                child.lifecycle = RetainedLifecycle::QueuedFollowUp;
                Ok(FollowUpOutcome::Queued)
            }
            RetainedLifecycle::Deleted
            | RetainedLifecycle::Expired
            | RetainedLifecycle::Invalidated => Ok(FollowUpOutcome::Rejected),
            _ => Ok(FollowUpOutcome::Rejected),
        }
    }
    pub fn delete(&mut self, parent_id: &str, child_id: &str) -> Result<(), RetainedError> {
        let child = self
            .children
            .get_mut(&(parent_id.into(), child_id.into()))
            .ok_or(RetainedError::InvalidScope)?;
        child.lifecycle = RetainedLifecycle::Deleted;
        child.registry_version += 1;
        Ok(())
    }
    pub fn list(
        &self,
        parent_id: &str,
        now_ms: u64,
    ) -> Result<Vec<RetainedChildV1>, RetainedError> {
        Ok(self
            .children
            .values()
            .filter(|c| {
                c.parent_id == parent_id
                    && c.retained_until_ms >= now_ms
                    && c.lifecycle != RetainedLifecycle::Deleted
            })
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hash_is_stable_and_payload_is_bounded() {
        let x = MailboxEntryV1 {
            version: 1,
            message_id: "m".into(),
            sender_id: "p".into(),
            receiver_id: "c".into(),
            family_root_id: "f".into(),
            mode: FollowUpMode::FollowUp,
            kind: "follow_up".into(),
            correlation_id: "x".into(),
            parent_sequence: 1,
            payload_ref: None,
            inline_payload: Some(b"ok".to_vec()),
            sensitivity: "public".into(),
            delivery: DeliveryState::Pending,
            delivered_at_ms: None,
            idempotency_key: "i".into(),
            created_at_ms: 1,
        };
        assert!(x.validate().is_ok());
        assert_eq!(canonical_hash(&x).unwrap(), canonical_hash(&x).unwrap());
    }
    #[test]
    fn terminal_transitions_are_one_way() {
        assert!(can_transition(
            RetainedLifecycle::IdleRetained,
            RetainedLifecycle::QueuedFollowUp
        ));
        assert!(!can_transition(
            RetainedLifecycle::Deleted,
            RetainedLifecycle::IdleRetained
        ));
    }
}

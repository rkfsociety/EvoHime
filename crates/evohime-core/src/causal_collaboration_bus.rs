//! Core-owned typed peer messaging inside a pinned Team SOP session.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_PAYLOAD_BYTES: usize = 32 * 1024;
pub const MAX_ENVELOPE_BYTES: usize = 64 * 1024;
pub const MAX_INBOX_PER_SESSION: u32 = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Address {
    RoleSlot { slot_id: String },
    DirectRoleInstance { slot_id: String, ordinal: u32 },
    ProtocolGroup,
    Parent,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Progress,
    Notice,
    ArtifactRef,
    Request,
    Response,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Accepted,
    Queued,
    Delivered,
    Consumed,
    Expired,
    Rejected,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    Internal,
    Secret,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationMessage {
    pub schema_version: u32,
    pub message_id: String,
    pub session_id: String,
    pub protocol_hash: String,
    pub sender: Address,
    pub receiver: Address,
    pub kind: MessageKind,
    pub correlation_id: String,
    pub causation_id: String,
    pub sequence: u64,
    pub idempotency_key: String,
    pub payload: Vec<u8>,
    pub payload_hash: String,
    pub sensitivity: Sensitivity,
    pub provenance_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusError {
    Invalid(&'static str),
    UnsupportedVersion,
    TooLarge,
    ForbiddenRoute,
    InboxFull,
    Duplicate,
    Stale,
    UnknownTerminal,
}
impl std::fmt::Display for BusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid(v) => v,
            Self::UnsupportedVersion => "unsupported_version",
            Self::TooLarge => "too_large",
            Self::ForbiddenRoute => "destination_forbidden",
            Self::InboxFull => "inbox_full",
            Self::Duplicate => "duplicate",
            Self::Stale => "stale",
            Self::UnknownTerminal => "unknown",
        })
    }
}
impl std::error::Error for BusError {}
fn id(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 128
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_' || b == b'/')
}
fn hash(v: &str) -> bool {
    v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit())
}
fn address(a: &Address) -> bool {
    match a {
        Address::RoleSlot { slot_id } => id(slot_id),
        Address::DirectRoleInstance { slot_id, ordinal } => id(slot_id) && *ordinal < 32,
        Address::ProtocolGroup | Address::Parent => true,
    }
}
pub fn canonical_hash(message: &CollaborationMessage) -> Result<String, BusError> {
    let mut copy = message.clone();
    copy.sequence = 0;
    copy.payload_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| BusError::Invalid("serialization"))?;
    if bytes.len() > MAX_ENVELOPE_BYTES {
        return Err(BusError::TooLarge);
    }
    Ok(hex::encode(Sha256::digest(bytes)))
}
pub fn validate(message: &CollaborationMessage) -> Result<(), BusError> {
    if message.schema_version != CONTRACT_VERSION {
        return Err(BusError::UnsupportedVersion);
    }
    if !id(&message.message_id)
        || !id(&message.session_id)
        || !hash(&message.protocol_hash)
        || !address(&message.sender)
        || !address(&message.receiver)
        || !id(&message.correlation_id)
        || !id(&message.causation_id)
        || !id(&message.idempotency_key)
        || !id(&message.provenance_id)
    {
        return Err(BusError::Invalid("identity"));
    }
    if message.payload.len() > MAX_PAYLOAD_BYTES || message.sensitivity == Sensitivity::Secret {
        return Err(BusError::TooLarge);
    }
    let actual = hex::encode(Sha256::digest(&message.payload));
    if message.payload_hash != actual {
        return Err(BusError::Invalid("payload_hash"));
    }
    if matches!(message.kind, MessageKind::ArtifactRef) && message.payload.is_empty() {
        return Err(BusError::Invalid("artifact_ref"));
    }
    Ok(())
}
pub fn route_allowed(
    message: &CollaborationMessage,
    allowed_routes: &BTreeSet<String>,
) -> Result<(), BusError> {
    validate(message)?;
    let route = match &message.receiver {
        Address::RoleSlot { slot_id } => slot_id.as_str(),
        Address::DirectRoleInstance { slot_id, .. } => slot_id.as_str(),
        Address::ProtocolGroup => "protocol_group",
        Address::Parent => "parent",
    };
    if !allowed_routes.contains(route) {
        return Err(BusError::ForbiddenRoute);
    }
    Ok(())
}

pub fn route_with_intervention(
    message: &CollaborationMessage,
    allowed_routes: &BTreeSet<String>,
    policy: &crate::message_intervention_policies::MessageInterventionPolicy,
    seen: bool,
) -> Result<crate::message_intervention_policies::InterventionVerdict, BusError> {
    route_allowed(message, allowed_routes)?;
    let context = crate::message_intervention_policies::MessageInterventionContext {
        team_session_id: message.session_id.clone(),
        sender: message.provenance_id.clone(),
        recipients: vec![match &message.receiver {
            Address::RoleSlot { slot_id } | Address::DirectRoleInstance { slot_id, .. } => slot_id.clone(),
            Address::ProtocolGroup => "protocol_group".into(),
            Address::Parent => "parent".into(),
        }],
        message_kind: serde_json::to_string(&message.kind).map_err(|_| BusError::Invalid("kind"))?.trim_matches('"').to_owned(),
        contract_ref: None,
        payload_metadata: format!("bytes={}", message.payload.len()),
        sensitivity: match message.sensitivity { Sensitivity::Public => crate::message_intervention_policies::SensitivityClass::Public, Sensitivity::Internal => crate::message_intervention_policies::SensitivityClass::Internal, Sensitivity::Secret => crate::message_intervention_policies::SensitivityClass::Secret },
        phase: crate::message_intervention_policies::HookPhase::BeforeDelivery,
        causation_id: Some(message.causation_id.clone()),
        routing_snapshot_hash: message.protocol_hash.clone(),
        idempotency_key: message.idempotency_key.clone(),
    };
    crate::message_intervention_policies::evaluate(policy, &context, seen).map_err(|error| match error { crate::message_intervention_policies::InterventionError::Duplicate => BusError::Duplicate, _ => BusError::ForbiddenRoute })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn m() -> CollaborationMessage {
        let p = b"ok".to_vec();
        CollaborationMessage {
            schema_version: 1,
            message_id: "m-1".into(),
            session_id: "s-1".into(),
            protocol_hash: "a".repeat(64),
            sender: Address::RoleSlot {
                slot_id: "a".into(),
            },
            receiver: Address::RoleSlot {
                slot_id: "b".into(),
            },
            kind: MessageKind::Notice,
            correlation_id: "c".into(),
            causation_id: "x".into(),
            sequence: 0,
            idempotency_key: "k".into(),
            payload: p.clone(),
            payload_hash: hex::encode(Sha256::digest(p)),
            sensitivity: Sensitivity::Internal,
            provenance_id: "p".into(),
        }
    }
    #[test]
    fn validates_and_hashes() {
        let x = m();
        validate(&x).unwrap();
        assert_eq!(canonical_hash(&x).unwrap().len(), 64)
    }
    #[test]
    fn rejects_secret_and_route() {
        let mut x = m();
        x.sensitivity = Sensitivity::Secret;
        assert_eq!(validate(&x), Err(BusError::TooLarge));
        let x = m();
        assert_eq!(
            route_allowed(&x, &BTreeSet::new()),
            Err(BusError::ForbiddenRoute)
        );
    }
    #[test]
    fn rejects_oversize() {
        let mut x = m();
        x.payload = vec![0; MAX_PAYLOAD_BYTES + 1];
        assert_eq!(validate(&x), Err(BusError::TooLarge));
    }

    #[test]
    fn intervention_runs_before_delivery_and_can_redact_projection() {
        let mut policy = crate::message_intervention_policies::MessageInterventionPolicy {
            schema_version: 1,
            id: "p".into(),
            version: 1,
            hooks: vec![crate::message_intervention_policies::MessageInterventionHook {
                id: "h".into(),
                version: 1,
                priority: 1,
                phases: vec![crate::message_intervention_policies::HookPhase::BeforeDelivery],
                action: crate::message_intervention_policies::InterventionAction::Redact,
                failure_mode: crate::message_intervention_policies::FailureMode::FailClosed,
                allowed_routes: vec!["b".into()],
                allowed_sensitivity: vec![crate::message_intervention_policies::SensitivityClass::Internal],
                message_kinds: vec!["notice".into()],
            }],
            content_hash: String::new(),
        };
        policy.content_hash = crate::message_intervention_policies::canonical_hash(&policy).unwrap();
        let mut routes = BTreeSet::new();
        routes.insert("b".into());
        let verdict = route_with_intervention(&m(), &routes, &policy, false).unwrap();
        assert_eq!(verdict.action, crate::message_intervention_policies::InterventionAction::Redact);
    }
}

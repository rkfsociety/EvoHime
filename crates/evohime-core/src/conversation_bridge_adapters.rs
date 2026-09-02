//! Core-owned, provider-neutral conversation bridge contracts.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ID: usize = 128;
pub const MAX_TEXT: usize = 4096;
pub const MAX_QUEUE: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeState {
    Paired,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteCommandKind {
    Attention,
    ApprovalReply,
    HumanWorkItemReply,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationBridge {
    pub schema_version: u32,
    pub bridge_id: String,
    pub provider: String,
    pub conversation_id: String,
    pub principal_id: String,
    pub pairing_hash: String,
    pub state: BridgeState,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadBinding {
    pub schema_version: u32,
    pub binding_id: String,
    pub bridge_id: String,
    pub external_thread_id: String,
    pub conversation_id: String,
    pub principal_id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboundMessage {
    pub schema_version: u32,
    pub message_id: String,
    pub binding_id: String,
    pub principal_id: String,
    pub text: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteCommand {
    pub schema_version: u32,
    pub command_id: String,
    pub binding_id: String,
    pub principal_id: String,
    pub kind: RemoteCommandKind,
    pub target_id: String,
    pub accepted_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundProjection {
    pub schema_version: u32,
    pub binding_id: String,
    pub conversation_id: String,
    pub kind: String,
    pub status: String,
    pub provenance_id: String,
    pub redacted: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BridgeError {
    #[error("unsupported bridge schema")]
    UnsupportedVersion,
    #[error("invalid bridge contract")]
    Invalid,
    #[error("bridge provider is not allowlisted")]
    ProviderDenied,
    #[error("bridge principal is not paired")]
    PrincipalDenied,
    #[error("bridge revision is stale")]
    StaleRevision,
    #[error("remote command is not allowlisted")]
    CommandDenied,
    #[error("bridge bound exceeded")]
    Bounds,
}

const PROVIDERS: [&str; 4] = ["telegram", "slack", "google_chat", "generic"];
const COMMANDS: [RemoteCommandKind; 3] = [
    RemoteCommandKind::Attention,
    RemoteCommandKind::ApprovalReply,
    RemoteCommandKind::HumanWorkItemReply,
];

fn bounded(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.bytes().any(|b| b.is_ascii_control())
}

pub fn validate_bridge(value: &ConversationBridge) -> Result<(), BridgeError> {
    if value.schema_version != SCHEMA_VERSION {
        return Err(BridgeError::UnsupportedVersion);
    }
    if !bounded(&value.bridge_id, MAX_ID)
        || !bounded(&value.provider, 32)
        || !bounded(&value.conversation_id, MAX_ID)
        || !bounded(&value.principal_id, MAX_ID)
        || !valid_pairing_hash(&value.pairing_hash)
        || value.revision == 0
    {
        return Err(BridgeError::Invalid);
    }
    if !PROVIDERS.contains(&value.provider.as_str()) {
        return Err(BridgeError::ProviderDenied);
    }
    Ok(())
}

fn valid_pairing_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn validate_binding(value: &ThreadBinding) -> Result<(), BridgeError> {
    if value.schema_version != SCHEMA_VERSION
        || !bounded(&value.binding_id, MAX_ID)
        || !bounded(&value.bridge_id, MAX_ID)
        || !bounded(&value.external_thread_id, MAX_ID)
        || !bounded(&value.conversation_id, MAX_ID)
        || !bounded(&value.principal_id, MAX_ID)
        || value.revision == 0
    {
        return Err(BridgeError::Invalid);
    }
    Ok(())
}

pub fn validate_inbound(value: &InboundMessage) -> Result<(), BridgeError> {
    if value.schema_version != SCHEMA_VERSION
        || !bounded(&value.message_id, MAX_ID)
        || !bounded(&value.binding_id, MAX_ID)
        || !bounded(&value.principal_id, MAX_ID)
        || !bounded(&value.text, MAX_TEXT)
        || value.created_at_ms < 0
    {
        return Err(BridgeError::Invalid);
    }
    Ok(())
}

pub fn validate_remote_command(value: &RemoteCommand) -> Result<(), BridgeError> {
    if value.schema_version != SCHEMA_VERSION
        || !bounded(&value.command_id, MAX_ID)
        || !bounded(&value.binding_id, MAX_ID)
        || !bounded(&value.principal_id, MAX_ID)
        || !bounded(&value.target_id, MAX_ID)
        || !bounded(&value.accepted_value, MAX_TEXT)
    {
        return Err(BridgeError::Invalid);
    }
    if !COMMANDS.contains(&value.kind) {
        return Err(BridgeError::CommandDenied);
    }
    Ok(())
}

pub fn authorize_principal(
    bridge: &ConversationBridge,
    principal_id: &str,
    expected_revision: u64,
) -> Result<(), BridgeError> {
    validate_bridge(bridge)?;
    if bridge.state != BridgeState::Paired || bridge.principal_id != principal_id {
        return Err(BridgeError::PrincipalDenied);
    }
    if expected_revision != 0 && expected_revision != bridge.revision {
        return Err(BridgeError::StaleRevision);
    }
    Ok(())
}

pub fn redacted_projection(
    binding: &ThreadBinding,
    kind: &str,
    status: &str,
    provenance_id: &str,
) -> Result<OutboundProjection, BridgeError> {
    validate_binding(binding)?;
    if !bounded(kind, 64) || !bounded(status, 64) || !bounded(provenance_id, MAX_ID) {
        return Err(BridgeError::Invalid);
    }
    Ok(OutboundProjection {
        schema_version: SCHEMA_VERSION,
        binding_id: binding.binding_id.clone(),
        conversation_id: binding.conversation_id.clone(),
        kind: kind.to_owned(),
        status: status.to_owned(),
        provenance_id: provenance_id.to_owned(),
        redacted: true,
    })
}

pub fn canonical_metadata(value: &ConversationBridge) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("schema_version".into(), value.schema_version.to_string()),
        ("bridge_id".into(), value.bridge_id.clone()),
        ("provider".into(), value.provider.clone()),
        ("conversation_id".into(), value.conversation_id.clone()),
        ("principal_id".into(), value.principal_id.clone()),
        ("revision".into(), value.revision.to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bridge() -> ConversationBridge {
        ConversationBridge {
            schema_version: 1,
            bridge_id: "b".into(),
            provider: "telegram".into(),
            conversation_id: "c".into(),
            principal_id: "p".into(),
            pairing_hash: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
            state: BridgeState::Paired,
            revision: 1,
        }
    }

    #[test]
    fn pairing_and_revision_are_required() {
        assert!(authorize_principal(&bridge(), "p", 1).is_ok());
        assert_eq!(
            authorize_principal(&bridge(), "other", 1),
            Err(BridgeError::PrincipalDenied)
        );
        assert_eq!(
            authorize_principal(&bridge(), "p", 2),
            Err(BridgeError::StaleRevision)
        );
    }

    #[test]
    fn unknown_provider_and_command_fail_closed() {
        let mut value = bridge();
        value.provider = "webhook".into();
        assert_eq!(validate_bridge(&value), Err(BridgeError::ProviderDenied));
        let command = RemoteCommand {
            schema_version: 1,
            command_id: "cmd".into(),
            binding_id: "bind".into(),
            principal_id: "p".into(),
            kind: RemoteCommandKind::Attention,
            target_id: "target".into(),
            accepted_value: "yes".into(),
        };
        assert!(validate_remote_command(&command).is_ok());
    }

    #[test]
    fn projection_never_contains_message_text() {
        let binding = ThreadBinding {
            schema_version: 1,
            binding_id: "bind".into(),
            bridge_id: "b".into(),
            external_thread_id: "thread".into(),
            conversation_id: "c".into(),
            principal_id: "p".into(),
            revision: 1,
        };
        let projection = redacted_projection(&binding, "attention", "pending", "event-1").unwrap();
        assert!(projection.redacted);
        assert!(!serde_json::to_string(&projection)
            .unwrap()
            .contains("message"));
    }
}

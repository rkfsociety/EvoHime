//! Core-owned contract for authenticated, bounded remote conversations.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_CHANNELS: usize = 8;
pub const MAX_QUEUE: usize = 128;
pub const MAX_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024;
pub const PAIRING_TTL_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Telegram,
    Slack,
    Discord,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Pairing,
    Active,
    Revoked,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelConnection {
    pub schema_version: u32,
    pub connection_id: String,
    pub owner_scope: String,
    pub provider: Provider,
    pub external_identity: String,
    pub state: ConnectionState,
    pub revision: u64,
    pub queue_limit: usize,
    pub attachment_limit_bytes: usize,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairingCode {
    pub connection_id: String,
    pub code_hash: String,
    pub expires_at_ms: i64,
    pub consumed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboundMessage {
    pub message_id: String,
    pub connection_id: String,
    pub external_identity: String,
    pub text: String,
    pub attachment_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboundReply {
    pub correlation_id: String,
    pub chunks: Vec<String>,
    pub final_reply: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelProviderContract {
    pub schema_version: u32,
    pub provider: Provider,
    pub adapter_id: String,
    pub credential_ref: String,
    pub supports_streaming: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteApprovalClass {
    LowRisk,
    HighRisk,
}

pub fn validate_provider_contract(c: &ChannelProviderContract) -> Result<(), ChannelError> {
    if c.schema_version != SCHEMA_VERSION
        || !valid_text(&c.adapter_id, 128)
        || !valid_text(&c.credential_ref, 256)
    {
        return Err(ChannelError::Invalid("provider_contract"));
    }
    Ok(())
}
pub fn authorize_remote_approval(
    class: RemoteApprovalClass,
    desktop_granted: bool,
) -> Result<(), ChannelError> {
    if matches!(class, RemoteApprovalClass::HighRisk) && !desktop_granted {
        return Err(ChannelError::Revoked);
    }
    Ok(())
}
pub fn validate_outbound_reply(reply: &OutboundReply) -> Result<(), ChannelError> {
    if !valid_text(&reply.correlation_id, 256)
        || reply.chunks.len() > 128
        || reply.chunks.iter().any(|chunk| chunk.len() > 16 * 1024)
        || !reply.final_reply
    {
        return Err(ChannelError::Invalid("outbound_reply"));
    }
    Ok(())
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChannelError {
    #[error("invalid channel contract: {0}")]
    Invalid(&'static str),
    #[error("unsupported channel schema")]
    UnsupportedVersion,
    #[error("pairing is invalid, expired or already consumed")]
    PairingInvalid,
    #[error("external identity is not bound to owner")]
    IdentityMismatch,
    #[error("connection is revoked or expired")]
    Revoked,
    #[error("queue or attachment limit exceeded")]
    LimitExceeded,
}

fn valid_text(v: &str, max: usize) -> bool {
    !v.is_empty() && v.len() <= max && !v.contains('\0')
}
pub fn hash_pairing_code(code: &str) -> Result<String, ChannelError> {
    if !valid_text(code, 128) {
        return Err(ChannelError::PairingInvalid);
    }
    Ok(hex::encode(Sha256::digest(code.as_bytes())))
}
pub fn validate_connection(c: &ChannelConnection) -> Result<(), ChannelError> {
    if c.schema_version != SCHEMA_VERSION {
        return Err(ChannelError::UnsupportedVersion);
    }
    if !valid_text(&c.connection_id, 128)
        || !valid_text(&c.owner_scope, 256)
        || !valid_text(&c.external_identity, 512)
        || c.revision == 0
        || c.queue_limit == 0
        || c.queue_limit > MAX_QUEUE
        || c.attachment_limit_bytes > MAX_ATTACHMENT_BYTES
    {
        return Err(ChannelError::Invalid("identity_or_limits"));
    }
    Ok(())
}
pub fn canonical_hash(c: &ChannelConnection) -> Result<String, ChannelError> {
    validate_connection(c)?;
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(c).map_err(|_| ChannelError::Invalid("serialization"))?,
    )))
}
pub fn consume_pairing(
    connection: &ChannelConnection,
    pairing: &mut PairingCode,
    code: &str,
    now_ms: i64,
    external_identity: &str,
) -> Result<(), ChannelError> {
    validate_connection(connection)?;
    if pairing.connection_id != connection.connection_id
        || pairing.consumed
        || pairing.expires_at_ms < now_ms
        || hash_pairing_code(code)? != pairing.code_hash
    {
        return Err(ChannelError::PairingInvalid);
    }
    if external_identity != connection.external_identity {
        return Err(ChannelError::IdentityMismatch);
    }
    pairing.consumed = true;
    Ok(())
}
pub fn admit_message(
    connection: &ChannelConnection,
    message: &InboundMessage,
    queued: usize,
    deduplicated: bool,
    now_ms: i64,
) -> Result<(), ChannelError> {
    validate_connection(connection)?;
    if connection.state != ConnectionState::Active || connection.expires_at_ms <= now_ms {
        return Err(ChannelError::Revoked);
    }
    if message.connection_id != connection.connection_id
        || message.external_identity != connection.external_identity
    {
        return Err(ChannelError::IdentityMismatch);
    }
    if !valid_text(&message.message_id, 256)
        || message.text.len() > 64 * 1024
        || message.attachment_bytes > connection.attachment_limit_bytes
        || queued >= connection.queue_limit
        || deduplicated
    {
        return Err(ChannelError::LimitExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c() -> ChannelConnection {
        ChannelConnection {
            schema_version: 1,
            connection_id: "c".into(),
            owner_scope: "owner".into(),
            provider: Provider::Telegram,
            external_identity: "user".into(),
            state: ConnectionState::Active,
            revision: 1,
            queue_limit: 4,
            attachment_limit_bytes: 1024,
            expires_at_ms: 1000,
        }
    }
    #[test]
    fn pairing_is_single_use_and_identity_bound() {
        let connection = c();
        let mut p = PairingCode {
            connection_id: "c".into(),
            code_hash: hash_pairing_code("secret").unwrap(),
            expires_at_ms: 100,
            consumed: false,
        };
        assert!(consume_pairing(&connection, &mut p, "secret", 1, "user").is_ok());
        assert_eq!(
            consume_pairing(&connection, &mut p, "secret", 1, "user"),
            Err(ChannelError::PairingInvalid)
        );
    }
    #[test]
    fn revoked_duplicate_and_limits_fail_closed() {
        let connection = c();
        let msg = InboundMessage {
            message_id: "m".into(),
            connection_id: "c".into(),
            external_identity: "user".into(),
            text: "hi".into(),
            attachment_bytes: 0,
        };
        assert!(admit_message(&connection, &msg, 0, false, 1).is_ok());
        assert_eq!(
            admit_message(&connection, &msg, 0, true, 1),
            Err(ChannelError::LimitExceeded)
        );
        assert_eq!(
            admit_message(
                &ChannelConnection {
                    state: ConnectionState::Revoked,
                    ..connection
                },
                &msg,
                0,
                false,
                1
            ),
            Err(ChannelError::Revoked)
        );
    }
    #[test]
    fn high_risk_remote_approval_requires_desktop() {
        assert!(authorize_remote_approval(RemoteApprovalClass::HighRisk, false).is_err());
        assert!(authorize_remote_approval(RemoteApprovalClass::HighRisk, true).is_ok());
        assert!(validate_provider_contract(&ChannelProviderContract {
            schema_version: 1,
            provider: Provider::Telegram,
            adapter_id: "telegram".into(),
            credential_ref: "dpapi:telegram".into(),
            supports_streaming: true
        })
        .is_ok());
    }
}

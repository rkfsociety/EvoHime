//! Core-owned contract for authenticated, bounded remote conversations.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current schema version for remote conversation channel records.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum number of connected remote channels per owner scope.
pub const MAX_CHANNELS: usize = 8;
/// Maximum queued inbound messages for one channel.
pub const MAX_QUEUE: usize = 128;
/// Maximum attachment size accepted by one channel, in bytes.
pub const MAX_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024;
/// Pairing code lifetime in milliseconds.
pub const PAIRING_TTL_MS: i64 = 5 * 60 * 1000;

/// Remote service adapter type for a conversation channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// Telegram conversation adapter.
    Telegram,
    /// Slack conversation adapter.
    Slack,
    /// Discord conversation adapter.
    Discord,
    /// Adapter implementing the generic channel contract.
    Generic,
}

/// Pairing and connection lifecycle state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// Pairing has started but is not yet confirmed.
    Pairing,
    /// External identity is paired and may exchange messages.
    Active,
    /// Owner revoked the connection.
    Revoked,
    /// Connection passed its expiry deadline.
    Expired,
}

/// Owner-scoped connection bound to one external identity and bounded queue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelConnection {
    /// Connection schema version.
    pub schema_version: u32,
    /// Stable local connection identifier.
    pub connection_id: String,
    /// Owner scope that controls this channel.
    pub owner_scope: String,
    /// Remote service provider.
    pub provider: Provider,
    /// Identity paired on the remote service.
    pub external_identity: String,
    /// Current connection lifecycle state.
    pub state: ConnectionState,
    /// Optimistic-concurrency revision.
    pub revision: u64,
    /// Maximum queued messages admitted at one time.
    pub queue_limit: usize,
    /// Maximum total attachment bytes accepted per message.
    pub attachment_limit_bytes: usize,
    /// Absolute Unix expiry timestamp in milliseconds.
    pub expires_at_ms: i64,
}

/// Single-use pairing verifier stored as a hash rather than plaintext code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairingCode {
    /// Connection the pairing code is allowed to activate.
    pub connection_id: String,
    /// Digest of the pairing code.
    pub code_hash: String,
    /// Absolute expiry timestamp in Unix milliseconds.
    pub expires_at_ms: i64,
    /// Whether the code has already been consumed.
    pub consumed: bool,
}

/// Inbound message metadata admitted from a paired remote identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboundMessage {
    /// Remote message identifier used for deduplication.
    pub message_id: String,
    /// Local channel connection receiving the message.
    pub connection_id: String,
    /// Sender identity asserted by the adapter.
    pub external_identity: String,
    /// Bounded message text.
    pub text: String,
    /// Aggregate attachment size in bytes.
    pub attachment_bytes: usize,
}

/// Correlated outbound response split into provider-sized text chunks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboundReply {
    /// Correlation identifier linking the reply to its inbound request.
    pub correlation_id: String,
    /// Ordered text chunks delivered to the remote provider.
    pub chunks: Vec<String>,
    /// Whether these chunks finish the response.
    pub final_reply: bool,
}

/// Provider adapter identity and credential reference metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelProviderContract {
    /// Provider contract schema version.
    pub schema_version: u32,
    /// Remote provider implemented by the adapter.
    pub provider: Provider,
    /// Stable adapter identifier.
    pub adapter_id: String,
    /// Opaque credential reference resolved by the credential subsystem.
    pub credential_ref: String,
    /// Whether the adapter supports incremental response delivery.
    pub supports_streaming: bool,
}

/// Risk class for approvals requested from a remote conversation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteApprovalClass {
    /// Low-risk action that may follow the configured remote policy.
    LowRisk,
    /// High-risk action that always requires a desktop grant.
    HighRisk,
}

/// Validates adapter identity and credential-reference bounds.
pub fn validate_provider_contract(c: &ChannelProviderContract) -> Result<(), ChannelError> {
    if c.schema_version != SCHEMA_VERSION
        || !valid_text(&c.adapter_id, 128)
        || !valid_text(&c.credential_ref, 256)
    {
        return Err(ChannelError::Invalid("provider_contract"));
    }
    Ok(())
}
/// Requires a desktop grant before accepting any high-risk remote approval.
pub fn authorize_remote_approval(
    class: RemoteApprovalClass,
    desktop_granted: bool,
) -> Result<(), ChannelError> {
    if matches!(class, RemoteApprovalClass::HighRisk) && !desktop_granted {
        return Err(ChannelError::Revoked);
    }
    Ok(())
}
/// Validates correlation, chunk limits, and final-response framing.
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

/// Invalid channel input, pairing, identity, lifecycle, or capacity failure.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChannelError {
    /// A channel or provider contract field is invalid.
    #[error("invalid channel contract: {0}")]
    Invalid(&'static str),
    /// Record uses a schema version that is not supported.
    #[error("unsupported channel schema")]
    UnsupportedVersion,
    /// Pairing code is invalid, expired, or already consumed.
    #[error("pairing is invalid, expired or already consumed")]
    PairingInvalid,
    /// Sender identity does not match the paired external identity.
    #[error("external identity is not bound to owner")]
    IdentityMismatch,
    /// Channel is revoked, expired, or lacks required approval.
    #[error("connection is revoked or expired")]
    Revoked,
    /// Queue or attachment limits have been exceeded.
    #[error("queue or attachment limit exceeded")]
    LimitExceeded,
}

fn valid_text(v: &str, max: usize) -> bool {
    !v.is_empty() && v.len() <= max && !v.contains('\0')
}
/// Hashes a bounded pairing code for safe persistence.
pub fn hash_pairing_code(code: &str) -> Result<String, ChannelError> {
    if !valid_text(code, 128) {
        return Err(ChannelError::PairingInvalid);
    }
    Ok(hex::encode(Sha256::digest(code.as_bytes())))
}
/// Validates connection identity, queue bounds, and attachment limits.
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
/// Validates a connection and computes its canonical serialized digest.
pub fn canonical_hash(c: &ChannelConnection) -> Result<String, ChannelError> {
    validate_connection(c)?;
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(c).map_err(|_| ChannelError::Invalid("serialization"))?,
    )))
}
/// Verifies and consumes a pairing code for the connection's bound identity.
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
/// Checks active state, identity, deduplication, and inbound resource bounds.
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

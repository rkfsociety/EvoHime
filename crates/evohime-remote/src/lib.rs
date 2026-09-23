#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![deny(missing_docs)]
//! Платформенно-независимый контракт удалённого control plane.
//!
//! Crate не открывает sockets и не владеет SQLite. Relay, Android и PC
//! adapters обязаны использовать эти bounded типы, а отсутствующий deployment
//! должен сообщать `Unavailable`, не выдавая себя за online transport.
//!
//! Frames can be validated and hashed locally without opening a connection:
//!
//! ```
//! use evohime_remote::{Frame, Operation, PROTOCOL_VERSION};
//!
//! let frame = Frame {
//!     protocol_version: PROTOCOL_VERSION,
//!     operation: Operation::Ping,
//!     request_id: "request-1".into(),
//!     sequence: 0,
//!     idempotency_key: "ping-1".into(),
//!     device_id: None,
//!     text: None,
//! };
//! assert!(frame.validate().is_ok());
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Version of the serialized remote-control frame contract.
pub const PROTOCOL_VERSION: u16 = 1;
/// Maximum serialized JSON frame size in bytes.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
/// Maximum UTF-8 byte length for a text field.
pub const MAX_TEXT_BYTES: usize = 32 * 1024;
/// Maximum number of devices accepted by a catalog projection.
pub const MAX_DEVICES: usize = 128;

/// Operation encoded by a remote-control frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    /// Pair a client with the control plane.
    Pair,
    /// Register a device after pairing.
    RegisterDevice,
    /// List devices available to the paired client.
    ListDevices,
    /// Start a chat task.
    ChatStart,
    /// Deliver a partial chat response.
    ChatDelta,
    /// Deliver a completed chat response.
    ChatComplete,
    /// Cancel a running task.
    Cancel,
    /// Probe control-plane availability.
    Ping,
}

/// Reported transport availability; it does not itself establish a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    /// A live adapter has confirmed that the device is reachable.
    Online,
    /// The adapter is deployed but the device is disconnected.
    Offline,
    /// No supported adapter/deployment is available.
    Unavailable,
}

/// Bounded device information projected to a remote client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// Stable identifier used by the control plane.
    pub device_id: String,
    /// User-facing device label.
    pub display_name: String,
    /// Last known adapter state.
    pub availability: Availability,
    /// Last-seen UTC Unix timestamp in milliseconds, when known.
    pub last_seen_ms: Option<u64>,
}

/// Versioned, sequenced request or response envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    /// Must match [`PROTOCOL_VERSION`].
    pub protocol_version: u16,
    /// The request/response operation.
    pub operation: Operation,
    /// Non-empty request correlation identifier.
    pub request_id: String,
    /// Monotonic sequence number enforced by [`SequenceGuard`].
    pub sequence: u64,
    /// Non-empty key used by adapters to make retries idempotent.
    pub idempotency_key: String,
    /// Device targeted by this frame, when the operation requires one.
    pub device_id: Option<String>,
    /// Optional bounded UTF-8 text payload.
    pub text: Option<String>,
}

/// Validation, sequencing, or availability failure for a remote frame.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RemoteError {
    /// Serialized frame exceeds [`MAX_FRAME_BYTES`].
    #[error("REMOTE_FRAME_TOO_LARGE")]
    FrameTooLarge,
    /// Text content exceeds [`MAX_TEXT_BYTES`].
    #[error("REMOTE_TEXT_TOO_LARGE")]
    TextTooLarge,
    /// Frame protocol version is not supported.
    #[error("REMOTE_UNSUPPORTED_VERSION")]
    UnsupportedVersion,
    /// A required field is empty or the frame cannot be encoded.
    #[error("REMOTE_INVALID_FIELD: {0}")]
    InvalidField(&'static str),
    /// Sequence number was repeated or skipped.
    #[error("REMOTE_REPLAY")]
    Replay,
    /// A required remote adapter is not available.
    #[error("REMOTE_UNAVAILABLE")]
    Unavailable,
}

impl Frame {
    /// Validates the version, required fields, text bound, and encoded size.
    ///
    /// This is local validation only; it does not authenticate a peer or send
    /// the frame over a network.
    pub fn validate(&self) -> Result<(), RemoteError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(RemoteError::UnsupportedVersion);
        }
        if self.request_id.is_empty() {
            return Err(RemoteError::InvalidField("request_id"));
        }
        if self.idempotency_key.is_empty() {
            return Err(RemoteError::InvalidField("idempotency_key"));
        }
        if self.device_id.as_deref().is_some_and(str::is_empty) {
            return Err(RemoteError::InvalidField("device_id"));
        }
        if self
            .text
            .as_ref()
            .is_some_and(|text| text.len() > MAX_TEXT_BYTES)
        {
            return Err(RemoteError::TextTooLarge);
        }
        let encoded = serde_json::to_vec(self).map_err(|_| RemoteError::InvalidField("frame"))?;
        if encoded.len() > MAX_FRAME_BYTES {
            return Err(RemoteError::FrameTooLarge);
        }
        Ok(())
    }

    /// Returns a domain-separated SHA-256 hash of the validated JSON frame.
    pub fn content_hash(&self) -> Result<String, RemoteError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| RemoteError::InvalidField("frame"))?;
        let mut hasher = Sha256::new();
        hasher.update(b"evohime-remote-frame-v1\0");
        hasher.update(bytes);
        Ok(format!("{:x}", hasher.finalize()))
    }
}

/// Enforces the next expected sequence number for one ordered stream.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SequenceGuard {
    next: u64,
}

impl SequenceGuard {
    /// Accepts only the current expected number, then advances the sequence.
    ///
    /// A rejected number does not advance the guard.
    pub fn accept(&mut self, sequence: u64) -> Result<(), RemoteError> {
        if sequence != self.next {
            return Err(RemoteError::Replay);
        }
        self.next = self.next.saturating_add(1);
        Ok(())
    }
}

/// Builds a device projection that explicitly reports unavailable transport.
///
/// Use this when the product surface exists but no remote adapter is deployed;
/// it prevents a placeholder from appearing online.
pub fn unavailable_device(device_id: impl Into<String>, display_name: impl Into<String>) -> Device {
    Device {
        device_id: device_id.into(),
        display_name: display_name.into(),
        availability: Availability::Unavailable,
        last_seen_ms: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(sequence: u64) -> Frame {
        Frame {
            protocol_version: PROTOCOL_VERSION,
            operation: Operation::Ping,
            request_id: "r".into(),
            sequence,
            idempotency_key: "i".into(),
            device_id: None,
            text: None,
        }
    }

    #[test]
    fn sequence_guard_rejects_replay_and_gaps() {
        let mut guard = SequenceGuard::default();
        assert!(guard.accept(0).is_ok());
        assert_eq!(guard.accept(0), Err(RemoteError::Replay));
        assert_eq!(guard.accept(2), Err(RemoteError::Replay));
    }

    #[test]
    fn frame_hash_is_deterministic_and_validation_is_bounded() {
        assert_eq!(frame(0).content_hash(), frame(0).content_hash());
        let mut oversized = frame(0);
        oversized.text = Some("x".repeat(MAX_TEXT_BYTES + 1));
        assert_eq!(oversized.validate(), Err(RemoteError::TextTooLarge));
    }
}

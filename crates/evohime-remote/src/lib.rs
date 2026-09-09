//! Платформенно-независимый контракт удалённого control plane.
//!
//! Crate не открывает sockets и не владеет SQLite. Relay, Android и PC
//! adapters обязаны использовать эти bounded типы, а отсутствующий deployment
//! должен сообщать `Unavailable`, не выдавая себя за online transport.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_TEXT_BYTES: usize = 32 * 1024;
pub const MAX_DEVICES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Pair,
    RegisterDevice,
    ListDevices,
    ChatStart,
    ChatDelta,
    ChatComplete,
    Cancel,
    Ping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Online,
    Offline,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub device_id: String,
    pub display_name: String,
    pub availability: Availability,
    pub last_seen_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    pub protocol_version: u16,
    pub operation: Operation,
    pub request_id: String,
    pub sequence: u64,
    pub idempotency_key: String,
    pub device_id: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RemoteError {
    #[error("REMOTE_FRAME_TOO_LARGE")]
    FrameTooLarge,
    #[error("REMOTE_TEXT_TOO_LARGE")]
    TextTooLarge,
    #[error("REMOTE_UNSUPPORTED_VERSION")]
    UnsupportedVersion,
    #[error("REMOTE_INVALID_FIELD: {0}")]
    InvalidField(&'static str),
    #[error("REMOTE_REPLAY")]
    Replay,
    #[error("REMOTE_UNAVAILABLE")]
    Unavailable,
}

impl Frame {
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

    pub fn content_hash(&self) -> Result<String, RemoteError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| RemoteError::InvalidField("frame"))?;
        let mut hasher = Sha256::new();
        hasher.update(b"evohime-remote-frame-v1\0");
        hasher.update(bytes);
        Ok(format!("{:x}", hasher.finalize()))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SequenceGuard {
    next: u64,
}

impl SequenceGuard {
    pub fn accept(&mut self, sequence: u64) -> Result<(), RemoteError> {
        if sequence != self.next {
            return Err(RemoteError::Replay);
        }
        self.next = self.next.saturating_add(1);
        Ok(())
    }
}

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

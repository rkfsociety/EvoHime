//! Core boundary for the remote client MVP.
//!
//! The transport is intentionally not implemented here. This module prevents
//! an adapter from claiming a connected remote session until a deployable relay
//! and an authenticated transport are present.

use evohime_remote::{Availability, Device, RemoteError, SequenceGuard};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorState {
    Unavailable,
    Offline,
    Connected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteConnectorSnapshot {
    pub state: ConnectorState,
    pub device: Device,
}

impl RemoteConnectorSnapshot {
    pub fn unavailable(device_id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            state: ConnectorState::Unavailable,
            device: Device {
                device_id: device_id.into(),
                display_name: display_name.into(),
                availability: Availability::Unavailable,
                last_seen_ms: None,
            },
        }
    }
}

#[derive(Debug, Default)]
pub struct RemoteStreamGuard {
    sequence: SequenceGuard,
}

impl RemoteStreamGuard {
    pub fn accept_sequence(&mut self, sequence: u64) -> Result<(), RemoteError> {
        self.sequence.accept(sequence)
    }
}

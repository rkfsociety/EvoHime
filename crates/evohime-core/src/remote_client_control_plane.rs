//! Core boundary for the remote client MVP.
//!
//! The transport is intentionally not implemented here. This module prevents
//! an adapter from claiming a connected remote session until a deployable relay
//! and an authenticated transport are present.

use evohime_remote::{Availability, Device, RemoteError, SequenceGuard};

/// State reported by the Core remote connector boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorState {
    /// No authenticated, deployable relay is configured.
    Unavailable,
    /// A connector exists but is not currently connected.
    Offline,
    /// An authenticated transport is connected.
    Connected,
}

/// Current connector state paired with the visible remote device metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteConnectorSnapshot {
    /// Connector state.
    pub state: ConnectorState,
    /// Device metadata associated with this connector.
    pub device: Device,
}

impl RemoteConnectorSnapshot {
    /// Constructs a snapshot that explicitly reports the connector unavailable.
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

/// Rejects duplicate or out-of-order remote event sequence numbers.
#[derive(Debug, Default)]
pub struct RemoteStreamGuard {
    sequence: SequenceGuard,
}

impl RemoteStreamGuard {
    /// Accepts the next sequence value or returns a protocol error.
    pub fn accept_sequence(&mut self, sequence: u64) -> Result<(), RemoteError> {
        self.sequence.accept(sequence)
    }
}

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Control-plane IPC contract between `evohime-core` / Electron shell and `evohime-hive-daemon`.

use serde::{Deserialize, Serialize};

/// Status snapshot served by the standalone `evohime-hive-daemon` over local IPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiveObservationSnapshot {
    /// Contract version of the status payload.
    pub schema_version: u32,
    /// Stable owner name for consumers that combine multiple runtime states.
    pub owner: String,
    /// `observing`, `waiting_for_limits`, `waiting_for_tasks`, `disabled`, or `sleeping` (`observation_failed`).
    pub status: String,
    /// Whether the observation cycle is active.
    pub enabled: bool,
    /// Unix milliseconds at which the next observation cycle is expected.
    pub next_wake_at_ms: Option<i64>,
}

/// Default Windows named pipe path used for observation control queries.
pub const DEFAULT_HIVE_OBSERVATION_CONTROL_PIPE: &str = r"\\.\pipe\evohime-hive-observation";

/// Environment variable overriding the Windows named pipe path for observation control.
pub const HIVE_OBSERVATION_CONTROL_PIPE_ENV: &str = "EVOHIME_HIVE_CONTROL_PIPE";

/// Environment variable overriding the Unix socket path for observation control.
pub const HIVE_OBSERVATION_CONTROL_SOCKET_ENV: &str = "EVOHIME_HIVE_CONTROL_SOCKET";

/// Control request sent to the external `evohime-hive-daemon`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum HiveObservationControlRequest {
    /// Requests the current `HiveObservationSnapshot`.
    GetObservationStatus,
    /// Requests the full `HiveStateSnapshot` built by `evohime-hive-daemon`.
    GetHiveStateSnapshot,
    /// Wakes the autonomous observation loop immediately after a settings change.
    NotifySettingsChanged,
    /// Requests the current city-archipelago map.
    GetCityMap,
    /// Requests the current 2D Honeycomb world state (Plan 200).
    GetHoneycombWorld,
    /// Requests incremental Honeycomb mutation events since `from_revision`.
    GetHoneycombDelta {
        /// Last revision already applied by the client.
        from_revision: u64,
    },
    /// Dispatches a vehicle to deliver a delegated task to an agent cell.
    DispatchAgentTask {
        /// Delegated Hive role slug (e.g. `researcher`, `planner`, `implementer`).
        role: String,
        /// Short task topic used to match or spawn the target agent.
        topic: String,
    },
    /// Polls whether a previously dispatched vehicle trip has arrived at its cell.
    PollTripArrival {
        /// Dispatched vehicle trip identifier returned by `DispatchAgentTask`.
        trip_id: String,
    },
    /// Dispatches a return vehicle from the agent cell back to the Queen Citadel
    /// carrying the stage result (`report`, `analysis`, `decision`).
    CompleteAgentTask {
        /// Delegated Hive role slug (e.g. `researcher`, `planner`, `implementer`).
        role: String,
        /// Short task topic used when dispatching the agent.
        topic: String,
    },
}

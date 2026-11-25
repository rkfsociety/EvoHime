#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Minimal boundary helpers for Hive-owned task identifiers.

/// Prefix of every Hive task created from an accepted EvaPulse signal.
///
/// The prefix is the Hive-world marker used by `hive_observation` and the
/// operations panel: a task accepted through this boundary has no chat binding,
/// so the marker is what keeps its approval and agents inside the Hive view
/// instead of the ordinary chat stream.
pub const EVA_PULSE_TASK_PREFIX: &str = "eva-pulse-";

/// Parses the runtime UUID of a task id, tolerating the EvaPulse marker.
///
/// `ToolContext` and other Core-owned structures store a `Uuid`, while a task
/// accepted from a signal carries the Hive marker. Stripping the marker keeps
/// that task's own identity instead of falling back to a random runtime id.
pub fn parse_hive_task_uuid(task_id: &str) -> Result<uuid::Uuid, uuid::Error> {
    let body = task_id
        .strip_prefix(EVA_PULSE_TASK_PREFIX)
        .unwrap_or(task_id);
    uuid::Uuid::parse_str(body)
}

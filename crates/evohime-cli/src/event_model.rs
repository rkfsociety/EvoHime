use serde::Serialize;
use serde_json::Value;

/// Schema identifier for serialized headless CLI events.
pub const CLI_SCHEMA: &str = "evohime.cli.event/v1";
/// Maximum serialized event size in bytes.
pub const MAX_EVENT_BYTES: usize = 256 * 1024;

/// Borrowed event envelope serialized by the CLI.
#[derive(Debug, Serialize)]
pub struct CliEvent<'a> {
    /// Event schema identifier.
    pub schema: &'static str,
    /// Monotonic event sequence number.
    pub sequence: u64,
    /// Event type name.
    pub kind: &'a str,
    /// Task or workflow run identifier.
    pub run_id: &'a str,
    /// Redacted JSON event payload.
    pub payload: Value,
}

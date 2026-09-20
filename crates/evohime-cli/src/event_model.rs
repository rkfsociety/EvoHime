use serde::Serialize;
use serde_json::Value;

pub const CLI_SCHEMA: &str = "evohime.cli.event/v1";
pub const MAX_EVENT_BYTES: usize = 256 * 1024;

#[derive(Debug, Serialize)]
pub struct CliEvent<'a> {
    pub schema: &'static str,
    pub sequence: u64,
    pub kind: &'a str,
    pub run_id: &'a str,
    pub payload: Value,
}

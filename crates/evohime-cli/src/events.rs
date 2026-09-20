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

pub fn emit(event: &CliEvent<'_>) -> String {
    serde_json::to_string(event).unwrap_or_else(|_| {
        "{\"schema\":\"evohime.cli.event/v1\",\"kind\":\"internal_error\"}".to_string()
    })
}

pub fn event_matches_run(event_task_id: &str, run_id: &str) -> bool {
    !run_id.is_empty() && event_task_id == run_id
}

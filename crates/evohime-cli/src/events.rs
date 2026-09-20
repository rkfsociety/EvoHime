use serde::Serialize;
use serde_json::Value;

pub const CLI_SCHEMA: &str = "evohime.cli.event/v1";
pub const MAX_EVENT_BYTES: usize = 256 * 1024;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Completed = 0,
    RunFailed = 1,
    InvalidInvocation = 2,
    ApprovalUnavailable = 3,
    CredentialsUnavailable = 4,
    PolicyDenied = 5,
    TimeoutOrBudget = 6,
    CoreUnavailable = 7,
    Cancelled = 8,
}

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

pub fn terminal_exit_code(event_type: &str) -> Option<ExitCode> {
    if !evohime_cli_contract::is_terminal_event(event_type) {
        return None;
    }
    Some(match event_type {
        "task.completed" | "workflow.completed" => ExitCode::Completed,
        "task.stopped" | "workflow.cancelled" => ExitCode::Cancelled,
        "task.failed" | "workflow.failed" => ExitCode::RunFailed,
        _ => return None,
    })
}

pub fn event_matches_run(event_task_id: &str, run_id: &str) -> bool {
    !run_id.is_empty() && event_task_id == run_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_terminal_events_to_stable_exit_codes() {
        for (event_type, expected) in [
            ("task.completed", ExitCode::Completed),
            ("workflow.completed", ExitCode::Completed),
            ("task.stopped", ExitCode::Cancelled),
            ("workflow.cancelled", ExitCode::Cancelled),
            ("task.failed", ExitCode::RunFailed),
            ("workflow.failed", ExitCode::RunFailed),
        ] {
            assert_eq!(terminal_exit_code(event_type), Some(expected));
        }
        assert_eq!(terminal_exit_code("task.progress"), None);
    }

    #[test]
    fn filters_events_to_the_requested_run() {
        assert!(event_matches_run("run-1", "run-1"));
        assert!(!event_matches_run("run-2", "run-1"));
        assert!(!event_matches_run("", "run-1"));
        assert!(!event_matches_run("run-1", ""));
    }
}

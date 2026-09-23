/// Process exit status for a terminal Core task/workflow event.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    /// Task or workflow completed successfully.
    Completed = 0,
    /// Task or workflow failed.
    RunFailed = 1,
    /// CLI arguments or command form are invalid.
    InvalidInvocation = 2,
    /// Required approval could not be obtained.
    ApprovalUnavailable = 3,
    /// Provider credentials are unavailable.
    CredentialsUnavailable = 4,
    /// Core policy denied the operation.
    PolicyDenied = 5,
    /// A time or execution budget was exhausted.
    TimeoutOrBudget = 6,
    /// The Core process or protocol is unavailable.
    CoreUnavailable = 7,
    /// The task was stopped or cancelled.
    Cancelled = 8,
}

/// Maps a known terminal task/workflow event to its process exit status.
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
}

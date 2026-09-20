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

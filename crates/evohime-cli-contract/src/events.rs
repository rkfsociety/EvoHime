/// Stable schema identifier for events emitted by the headless CLI.
pub const EVENT_SCHEMA: &str = "evohime.cli.event/v1";

/// Returns whether `event_type` ends a task or workflow run.
///
/// Unknown event names are treated as non-terminal so adding a progress event
/// cannot accidentally end a client stream.
pub fn is_terminal_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "task.completed"
            | "task.failed"
            | "task.stopped"
            | "workflow.completed"
            | "workflow.failed"
            | "workflow.cancelled"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_events_are_explicit() {
        assert!(is_terminal_event("task.completed"));
        assert!(!is_terminal_event("task.progress"));
    }
}

pub const EVENT_SCHEMA: &str = "evohime.cli.event/v1";

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

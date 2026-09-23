use super::event_model::CliEvent;

/// Serializes an event to one compact JSON line.
pub fn emit(event: &CliEvent<'_>) -> String {
    serde_json::to_string(event).unwrap_or_else(|_| {
        "{\"schema\":\"evohime.cli.event/v1\",\"kind\":\"internal_error\"}".to_string()
    })
}

/// Returns whether an event belongs to the requested non-empty run identifier.
pub fn event_matches_run(event_task_id: &str, run_id: &str) -> bool {
    !run_id.is_empty() && event_task_id == run_id
}

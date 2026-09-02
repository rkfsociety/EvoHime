use evohime_core::headless_core_cli::{
    is_terminal_event, validate_request, ApprovalMode, OutputMode, RunRequest, SCHEMA_VERSION,
};

#[test]
fn headless_request_uses_the_core_contract_and_bounds() {
    let request = RunRequest {
        schema_version: SCHEMA_VERSION,
        prompt: "Review the bounded input".into(),
        workspace: "C:\\workspace".into(),
        output_mode: OutputMode::Ndjson,
        approval_mode: ApprovalMode::DenyIfApprovalRequired,
        detach: false,
    };
    validate_request(&request).expect("valid request");
}

#[test]
fn unknown_and_non_terminal_events_do_not_finish_a_run() {
    assert!(!is_terminal_event("approval.required"));
    assert!(!is_terminal_event("run.unknown"));
    assert!(is_terminal_event("task.completed"));
    assert!(is_terminal_event("task.failed"));
    assert!(is_terminal_event("workflow.completed"));
}

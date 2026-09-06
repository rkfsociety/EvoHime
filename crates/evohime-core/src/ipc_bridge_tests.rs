mod tests {
    use super::*;
    use crate::CoreEvent;
    use tokio::io::duplex;

    fn sample_typed_ledger_event(
        event_id: &str,
        action_id: &str,
    ) -> execution_ledger::ExecutionEventV1 {
        execution_ledger::ExecutionEventV1 {
            schema_version: 1,
            event_id: event_id.to_string(),
            sequence_id: None,
            run_scope: execution_ledger::RunScope::Standalone,
            run_id: "run-ipc-1".into(),
            session_id: Some("session-ipc-1".into()),
            task_id: "task-ipc".into(),
            created_at_ms: 1_700_000_000_000,
            state_after: Some(execution_ledger::ActionState::Running),
            action_id: Some(action_id.to_string()),
            tool_call_id: None,
            observation_id: None,
            receipt_id: None,
            failure_id: None,
            workflow_run_id: None,
            node_id: None,
            attempt_id: None,
            effect_id: None,
            model_request_id: None,
            body: execution_ledger::ExecutionEventBody::ToolCall {
                tool_name: "shell".into(),
                tool_call_hash: "hash-1".into(),
                manifest_hash: None,
            },
            redaction: execution_ledger::RedactionMeta::default(),
        }
    }

    /// Typed ledger rows written by 08-2's `append_ledger_event` must reach
    /// the IPC replay path (план 08-3) as an additive `execution_event`
    /// projection, without disturbing the generic backward-compat fields.

#[path = "../ipc_bridge_tests_projection.rs"]
mod projection;
#[path = "../ipc_bridge_tests_workspace_research.rs"]
mod workspace_research;
#[path = "../ipc_bridge_tests_memory_capabilities.rs"]
mod memory_capabilities;
#[path = "../ipc_bridge_tests_ambient.rs"]
mod ambient;
#[path = "../ipc_bridge_tests_workflow.rs"]
mod workflow;
}

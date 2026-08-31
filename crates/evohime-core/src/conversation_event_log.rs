use serde::{Deserialize, Serialize};

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationEventDraft {
    pub kind: String,
    pub category: String,
    pub persistence_class: String,
    pub sensitivity: String,
    pub authoritative_payload: Vec<u8>,
    pub renderer_payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RendererConversationEvent {
    pub schema_version: u32,
    pub conversation_id: String,
    pub event_id: String,
    pub sequence: u64,
    pub timestamp_ms: i64,
    pub kind: String,
    pub category: String,
    pub payload_json: Vec<u8>,
    pub correlation_id: String,
    pub causation_id: String,
    pub task_id: String,
    pub run_id: String,
    pub turn_id: String,
    pub client_message_id: String,
    pub persistence_class: String,
    pub sensitivity: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConversationEventError {
    #[error("conversation event payload is invalid")]
    InvalidPayload,
    #[error("conversation event payload exceeds bound")]
    PayloadTooLarge,
    #[error("conversation event type is unsupported")]
    UnsupportedEvent,
}

pub fn user_message_draft(content: &str) -> Result<ConversationEventDraft, ConversationEventError> {
    if content.is_empty() {
        return Err(ConversationEventError::InvalidPayload);
    }
    let authoritative = serde_json::json!({"content": content});
    draft(
        "user_message_accepted",
        "message",
        "durable",
        "user_content",
        authoritative,
    )
}

pub fn project_core_event(
    event_type: &str,
    payload: &[u8],
) -> Result<Vec<ConversationEventDraft>, ConversationEventError> {
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(ConversationEventError::PayloadTooLarge);
    }
    let parsed: serde_json::Value =
        serde_json::from_slice(payload).map_err(|_| ConversationEventError::InvalidPayload)?;
    let normalized = normalize_payload(parsed);
    let mut events = Vec::new();
    match event_type {
        "agent.message.delta" => events.push(draft(
            "assistant_message_delta",
            "message",
            "transient_stream",
            "user_content",
            normalized,
        )?),
        "task.completed" => {
            let message = normalized
                .get("final_message")
                .cloned()
                .unwrap_or(serde_json::Value::String(String::new()));
            events.push(draft(
                "assistant_message_finalized",
                "message",
                "durable",
                "user_content",
                serde_json::json!({"content": message}),
            )?);
            events.push(draft(
                "task_completed",
                "task",
                "durable",
                "internal",
                status_payload(&normalized),
            )?);
        }
        "task.failed" => {
            events.push(draft(
                "assistant_message_failed",
                "message",
                "durable",
                "internal",
                normalized.clone(),
            )?);
            events.push(draft(
                "task_failed",
                "error",
                "durable",
                "internal",
                status_payload(&normalized),
            )?);
        }
        "task.stopped" => events.push(draft(
            "task_stopped",
            "task",
            "durable",
            "internal",
            status_payload(&normalized),
        )?),
        "task.started" => {
            events.push(draft(
                "assistant_message_started",
                "message",
                "transient_stream",
                "internal",
                serde_json::json!({}),
            )?);
            events.push(draft(
                "task_started",
                "task",
                "durable",
                "internal",
                status_payload(&normalized),
            )?);
        }
        "tool.started" | "tool.output" => {
            let category = tool_category(&normalized);
            events.push(draft(
                if event_type == "tool.started" {
                    "tool_started"
                } else {
                    "tool_output"
                },
                category,
                "compactable",
                "tool_content",
                normalized,
            )?);
        }
        "approval.required" => events.push(draft(
            "approval_required",
            "approval",
            "durable",
            "internal",
            normalized,
        )?),
        "child.workflow" => events.push(draft(
            "child_run_summary",
            "child_run",
            "durable",
            "internal",
            child_summary(&normalized),
        )?),
        "model.context" | "model.usage" => events.push(draft(
            "usage_snapshot",
            "usage",
            "compactable",
            "internal",
            usage_summary(&normalized),
        )?),
        "routing.terminal" | "routing.pending_approval" => events.push(draft(
            "backend_snapshot",
            "backend",
            "durable",
            "internal",
            status_payload(&normalized),
        )?),
        "workflow.progress" | "review.progress" | "revision.progress" => events.push(draft(
            "task_progress",
            "task",
            "compactable",
            "internal",
            normalized,
        )?),
        "workspace.index_progress" | "workspace.retrieval_progress" | "storage.progress" => events
            .push(draft(
                "file_activity",
                "file",
                "compactable",
                "internal",
                status_payload(&normalized),
            )?),
        value if value.starts_with("goal.") => events.push(draft(
            "goal_snapshot",
            "goal",
            "durable",
            "internal",
            status_payload(&normalized),
        )?),
        value if value.starts_with("artifact.") => events.push(draft(
            "artifact_snapshot",
            "artifact",
            "compactable",
            "internal",
            status_payload(&normalized),
        )?),
        "task.checkpoint" | "task.recovery" => events.push(draft(
            "recovery_snapshot",
            "recovery",
            "durable",
            "internal",
            status_payload(&normalized),
        )?),
        _ => {
            events.push(draft(
                "agent_status",
                "agent_status",
                "compactable",
                "internal",
                serde_json::json!({"source_event_type": event_type}),
            )?);
        }
    }
    Ok(events)
}

pub fn renderer_event(
    event: &evohime_local_storage::conversation_event_log_store::StoredConversationEvent,
) -> Result<RendererConversationEvent, ConversationEventError> {
    serde_json::from_slice::<serde_json::Value>(&event.renderer_payload)
        .map_err(|_| ConversationEventError::InvalidPayload)?;
    Ok(RendererConversationEvent {
        schema_version: event.schema_version,
        conversation_id: event.conversation_id.clone(),
        event_id: event.event_id.clone(),
        sequence: event.sequence,
        timestamp_ms: event.timestamp_ms,
        kind: event.kind.clone(),
        category: event.category.clone(),
        payload_json: event.renderer_payload.clone(),
        correlation_id: event.correlation_id.clone().unwrap_or_default(),
        causation_id: event.causation_id.clone().unwrap_or_default(),
        task_id: event.task_id.clone().unwrap_or_default(),
        run_id: event.run_id.clone().unwrap_or_default(),
        turn_id: event.turn_id.clone().unwrap_or_default(),
        client_message_id: event.client_message_id.clone().unwrap_or_default(),
        persistence_class: event.persistence_class.clone(),
        sensitivity: event.sensitivity.clone(),
    })
}

fn draft(
    kind: &str,
    category: &str,
    persistence_class: &str,
    sensitivity: &str,
    authoritative: serde_json::Value,
) -> Result<ConversationEventDraft, ConversationEventError> {
    let local_policy = crate::sensitive_data_guardrails::default_policy("conversation_local");
    let authoritative = redact_projection(&local_policy, &authoritative)?;
    let authoritative_payload =
        serde_json::to_vec(&authoritative).map_err(|_| ConversationEventError::InvalidPayload)?;
    if authoritative_payload.len() > MAX_PAYLOAD_BYTES {
        return Err(ConversationEventError::PayloadTooLarge);
    }
    let policy = crate::sensitive_data_guardrails::default_policy("conversation_renderer");
    let renderer = redact_projection(&policy, &authoritative)?;
    let renderer_payload =
        serde_json::to_vec(&renderer).map_err(|_| ConversationEventError::InvalidPayload)?;
    Ok(ConversationEventDraft {
        kind: kind.into(),
        category: category.into(),
        persistence_class: persistence_class.into(),
        sensitivity: sensitivity.into(),
        authoritative_payload,
        renderer_payload,
    })
}

fn redact_projection(
    policy: &crate::sensitive_data_guardrails::PolicySnapshot,
    value: &serde_json::Value,
) -> Result<serde_json::Value, ConversationEventError> {
    match crate::sensitive_data_guardrails::redact_json(policy, value) {
        Ok((value, _)) => Ok(value),
        Err(crate::sensitive_data_guardrails::GuardrailError::Blocked(_)) => {
            Ok(serde_json::json!({"redacted": true, "reason": "sensitive_data_blocked"}))
        }
        Err(_) => Err(ConversationEventError::InvalidPayload),
    }
}

fn normalize_payload(value: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(object) = &value else {
        return value;
    };
    if object.len() == 1 {
        if let Some(inner) = object.values().next() {
            if inner.is_object() {
                return inner.clone();
            }
        }
    }
    value
}

fn tool_category(value: &serde_json::Value) -> &'static str {
    let tool = value
        .get("tool_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if tool.starts_with("browser.") {
        "browser"
    } else if tool.starts_with("filesystem.") || tool.starts_with("git.") {
        "file"
    } else if tool.starts_with("shell.")
        || tool.starts_with("process.")
        || tool.starts_with("terminal.")
    {
        "command"
    } else {
        "tool"
    }
}

fn status_payload(value: &serde_json::Value) -> serde_json::Value {
    let mut output = serde_json::Map::new();
    for key in [
        "task_id",
        "run_id",
        "status",
        "error",
        "stage",
        "completed",
        "total",
    ] {
        if let Some(item) = value.get(key) {
            output.insert(key.into(), item.clone());
        }
    }
    serde_json::Value::Object(output)
}

fn usage_summary(value: &serde_json::Value) -> serde_json::Value {
    let mut output = serde_json::Map::new();
    for key in [
        "task_id",
        "model",
        "estimated_tokens",
        "context_limit_tokens",
        "input_tokens",
        "output_tokens",
        "cache_tokens",
        "cost_micros",
        "source",
        "purpose",
    ] {
        if let Some(item) = value.get(key) {
            output.insert(key.into(), item.clone());
        }
    }
    output
        .entry("source")
        .or_insert_with(|| serde_json::Value::String("main_model".into()));
    output.entry("input_tokens").or_insert_with(|| {
        value
            .get("estimated_tokens")
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    });
    output
        .entry("output_tokens")
        .or_insert_with(|| serde_json::Value::Number(0.into()));
    serde_json::Value::Object(output)
}

fn child_summary(value: &serde_json::Value) -> serde_json::Value {
    let projection = value.get("projection").unwrap_or(value);
    let mut output = serde_json::Map::new();
    for key in ["task_id", "child_id", "role", "status", "conversation_ref"] {
        if let Some(item) = projection.get(key) {
            output.insert(key.into(), item.clone());
        }
    }
    serde_json::Value::Object(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_message_masks_secrets_before_local_persistence_and_renderer_projection() {
        let draft =
            user_message_draft("email roman@example.com token sk-12345678901234567890").unwrap();
        let authoritative: serde_json::Value =
            serde_json::from_slice(&draft.authoritative_payload).unwrap();
        let renderer: serde_json::Value = serde_json::from_slice(&draft.renderer_payload).unwrap();
        assert_ne!(
            authoritative["content"],
            "email roman@example.com token sk-12345678901234567890"
        );
        assert!(!authoritative["content"]
            .as_str()
            .unwrap()
            .contains("roman@example.com"));
        assert!(!authoritative["content"]
            .as_str()
            .unwrap()
            .contains("sk-12345678901234567890"));
        assert!(!renderer["content"]
            .as_str()
            .unwrap()
            .contains("roman@example.com"));
        assert!(!renderer["content"]
            .as_str()
            .unwrap()
            .contains("sk-12345678901234567890"));
    }

    #[test]
    fn streaming_delta_is_transient_and_completion_is_authoritative_finalized() {
        let delta = project_core_event("agent.message.delta", br#"{"content":"part"}"#).unwrap();
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].kind, "assistant_message_delta");
        assert_eq!(delta[0].persistence_class, "transient_stream");

        let completed = project_core_event(
            "task.completed",
            br#"{"final_message":"done","run_id":"run-1"}"#,
        )
        .unwrap();
        assert!(completed.iter().any(|event| {
            event.kind == "assistant_message_finalized" && event.persistence_class == "durable"
        }));
        assert!(completed.iter().any(|event| event.kind == "task_completed"));
    }

    #[test]
    fn existing_activity_types_map_to_one_log_category_model() {
        let cases = [
            ("tool.output", "tool"),
            ("approval.required", "approval"),
            ("child.workflow", "child_run"),
            ("workflow.progress", "task"),
            ("model.context", "usage"),
            ("task.failed", "error"),
            ("goal.updated", "goal"),
            ("artifact.saved", "artifact"),
            ("task.recovery", "recovery"),
        ];
        for (event_type, expected_category) in cases {
            let projected = project_core_event(event_type, br#"{}"#).unwrap();
            assert!(
                projected
                    .iter()
                    .any(|event| event.category == expected_category),
                "{event_type}"
            );
        }
    }

    #[test]
    fn model_context_projects_usage_fields_consumed_by_the_renderer() {
        let projected = project_core_event(
            "model.context",
            br#"{"model":"gpt-5","estimated_tokens":42,"context_limit_tokens":128000}"#,
        )
        .unwrap();
        let payload: serde_json::Value =
            serde_json::from_slice(&projected[0].renderer_payload).unwrap();
        assert_eq!(payload["source"], "main_model");
        assert_eq!(payload["input_tokens"], 42);
        assert_eq!(payload["output_tokens"], 0);
    }

    #[tokio::test]
    async fn event_journal_projects_bound_task_events_and_recovers_after_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("conversation.db");
        let journal = crate::EventJournal::open(&path).unwrap();
        let (accepted, _) = journal
            .accept_conversation_message(
                "conversation-1",
                "workspace-1",
                "task-1",
                "client-1",
                "secret sk-12345678901234567890",
            )
            .await
            .unwrap();
        assert!(!accepted.deduplicated);
        journal
            .record(&crate::CoreEvent::AssistantDelta {
                task_id: "task-1".into(),
                content: "part".into(),
            })
            .await
            .unwrap();
        journal
            .record(&crate::CoreEvent::TaskCompleted {
                task_id: "task-1".into(),
                final_message: "done".into(),
            })
            .await
            .unwrap();

        let page = journal
            .conversation_history_after("conversation-1", 0, 10)
            .await
            .unwrap();
        assert_eq!(
            page.events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(page.events[1].kind, "assistant_message_delta");
        assert_eq!(page.events[2].kind, "assistant_message_finalized");
        assert_eq!(page.events[3].kind, "task_completed");
        let renderer = String::from_utf8(page.events[0].renderer_payload.clone()).unwrap();
        assert!(!renderer.contains("sk-12345678901234567890"));
        drop(journal);

        let reopened = crate::EventJournal::open(&path).unwrap();
        let resumed = reopened
            .conversation_history_after("conversation-1", 2, 10)
            .await
            .unwrap();
        assert_eq!(
            resumed
                .events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![3, 4]
        );
        let (retry, _) = reopened
            .accept_conversation_message(
                "conversation-1",
                "workspace-1",
                "task-retry",
                "client-1",
                "secret sk-12345678901234567890",
            )
            .await
            .unwrap();
        assert!(retry.deduplicated);
        assert_eq!(retry.task_id, "task-1");
    }
}

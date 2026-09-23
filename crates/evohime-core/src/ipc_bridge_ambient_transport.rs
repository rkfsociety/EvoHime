use super::*;
use serde_json::{json, Value};

const TRACE_PROJECTION_VERSION: u64 = 3;
const TRACE_TEXT_BYTES_LIMIT: usize = 512 * 1024;

/// Keeps the conversation-bound event stream redacted while retaining bounded
/// diagnostic metadata. Raw prompts, arguments, tool output, paths and
/// provider errors remain Core-owned and never cross this projection boundary.
fn conversation_bound_trace_payload(event_type: &str, payload: &[u8]) -> Vec<u8> {
    let fallback = || {
        serde_json::to_vec(&json!({
            "redacted": true,
            "conversation_projection": true,
            "projection_version": TRACE_PROJECTION_VERSION,
            "event_type": event_type,
        }))
        .unwrap_or_else(|_| b"{\"redacted\":true}".to_vec())
    };
    let value = serde_json::from_slice::<Value>(payload)
        .ok()
        .map(crate::conversation_event_log::normalize_payload)
        .unwrap_or(Value::Null);
    let object = value.as_object();
    let mut projection = serde_json::Map::new();
    projection.insert("redacted".into(), Value::Bool(true));
    projection.insert("conversation_projection".into(), Value::Bool(true));
    projection.insert(
        "projection_version".into(),
        Value::Number(TRACE_PROJECTION_VERSION.into()),
    );
    projection.insert("event_type".into(), Value::String(event_type.to_owned()));
    if let Some(kind) = trace_projection_kind(event_type, payload) {
        projection.insert("projection_kind".into(), Value::String(kind));
    }

    match event_type {
        "task.started" => {
            projection.insert("terminal".into(), Value::Bool(false));
            projection.insert("status".into(), Value::String("started".into()));
            add_text_size(&mut projection, "prompt_bytes", object, "prompt");
        }
        "task.completed" => {
            projection.insert("terminal".into(), Value::Bool(true));
            projection.insert("status".into(), Value::String("completed".into()));
            add_text_size(
                &mut projection,
                "final_message_bytes",
                object,
                "final_message",
            );
        }
        "task.failed" => {
            projection.insert("terminal".into(), Value::Bool(true));
            if let Value::Object(diagnostics) =
                crate::conversation_event_log::failure_projection(&value)
            {
                projection.extend(diagnostics);
            }
        }
        "task.stopped" => {
            projection.insert("terminal".into(), Value::Bool(true));
            projection.insert("status".into(), Value::String("stopped".into()));
        }
        "tool.started" => {
            projection.insert("terminal".into(), Value::Bool(false));
            projection.insert("phase".into(), Value::String("started".into()));
            add_safe_token(&mut projection, "tool_name", object, "tool_name");
        }
        "tool.output" => {
            projection.insert("terminal".into(), Value::Bool(false));
            projection.insert("phase".into(), Value::String("output".into()));
            add_safe_token(&mut projection, "tool_name", object, "tool_name");
            add_text_size(&mut projection, "output_bytes", object, "output");
            projection.insert("output_redacted".into(), Value::Bool(true));
        }
        "tool.telemetry" => {
            projection.insert("terminal".into(), Value::Bool(false));
            projection.insert("phase".into(), Value::String("telemetry".into()));
            add_safe_token(&mut projection, "tool_name", object, "tool_name");
            add_u64(&mut projection, "iteration", object, "iteration");
            add_bool(&mut projection, "ok", object, "ok");
            add_safe_token(&mut projection, "failure_kind", object, "failure_kind");
            add_safe_token(&mut projection, "path_form", object, "path_form");
            add_safe_token(&mut projection, "path_scope", object, "path_scope");
            add_safe_token(
                &mut projection,
                "path_boundary_reason",
                object,
                "path_boundary_reason",
            );
            add_u64(&mut projection, "output_bytes", object, "output_bytes");
            add_bool(&mut projection, "recovery_hint", object, "recovery_hint");
            add_bool(&mut projection, "escalated", object, "escalated");
        }
        "model.context" | "model.usage" => {
            projection.insert("terminal".into(), Value::Bool(false));
            projection.insert("phase".into(), Value::String("usage".into()));
            add_safe_token(&mut projection, "model", object, "model");
            add_safe_token(&mut projection, "source", object, "source");
            add_safe_token(&mut projection, "purpose", object, "purpose");
            for key in [
                "estimated_tokens",
                "context_limit_tokens",
                "input_tokens",
                "output_tokens",
                "cache_tokens",
                "cost_micros",
            ] {
                add_u64(&mut projection, key, object, key);
            }
            if let Some(tools) = object.and_then(|item| item.get("tools")) {
                if let Some(tools) = tools.as_array() {
                    projection.insert("tools_count".into(), (tools.len() as u64).into());
                }
            }
        }
        "routing.terminal" => {
            projection.insert("terminal".into(), Value::Bool(true));
            projection.insert("phase".into(), Value::String("routing".into()));
            let trace = object
                .and_then(|item| item.get("trace"))
                .and_then(Value::as_object)
                .or(object);
            for key in [
                "selected_route",
                "terminal_status",
                "reason_code",
                "event",
                "classification",
                "privacy_label",
                "safe_next_action",
            ] {
                add_safe_token_from(&mut projection, key, trace, key);
            }
            for key in [
                "attempt_id",
                "fallback_count",
                "latency_ms",
                "estimated_input_tokens",
            ] {
                add_u64_from(&mut projection, key, trace, key);
            }
            if let Some(candidates) = trace
                .and_then(|item| item.get("candidates"))
                .and_then(Value::as_array)
            {
                projection.insert("candidates_count".into(), (candidates.len() as u64).into());
            }
        }
        "routing.pending_approval" => {
            projection.insert("terminal".into(), Value::Bool(false));
            projection.insert("phase".into(), Value::String("routing".into()));
            add_safe_token(&mut projection, "route_id", object, "route_id");
            add_u64(&mut projection, "expires_at_ms", object, "expires_at_ms");
        }
        "agent.message.delta" => {
            projection.insert("terminal".into(), Value::Bool(false));
            projection.insert("phase".into(), Value::String("message_delta".into()));
            add_text_size(&mut projection, "content_bytes", object, "content");
        }
        "approval.required" => {
            projection.insert("terminal".into(), Value::Bool(false));
            projection.insert("phase".into(), Value::String("approval".into()));
            add_safe_token(&mut projection, "tool_name", object, "tool_name");
            add_safe_token(&mut projection, "permission", object, "permission");
            if object.and_then(|item| item.get("scope")).is_some() {
                projection.insert("scope_present".into(), Value::Bool(true));
            }
        }
        "conversation.event" => {
            projection.insert("terminal".into(), Value::Bool(false));
            projection.insert(
                "phase".into(),
                Value::String("conversation_projection".into()),
            );
            add_safe_token(&mut projection, "conversation_kind", object, "kind");
            add_safe_token(&mut projection, "category", object, "category");
            add_safe_token(
                &mut projection,
                "persistence_class",
                object,
                "persistence_class",
            );
            add_safe_token(&mut projection, "sensitivity", object, "sensitivity");
            add_u64(&mut projection, "inner_sequence", object, "sequence");
            if let Some(payload_json) = object.and_then(|item| item.get("payload_json")) {
                if let Some(payload) = payload_json.as_array() {
                    projection.insert("payload_bytes".into(), (payload.len() as u64).into());
                }
            }
        }
        "workflow.progress" | "review.progress" | "revision.progress" => {
            projection.insert("terminal".into(), Value::Bool(false));
            projection.insert("phase".into(), Value::String("progress".into()));
            add_safe_token(&mut projection, "status", object, "status");
            add_safe_token(&mut projection, "stage", object, "stage");
            add_safe_token(&mut projection, "model", object, "model");
            add_u64(&mut projection, "completed", object, "completed");
            add_u64(&mut projection, "total", object, "total");
        }
        _ => {}
    }

    serde_json::to_vec(&Value::Object(projection)).unwrap_or_else(|_| fallback())
}

fn trace_projection_kind(event_type: &str, payload: &[u8]) -> Option<String> {
    crate::conversation_event_log::project_core_event(event_type, payload)
        .ok()
        .and_then(|drafts| drafts.into_iter().next())
        .map(|draft| draft.kind)
}

fn add_safe_token(
    projection: &mut serde_json::Map<String, Value>,
    output_key: &str,
    object: Option<&serde_json::Map<String, Value>>,
    input_key: &str,
) {
    add_safe_token_from(projection, output_key, object, input_key);
}

fn add_safe_token_from(
    projection: &mut serde_json::Map<String, Value>,
    output_key: &str,
    object: Option<&serde_json::Map<String, Value>>,
    input_key: &str,
) {
    let Some(value) = object
        .and_then(|item| item.get(input_key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.chars().count() <= 128
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric()
                        || matches!(character, '_' | '-' | '.' | ':' | '/' | '@')
                })
                && !value.contains("://")
                && !value.to_ascii_lowercase().contains("secret")
                && !value.to_ascii_lowercase().contains("token")
                && !value.to_ascii_lowercase().contains("password")
                && !value.to_ascii_lowercase().contains("bearer")
                && !value.to_ascii_lowercase().contains("sk-")
        })
    else {
        return;
    };
    projection.insert(output_key.into(), Value::String(value.to_owned()));
}

fn add_u64(
    projection: &mut serde_json::Map<String, Value>,
    output_key: &str,
    object: Option<&serde_json::Map<String, Value>>,
    input_key: &str,
) {
    add_u64_from(projection, output_key, object, input_key);
}

fn add_u64_from(
    projection: &mut serde_json::Map<String, Value>,
    output_key: &str,
    object: Option<&serde_json::Map<String, Value>>,
    input_key: &str,
) {
    if let Some(value) = object
        .and_then(|item| item.get(input_key))
        .and_then(Value::as_u64)
    {
        projection.insert(output_key.into(), value.min(u64::from(u32::MAX)).into());
    }
}

fn add_bool(
    projection: &mut serde_json::Map<String, Value>,
    output_key: &str,
    object: Option<&serde_json::Map<String, Value>>,
    input_key: &str,
) {
    if let Some(value) = object
        .and_then(|item| item.get(input_key))
        .and_then(Value::as_bool)
    {
        projection.insert(output_key.into(), Value::Bool(value));
    }
}

fn add_text_size(
    projection: &mut serde_json::Map<String, Value>,
    output_key: &str,
    object: Option<&serde_json::Map<String, Value>>,
    input_key: &str,
) {
    if let Some(value) = object
        .and_then(|item| item.get(input_key))
        .and_then(Value::as_str)
    {
        projection.insert(
            output_key.into(),
            (value.len().min(TRACE_TEXT_BYTES_LIMIT) as u64).into(),
        );
        projection.insert(
            format!("{output_key}_bounded"),
            Value::Bool(value.len() > TRACE_TEXT_BYTES_LIMIT),
        );
    }
}

impl IpcBridge {
    pub fn with_selected_model(mut self, selected: SelectedModel) -> Self {
        self.selected_model = selected;
        self
    }

    /// Streams journal entries newer than `after_sequence` to a connected
    /// client and returns the sequence it has now seen.
    ///
    /// Task progress reaches the shell this way rather than straight from the
    /// in-memory broadcast: the journal is what assigns sequence numbers, and
    /// the shell relies on them for resync after a reconnect.
    pub async fn push_journal_tail<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        after_sequence: u64,
    ) -> Result<u64, IpcBridgeError> {
        let batch = self
            .journal
            .replay_bounded(after_sequence as i64, 256)
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let mut last_sequence = after_sequence;
        for record in batch.events {
            last_sequence = record.sequence_id as u64;
            let task_is_conversation_bound = if record.task_id.is_empty() {
                false
            } else {
                let database = self.journal.database().lock().await;
                match evohime_local_storage::domains::audit::task_binding(
                    database.connection(),
                    &record.task_id,
                ) {
                    Ok(binding) => binding.is_some(),
                    Err(_) => true,
                }
            };
            // Typed ledger rows (план 08-1/08-2) carry ExecutionEventV1 JSON
            // in payload; project it additively into the oneof without
            // touching the generic event_type/payload backward-compat path.
            let execution_event = record
                .event_type
                .starts_with("ledger.")
                .then(|| decode_typed_execution_event(&record.payload))
                .flatten();
            let typed_event = if record.event_type == "project_instruction_stack.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("ProjectInstructionStack").unwrap_or(&value);
                        Some(generated::event_envelope::Event::ProjectInstructionStack(
                            generated::ProjectInstructionStackEvent {
                                schema_version: 1,
                                workspace_root: event.get("workspace_root")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                revision: event
                                    .get("revision")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                            },
                        ))
                    })
            } else if record.event_type == "team_coordinator.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("TeamCoordinator").unwrap_or(&value);
                        Some(generated::event_envelope::Event::TeamCoordinator(
                            generated::TeamCoordinatorEvent {
                                schema_version: 1,
                                work_item_id: event.get("work_item_id")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                revision: event
                                    .get("revision")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: event
                                    .get("status")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                            },
                        ))
                    })
            } else if record.event_type == "conversation.event" {
                let subscription = self.conversation_subscription.read().await.clone();
                decode_conversation_event(&record.payload).and_then(|conversation| {
                    let allowed = subscription
                        .as_ref()
                        .is_some_and(|(conversation_id, kinds)| {
                            conversation_id == &conversation.conversation_id
                                && (kinds.is_empty() || kinds.contains(&conversation.kind))
                        });
                    allowed.then(|| {
                        generated::event_envelope::Event::ConversationEventLog(
                            generated::ConversationEventLogEvent {
                                schema_version: crate::conversation_event_log::CONTRACT_VERSION,
                                operation: "live".into(),
                                conversation_id: conversation.conversation_id.clone(),
                                oldest_sequence: conversation.sequence,
                                newest_sequence: conversation.sequence,
                                has_older: false,
                                has_newer: false,
                                earliest_available_sequence: 0,
                                error_code: String::new(),
                                events: vec![conversation],
                            },
                        )
                    })
                })
            } else if record.event_type == "workspace_sets.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("WorkspaceSets").unwrap_or(&value);
                        Some(generated::event_envelope::Event::WorkspaceSets(
                            generated::WorkspaceSetsEvent {
                                schema_version: 1,
                                set_id: event.get("set_id")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                            },
                        ))
                    })
            } else if record.event_type == "knowledge_source_registry.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value
                            .get("KnowledgeSourceRegistryProjectRole")
                            .unwrap_or(&value);
                        Some(generated::event_envelope::Event::KnowledgeSourceRegistry(
                            generated::KnowledgeSourceRegistryProjectRoleEvent {
                                schema_version: 1,
                                source_id: event.get("source_id")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                            },
                        ))
                    })
            } else if record.event_type == "durable_remote_task_bridge.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("DurableRemoteTaskBridge").unwrap_or(&value);
                        Some(generated::event_envelope::Event::DurableRemoteTaskBridge(
                            generated::DurableRemoteTaskBridgeEvent {
                                schema_version: 1,
                                remote_task_id: event.get("remote_task_id")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        ))
                    })
            } else if record.event_type == "message_intervention_policies.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("MessageInterventionPolicies").unwrap_or(&value);
                        Some(
                            generated::event_envelope::Event::MessageInterventionPolicies(
                                generated::MessageInterventionPoliciesEvent {
                                    schema_version: 1,
                                    operation: event.get("operation")?.as_str()?.to_owned(),
                                    version: event
                                        .get("version")
                                        .and_then(serde_json::Value::as_u64)
                                        .unwrap_or_default(),
                                    status: String::new(),
                                    error_code: String::new(),
                                    projection_json: event
                                        .get("projection_json")
                                        .and_then(serde_json::Value::as_str)
                                        .unwrap_or("{}")
                                        .as_bytes()
                                        .to_vec(),
                                    truncated: false,
                                },
                            ),
                        )
                    })
            } else if record.event_type == "batch_invocation_runtime.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("BatchInvocationRuntime").unwrap_or(&value);
                        Some(generated::event_envelope::Event::BatchInvocationRuntime(
                            generated::BatchInvocationRuntimeEvent {
                                schema_version: 1,
                                batch_id: event.get("batch_id")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        ))
                    })
            } else if record.event_type == "policy_aware_tool_result_cache.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("PolicyAwareToolResultCache").unwrap_or(&value);
                        Some(
                            generated::event_envelope::Event::PolicyAwareToolResultCache(
                                generated::PolicyAwareToolResultCacheEvent {
                                    schema_version: 1,
                                    operation: event.get("operation")?.as_str()?.to_owned(),
                                    version: event
                                        .get("version")
                                        .and_then(serde_json::Value::as_u64)
                                        .unwrap_or_default(),
                                    status: String::new(),
                                    error_code: String::new(),
                                    projection_json: event
                                        .get("projection_json")
                                        .and_then(serde_json::Value::as_str)
                                        .unwrap_or("{}")
                                        .as_bytes()
                                        .to_vec(),
                                    truncated: false,
                                },
                            ),
                        )
                    })
            } else if record.event_type == "code_anchored_intent_markers.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("CodeAnchoredIntentMarkers").unwrap_or(&value);
                        Some(generated::event_envelope::Event::CodeAnchoredIntentMarkers(
                            generated::CodeAnchoredIntentMarkersEvent {
                                schema_version: 1,
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or(1),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        ))
                    })
            } else if record.event_type == "model_purpose_routing.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("ModelPurposeRouting").unwrap_or(&value);
                        Some(generated::event_envelope::Event::ModelPurposeRouting(
                            generated::ModelPurposeRoutingEvent {
                                schema_version: 1,
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or(1),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        ))
                    })
            } else if record.event_type == "local_model_runtime_manager.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("LocalModelRuntimeManager").unwrap_or(&value);
                        Some(generated::event_envelope::Event::LocalModelRuntimeManager(
                            generated::LocalModelRuntimeManagerEvent {
                                schema_version: 1,
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or(1),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        ))
                    })
            } else if record.event_type == "architecture_snapshot.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("ArchitectureSnapshot").unwrap_or(&value);
                        Some(generated::event_envelope::Event::ArchitectureSnapshot(
                            generated::ArchitectureSnapshotEvent {
                                schema_version: 1,
                                snapshot_id: event.get("snapshot_id")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or(1),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        ))
                    })
            } else if record.event_type == "persistent_agent_organization_registry.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .map(|value| {
                        let event = value
                            .get("PersistentAgentOrganizationRegistry")
                            .unwrap_or(&value);
                        generated::event_envelope::Event::PersistentAgentOrganizationRegistry(
                            generated::PersistentAgentOrganizationRegistryEvent {
                                schema_version: 1,
                                request_id: String::new(),
                                agent_id: event
                                    .get("agent_id")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                operation: event
                                    .get("operation")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                revision: event
                                    .get("revision")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: event
                                    .get("status")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        )
                    })
            } else if record.event_type == "execution_environment_profile.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .map(|value| {
                        let profile = value.get("profile").unwrap_or(&value);
                        let snapshot = value.get("snapshot").unwrap_or(&value);
                        generated::event_envelope::Event::ExecutionEnvironmentProfile(
                            generated::ExecutionEnvironmentProfileEvent {
                                schema_version: 1,
                                request_id: String::new(),
                                profile_id: profile
                                    .get("id")
                                    .or_else(|| snapshot.get("profile_id"))
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                operation: value
                                    .get("operation")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                revision: profile
                                    .get("revision")
                                    .or_else(|| snapshot.get("profile_revision"))
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: value
                                    .get("preflight")
                                    .and_then(|check| check.get("state"))
                                    .or_else(|| snapshot.get("state"))
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("unknown")
                                    .to_owned(),
                                error_code: String::new(),
                                projection_json: record.payload.clone(),
                                truncated: record.payload.len() > 64 * 1024,
                            },
                        )
                    })
            } else {
                execution_event
                    .map(|event| generated::event_envelope::Event::ExecutionEvent(Box::new(event)))
            };
            if record.event_type == "conversation.event" && typed_event.is_none() {
                continue;
            }
            let payload = if task_is_conversation_bound {
                conversation_bound_trace_payload(&record.event_type, &record.payload)
            } else {
                record.payload
            };
            let event = generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: record.sequence_id as u64,
                task_id: record.task_id,
                event_type: record.event_type,
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: typed_event,
            };
            transport::write_frame(writer, &event.encode_to_vec()).await?;
        }
        Ok(last_sequence)
    }

    /// Sequence the journal has already durably recorded.
    pub async fn latest_sequence(&self) -> u64 {
        self.journal.latest_sequence().await.max(0) as u64
    }

    /// Listener that fires whenever a task emits, so the server knows there is
    /// a journal tail worth flushing.
    /// Signal that fires once an event is durably journalled. The pipe server
    /// pushes the journal tail on this instead of on the broadcast itself,
    /// which used to overtake the writer and strand the last event of a task.
    pub fn journalled(&self) -> Option<tokio::sync::watch::Receiver<u64>> {
        self.coordinator
            .as_ref()
            .map(|coordinator| coordinator.journalled())
    }

    pub(crate) fn receipt_status(&self) -> serde_json::Value {
        let manager = &self.receipt_keys;
        let active = manager.active_path().exists();
        let history = manager.history_path().exists();
        let status = if !active && !history {
            "not_initialized".to_string()
        } else if !active || !history {
            "key.recovery_required".to_string()
        } else if manager.journal_path().exists() {
            "key.rotation_incomplete".to_string()
        } else {
            match manager.verify_history(None) {
                Ok(VerificationStatus::Verified) => "verified_unpinned".to_string(),
                Ok(VerificationStatus::Untrusted) => {
                    let loaded = manager.load_history().ok();
                    if loaded.as_ref().is_some_and(|items| {
                        items.iter().any(|item| {
                            matches!(item.continuity.as_str(), "broken" | "compromised")
                        })
                    }) {
                        return serde_json::json!({
                            "status": "key.trust_required",
                            "key_id": manager.load_signer().ok().map(|(metadata, _)| metadata.key_id),
                            "history_present": history,
                            "active_present": active,
                            "rotation_journal_present": manager.journal_path().exists(),
                        });
                    }
                    let genesis =
                        loaded.and_then(|items| items.first().map(|item| item.new_key_id.clone()));
                    match genesis.and_then(|key| manager.trusted_genesis(&key).ok()) {
                        Some(true) => "trusted".to_string(),
                        _ => "key.trust_required".to_string(),
                    }
                }
                Ok(VerificationStatus::Broken) => "key.history_incomplete".to_string(),
                Ok(VerificationStatus::Unsupported) => "unsupported".to_string(),
                Err(error) => error.to_string(),
            }
        };
        let key_id = std::fs::read(manager.active_path())
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<evohime_receipts::key_lifecycle::ActiveKeyMetadata>(&bytes)
                    .ok()
            })
            .map(|metadata| metadata.key_id);
        serde_json::json!({"status": status, "key_id": key_id, "history_present": history, "active_present": active, "rotation_journal_present": manager.journal_path().exists()})
    }

    pub(crate) async fn take_receipt_approval<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        approval_id: &str,
        operation: &str,
    ) -> Result<bool, IpcBridgeError> {
        let Some(approvals) = &self.approvals else {
            self.write_response(
                writer,
                "key.approval_required",
                serde_json::to_vec(
                    &serde_json::json!({"operation": operation, "error_code":"approval.required"}),
                )?,
            )
            .await?;
            return Ok(false);
        };
        let Ok(id) = uuid::Uuid::parse_str(approval_id) else {
            self.write_response(
                writer,
                "key.approval_required",
                serde_json::to_vec(
                    &serde_json::json!({"operation": operation, "error_code":"approval.required"}),
                )?,
            )
            .await?;
            return Ok(false);
        };
        if approvals.consume_approved(id).await {
            Ok(true)
        } else {
            self.write_response(writer, "key.approval_required", serde_json::to_vec(&serde_json::json!({"operation": operation, "approval_id": id.to_string(), "error_code":"approval.required"}))?).await?;
            Ok(false)
        }
    }

    pub(crate) async fn dispatch_save_continuation_policy(
        &self,
        request: generated::SaveContinuationPolicy,
        client_id: &str,
        request_id: &str,
        command_hash: &str,
    ) -> Result<Vec<u8>, String> {
        let policy: crate::continuation::ContinuationPolicyV1 =
            serde_json::from_slice(&request.policy_json)
                .map_err(|_| "invalid_argument".to_string())?;
        if request.policy_json.len() > crate::continuation::MAX_POLICY_BYTES
            || (!request.owner_scope.is_empty() && request.owner_scope != policy.scope.owner_scope)
            || (!request.actor.is_empty() && request.actor != policy.actor)
        {
            return Err("invalid_argument".into());
        }
        policy
            .validate()
            .map_err(|_| "invalid_policy".to_string())?;
        for gate in &policy.gates {
            let available = match gate.kind {
                crate::continuation::GateKind::Tool => {
                    self.tools
                        .as_ref()
                        .is_some_and(|tools| tools.manifest_for(&gate.capability_ref).is_some())
                        && gate.capability_ref != "shell"
                        && !gate.capability_ref.starts_with("shell.")
                }
                crate::continuation::GateKind::Workflow => {
                    crate::workflow_templates::template(&gate.capability_ref).is_some()
                }
                crate::continuation::GateKind::Evidence => self
                    .workflow_registry
                    .provider(&gate.capability_ref)
                    .is_some(),
                crate::continuation::GateKind::Approval => gate.capability_ref == "approval",
            };
            if !available {
                return Err("gate_unavailable".into());
            }
        }
        let canonical = policy
            .canonical_json()
            .map_err(|_| "invalid_policy".to_string())?;
        let result = serde_json::to_vec(&serde_json::json!({
            "schema_version": crate::continuation::POLICY_SCHEMA_VERSION,
            "policy_id": policy.id,
            "revision": policy.revision,
            "content_hash": policy.content_hash,
            "enabled": policy.enabled
        }))
        .map_err(|_| "serialization_failed".to_string())?;
        let journal = self.journal.clone();
        let database = journal.database().lock().await;
        if let Some(previous) = database
            .record_deduplicated(client_id, request_id, command_hash, &[])
            .map_err(|_| "idempotency_conflict".to_string())?
        {
            return Ok(previous);
        }
        evohime_local_storage::domains::runs::save_policy(
            database.connection(),
            &evohime_local_storage::domains::runs::PolicyRecord {
                policy_id: policy.id.clone(),
                revision: policy.revision as i64,
                owner_scope: policy.scope.owner_scope.clone(),
                actor: policy.actor.clone(),
                enabled: policy.enabled,
                canonical_json: canonical,
                content_hash: policy.content_hash.clone(),
                created_at_ms: policy.created_at_ms,
                updated_at_ms: policy.updated_at_ms,
            },
        )
        .map_err(|_| "storage_failed".to_string())?;
        database
            .record_deduplicated(client_id, request_id, command_hash, &result)
            .map_err(|_| "idempotency_conflict".to_string())?;
        Ok(result)
    }

    pub(crate) async fn dispatch_start_continuation(
        &self,
        request: generated::StartContinuationRun,
    ) -> Result<Vec<u8>, String> {
        if request.run_id.is_empty()
            || request.policy_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.task_id.is_empty()
        {
            return Err("invalid_argument".into());
        }
        let journal = self.journal.clone();
        let database = journal.database().lock().await;
        if let Some(existing) = evohime_local_storage::domains::runs::get_run_by_idempotency(
            database.connection(),
            &request.owner_scope,
            &request.idempotency_key,
        )
        .map_err(|_| "storage_failed".to_string())?
        {
            if existing.run_id == request.run_id
                && existing.task_id == request.task_id
                && existing.policy_id == request.policy_id
                && existing.policy_revision == request.policy_revision as i64
            {
                return continuation_public_json(&existing, &[]);
            }
            return Err("idempotency_conflict".into());
        }
        let policy = evohime_local_storage::domains::runs::get_policy(
            database.connection(),
            &request.policy_id,
            request.policy_revision as i64,
            &request.owner_scope,
        )
        .map_err(|_| "storage_failed".to_string())?
        .ok_or_else(|| "policy_not_found".to_string())?;
        if !policy.enabled {
            return Err("policy_disabled".into());
        }
        let policy_json: crate::continuation::ContinuationPolicyV1 =
            serde_json::from_slice(&policy.canonical_json)
                .map_err(|_| "policy_corrupt".to_string())?;
        let now = crate::task_memory::now_millis() as i64;
        let record = evohime_local_storage::domains::runs::RunRecord {
            run_id: request.run_id.clone(),
            idempotency_key: request.idempotency_key,
            task_id: request.task_id,
            owner_scope: request.owner_scope,
            policy_id: request.policy_id,
            policy_revision: request.policy_revision as i64,
            policy_hash: policy.content_hash,
            goal_id: (!request.goal_id.is_empty()).then_some(request.goal_id),
            goal_version: (request.goal_version > 0).then_some(request.goal_version as i64),
            state: "running".into(),
            continuation_index: 0,
            max_continuations: policy_json.budget.max_continuations as i64,
            max_model_turns: policy_json.budget.max_model_turns as i64,
            used_model_turns: 0,
            token_budget: policy_json.budget.max_tokens.map(|v| v as i64),
            token_used: 0,
            cost_budget_micros: policy_json.budget.max_cost_micros.map(|v| v as i64),
            cost_used_micros: 0,
            stop_reason: None,
            prompt: None,
            workspace_path: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        evohime_local_storage::domains::runs::create_run(database.connection(), &record).map_err(
            |error| {
                if matches!(error, rusqlite::Error::SqliteFailure(_, _)) {
                    "run_exists"
                } else {
                    "storage_failed"
                }
                .to_string()
            },
        )?;
        continuation_public_json(&record, &[])
    }

    pub(crate) async fn dispatch_get_continuation(
        &self,
        request: generated::GetContinuationRun,
    ) -> Result<Vec<u8>, String> {
        let database = self.journal.database().lock().await;
        let run =
            evohime_local_storage::domains::runs::get_run(database.connection(), &request.run_id)
                .map_err(|_| "storage_failed".to_string())?
                .ok_or_else(|| "run_not_found".to_string())?;
        let gates = evohime_local_storage::domains::runs::list_latest_gate_results(
            database.connection(),
            &run.run_id,
        )
        .map_err(|_| "storage_failed".to_string())?;
        continuation_public_json(&run, &gates)
    }

    pub(crate) async fn dispatch_stop_continuation(
        &self,
        request: generated::StopContinuation,
    ) -> Result<Vec<u8>, String> {
        if request.run_id.is_empty() || request.expected_state != "running" {
            return Err("invalid_argument".into());
        }
        let mut database = self.journal.database().lock().await;
        evohime_local_storage::domains::runs::apply_transition_action(
            database.connection_mut(),
            evohime_local_storage::domains::runs::TransitionActionInput {
                run_id: &request.run_id,
                idempotency_key: &request.idempotency_key,
                action: "stop",
                expected_state: &request.expected_state,
                next_state: "stopped",
                stop_reason: "user_stop",
                now_ms: crate::task_memory::now_millis() as i64,
            },
        )
        .map_err(|_| "storage_failed".to_string())
    }

    pub(crate) async fn dispatch_transition_continuation(
        &self,
        run_id: String,
        idempotency_key: String,
        expected_state: String,
        next_state: &'static str,
        action: &'static str,
    ) -> Result<Vec<u8>, String> {
        if run_id.is_empty()
            || idempotency_key.is_empty()
            || (expected_state != "running" && expected_state != "paused")
        {
            return Err("invalid_argument".into());
        }
        let mut database = self.journal.database().lock().await;
        evohime_local_storage::domains::runs::apply_transition_action(
            database.connection_mut(),
            evohime_local_storage::domains::runs::TransitionActionInput {
                run_id: &run_id,
                idempotency_key: &idempotency_key,
                action,
                expected_state: &expected_state,
                next_state,
                stop_reason: action,
                now_ms: crate::task_memory::now_millis() as i64,
            },
        )
        .map_err(|_| "storage_failed".to_string())
    }

    pub(crate) async fn dispatch_resume_continuation(
        &self,
        request: generated::ResumeContinuation,
    ) -> Result<evohime_local_storage::domains::runs::RunRecord, String> {
        if request.run_id.is_empty()
            || request.idempotency_key.is_empty()
            || request.expected_state != "paused"
        {
            return Err("invalid_argument".into());
        }
        let mut database = self.journal.database().lock().await;
        let run =
            evohime_local_storage::domains::runs::get_run(database.connection(), &request.run_id)
                .map_err(|_| "storage_failed".to_string())?
                .ok_or_else(|| "run_not_found".to_string())?;
        if run.prompt.is_none() || run.workspace_path.is_none() {
            return Err("resume_context_unavailable".into());
        }
        let _action_result = evohime_local_storage::domains::runs::apply_transition_action(
            database.connection_mut(),
            evohime_local_storage::domains::runs::TransitionActionInput {
                run_id: &request.run_id,
                idempotency_key: &request.idempotency_key,
                action: "resume",
                expected_state: "paused",
                next_state: "running",
                stop_reason: "approval_resolution",
                now_ms: crate::task_memory::now_millis() as i64,
            },
        )
        .map_err(|_| "storage_failed".to_string())?;
        evohime_local_storage::domains::runs::get_run(database.connection(), &request.run_id)
            .map_err(|_| "storage_failed".to_string())?
            .ok_or_else(|| "run_not_found".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_trace_keeps_only_safe_terminal_diagnostics() {
        let payload = conversation_bound_trace_payload(
            "task.failed",
            br#"{"error":"net::ERR_BLOCKED_BY_CLIENT https://example.test/download","prompt":"secret context","operation":"browser.navigate","source":"browser","run_id":"run-1","secret":"sk-test"}"#,
        );
        let value: Value = serde_json::from_slice(&payload).expect("valid trace projection");
        assert_eq!(value["redacted"], true);
        assert_eq!(value["conversation_projection"], true);
        assert_eq!(value["terminal"], true);
        assert_eq!(value["error_code"], "client_blocked");
        assert_eq!(value["source"], "browser");
        assert_eq!(value["operation"], "browser.navigate");
        assert!(value.get("error").is_none());
        assert!(value.get("prompt").is_none());
        assert!(value.get("run_id").is_none());
        assert!(!serde_json::to_string(&value)
            .unwrap()
            .contains("example.test"));
        assert!(!serde_json::to_string(&value).unwrap().contains("secret"));
    }

    #[test]
    fn conversation_trace_exposes_bounded_tool_metadata_without_payloads() {
        let started = conversation_bound_trace_payload(
            "tool.started",
            br#"{"ToolStarted":{"task_id":"task-1","tool_name":"filesystem.search"}}"#,
        );
        let started_value: Value =
            serde_json::from_slice(&started).expect("valid start projection");
        assert_eq!(started_value["projection_version"], 3);
        assert_eq!(started_value["projection_kind"], "tool_started");
        assert_eq!(started_value["tool_name"], "filesystem.search");
        assert_eq!(started_value["phase"], "started");

        let output = conversation_bound_trace_payload(
            "tool.output",
            br#"{"ToolOutput":{"task_id":"task-1","tool_name":"filesystem.search","output":"private https://example.test token sk-test"}}"#,
        );
        let output_value: Value = serde_json::from_slice(&output).expect("valid output projection");
        assert_eq!(output_value["projection_kind"], "tool_output");
        assert_eq!(output_value["tool_name"], "filesystem.search");
        assert_eq!(output_value["output_redacted"], true);
        assert!(output_value.get("output").is_none());
        let serialized = serde_json::to_string(&output_value).unwrap();
        assert!(!serialized.contains("example.test"));
        assert!(!serialized.contains("sk-test"));
    }

    #[test]
    fn conversation_trace_exposes_safe_telemetry_and_routing_metadata() {
        let telemetry = conversation_bound_trace_payload(
            "tool.telemetry",
            br#"{"tool_name":"filesystem.read","iteration":2,"ok":false,"failure_kind":"denied_policy","path_form":"absolute","path_scope":"outside_workspace","path_boundary_reason":"absolute_path_not_allowed","output_bytes":17,"recovery_hint":true,"escalated":false,"secret":"sk-test"}"#,
        );
        let telemetry_value: Value =
            serde_json::from_slice(&telemetry).expect("valid telemetry projection");
        assert_eq!(telemetry_value["tool_name"], "filesystem.read");
        assert_eq!(telemetry_value["iteration"], 2);
        assert_eq!(telemetry_value["ok"], false);
        assert_eq!(telemetry_value["failure_kind"], "denied_policy");
        assert_eq!(telemetry_value["path_form"], "absolute");
        assert_eq!(telemetry_value["path_scope"], "outside_workspace");
        assert_eq!(
            telemetry_value["path_boundary_reason"],
            "absolute_path_not_allowed"
        );
        assert!(serde_json::to_string(&telemetry_value)
            .unwrap()
            .contains("redacted"));
        assert!(!serde_json::to_string(&telemetry_value)
            .unwrap()
            .contains("sk-test"));

        let routing = conversation_bound_trace_payload(
            "routing.terminal",
            br#"{"RoutingTrace":{"task_id":"task-1","trace":{"selected_route":"cloud","terminal_status":"success","reason_code":"only_candidate","fallback_count":1,"latency_ms":42,"candidates":[{}],"snapshot_hash":"secret-hash"}}}"#,
        );
        let routing_value: Value =
            serde_json::from_slice(&routing).expect("valid routing projection");
        assert_eq!(routing_value["selected_route"], "cloud");
        assert_eq!(routing_value["terminal_status"], "success");
        assert_eq!(routing_value["fallback_count"], 1);
        assert_eq!(routing_value["candidates_count"], 1);
        assert!(!serde_json::to_string(&routing_value)
            .unwrap()
            .contains("secret-hash"));
    }

    #[test]
    fn conversation_trace_keeps_generic_events_bounded_when_projection_is_unknown() {
        let payload = conversation_bound_trace_payload(
            "workflow.progress",
            br#"{"status":"failed","error":"internal"}"#,
        );
        let value: Value = serde_json::from_slice(&payload).expect("valid trace projection");
        assert_eq!(value["projection_version"], 3);
        assert_eq!(value["projection_kind"], "task_progress");
        assert_eq!(value["status"], "failed");
        assert!(value.get("error").is_none());
    }

    #[test]
    fn conversation_trace_malformed_failure_payload_still_gets_safe_diagnostics() {
        let payload =
            conversation_bound_trace_payload("task.failed", b"not-json-with-secret-token");
        let value: Value = serde_json::from_slice(&payload).expect("valid trace projection");
        assert_eq!(value["error_code"], "task_failed");
        assert_eq!(value["source"], "core");
        assert_eq!(value["operation"], "task.execute");
        assert_eq!(value["redacted"], true);
        assert!(!serde_json::to_string(&value).unwrap().contains("secret"));
    }

    #[test]
    fn conversation_trace_oversized_failure_payload_still_gets_safe_diagnostics() {
        let payload = vec![b'x'; crate::conversation_event_log::MAX_PAYLOAD_BYTES + 1];
        let projected = conversation_bound_trace_payload("task.failed", &payload);
        let value: Value = serde_json::from_slice(&projected).expect("valid trace projection");
        assert_eq!(value["error_code"], "task_failed");
        assert_eq!(value["source"], "core");
        assert_eq!(value["operation"], "task.execute");
    }

    #[test]
    fn conversation_trace_classifies_blocked_ollama_download_without_url() {
        let payload = conversation_bound_trace_payload(
            "task.failed",
            br#"{"error":"net::ERR_BLOCKED_BY_CLIENT https://ollama.com/download/OllamaSetup.exe"}"#,
        );
        let value: Value = serde_json::from_slice(&payload).expect("valid trace projection");
        assert_eq!(value["error_code"], "client_blocked");
        assert_eq!(value["source"], "electron_transport");
        assert_eq!(value["operation"], "ollama.download");
        assert!(!serde_json::to_string(&value)
            .unwrap()
            .contains("ollama.com"));
    }
}

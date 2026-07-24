//! Post-task memory extract, feedback, and accept/reject.
use crate::app::AppState;
use crate::task::helpers::emit_event;
use evohime_model_gateway::providers::{ChatMessage, ChatRole};
use evohime_model_gateway::ModelGateway;
use evohime_protocol::ServerEvent;
use std::sync::Arc;
use uuid::Uuid;

#[cfg(test)]
pub(crate) fn summarize_task_memory(user_message: &str, final_message: &str) -> String {
    const LIMIT: usize = 400;
    let summary = format!(
        "User asked: {}; assistant replied: {}",
        user_message.trim(),
        final_message.trim()
    );
    summary.chars().take(LIMIT).collect()
}

pub(crate) const MEMORY_EXTRACT_PROMPT: &str = r#"Extract durable memory candidates from this completed task.
Return ONLY a JSON array (no markdown, no prose). Each object:
{"scope":"session|workspace|project|global|experience","kind":"fact|preference|constraint|failure_pattern|success_pattern|verification_rule|playbook","content":"...","confidence":0.0-1.0,"importance":0.0-1.0,"pinned":false,"playbook":{"trigger":"...","steps":["..."],"verify":"...","rollback_hint":"..."}}
Rules:
- Prefer workspace/session facts and preferences for ordinary notes.
- For reusable how-to / avoid / verify knowledge use scope=experience with success_pattern, failure_pattern, verification_rule, or playbook.
- Playbooks MUST include playbook{trigger,steps,verify?,rollback_hint?} (content may be empty; it will be derived).
- Use global/pinned/constraint only when clearly standing operator policy.
- Never include secrets, tokens, passwords, or private keys.
- Max 5 items. Empty array [] if nothing worth remembering."#;

/// Failure-lane prompt (7.103): a broken run is not a source of world facts,
/// so only reusable lessons are requested.
pub(crate) const FAILURE_EXTRACT_PROMPT: &str = r#"Analyze this FAILED task and extract at most 2 reusable lessons.
Return ONLY a JSON array (no markdown, no prose). Each object:
{"scope":"experience","kind":"failure_pattern|verification_rule","content":"...","confidence":0.0-1.0,"importance":0.0-1.0}
Rules:
- failure_pattern: what concretely went wrong and why (symptom + likely cause), phrased so the same mistake is recognizable next time.
- verification_rule: what should be checked BEFORE attempting a similar task.
- Only scope=experience; never facts, preferences, playbooks or success patterns from a failed run.
- Skip transient infrastructure noise (rate limits, provider 5xx, network timeouts) — those are not lessons.
- Never include secrets, tokens, passwords, or private keys.
- Empty array [] if the failure teaches nothing reusable."#;

pub(crate) fn scope_key_for(
    scope: evohime_storage::MemoryScope,
    session_id: Uuid,
    workspace_scope: &str,
) -> String {
    match scope {
        evohime_storage::MemoryScope::Session => session_id.to_string(),
        evohime_storage::MemoryScope::Workspace | evohime_storage::MemoryScope::Project => {
            workspace_scope.to_string()
        }
        evohime_storage::MemoryScope::Global | evohime_storage::MemoryScope::Experience => {
            evohime_storage::LOCAL_OPERATOR_SCOPE_KEY.to_string()
        }
    }
}

pub(crate) async fn collect_gateway_text(
    gateway: &ModelGateway,
    messages: &[ChatMessage],
    timeout: std::time::Duration,
) -> Option<String> {
    use evohime_model_gateway::ChatStreamItem;
    use futures_util::StreamExt;
    let stream = gateway.stream_chat(messages);
    let collect = async {
        let mut output = String::new();
        let mut stream = stream;
        while let Some(chunk) = stream.next().await {
            match chunk.ok()? {
                ChatStreamItem::Delta(text) => output.push_str(&text),
                ChatStreamItem::Usage(_) => {}
            }
        }
        Some(output)
    };
    tokio::time::timeout(timeout, collect).await.ok().flatten()
}

pub(crate) async fn llm_extract_memory_json(
    gateway: &ModelGateway,
    user_message: &str,
    final_message: &str,
    task_ok: bool,
) -> Option<String> {
    let status = if task_ok { "completed" } else { "failed" };
    let user = format!(
        "Task status: {status}\nUser message:\n{user_message}\n\nAssistant reply:\n{final_message}"
    );
    let system = if task_ok {
        MEMORY_EXTRACT_PROMPT
    } else {
        FAILURE_EXTRACT_PROMPT
    };
    let messages = [
        ChatMessage::text(ChatRole::System, system),
        ChatMessage::text(ChatRole::User, user),
    ];
    collect_gateway_text(gateway, &messages, std::time::Duration::from_secs(20)).await
}

pub(crate) async fn apply_task_memory_feedback(
    state: &Arc<AppState>,
    session_id: Uuid,
    task_id: Uuid,
    used_memory_ids: &[Uuid],
    task_ok: bool,
) {
    if !used_memory_ids.is_empty() {
        let results = if task_ok {
            evohime_memory::record_memory_helpful(&state.pool, used_memory_ids, Some(task_id)).await
        } else {
            evohime_memory::record_memory_harmful(&state.pool, used_memory_ids, Some(task_id)).await
        };
        match results {
            Ok(applied) => {
                for item in applied {
                    let _ = emit_event(
                        state,
                        session_id,
                        Some(task_id),
                        ServerEvent::MemoryUsed {
                            memory_id: item.memory_id,
                            task_id,
                            signal: item.signal.as_str().to_string(),
                            confidence: item.row.confidence,
                        },
                    )
                    .await;
                }
            }
            Err(error) => {
                tracing::warn!(%task_id, %error, "memory feedback apply failed");
            }
        }
    }

    match evohime_memory::decay_unused_memory(
        &state.pool,
        evohime_memory::DEFAULT_IDLE_DAYS,
        evohime_memory::DEFAULT_IDLE_BATCH,
    )
    .await
    {
        Ok(decayed) if !decayed.is_empty() => {
            tracing::info!(
                %task_id,
                decayed = decayed.len(),
                "applied idle memory decay"
            );
        }
        Err(error) => {
            tracing::warn!(%task_id, %error, "idle memory decay failed");
        }
        _ => {}
    }
}

pub(crate) async fn persist_structured_memory(
    state: &Arc<AppState>,
    gateway: &ModelGateway,
    session_id: Uuid,
    task: &evohime_storage::TaskRow,
    workspace_scope: &str,
    final_message: &str,
    task_ok: bool,
) {
    let llm_raw =
        llm_extract_memory_json(gateway, &task.user_message, final_message, task_ok).await;
    // Failed tasks use the restricted lane (7.103): at most two experience
    // lessons (failure_pattern / verification_rule), confidence capped below
    // auto-promote, no heuristic fallback — every lesson goes through Ask.
    let candidates = if task_ok {
        evohime_memory::extract_candidates(
            llm_raw.as_deref(),
            &task.user_message,
            final_message,
            task_ok,
        )
    } else {
        evohime_memory::extract_failure_candidates(llm_raw.as_deref())
    };

    for (index, candidate) in candidates.into_iter().enumerate() {
        let scope = candidate.scope;
        let scope_key = scope_key_for(scope, session_id, workspace_scope);
        let item = candidate.into_new_item(
            scope_key,
            Some(session_id),
            Some(task.id),
            format!("extract:{}:{}", task.id, index),
        );

        let outcome = match evohime_memory::admit_memory_item(&state.pool, item).await {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::warn!(task_id = %task.id, %error, "memory admit failed");
                continue;
            }
        };

        let (decision, row) = evohime_memory::gate_after_admit(&outcome);
        let Some(row) = row else {
            continue;
        };

        let _ = emit_event(
            state,
            session_id,
            Some(task.id),
            ServerEvent::MemoryProposed {
                memory_id: row.id,
                task_id: task.id,
                scope: row.scope.clone(),
                kind: row.kind.clone(),
                content: row.content.clone(),
                confidence: row.confidence,
                status: row.status.clone(),
            },
        )
        .await;

        match decision {
            evohime_memory::GateDecision::AutoPromote => {
                match evohime_memory::promote_memory_item(&state.pool, row.id).await {
                    Ok(Some(_)) => {
                        let _ = emit_event(
                            state,
                            session_id,
                            Some(task.id),
                            ServerEvent::MemoryAccepted {
                                memory_id: row.id,
                                task_id: task.id,
                            },
                        )
                        .await;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(memory_id = %row.id, %error, "memory promote failed");
                    }
                }
            }
            evohime_memory::GateDecision::Ask { reason } => {
                let _ = emit_event(
                    state,
                    session_id,
                    Some(task.id),
                    ServerEvent::MemoryAsk {
                        memory_id: row.id,
                        task_id: task.id,
                        scope: row.scope.clone(),
                        kind: row.kind.clone(),
                        content: row.content.clone(),
                        confidence: row.confidence,
                        status: row.status.clone(),
                        reason,
                    },
                )
                .await;
            }
            evohime_memory::GateDecision::Drop { reason } => {
                tracing::debug!(memory_id = %row.id, %reason, "memory gate drop");
                let _ = evohime_memory::reject_memory_item(&state.pool, row.id).await;
                let _ = emit_event(
                    state,
                    session_id,
                    Some(task.id),
                    ServerEvent::MemoryRejected {
                        memory_id: row.id,
                        task_id: task.id,
                    },
                )
                .await;
            }
        }
    }
}

pub(crate) async fn handle_memory_decision(
    state: &Arc<AppState>,
    session_id: Uuid,
    memory_id: Uuid,
    accept: bool,
) {
    let existing = match evohime_storage::get_memory_item(&state.pool, memory_id).await {
        Ok(row) => row,
        Err(error) => {
            tracing::warn!(%memory_id, %error, "failed to load memory for decision");
            return;
        }
    };
    let Some(existing) = existing else {
        return;
    };
    let task_id = existing.source_task_id.unwrap_or(Uuid::nil());

    if accept {
        match evohime_memory::accept_memory_item(&state.pool, memory_id).await {
            Ok(Some(_)) => {
                let _ = evohime_memory::record_memory_corrected(
                    &state.pool,
                    memory_id,
                    existing.source_task_id,
                )
                .await;
                let _ = emit_event(
                    state,
                    session_id,
                    Some(task_id),
                    ServerEvent::MemoryAccepted { memory_id, task_id },
                )
                .await;
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%memory_id, %error, "memory accept failed"),
        }
    } else {
        match evohime_memory::record_memory_rejected(
            &state.pool,
            memory_id,
            existing.source_task_id,
        )
        .await
        {
            Ok(Some(_)) => {
                let _ = emit_event(
                    state,
                    session_id,
                    Some(task_id),
                    ServerEvent::MemoryRejected { memory_id, task_id },
                )
                .await;
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%memory_id, %error, "memory reject failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_task_memory() {
        let note = summarize_task_memory(
            "Find the project index slice and make it work",
            "Done. Project index and MCP bridge are in place.",
        );

        assert!(note.contains("User asked: Find the project index slice and make it work"));
        assert!(
            note.contains("assistant replied: Done. Project index and MCP bridge are in place.")
        );
    }
}

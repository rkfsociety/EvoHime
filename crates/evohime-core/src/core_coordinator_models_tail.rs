use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::ConversationBridgeAdapters {
            operation,
            bridge_id,
            payload,
            expected_revision,
            idempotency_key,
            correlation_id,
            reply,
        } => {
            let result = async {
                    use crate::conversation_bridge_adapters as b;
                    use evohime_local_storage::conversation_bridge_adapters_store as store;
                    let journal = state
                        .lock()
                        .await
                        .journal
                        .clone()
                        .ok_or_else(|| "storage journal is not configured".to_string())?;
                    let db = journal.database().lock().await;
                    if idempotency_key.is_empty() || idempotency_key.len() > 128 {
                        return Err("invalid_idempotency_key".into());
                    }
                    if !correlation_id.is_empty() && correlation_id.len() > 128 {
                        return Err("invalid_correlation_id".into());
                    }
                    if !store::claim_idempotency(db.connection(), &idempotency_key, &operation)
                        .map_err(|_| "storage_failed".to_string())?
                    {
                        return serde_json::to_vec(
                            &serde_json::json!({"status":"replayed","redacted":true}),
                        )
                        .map_err(|_| "serialization_failed".to_string());
                    }
                    match operation.as_str() {
                        "create" | "revoke" => {
                            let mut bridge: b::ConversationBridge =
                                serde_json::from_slice(&payload)
                                    .map_err(|_| "invalid_bridge".to_string())?;
                            if bridge.bridge_id != bridge_id {
                                return Err("bridge_id_mismatch".into());
                            }
                            if expected_revision != 0
                                && store::bridge_revision(db.connection(), &bridge_id)
                                    .map_err(|_| "storage_failed".to_string())?
                                    != Some(expected_revision)
                            {
                                return Err("stale_revision".into());
                            }
                            if operation == "revoke" {
                                let stored = store::get_bridge(db.connection(), &bridge_id)
                                    .map_err(|_| "storage_failed".to_string())?
                                    .ok_or_else(|| "bridge_not_found".to_string())?;
                                let stored: b::ConversationBridge =
                                    serde_json::from_slice(&stored)
                                        .map_err(|_| "corrupt_bridge".to_string())?;
                                if stored.principal_id != bridge.principal_id
                                    || stored.state != b::BridgeState::Paired
                                {
                                    return Err("principal_binding_denied".into());
                                }
                            }
                            if operation == "revoke" {
                                bridge.state = b::BridgeState::Revoked;
                                bridge.revision = bridge.revision.saturating_add(1);
                            }
                            b::validate_bridge(&bridge).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&bridge)
                                .map_err(|_| "serialization_failed".to_string())?;
                            store::put_bridge(
                                db.connection(),
                                &bridge.bridge_id,
                                &json,
                                bridge.revision,
                            )
                            .map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({
                                "status": if operation == "revoke" { "revoked" } else { "paired" },
                                "bridge_id": bridge.bridge_id,
                                "revision": bridge.revision,
                                "redacted": true
                            }))
                            .map_err(|_| "serialization_failed".to_string())
                        }
                        "bind" => {
                            let binding: b::ThreadBinding = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_binding".to_string())?;
                            if binding.bridge_id != bridge_id {
                                return Err("bridge_id_mismatch".into());
                            }
                            b::validate_binding(&binding).map_err(|e| e.to_string())?;
                            let stored_bridge = store::get_bridge(db.connection(), &bridge_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "bridge_not_found".to_string())?;
                            let stored_bridge: b::ConversationBridge =
                                serde_json::from_slice(&stored_bridge)
                                    .map_err(|_| "corrupt_bridge".to_string())?;
                            b::authorize_principal(
                                &stored_bridge,
                                &binding.principal_id,
                                expected_revision,
                            )
                            .map_err(|e| e.to_string())?;
                            let inserted = store::put_binding(
                                db.connection(),
                                &payload,
                                &binding.binding_id,
                                &binding.bridge_id,
                                &binding.external_thread_id,
                                binding.revision,
                            )
                            .map_err(|_| "binding_conflict".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":if inserted{"bound"}else{"duplicate"},"binding_id":binding.binding_id,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "inbound" => {
                            let message: b::InboundMessage = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_inbound".to_string())?;
                            b::validate_inbound(&message).map_err(|e| e.to_string())?;
                            let stored_binding = store::get_binding(
                                db.connection(),
                                &message.binding_id,
                            )
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "binding_not_found".to_string())?;
                            let stored_binding: b::ThreadBinding =
                                serde_json::from_slice(&stored_binding)
                                    .map_err(|_| "corrupt_binding".to_string())?;
                            if stored_binding.bridge_id != bridge_id
                                || stored_binding.principal_id != message.principal_id
                            {
                                return Err("principal_binding_denied".into());
                            }
                            let stored_bridge = store::get_bridge(db.connection(), &bridge_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "bridge_not_found".to_string())?;
                            let stored_bridge: b::ConversationBridge =
                                serde_json::from_slice(&stored_bridge)
                                    .map_err(|_| "corrupt_bridge".to_string())?;
                            b::authorize_principal(
                                &stored_bridge,
                                &message.principal_id,
                                expected_revision,
                            )
                            .map_err(|e| e.to_string())?;
                            let accepted = store::put_inbound(
                                db.connection(),
                                &message.message_id,
                                &message.binding_id,
                                &payload,
                                message.created_at_ms,
                            )
                            .map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":if accepted{"accepted"}else{"duplicate_or_bounded"},"message_id":message.message_id,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "remote_command" => {
                            let command: b::RemoteCommand = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_remote_command".to_string())?;
                            b::validate_remote_command(&command).map_err(|e| e.to_string())?;
                            let stored_binding = store::get_binding(
                                db.connection(),
                                &command.binding_id,
                            )
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "binding_not_found".to_string())?;
                            let stored_binding: b::ThreadBinding =
                                serde_json::from_slice(&stored_binding)
                                    .map_err(|_| "corrupt_binding".to_string())?;
                            if stored_binding.bridge_id != bridge_id
                                || stored_binding.principal_id != command.principal_id
                            {
                                return Err("principal_binding_denied".into());
                            }
                            let stored_bridge = store::get_bridge(db.connection(), &bridge_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "bridge_not_found".to_string())?;
                            let stored_bridge: b::ConversationBridge =
                                serde_json::from_slice(&stored_bridge)
                                    .map_err(|_| "corrupt_bridge".to_string())?;
                            b::authorize_principal(
                                &stored_bridge,
                                &command.principal_id,
                                expected_revision,
                            )
                            .map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"accepted_for_core_dispatch","command_id":command.command_id,"kind":format!("{:?}",command.kind),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "project" => {
                            let request: BridgeProjectionRequest = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_projection_request".to_string())?;
                            let projection = b::redacted_projection(
                                &request.binding,
                                &request.kind,
                                &request.status,
                                &request.provenance_id,
                            )
                            .map_err(|e| e.to_string())?;
                            serde_json::to_vec(&projection)
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "list" => {
                            let count = store::list_inbound(db.connection())
                                .map_err(|_| "storage_failed".to_string())?
                                .len();
                            serde_json::to_vec(&serde_json::json!({"bridge_id":bridge_id,"inbound_count":count,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "clear" => {
                            store::clear_bridge(db.connection(), &bridge_id)
                                .map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"cleared","bridge_id":bridge_id,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_bridge_operation".into()),
                    }
                }
                .await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::ConversationBridgeAdapters {
                operation,
                bridge_id,
                revision: 1,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}

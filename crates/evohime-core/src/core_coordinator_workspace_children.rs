use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::RetainChild {
            child,
            now_ms,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                child.validate(now_ms).map_err(|e| e.to_string())?;
                let journal = journal.ok_or_else(|| "storage_unavailable".to_string())?;
                let database = journal.database().lock().await;
                let applied = state
                    .lock()
                    .await
                    .retained_children
                    .retain(child.clone(), now_ms)
                    .map_err(|e| e.to_string())?;
                if applied {
                    evohime_local_storage::domains::agents::RetainedChildStore::upsert_child(
                        database.connection(),
                        evohime_local_storage::domains::agents::UpsertChildInput {
                            parent_id: &child.parent_id,
                            child_id: &child.child_id,
                            family_root_id: &child.family_root_id,
                            revision: child.revision,
                            registry_version: child.registry_version,
                            lifecycle: "idle_retained",
                            record: &child,
                            created_at_ms: child.created_at_ms,
                            last_active_at_ms: child.last_active_at_ms,
                            retained_until_ms: child.retained_until_ms,
                        },
                    )
                    .map_err(|e| e.to_string())?;
                }
                serde_json::to_vec(
                    &serde_json::json!({"applied":applied,"child_id":child.child_id}),
                )
                .map_err(|e| e.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::GetRetainedChild {
            parent_id,
            child_id,
            now_ms,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal = journal.ok_or_else(|| "storage_unavailable".to_string())?;
                let database = journal.database().lock().await;
                let child =
                    evohime_local_storage::domains::agents::RetainedChildStore::get_child::<
                        crate::retained_child::RetainedChildV1,
                    >(database.connection(), &parent_id, &child_id)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "not_found".to_string())?;
                child.validate(now_ms).map_err(|e| e.to_string())?;
                let projection = crate::retained_child::RetainedChildProjectionV1::from(&child);
                serde_json::to_vec(&projection).map_err(|e| e.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::SendChildFollowUp {
            request,
            now_ms,
            busy,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            if let Some(journal) = &journal {
                let database = journal.database().lock().await;
                if let Ok(Some(child)) =
                    evohime_local_storage::domains::agents::RetainedChildStore::get_child::<
                        crate::retained_child::RetainedChildV1,
                    >(
                        database.connection(), &request.parent_id, &request.child_id
                    )
                {
                    let _ = state.lock().await.retained_children.restore(child);
                }
            }
            let result = async {
                    let durable_duplicate = if let Some(journal) = &journal {
                        let database = journal.database().lock().await;
                        evohime_local_storage::domains::agents::RetainedChildStore::has_follow_up(
                            database.connection(),
                            &request.idempotency_key,
                        ).map_err(|e| e.to_string())?
                    } else { false };
                    let outcome = if durable_duplicate {
                        crate::retained_child::FollowUpOutcome::Duplicate
                    } else {
                        state.lock().await.retained_children.follow_up(&request, now_ms, busy)
                            .map_err(|e| e.to_string())?
                    };
                    if !matches!(outcome, crate::retained_child::FollowUpOutcome::Duplicate) {
                        let journal = journal.ok_or_else(|| "storage_unavailable".to_string())?;
                        let mut database = journal.database().lock().await;
                        let message_id = uuid::Uuid::new_v4().to_string();
                        let delivery = if matches!(outcome, crate::retained_child::FollowUpOutcome::Dispatched) {
                            crate::retained_child::DeliveryState::Dispatched
                        } else {
                            crate::retained_child::DeliveryState::Pending
                        };
                        evohime_local_storage::domains::agents::RetainedChildStore::enqueue_follow_up(
                            database.connection_mut(),
                            evohime_local_storage::domains::agents::EnqueueFollowUpInput {
                                parent_id: &request.parent_id,
                                child_id: &request.child_id,
                                idempotency_key: &request.idempotency_key,
                                expected_revision: request.expected_child_revision,
                                request: &request,
                                message_id: &message_id,
                                build_entry: |sequence| crate::retained_child::MailboxEntryV1 {
                                    version: 1,
                                    message_id: message_id.clone(),
                                    sender_id: request.parent_id.clone(),
                                    receiver_id: request.child_id.clone(),
                                    family_root_id: request.family_root_id.clone(),
                                    mode: request.mode,
                                    kind: "follow_up".into(),
                                    correlation_id: request.correlation_id.clone(),
                                    parent_sequence: sequence,
                                    payload_ref: None,
                                    inline_payload: Some(request.instruction.as_bytes().to_vec()),
                                    sensitivity: "public".into(),
                                    delivery,
                                    delivered_at_ms: None,
                                    idempotency_key: request.idempotency_key.clone(),
                                    created_at_ms: now_ms,
                                },
                                now_ms,
                            },
                        )
                        .map_err(|e| e.to_string())?;
                        if matches!(outcome, crate::retained_child::FollowUpOutcome::Dispatched | crate::retained_child::FollowUpOutcome::Queued) {
                            if let Some(child) = state.lock().await.retained_children.get(&request.parent_id, &request.child_id, now_ms).ok().cloned() {
                                let lifecycle = match child.lifecycle {
                                    crate::retained_child::RetainedLifecycle::RunningFollowUp => "running_follow_up",
                                    crate::retained_child::RetainedLifecycle::QueuedFollowUp => "queued_follow_up",
                                    _ => "idle_retained",
                                };
                                evohime_local_storage::domains::agents::RetainedChildStore::upsert_child(database.connection(), evohime_local_storage::domains::agents::UpsertChildInput { parent_id: &child.parent_id, child_id: &child.child_id, family_root_id: &child.family_root_id, revision: child.revision, registry_version: child.registry_version, lifecycle, record: &child, created_at_ms: child.created_at_ms, last_active_at_ms: child.last_active_at_ms, retained_until_ms: child.retained_until_ms }).map_err(|e| e.to_string())?;
                            }
                        }
                    }
                    serde_json::to_vec(&serde_json::json!({
                        "outcome": format!("{outcome:?}").to_ascii_lowercase(),
                        "idempotency_key": request.idempotency_key,
                    }))
                    .map_err(|e| e.to_string())
                }
                .await;
            let _ = reply.send(result);
        }
        CoreCommand::ListRetainedChildren {
            parent_id,
            now_ms,
            limit,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal = journal.ok_or_else(|| "storage_unavailable".to_string())?;
                let database = journal.database().lock().await;
                let items =
                    evohime_local_storage::domains::agents::RetainedChildStore::list_children::<
                        crate::retained_child::RetainedChildV1,
                    >(database.connection(), &parent_id, now_ms, limit)
                    .map_err(|e| e.to_string())?;
                let projections: Vec<_> = items
                    .iter()
                    .map(crate::retained_child::RetainedChildProjectionV1::from)
                    .collect();
                serde_json::to_vec(&serde_json::json!({"children": projections}))
                    .map_err(|e| e.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::DeleteRetainedChild {
            parent_id,
            child_id,
            expected_registry_version,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            if let Some(journal) = &journal {
                let database = journal.database().lock().await;
                if let Ok(Some(child)) =
                    evohime_local_storage::domains::agents::RetainedChildStore::get_child::<
                        crate::retained_child::RetainedChildV1,
                    >(database.connection(), &parent_id, &child_id)
                {
                    let _ = state.lock().await.retained_children.restore(child);
                }
            }
            let result = async {
                let journal = journal.ok_or_else(|| "storage_unavailable".to_string())?;
                let database = journal.database().lock().await;
                let deleted =
                    evohime_local_storage::domains::agents::RetainedChildStore::delete_child(
                        database.connection(),
                        &parent_id,
                        &child_id,
                        expected_registry_version,
                    )
                    .map_err(|e| e.to_string())?;
                if !deleted {
                    return Err("stale_revision".into());
                }
                let _ = state
                    .lock()
                    .await
                    .retained_children
                    .delete(&parent_id, &child_id);
                serde_json::to_vec(&serde_json::json!({"deleted":true,"child_id":child_id}))
                    .map_err(|e| e.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}

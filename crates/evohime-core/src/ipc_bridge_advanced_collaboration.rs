use super::*;

impl IpcBridge {
    pub(crate) async fn dispatch_agent_role_profiles(
        &self,
        request: generated::AgentRoleProfilesCommand,
    ) -> serde_json::Value {
        use crate::agent_role_profiles::{
            canonical_hash, AgentRoleProfile, RoleProfileError, CONTRACT_VERSION,
        };
        if request.schema_version != CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","state":"failed","error_code":"invalid_request","projection_json":{"raw_prompt":false,"credentials":false}});
        }
        let mut registry = self.role_profiles.lock().await;
        if request.operation == "list" && registry.profiles.is_empty() {
            if let Ok(database) = self.journal.database().try_lock() {
                if let Ok(rows) = evohime_local_storage::agent_role_profiles_store::load_all_json(
                    database.connection(),
                ) {
                    for row in rows {
                        if let Ok(profile) = serde_json::from_slice::<
                            crate::agent_role_profiles::AgentRoleProfile,
                        >(&row)
                        {
                            registry.profiles.insert(profile.id.clone(), profile);
                        }
                    }
                }
            }
        }
        let mut status = "ok";
        let mut error_code = String::new();
        let mut profile_id = String::new();
        let mut revision = 0_u64;
        let mut state = "pinned";
        let mut projection = serde_json::json!({"schema_version":1,"profile_count":registry.profiles.len(),"raw_prompt":false,"credentials":false,"executable_code":false});
        let result: Result<(), RoleProfileError> = (|| match request.operation.as_str() {
            "list" => Ok(()),
            "get" => {
                let payload: serde_json::Value =
                    serde_json::from_slice(&request.payload).unwrap_or_default();
                profile_id = payload
                    .get("profile_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned();
                if let Some(profile) = registry.profiles.get(&profile_id) {
                    revision = profile.revision;
                    projection = serde_json::json!({"schema_version":1,"profile_id":profile.id,"revision":profile.revision,"content_hash":canonical_hash(profile).unwrap_or_default(),"execution_mode":profile.execution_mode,"raw_prompt":false,"credentials":false});
                    Ok(())
                } else {
                    Err(RoleProfileError::NotFound)
                }
            }
            "create" | "revise" => {
                let profile: AgentRoleProfile = serde_json::from_slice(&request.payload)
                    .map_err(|_| RoleProfileError::Invalid("payload"))?;
                profile_id = profile.id.clone();
                revision = profile.revision;
                let saved = if request.operation == "create" {
                    registry.create(profile.clone(), &request.idempotency_key)?
                } else {
                    registry.revise(
                        profile.clone(),
                        request.expected_revision,
                        &request.idempotency_key,
                    )?
                };
                let hash = canonical_hash(&saved)?;
                if let Ok(database) = self.journal.database().try_lock() {
                    let json = serde_json::to_vec(&saved).unwrap_or_default();
                    let _ = evohime_local_storage::agent_role_profiles_store::save_revision(
                        database.connection(),
                        &saved.id,
                        saved.revision,
                        &hash,
                        &json,
                        chrono::Utc::now().timestamp_millis(),
                    );
                }
                projection = serde_json::json!({"schema_version":1,"profile_id":saved.id,"revision":saved.revision,"content_hash":hash,"execution_mode":saved.execution_mode,"raw_prompt":false,"credentials":false});
                Ok(())
            }
            "start" => {
                let payload: AgentRoleRuntimePayload = serde_json::from_slice(&request.payload)
                    .map_err(|_| RoleProfileError::Invalid("payload"))?;
                let run_id = payload.run_id;
                profile_id = payload.profile_id;
                revision = payload.revision;
                let grants = payload.requested_grants;
                let allowed = vec![
                    "workspace.read".to_owned(),
                    "test.execute".to_owned(),
                    "review".to_owned(),
                ];
                let run = registry.start(crate::agent_role_profiles::StartRuntimeInput {
                    run_id,
                    profile_id: &profile_id,
                    revision,
                    grants,
                    parent: &allowed,
                    policy: &allowed,
                    registry: &allowed,
                })?;
                state = "pinned";
                projection = serde_json::json!({"schema_version":1,"profile_id":run.snapshot.profile_id,"revision":run.snapshot.revision,"content_hash":run.snapshot.content_hash,"run_id":run.run_id,"effective_grants":run.effective_grants,"state":run.state,"raw_prompt":false,"credentials":false});
                Ok(())
            }
            "cancel" => {
                let payload: AgentRoleRuntimePayload = serde_json::from_slice(&request.payload)
                    .map_err(|_| RoleProfileError::Invalid("payload"))?;
                let run = registry.cancel(&payload.run_id)?;
                state = "cancelling";
                profile_id = run.snapshot.profile_id;
                revision = run.snapshot.revision;
                projection = serde_json::json!({"schema_version":1,"run_id":run.run_id,"state":run.state,"profile_id":profile_id,"revision":revision,"raw_prompt":false,"credentials":false});
                Ok(())
            }
            _ => Err(RoleProfileError::Invalid("unsupported_operation")),
        })();
        if let Err(error) = result {
            status = "rejected";
            error_code = error.to_string();
            state = "failed";
        }
        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":status,"profile_id":profile_id,"revision":revision,"state":state,"error_code":error_code,"projection_json":projection})
    }

    pub(crate) async fn write_agent_role_profiles_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: IpcResponseFields = serde_json::from_slice(&payload)?;
        let result = generated::AgentRoleProfilesEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            profile_id: value.profile_id,
            revision: value.revision,
            content_hash: value.projection_json["content_hash"]
                .as_str()
                .unwrap_or_default()
                .into(),
            state: value.state,
            error_code: value.error_code,
            projection_json: serde_json::to_vec(&value.projection_json)?,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "agent_role_profiles.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::AgentRoleProfiles(result)),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn dispatch_conversation_event_log(
        &self,
        request: generated::ConversationEventLogRequest,
        operation: &str,
    ) -> generated::ConversationEventLogEvent {
        let limit = if request.limit == 0 {
            100
        } else {
            request.limit as usize
        };
        if request.schema_version != crate::conversation_event_log::CONTRACT_VERSION {
            return conversation_event_log_error(
                operation,
                &request.conversation_id,
                "event_schema_unsupported",
            );
        }
        if operation == "subscribed" {
            *self.conversation_subscription.lock().await = Some((
                request.conversation_id.clone(),
                request.kinds_filter.iter().cloned().collect(),
            ));
        }
        let invalid = request.conversation_id.is_empty()
            || request.conversation_id.len() > 128
            || limit > evohime_local_storage::conversation_event_log_store::MAX_PAGE_EVENTS
            || request.kinds_filter.len() > 16
            || request
                .kinds_filter
                .iter()
                .any(|kind| kind.is_empty() || kind.len() > 96)
            || (request.use_before_sequence && request.use_after_sequence)
            || (request.use_before_sequence && request.before_sequence == 0);
        if invalid {
            return conversation_event_log_error(
                operation,
                &request.conversation_id,
                "invalid_argument",
            );
        }
        let page = if request.use_before_sequence {
            self.journal
                .conversation_history_before(
                    &request.conversation_id,
                    request.before_sequence,
                    limit,
                )
                .await
        } else if request.use_after_sequence {
            self.journal
                .conversation_history_after(&request.conversation_id, request.after_sequence, limit)
                .await
        } else {
            self.journal
                .conversation_history_before(&request.conversation_id, u64::MAX, limit)
                .await
        };
        let page = match page {
            Ok(page) => page,
            Err(StorageError::ConversationEventLog(
                evohime_local_storage::conversation_event_log_store::ConversationStoreError::CursorExpired {
                    earliest_available_sequence,
                },
            )) => return conversation_event_log_error_with_earliest(
                operation,
                &request.conversation_id,
                "cursor_expired",
                earliest_available_sequence,
            ),
            Err(StorageError::ConversationEventLog(
                evohime_local_storage::conversation_event_log_store::ConversationStoreError::ConversationNotFound,
            )) => return conversation_event_log_error(operation, &request.conversation_id, "conversation_not_found"),
            Err(_) => return conversation_event_log_error(operation, &request.conversation_id, "history_unavailable"),
        };
        let filters = request
            .kinds_filter
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        let events = page
            .events
            .into_iter()
            .filter_map(|event| {
                if !filters.is_empty() && !filters.contains(&event.kind) {
                    return None;
                }
                crate::conversation_event_log::renderer_event(&event)
                    .ok()
                    .map(conversation_event_projection)
            })
            .collect::<Vec<_>>();
        generated::ConversationEventLogEvent {
            schema_version: crate::conversation_event_log::CONTRACT_VERSION,
            operation: operation.into(),
            conversation_id: request.conversation_id,
            oldest_sequence: page.oldest_sequence.unwrap_or(0),
            newest_sequence: page.newest_sequence.unwrap_or(0),
            has_older: page.has_older,
            has_newer: page.has_newer,
            earliest_available_sequence: page.earliest_available_sequence,
            error_code: String::new(),
            events,
        }
    }

    pub(crate) async fn write_conversation_event_log_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        result: generated::ConversationEventLogEvent,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: format!("conversation.{}", result.operation),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::ConversationEventLog(
                result,
            )),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn dispatch_conversation_workbench(
        &self,
        request: generated::ConversationWorkbenchRequest,
    ) -> generated::ConversationWorkbenchEvent {
        let error = |code: &'static str| generated::ConversationWorkbenchEvent {
            schema_version: crate::conversation_workbench::CONTRACT_VERSION,
            request_id: request.request_id.clone(),
            operation: "get".into(),
            conversation_id: request.conversation_id.clone(),
            event_cursor: 0,
            status: "rejected".into(),
            error_code: code.into(),
            projection_json: Vec::new(),
        };
        if request.schema_version != crate::conversation_workbench::CONTRACT_VERSION
            || request.request_id.is_empty()
            || crate::conversation_workbench::validate_scope(
                &request.conversation_id,
                &request.workspace_id,
                &request.run_id,
                &request.backend_snapshot_hash,
                &request.capability_snapshot_hash,
                request.after_sequence,
                request.limit as usize,
            )
            .is_err()
        {
            return error("invalid_request");
        }
        let limit = request.limit as usize;
        let page = if request.after_sequence == 0 {
            self.journal
                .conversation_history_before(&request.conversation_id, u64::MAX, limit)
                .await
        } else {
            self.journal
                .conversation_history_after(&request.conversation_id, request.after_sequence, limit)
                .await
        };
        let page = match page {
            Ok(page) => page,
            Err(StorageError::ConversationEventLog(
                evohime_local_storage::conversation_event_log_store::ConversationStoreError::CursorExpired { .. },
            )) => return error("cursor_expired"),
            Err(StorageError::ConversationEventLog(
                evohime_local_storage::conversation_event_log_store::ConversationStoreError::ConversationNotFound,
            )) => return error("conversation_not_found"),
            Err(_) => return error("projection_unavailable"),
        };
        let projection = crate::conversation_workbench::build_projection(
            request.conversation_id.clone(),
            request.workspace_id,
            request.run_id,
            request.backend_snapshot_hash,
            request.capability_snapshot_hash,
            page.newest_sequence.unwrap_or(request.after_sequence),
            &page.events,
        );
        let projection_json = match serde_json::to_vec(&projection) {
            Ok(value) if value.len() <= crate::conversation_workbench::MAX_PROJECTION_BYTES => {
                value
            }
            _ => return error("projection_too_large"),
        };
        generated::ConversationWorkbenchEvent {
            schema_version: crate::conversation_workbench::CONTRACT_VERSION,
            request_id: request.request_id,
            operation: "get".into(),
            conversation_id: projection.conversation_id,
            event_cursor: projection.event_cursor,
            status: "ok".into(),
            error_code: String::new(),
            projection_json,
        }
    }

    pub(crate) async fn write_conversation_workbench_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        result: generated::ConversationWorkbenchEvent,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: "conversation.workbench".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::ConversationWorkbench(
                result,
            )),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn dispatch_causal_collaboration_bus(
        &self,
        request: generated::CausalCollaborationBusCommand,
    ) -> serde_json::Value {
        use crate::causal_collaboration_bus::{
            validate, Address, CollaborationMessage, DeliveryState, MessageKind, Sensitivity,
            CONTRACT_VERSION,
        };
        let base = |status: &str, code: &str, projection: serde_json::Value| serde_json::json!({"schema_version": CONTRACT_VERSION, "request_id": request.request_id, "operation": request.operation, "status": status, "error_code": code, "version": 0, "projection_json": projection});
        if request.schema_version != CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.correlation_id.is_empty()
            || request.payload.len() > crate::causal_collaboration_bus::MAX_PAYLOAD_BYTES
        {
            return base(
                "rejected",
                "invalid_request",
                serde_json::json!({"raw_payload":false}),
            );
        }
        if request.operation == "list" || request.operation == "reconcile" {
            let database = self.journal.database().lock().await;
            if request.operation == "reconcile" {
                let _ =
                    evohime_local_storage::collaboration_store::reconcile(database.connection());
            }
            let messages =
                evohime_local_storage::collaboration_store::list::<CollaborationMessage>(
                    database.connection(),
                    &request.owner_scope,
                    128,
                )
                .unwrap_or_default();
            return base(
                "ok",
                "",
                serde_json::json!({"session_id":request.owner_scope,"count":messages.len(),"messages":messages.iter().map(|m| serde_json::json!({"message_id":m.message_id,"kind":m.kind,"sender":m.sender,"receiver":m.receiver,"sequence":m.sequence,"payload_hash":m.payload_hash,"sensitivity":m.sensitivity,"provenance_id":m.provenance_id,"delivery":DeliveryState::Queued})).collect::<Vec<_>>(),"raw_payload":false}),
            );
        }
        if request.operation != "publish" {
            return base(
                "unavailable",
                "unsupported_operation",
                serde_json::json!({"raw_payload":false}),
            );
        }
        let mut message: CollaborationMessage = match serde_json::from_slice(&request.payload) {
            Ok(v) => v,
            Err(_) => {
                return base(
                    "rejected",
                    "invalid_payload",
                    serde_json::json!({"raw_payload":false}),
                )
            }
        };
        message.session_id = request.owner_scope.clone();
        message.idempotency_key = request.idempotency_key.clone();
        message.correlation_id = request.correlation_id.clone();
        message.sender = Address::Parent;
        if matches!(
            message.kind,
            MessageKind::Progress
                | MessageKind::Notice
                | MessageKind::ArtifactRef
                | MessageKind::Request
                | MessageKind::Response
        ) && message.sensitivity != Sensitivity::Secret
        {
        } else {
            return base(
                "rejected",
                "invalid_message",
                serde_json::json!({"raw_payload":false}),
            );
        }
        if let Err(error) = validate(&message) {
            return base(
                "rejected",
                &error.to_string(),
                serde_json::json!({"raw_payload":false}),
            );
        }
        {
            let team = self.team_sop.lock().await;
            let Some(session) = team.sessions.get(&message.session_id) else {
                return base(
                    "rejected",
                    "destination_forbidden",
                    serde_json::json!({"raw_payload":false}),
                );
            };
            if !matches!(
                session.status,
                crate::team_sop_protocols::SessionStatus::Pinned
                    | crate::team_sop_protocols::SessionStatus::Running
                    | crate::team_sop_protocols::SessionStatus::Paused
            ) || session.snapshot.content_hash != message.protocol_hash
            {
                return base(
                    "rejected",
                    "destination_forbidden",
                    serde_json::json!({"raw_payload":false}),
                );
            }
            if let Address::RoleSlot { slot_id } | Address::DirectRoleInstance { slot_id, .. } =
                &message.receiver
            {
                let Some(slot) = serde_json::from_slice::<crate::team_sop_protocols::TeamProtocol>(
                    &session.snapshot.protocol_json,
                )
                .ok()
                .and_then(|p| {
                    p.participants
                        .into_iter()
                        .find(|slot| slot.slot_id == *slot_id)
                }) else {
                    return base(
                        "rejected",
                        "destination_forbidden",
                        serde_json::json!({"raw_payload":false}),
                    );
                };
                if !slot.allowed_peer_routes.is_empty()
                    && !slot
                        .allowed_peer_routes
                        .iter()
                        .any(|route| route == "parent" || route == "*")
                {
                    return base(
                        "rejected",
                        "destination_forbidden",
                        serde_json::json!({"raw_payload":false}),
                    );
                }
            }
        }
        let mut database = self.journal.database().lock().await;
        if evohime_local_storage::collaboration_store::exists(
            database.connection(),
            &request.idempotency_key,
        )
        .unwrap_or(false)
        {
            return base(
                "ok",
                "duplicate",
                serde_json::json!({"session_id":message.session_id,"message_id":message.message_id,"deduplicated":true,"raw_payload":false}),
            );
        }
        let sequence = match evohime_local_storage::retained_child_store::RetainedChildStore::next_parent_sequence(database.connection_mut(), &message.session_id) { Ok(v)=>v, Err(_)=>return base("unavailable","storage_failed",serde_json::json!({"raw_payload":false})) };
        message.sequence = sequence;
        let sender = message.sender.clone();
        let receiver = message.receiver.clone();
        match evohime_local_storage::collaboration_store::enqueue(
            database.connection_mut(),
            evohime_local_storage::collaboration_store::EnqueueInput {
                session: &message.session_id,
                key: &message.idempotency_key,
                message_id: &message.message_id,
                sender: &sender,
                receiver: &receiver,
                envelope: &message,
                sequence,
                now: chrono::Utc::now().timestamp_millis(),
            },
        ) {
            Ok(true) => base(
                "accepted",
                "",
                serde_json::json!({"session_id":message.session_id,"message_id":message.message_id,"sequence":sequence,"delivery":"queued","raw_payload":false}),
            ),
            Ok(false) => base(
                "ok",
                "duplicate",
                serde_json::json!({"deduplicated":true,"raw_payload":false}),
            ),
            Err(_) => base(
                "rejected",
                "inbox_full",
                serde_json::json!({"raw_payload":false}),
            ),
        }
    }

    pub(crate) async fn dispatch_causal_collaboration_subscribe(
        &self,
        request: generated::CausalCollaborationBusSubscribeCommand,
    ) -> serde_json::Value {
        if request.schema_version != crate::causal_collaboration_bus::CONTRACT_VERSION
            || request.owner_scope.is_empty()
            || request.session_id != request.owner_scope
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"destination_forbidden","projection_json":{"raw_payload":false}});
        }
        let database = self.journal.database().lock().await;
        let messages = evohime_local_storage::collaboration_store::list::<
            crate::causal_collaboration_bus::CollaborationMessage,
        >(
            database.connection(),
            &request.session_id,
            request.limit.clamp(1, 128),
        )
        .unwrap_or_default();
        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"ok","error_code":"","version":0,"projection_json":{"session_id":request.session_id,"after_sequence":request.after_sequence,"messages":messages.into_iter().filter(|m|m.sequence>request.after_sequence).map(|m|serde_json::json!({"message_id":m.message_id,"kind":m.kind,"sender":m.sender,"receiver":m.receiver,"sequence":m.sequence,"payload_hash":m.payload_hash,"provenance_id":m.provenance_id,"delivery":"queued","raw_payload":false})).collect::<Vec<_>>()}})
    }

    pub(crate) async fn write_causal_collaboration_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        value: serde_json::Value,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::CausalCollaborationBusEvent {
            schema_version: 1,
            request_id: value["request_id"].as_str().unwrap_or_default().into(),
            operation: value["operation"].as_str().unwrap_or_default().into(),
            status: value["status"].as_str().unwrap_or_default().into(),
            error_code: value["error_code"].as_str().unwrap_or_default().into(),
            version: value["version"].as_u64().unwrap_or_default(),
            projection_json: serde_json::to_vec(&value["projection_json"])?,
            truncated: false,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "causal_collaboration_bus.result".into(),
                payload: serde_json::to_vec(&value)?,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::CausalCollaborationBus(
                    event,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn dispatch_human_work_items(
        &self,
        request: generated::HumanWorkItemsCommand,
    ) -> serde_json::Value {
        use crate::human_work_items::{
            HumanWorkItem, HumanWorkItemError, HumanWorkItemState, CONTRACT_VERSION,
        };
        if request.schema_version != CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","state":"unknown","error_code":"invalid_request","projection_json":{"raw_prompt":false,"credentials":false}});
        }
        // A team-bound item is admissible only for the immutable role snapshot
        // selected by Core and only when that profile explicitly permits a human.
        if request.operation == "create" {
            if let Ok(item) = serde_json::from_slice::<HumanWorkItem>(&request.payload) {
                if let Some(slot_ref) = item.team_slot.as_ref() {
                    let team = self.team_sop.lock().await;
                    let allowed = team.sessions.get(&slot_ref.session_id).and_then(|session| {
                        if session.snapshot.content_hash != slot_ref.protocol_hash {
                            return None;
                        }
                        serde_json::from_slice::<crate::team_sop_protocols::TeamProtocol>(
                            &session.snapshot.protocol_json,
                        )
                        .ok()
                        .and_then(|protocol| {
                            protocol
                                .participants
                                .into_iter()
                                .find(|slot| slot.slot_id == slot_ref.slot_id)
                                .map(|slot| slot.role_profile_ref.id)
                        })
                    });
                    drop(team);
                    let human = if let Some(profile_id) = allowed {
                        self.role_profiles
                            .lock()
                            .await
                            .profiles
                            .get(&profile_id)
                            .is_some_and(|profile| {
                                matches!(
                                    profile.execution_mode,
                                    crate::agent_role_profiles::ExecutionMode::Human
                                )
                            })
                    } else {
                        false
                    };
                    if !human {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","state":"failed","error_code":"human_slot_denied","projection_json":{"raw_prompt":false,"credentials":false,"approval":false}});
                    }
                }
            }
        }
        let mut registry = self.human_work_items.lock().await;
        if registry.items.is_empty() {
            if let Ok(database) = self.journal.database().try_lock() {
                if let Ok(rows) = evohime_local_storage::human_work_items_store::load_all_json(
                    database.connection(),
                ) {
                    for row in rows {
                        if let Ok(item) = serde_json::from_slice::<HumanWorkItem>(&row) {
                            registry.items.insert(item.id.clone(), item);
                        }
                    }
                }
            }
        }
        let mut item_id = String::new();
        let mut revision = 0_u64;
        let mut state = "waiting_for_human".to_owned();
        let mut projection = serde_json::json!({"schema_version":1,"count":registry.items.len(),"raw_prompt":false,"credentials":false,"approval":false});
        let result: Result<(), HumanWorkItemError> = (|| match request.operation.as_str() {
            "list" => {
                projection = serde_json::json!({"schema_version":1,"count":registry.items.len(),"items":registry.list().into_iter().map(|item| serde_json::json!({"id":item.id,"revision":item.revision,"title":item.title,"state":item.state,"team_slot":item.team_slot,"expires_at_ms":item.expires_at_ms})).collect::<Vec<_>>(),"raw_prompt":false,"credentials":false,"approval":false});
                Ok(())
            }
            "get" => {
                let payload: HumanWorkItemCommandPayload = serde_json::from_slice(&request.payload)
                    .map_err(|_| HumanWorkItemError::Invalid("payload"))?;
                item_id = payload.item_id;
                let item = registry
                    .items
                    .get(&item_id)
                    .ok_or(HumanWorkItemError::NotFound)?;
                revision = item.revision;
                state = format!("{:?}", item.state).to_lowercase();
                projection = serde_json::json!({"schema_version":1,"id":item.id,"revision":item.revision,"title":item.title,"instructions":item.instructions,"response_schema":item.response_schema,"state":item.state,"team_slot":item.team_slot,"expires_at_ms":item.expires_at_ms,"raw_prompt":false,"credentials":false,"approval":false});
                Ok(())
            }
            "create" => {
                let item: HumanWorkItem = serde_json::from_slice(&request.payload)
                    .map_err(|_| HumanWorkItemError::Invalid("payload"))?;
                item_id = item.id.clone();
                revision = item.revision;
                state = format!("{:?}", item.state).to_lowercase();
                let saved = registry.create(item, &request.idempotency_key)?;
                projection = serde_json::json!({"schema_version":1,"id":saved.id,"revision":saved.revision,"title":saved.title,"state":saved.state,"team_slot":saved.team_slot,"raw_prompt":false,"credentials":false,"approval":false});
                Ok(())
            }
            "start" | "submit" | "accept" | "revise" | "return" | "cancel" => {
                let payload: HumanWorkItemCommandPayload = serde_json::from_slice(&request.payload)
                    .map_err(|_| HumanWorkItemError::Invalid("payload"))?;
                item_id = payload.item_id;
                let response = payload.response;
                let saved = registry.transition_idempotent(
                    crate::human_work_items::TransitionIdempotentInput {
                        id: item_id.clone(),
                        expected: request.expected_revision,
                        operation: request.operation.clone(),
                        response,
                        actor: "shell".into(),
                        now_ms: chrono::Utc::now().timestamp_millis(),
                        key: request.idempotency_key.clone(),
                    },
                )?;
                revision = saved.revision;
                state = format!("{:?}", saved.state).to_lowercase();
                projection = serde_json::json!({"schema_version":1,"id":saved.id,"revision":saved.revision,"title":saved.title,"state":saved.state,"team_slot":saved.team_slot,"response_present":saved.response.is_some(),"submitted_by":saved.submitted_by,"raw_prompt":false,"credentials":false,"approval":false});
                Ok(())
            }
            "expire_due" => {
                let changed = registry.expire_due(chrono::Utc::now().timestamp_millis());
                projection = serde_json::json!({"schema_version":1,"expired":changed.len(),"state":HumanWorkItemState::Expired,"raw_prompt":false,"credentials":false,"approval":false});
                Ok(())
            }
            _ => Err(HumanWorkItemError::Invalid("unsupported_operation")),
        })();
        let (status, error_code) = match result {
            Ok(()) => ("ok", String::new()),
            Err(error) => ("rejected", error.to_string()),
        };
        if status == "ok" && !item_id.is_empty() {
            if let Some(item) = registry.items.get(&item_id) {
                if let Ok(database) = self.journal.database().try_lock() {
                    let json = serde_json::to_vec(item).unwrap_or_default();
                    let _ = evohime_local_storage::human_work_items_store::save(
                        database.connection(),
                        &item.id,
                        item.revision,
                        &format!("{:?}", item.state).to_lowercase(),
                        &json,
                        &request.operation,
                        chrono::Utc::now().timestamp_millis(),
                    );
                }
            }
        }
        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":status,"item_id":item_id,"revision":revision,"state":state,"error_code":error_code,"projection_json":projection})
    }

}

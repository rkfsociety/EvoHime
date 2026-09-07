use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::CreateMemory {
            scope_kind,
            project_id,
            secondary_id,
            title,
            content,
            provenance_kind,
            provenance_id,
            provenance_locator,
            privacy,
            ttl_ms,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let domain_scope = memory_domain_scope(&scope_kind, &project_id, &secondary_id)?;
                let provenance = crate::memory_domain::ProvenanceRef::new(
                    provenance_kind,
                    provenance_id,
                    (!provenance_locator.trim().is_empty()).then_some(provenance_locator),
                )
                .map_err(|error| error.to_string())?;
                let privacy_label = parse_memory_privacy(&privacy)?;
                let created_at_ms = SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let id = uuid::Uuid::new_v4().to_string();
                let record = crate::memory_domain::MemoryDomain::new()
                    .create(crate::memory_domain::CreateMemory {
                        id: id.clone(),
                        scope: domain_scope,
                        title,
                        content,
                        provenance,
                        privacy: privacy_label,
                        created_at_ms,
                        ttl_ms,
                    })
                    .map_err(|error| error.to_string())?;
                let store_scope = memory_store_scope(&scope_kind)?;
                let store_privacy = memory_store_privacy(record.privacy)?;
                let provenance_json =
                    serde_json::to_string(&record.provenance).map_err(|error| error.to_string())?;
                let store_record = evohime_local_storage::domains::memory::MemoryRecord::new(
                    evohime_local_storage::domains::memory::MemoryRecordInput {
                        id: record.id.clone(),
                        scope: store_scope,
                        scope_id: encode_memory_scope_id(&project_id, &secondary_id),
                        title: record.title.clone(),
                        content: record.content.clone(),
                        provenance: provenance_json,
                        privacy: store_privacy,
                        created_at: record.created_at_ms.to_string(),
                        expires_at: Some(record.expires_at_ms.to_string()),
                    },
                )
                .map_err(|error| error.to_string())?;
                journal.save_memory(&store_record).await?;
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Evidence,
                    project_id.clone(),
                    "memory.created",
                    [
                        ("memory_id".to_owned(), record.id.clone()),
                        ("scope_kind".to_owned(), scope_kind),
                    ],
                )
                .await;
                serde_json::to_vec(&serde_json::json!({ "record": record }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ListMemory {
            scope_kind,
            project_id,
            secondary_id,
            include_archived,
            limit,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let store_scope = memory_store_scope(&scope_kind)?;
                let scope_id = encode_memory_scope_id(&project_id, &secondary_id);
                let records = journal
                    .list_memory(store_scope, &scope_id, include_archived, limit)
                    .await?;
                let records = records
                    .iter()
                    .map(memory_record_to_json)
                    .collect::<Result<Vec<_>, _>>()?;
                serde_json::to_vec(&serde_json::json!({ "records": records }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::SearchMemory {
            scope_kind,
            project_id,
            secondary_id,
            query,
            limit,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let store_scope = memory_store_scope(&scope_kind)?;
                let scope_id = encode_memory_scope_id(&project_id, &secondary_id);
                let now_ms = SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let records = journal
                    .search_memory(store_scope, &scope_id, &query, &now_ms.to_string(), limit)
                    .await?;
                let records = records
                    .iter()
                    .map(memory_record_to_json)
                    .collect::<Result<Vec<_>, _>>()?;
                serde_json::to_vec(&serde_json::json!({ "records": records }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ArchiveMemory {
            id,
            approval_id,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                crate::memory_api::Approval::new(
                    approval_id.clone(),
                    crate::memory_api::MemoryOperation::Archive,
                )
                .map_err(|error| error.to_string())?;
                let changed = journal.archive_memory(&id).await?;
                if !changed {
                    return Err(
                        "memory record was not found or is already archived/forgotten".to_string(),
                    );
                }
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Approval,
                    id.clone(),
                    "memory.archived",
                    [
                        ("memory_id".to_owned(), id.clone()),
                        ("approval_id".to_owned(), approval_id),
                    ],
                )
                .await;
                serde_json::to_vec(&serde_json::json!({ "id": id, "archived": true }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ForgetMemory {
            id,
            approval_id,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                crate::memory_api::Approval::new(
                    approval_id.clone(),
                    crate::memory_api::MemoryOperation::Forget,
                )
                .map_err(|error| error.to_string())?;
                // The tombstone id is random and unlinkable to the erased
                // body: audit keeps only kind, scope, timestamps, a reason
                // class and a digest.
                let tombstone_id = uuid::Uuid::new_v4().to_string();
                let forgotten_at = memory_now_ms().to_string();
                let changed = journal
                    .forget_memory_with_tombstone(&id, &tombstone_id, "user_request", &forgotten_at)
                    .await?;
                if !changed {
                    return Err("memory record was not found or is already forgotten".to_string());
                }
                // The erased statement still exists inside every backup
                // taken before this point, so forget also rotates the
                // containers that have aged past the retention window.
                let rotated = evohime_local_storage::LocalDatabase::purge_expired_backups(
                    crate::export::local_data_dir(),
                    crate::memory_extraction::FORGET_BACKUP_RETENTION_MS,
                    memory_now_ms(),
                )
                .map(|removed| removed.len())
                .unwrap_or(0);
                // План 01.5: каскад удаляет производные записи scratchpad и
                // task artifacts. Содержимое стирается, а факт удаления
                // остаётся в redacted аудите.
                let (removed_notes, removed_artifacts) = journal
                    .forget_context_derivatives(&id, &id)
                    .await
                    .unwrap_or((0, 0));
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Approval,
                    id.clone(),
                    "memory.forgotten",
                    [
                        ("memory_id".to_owned(), id.clone()),
                        ("approval_id".to_owned(), approval_id),
                        ("tombstone_id".to_owned(), tombstone_id.clone()),
                        ("reason_class".to_owned(), "user_request".to_owned()),
                        ("rotated_backups".to_owned(), rotated.to_string()),
                        ("removed_scratchpad".to_owned(), removed_notes.to_string()),
                        (
                            "removed_artifacts".to_owned(),
                            removed_artifacts.to_string(),
                        ),
                    ],
                )
                .await;
                serde_json::to_vec(&serde_json::json!({
                    "id": id,
                    "forgotten": true,
                    "tombstone_id": tombstone_id,
                    "rotated_backups": rotated,
                    "removed_scratchpad": removed_notes,
                    "removed_artifacts": removed_artifacts,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::MemoryViewsAndAdaptiveRecall {
            operation,
            view_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_view_id = view_id.clone();
            let result = async {
                    use crate::memory_views_and_adaptive_recall as v;
                    use evohime_local_storage::domains::memory as store;
                    if view_id.is_empty() || idempotency_key.is_empty() || idempotency_key.len() > 128 {
                        return Err("invalid_memory_view_request".to_string());
                    }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let db = journal.database().lock().await;
                    match operation.as_str() {
                        "save_view" => {
                            let request: MemoryViewSaveRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_memory_view_payload".to_string())?;
                            let view = request.view;
                            if view.id != view_id { return Err("view_id_mismatch".into()); }
                            v::validate_view(&view).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&view).map_err(|_| "serialization_failed".to_string())?;
                            let hash = v::canonical_hash(&view).map_err(|e| e.to_string())?;
                            let saved = store::save_view(db.connection(), store::ViewInput { view_id: &view.id, owner_scope: &view.owner_scope, revision: view.revision, view_json: &json, content_hash: &hash, expected_version, idempotency_key: &idempotency_key, now_ms: memory_now_ms() as i64 }).map_err(|_| "storage_failed".to_string())?;
                            if !saved { return Err("stale_version_or_idempotency_conflict".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"view_saved","view_id":view.id,"revision":view.revision,"content_hash":hash,"rights":view.rights,"scope_count":view.scopes.len(),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "inspect" => {
                            let record = store::load_view(db.connection(), &view_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "view_not_found".to_string())?;
                            let view: v::MemoryView = serde_json::from_slice(&record.view_json).map_err(|_| "corrupt_memory_view".to_string())?;
                            v::validate_view(&view).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"view","view_id":view.id,"revision":record.revision,"owner_scope":record.owner_scope,"rights":view.rights,"root_scope_ids":view.root_scope_ids,"scope_count":view.scopes.len(),"content_hash":record.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "recall" => {
                            let request: MemoryRecallRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_memory_view_payload".to_string())?;
                            let record = store::load_view(db.connection(), &view_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "view_not_found".to_string())?;
                            let view: v::MemoryView = serde_json::from_slice(&record.view_json).map_err(|_| "corrupt_memory_view".to_string())?;
                            v::validate_view(&view).map_err(|e| e.to_string())?;
                            let scope_id = request.scope_id.as_deref().unwrap_or(&view.root_scope_ids[0]);
                            v::authorize_read(&view, scope_id).map_err(|e| e.to_string())?;
                            let decision = v::decide_recall(&view, &request.policy, request.mode, request.complexity, &request.query, request.read_barrier_generation).map_err(|e| e.to_string())?;
                            let ranked = v::rank_candidates(&view, request.candidates).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&decision).map_err(|_| "serialization_failed".to_string())?;
                            let saved = store::save_recall(db.connection(), store::RecallInput { view_id: &view_id, view_revision: record.revision, barrier_generation: request.read_barrier_generation, decision_json: &json, expected_version, idempotency_key: &idempotency_key, now_ms: memory_now_ms() as i64 }).map_err(|_| "storage_failed".to_string())?;
                            if !saved { return Err("stale_version_or_idempotency_conflict".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"recall_planned","view_id":view_id,"view_revision":record.revision,"decision":decision,"ranked_candidates":ranked,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_memory_view_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::MemoryViewsAndAdaptiveRecall {
                operation: event_operation,
                view_id: event_view_id,
                version: expected_version.saturating_add(1),
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::ModelEditProtocolRegistry {
            operation,
            protocol_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_protocol_id = protocol_id.clone();
            let result = async {
                    use crate::model_edit_protocol_registry as v;
                    use evohime_local_storage::model_edit_protocol_registry_store as store;
                    if protocol_id.is_empty() || idempotency_key.is_empty() || idempotency_key.len() > 128 { return Err("invalid_model_edit_request".into()); }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let db = journal.database().lock().await;
                    let request: ModelEditRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_model_edit_payload".to_string())?;
                    match operation.as_str() {
                        "register" => {
                            let definition = request.definition.ok_or_else(|| "definition_required".to_string())?;
                            if definition.protocol_id != protocol_id { return Err("protocol_id_mismatch".into()); }
                            v::validate(&definition).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&definition).map_err(|_| "serialization_failed".to_string())?;
                            let content_hash = v::canonical_hash(&definition).map_err(|e| e.to_string())?;
                            let saved = store::save(db.connection(), store::DefinitionInput { protocol_id: &protocol_id, revision: definition.revision, model_profile_id: &definition.model_profile_id, definition_json: &json, content_hash: &content_hash, idempotency_key: &idempotency_key, expected_version, now_ms: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed".to_string())?;
                            if !saved { return Err("stale_version_or_idempotency_conflict".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"registered","protocol_id":protocol_id,"revision":definition.revision,"model_profile_id":definition.model_profile_id,"content_hash":content_hash,"redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        "inspect" => {
                            let record = store::load(db.connection(), &protocol_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "protocol_not_found".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"registered","protocol_id":protocol_id,"revision":record.revision,"model_profile_id":record.model_profile_id,"content_hash":record.content_hash,"version":record.version,"redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        "preflight" | "apply" => {
                            let record = store::load(db.connection(), &protocol_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "protocol_not_found".to_string())?;
                            if expected_version != record.version { return Err("stale_protocol_version".into()); }
                            let definition: v::EditProtocolDefinition = serde_json::from_slice(&record.definition_json).map_err(|_| "corrupt_edit_protocol".to_string())?;
                            let original = request.original.as_deref().ok_or_else(|| "original_required".to_string())?;
                            let preflight = v::preflight(&definition, original).map_err(|e| e.to_string())?;
                            if operation == "apply" { return Err("apply_requires_approved_revision_safe_files_tool".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"preflight_ok","protocol_id":protocol_id,"version":record.version,"preflight":preflight,"mutation":"not_dispatched","redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        "repair_feedback" => { let error_code = request.error_code.as_deref().unwrap_or("edit_failed"); let feedback = v::repair_feedback(&v::EditProtocolError::Invalid("edit_failed"), request.attempt).map_err(|_| format!("{error_code}:repair_exhausted"))?; serde_json::to_vec(&feedback).map_err(|_| "serialization_failed".into()) }
                        _ => Err("unsupported_model_edit_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::ModelEditProtocolRegistry {
                operation: event_operation,
                protocol_id: event_protocol_id,
                version: expected_version.saturating_add(1),
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::RemoteConversationChannels {
            operation,
            connection_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_connection_id = connection_id.clone();
            let result = async {
                    use crate::remote_conversation_channels as v; use evohime_local_storage::remote_conversation_channels_store as store;
                    if connection_id.is_empty() || idempotency_key.is_empty() || idempotency_key.len() > 128 { return Err("invalid_remote_channel_request".into()); }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?; let db = journal.database().lock().await;
                    let request: RemoteChannelRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_remote_channel_payload".to_string())?;
                    match operation.as_str() {
                        "save" => { let connection = request.connection.ok_or_else(|| "connection_required".to_string())?; if connection.connection_id != connection_id{return Err("connection_id_mismatch".into())}; v::validate_connection(&connection).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&connection).map_err(|_|"serialization_failed".to_string())?; let h=v::canonical_hash(&connection).map_err(|e|e.to_string())?; if !store::save(db.connection(),store::ConnectionInput{id:&connection_id,owner_scope:&connection.owner_scope,connection_json:&json,content_hash:&h,expected_version,idempotency_key:&idempotency_key,now_ms:crate::task_memory::now_millis() as i64}).map_err(|_|"storage_failed".to_string())?{return Err("stale_version_or_idempotency_conflict".into())}; serde_json::to_vec(&serde_json::json!({"status":"saved","connection_id":connection_id,"provider":connection.provider,"state":connection.state,"content_hash":h,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "inspect" => { let row=store::load(db.connection(),&connection_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"connection_not_found".to_string())?; serde_json::to_vec(&serde_json::json!({"status":"stored","connection_id":connection_id,"owner_scope":row.owner_scope,"content_hash":row.content_hash,"version":row.version,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "pair" => { let row=store::load(db.connection(),&connection_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"connection_not_found".to_string())?; let connection:v::ChannelConnection=serde_json::from_slice(&row.connection_json).map_err(|_|"corrupt_channel".to_string())?; let code=request.code.as_deref().ok_or_else(||"pairing_code_required".to_string())?; let identity=request.external_identity.as_deref().ok_or_else(||"external_identity_required".to_string())?; let now=crate::task_memory::now_millis() as i64; let ok=store::consume_pairing(db.connection(),&connection_id,&v::hash_pairing_code(code).map_err(|e|e.to_string())?,identity,now).map_err(|_|"storage_failed".to_string())?; if !ok{return Err("pairing_invalid_or_expired".into())}; if identity!=connection.external_identity{return Err("identity_mismatch".into())}; serde_json::to_vec(&serde_json::json!({"status":"paired","connection_id":connection_id,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "admit" => { let row=store::load(db.connection(),&connection_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"connection_not_found".to_string())?; let connection:v::ChannelConnection=serde_json::from_slice(&row.connection_json).map_err(|_|"corrupt_channel".to_string())?; let message=request.message.ok_or_else(||"message_required".to_string())?; let ok=store::claim_message(db.connection(),&connection_id,&message.message_id,crate::task_memory::now_millis() as i64).map_err(|_|"storage_failed".to_string())?; v::admit_message(&connection,&message,0,!ok,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"admitted","message_id":message.message_id,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "revoke" => { let row=store::load(db.connection(),&connection_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"connection_not_found".to_string())?; let mut connection:v::ChannelConnection=serde_json::from_slice(&row.connection_json).map_err(|_|"corrupt_channel".to_string())?; connection.state=v::ConnectionState::Revoked; connection.revision=connection.revision.saturating_add(1); let json=serde_json::to_vec(&connection).map_err(|_|"serialization_failed".to_string())?; let h=v::canonical_hash(&connection).map_err(|e|e.to_string())?; if !store::save(db.connection(),store::ConnectionInput{id:&connection_id,owner_scope:&connection.owner_scope,connection_json:&json,content_hash:&h,expected_version,idempotency_key:&idempotency_key,now_ms:crate::task_memory::now_millis() as i64}).map_err(|_|"storage_failed".to_string())?{return Err("stale_version_or_idempotency_conflict".into())}; serde_json::to_vec(&serde_json::json!({"status":"revoked","connection_id":connection_id,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        _ => Err("unsupported_remote_channel_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::RemoteConversationChannels {
                operation: event_operation,
                connection_id: event_connection_id,
                version: expected_version.saturating_add(1),
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::PromptCachePlanner {
            operation,
            plan_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_plan_id = plan_id.clone();
            let result=async { use crate::prompt_cache_planner as v; if plan_id.is_empty()||idempotency_key.is_empty(){return Err("invalid_prompt_cache_request".into())}; let request:PromptCacheRequest=serde_json::from_slice(&payload).map_err(|_|"invalid_prompt_cache_payload".to_string())?; match operation.as_str(){"plan"=>{let segments=request.segments;let profile=request.profile.ok_or_else(||"profile_required".to_string())?;let plan=v::build_plan(segments,&profile,&request.context_revision,&request.policy_version,request.keepalive_ms).map_err(|e|e.to_string())?;serde_json::to_vec(&serde_json::json!({"status":"planned","plan_id":plan_id,"cache_key":plan.cache_key,"segment_count":plan.segments.len(),"provider_profile_id":plan.provider_profile_id,"keepalive_ms":plan.keepalive_ms,"redacted":true})).map_err(|_|"serialization_failed".into())},"metric"=>{let metric=request.metric.ok_or_else(||"metric_required".to_string())?;v::validate_metric(&metric).map_err(|e|e.to_string())?;serde_json::to_vec(&serde_json::json!({"status":"metric_accepted","plan_id":plan_id,"cache_key":metric.cache_key,"hit":metric.hit,"cached_tokens":metric.cached_tokens,"redacted":true})).map_err(|_|"serialization_failed".into())},"inspect"=>serde_json::to_vec(&serde_json::json!({"status":"available","plan_id":plan_id,"version":expected_version,"idempotency_key_present":!idempotency_key.is_empty(),"redacted":true})).map_err(|_|"serialization_failed".into()),_=>Err("unsupported_prompt_cache_operation".into())}}.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::PromptCachePlanner {
                operation: event_operation,
                plan_id: event_plan_id,
                version: expected_version.saturating_add(1),
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            };
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::DeclarativeRuntimeComponents {
            operation,
            component_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_component_id = component_id.clone();
            let result = async {
                    use crate::declarative_runtime_components as v;
                    use evohime_local_storage::declarative_runtime_components_store as store;
                    if component_id.is_empty() || idempotency_key.is_empty() || idempotency_key.len() > 128 { return Err("invalid_declarative_component_request".into()); }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let db = journal.database().lock().await;
                    let request: DeclarativeComponentRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_declarative_component_payload".to_string())?;
                    match operation.as_str() {
                        "save" => {
                            let config = request.config.ok_or_else(|| "config_required".to_string())?;
                            let providers = request.registry.ok_or_else(|| "registry_required".to_string())?;
                            if config.component_id != component_id { return Err("component_id_mismatch".into()); }
                            v::validate(&config, &providers).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&config).map_err(|_| "serialization_failed".to_string())?;
                            if !store::save(db.connection(), store::SaveInput { id: &component_id, expected: expected_version, revision: config.revision, json: &json, hash: &config.content_hash, idem: &idempotency_key, now: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed".to_string())? { return Err("stale_version_or_idempotency_conflict".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"saved","component_id":component_id,"revision":config.revision,"content_hash":config.content_hash,"redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        "inspect" => {
                            let (revision, _, hash) = store::load(db.connection(), &component_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "component_not_found".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"stored","component_id":component_id,"revision":revision,"content_hash":hash,"redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        "rehydrate" => {
                            let (_, json, _) = store::load(db.connection(), &component_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "component_not_found".to_string())?;
                            let config: v::ComponentConfig = serde_json::from_slice(&json).map_err(|_| "corrupt_component".to_string())?;
                            let providers = request.registry.ok_or_else(|| "registry_required".to_string())?;
                            let policy = request.policy.ok_or_else(|| "policy_required".to_string())?;
                            v::rehydrate(&config, &providers, &policy).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"rehydrated","component_id":component_id,"revision":config.revision,"state":config.runtime_state,"redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        "transition" => {
                            let (revision, json, _) = store::load(db.connection(), &component_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "component_not_found".to_string())?;
                            let mut config: v::ComponentConfig = serde_json::from_slice(&json).map_err(|_| "corrupt_component".to_string())?;
                            let next = request.state.ok_or_else(|| "state_required".to_string())?;
                            v::validate_transition(&config.runtime_state, &next).map_err(|e| e.to_string())?;
                            config.runtime_state = next; config.revision = revision.saturating_add(1); config.content_hash = v::canonical_hash(&config).map_err(|e| e.to_string())?;
                            let out = serde_json::to_vec(&config).map_err(|_| "serialization_failed".to_string())?;
                            if !store::save(db.connection(), store::SaveInput { id: &component_id, expected: expected_version.max(revision), revision: config.revision, json: &out, hash: &config.content_hash, idem: &idempotency_key, now: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed".to_string())? { return Err("stale_version_or_idempotency_conflict".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"transitioned","component_id":component_id,"revision":config.revision,"state":config.runtime_state,"redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        _ => Err("unsupported_declarative_component_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::DeclarativeRuntimeComponents {
                operation: event_operation,
                component_id: event_component_id,
                version: expected_version.saturating_add(1),
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

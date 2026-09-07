use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::KnowledgeSourceRegistryProjectRole {
            operation,
            source_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let _ = idempotency_key;
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::knowledge_source_registry_project_role as knowledge;
                    use evohime_local_storage::knowledge_source_registry_project_role_store as store;
                    let policy = knowledge::default_policy();
                    match operation.as_str() {
                        "collection_register" => {
                            let collection: knowledge::KnowledgeCollection = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_collection".to_string())?;
                            knowledge::validate_collection(&collection, &policy).map_err(|e| e.to_string())?;
                            for source_id in &collection.source_ids {
                                if store::get_source(database.connection(), source_id).map_err(|_| "storage_failed".to_string())?.is_none() {
                                    return Err("knowledge_source_not_found".into());
                                }
                            }
                            let json = serde_json::to_vec(&collection).map_err(|_| "serialization_failed".to_string())?;
                            if !store::put_collection(database.connection(), &collection.id, collection.version, &collection.content_hash, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? {
                                return Err("knowledge_collection_stale_version".into());
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"collection_id":collection.id,"version":collection.version,"source_count":collection.source_ids.len(),"status":collection.status,"scope":collection.scope,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "collection_get" => {
                            let json = store::get_collection(database.connection(), &source_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "knowledge_collection_not_found".to_string())?;
                            let collection: knowledge::KnowledgeCollection = serde_json::from_slice(&json).map_err(|_| "corrupt_knowledge_collection".to_string())?;
                            knowledge::validate_collection(&collection, &policy).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"collection_id":collection.id,"version":collection.version,"source_count":collection.source_ids.len(),"status":collection.status,"scope":collection.scope,"content_hash":collection.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "collection_view" => {
                            let request: KnowledgeCollectionViewRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_collection_view".to_string())?;
                            let collection_json = store::get_collection(database.connection(), &source_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "knowledge_collection_not_found".to_string())?;
                            let collection: knowledge::KnowledgeCollection = serde_json::from_slice(&collection_json).map_err(|_| "corrupt_knowledge_collection".to_string())?;
                            let target_kind = request.target_kind;
                            let target_id = request.target_id.as_str();
                            let sources = collection.source_ids.iter().filter_map(|id| store::get_source(database.connection(), id).ok().flatten()).filter_map(|json| serde_json::from_slice::<knowledge::KnowledgeSource>(&json).ok()).collect::<Vec<_>>();
                            let mut bindings = Vec::new();
                            for id in &collection.source_ids {
                                bindings.extend(store::list_bindings(database.connection(), id, knowledge::MAX_BINDINGS_PER_SOURCE).map_err(|_| "storage_failed".to_string())?.into_iter().filter_map(|json| serde_json::from_slice::<knowledge::KnowledgeBinding>(&json).ok()));
                            }
                            let view = knowledge::build_collection_view(knowledge::BuildCollectionViewInput { collection: &collection, sources: &sources, bindings: &bindings, target_kind, target_id, max_sensitivity: knowledge::Sensitivity::Internal, expires_at_ms: None, policy: &policy }).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"collection_id":collection.id,"version":collection.version,"view_id":view.id,"source_ids":view.source_ids,"content_hash":view.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "register" => {
                            let source: knowledge::KnowledgeSource = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_source".to_string())?;
                            knowledge::validate_source(&source, &policy).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&source).map_err(|_| "serialization_failed".to_string())?;
                            store::put_source(database.connection(), &source.id, source.version, &source.content_hash, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":source.id,"version":source.version,"kind":source.kind,"status":source.status,"fingerprint":source.source_fingerprint,"sensitivity":source.sensitivity,"content_hash":source.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "get" => {
                            let json = store::get_source(database.connection(), &source_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "knowledge_source_not_found".to_string())?;
                            let source: knowledge::KnowledgeSource = serde_json::from_slice(&json).map_err(|_| "corrupt_knowledge_source".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":source.id,"version":source.version,"kind":source.kind,"status":source.status,"fingerprint":source.source_fingerprint,"sensitivity":source.sensitivity,"content_hash":source.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "bind" => {
                            let binding: knowledge::KnowledgeBinding = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_binding".to_string())?;
                            knowledge::validate_binding(&binding, &policy).map_err(|e| e.to_string())?;
                            if binding.source_id != source_id { return Err("knowledge_source_mismatch".into()); }
                            let binding_id = format!("{}:{}:{}", binding.target_kind as u8, binding.target_id, binding.source_id);
                            let json = serde_json::to_vec(&binding).map_err(|_| "serialization_failed".to_string())?;
                            store::put_binding(database.connection(), &binding_id, &binding.source_id, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":binding.source_id,"target_id":binding.target_id,"bound":true,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "index" => {
                            let chunks: Vec<knowledge::KnowledgeChunk> = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_chunks".to_string())?;
                            if chunks.len() > knowledge::MAX_CHUNKS_PER_SOURCE { return Err("knowledge_chunk_limit".into()); }
                            for chunk in &chunks {
                                if chunk.source_id != source_id || chunk.content_projection.len() > knowledge::MAX_CHUNK_BYTES { return Err("invalid_knowledge_chunk".into()); }
                                let json = serde_json::to_vec(chunk).map_err(|_| "serialization_failed".to_string())?;
                                store::put_chunk(database.connection(), store::PutChunkInput { id: &chunk.id, source_id: &chunk.source_id, revision: chunk.source_revision, ordinal: chunk.ordinal, locator: &chunk.locator, json: &json, now_ms: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed".to_string())?;
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":source_id,"indexed_chunks":chunks.len(),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "retrieve" => {
                            let request: KnowledgeQueryRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_query".to_string())?;
                            let query = request.query.as_str();
                            if query.is_empty() || query.len() > knowledge::MAX_ID_BYTES { return Err("invalid_knowledge_query".into()); }
                            let target_kind = request.target_kind;
                            let target_id = request.target_id.as_str();
                            let source_json = store::get_source(database.connection(), &source_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "knowledge_source_not_found".to_string())?;
                            let source: knowledge::KnowledgeSource = serde_json::from_slice(&source_json).map_err(|_| "corrupt_knowledge_source".to_string())?;
                            let bindings = store::list_bindings(database.connection(), &source_id, knowledge::MAX_BINDINGS_PER_SOURCE).map_err(|_| "storage_failed".to_string())?.into_iter().filter_map(|json| serde_json::from_slice(&json).ok()).collect::<Vec<knowledge::KnowledgeBinding>>();
                            let view = knowledge::build_view(knowledge::BuildViewInput { id: format!("view-{source_id}"), run_id: "runtime".into(), sources: std::slice::from_ref(&source), bindings: &bindings, target_kind, target_id, max_sensitivity: knowledge::Sensitivity::Internal, retrieval_profile: "keyword".into(), expires_at_ms: None, policy: &policy }).map_err(|e| e.to_string())?;
                            let mut hits = Vec::new();
                            for json in store::list_chunks(database.connection(), &source_id, knowledge::MAX_CHUNKS_PER_SOURCE).map_err(|_| "storage_failed".to_string())? {
                                let chunk: knowledge::KnowledgeChunk = serde_json::from_slice(&json).map_err(|_| "corrupt_knowledge_chunk".to_string())?;
                                if chunk.content_projection.to_ascii_lowercase().contains(&query.to_ascii_lowercase()) {
                                    let hit = knowledge::KnowledgeHit { source_id: chunk.source_id, source_revision: chunk.source_revision, chunk_id: chunk.id, locator: chunk.locator, excerpt: chunk.content_projection, score: 1, match_reasons: vec!["keyword".into()], freshness: if source.status == knowledge::SourceStatus::Ready { "current".into() } else { "stale".into() }, trust_class: source.trust_class.clone() };
                                    knowledge::validate_hit(&hit, &view, &policy).map_err(|e| e.to_string())?;
                                    hits.push(hit); if hits.len() >= knowledge::MAX_HITS { break; }
                                }
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":source_id,"view_id":view.id,"hit_count":hits.len(),"hits":hits,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_knowledge_registry_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::KnowledgeSourceRegistryProjectRole {
                source_id,
                operation,
                version: expected_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::DurableRemoteTaskBridge {
            operation,
            remote_task_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_task_id = remote_task_id.clone();
            let result = async {
                if idempotency_key.is_empty() {
                    return Err("invalid_remote_task_idempotency_key".to_string());
                }
                let journal = state
                    .lock()
                    .await
                    .journal
                    .clone()
                    .ok_or_else(|| "storage journal is not configured".to_string())?;
                let database = journal.database().lock().await;
                use crate::durable_remote_task_bridge as bridge;
                use evohime_local_storage::durable_remote_task_bridge_store as store;
                let policy = bridge::default_policy();
                match operation.as_str() {
                    "submit" => {
                        let request: RemoteTaskSubmitRequest = serde_json::from_slice(&payload)
                            .map_err(|_| "invalid_remote_task_submit".to_string())?;
                        let request_bytes = serde_json::to_vec(&request.request)
                            .map_err(|_| "invalid_remote_task_request".to_string())?;
                        let record = bridge::build_record(
                            remote_task_id.clone(),
                            &request.toolset,
                            request.operation,
                            &request_bytes,
                            request.provenance_ref,
                            crate::task_memory::now_millis() as i64,
                            &policy,
                        )
                        .map_err(|e| e.to_string())?;
                        let json = serde_json::to_vec(&record)
                            .map_err(|_| "serialization_failed".to_string())?;
                        if !store::put_record(
                            database.connection(),
                            &record.id,
                            record.version,
                            &format!("{:?}", record.status),
                            &record.content_hash,
                            &json,
                            record.updated_at_ms,
                        )
                        .map_err(|_| "storage_failed".to_string())?
                        {
                            return Err("remote_task_stale_version".into());
                        }
                        serde_json::to_vec(&bridge::status_projection(&record))
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    "status" => {
                        let json = store::get_record(database.connection(), &remote_task_id)
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "remote_task_not_found".to_string())?;
                        let record: bridge::RemoteTaskRecord = serde_json::from_slice(&json)
                            .map_err(|_| "corrupt_remote_task".to_string())?;
                        serde_json::to_vec(&bridge::status_projection(&record))
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    "cancel" | "poll" | "result" => {
                        let json = store::get_record(database.connection(), &remote_task_id)
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "remote_task_not_found".to_string())?;
                        let mut record: bridge::RemoteTaskRecord = serde_json::from_slice(&json)
                            .map_err(|_| "corrupt_remote_task".to_string())?;
                        if expected_version != 0 && expected_version != record.version {
                            return Err("remote_task_stale_version".into());
                        }
                        let now = crate::task_memory::now_millis() as i64;
                        match operation.as_str() {
                            "cancel" => {
                                let version = record.version;
                                bridge::cancel(&mut record, version, now)
                                    .map_err(|e| e.to_string())?;
                            }
                            "poll" => {
                                let request: RemoteTaskPollRequest =
                                    serde_json::from_slice(&payload)
                                        .map_err(|_| "invalid_remote_task_poll".to_string())?;
                                bridge::lease_for_poll(
                                    &mut record,
                                    &request.lease_owner,
                                    now,
                                    &policy,
                                )
                                .map_err(|e| e.to_string())?;
                            }
                            "result" => {
                                let request: RemoteTaskResultRequest =
                                    serde_json::from_slice(&payload)
                                        .map_err(|_| "invalid_remote_task_result".to_string())?;
                                let status = request.status;
                                if !matches!(
                                    status,
                                    bridge::RemoteTaskStatus::InputRequired
                                        | bridge::RemoteTaskStatus::Completed
                                        | bridge::RemoteTaskStatus::Failed
                                        | bridge::RemoteTaskStatus::Cancelled
                                        | bridge::RemoteTaskStatus::Unknown
                                ) {
                                    return Err("invalid_remote_task_transition".into());
                                }
                                record.status = status;
                                record.transport_status = request.transport_status;
                                record.result_artifact_ref = request.result_artifact_ref;
                                record.version += 1;
                                record.updated_at_ms = now;
                                bridge::validate_record(
                                    &record,
                                    &bridge::RemoteTaskToolset {
                                        schema_version: 1,
                                        id: record.toolset_id.clone(),
                                        version: 1,
                                        provider_kind: bridge::RemoteProviderKind::Mcp,
                                        provider_ref: "trusted-adapter".into(),
                                        operation_names: vec![record.operation.clone()],
                                        content_hash: "trusted".into(),
                                    },
                                    &policy,
                                )
                                .map_err(|e| e.to_string())?;
                            }
                            _ => unreachable!(),
                        }
                        bridge::refresh_content_hash(&mut record).map_err(|e| e.to_string())?;
                        let json = serde_json::to_vec(&record)
                            .map_err(|_| "serialization_failed".to_string())?;
                        store::put_record(
                            database.connection(),
                            &record.id,
                            record.version,
                            &format!("{:?}", record.status),
                            &record.content_hash,
                            &json,
                            record.updated_at_ms,
                        )
                        .map_err(|_| "storage_failed".to_string())?;
                        serde_json::to_vec(&bridge::status_projection(&record))
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    _ => Err("unsupported_remote_task_operation".into()),
                }
            }
            .await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::DurableRemoteTaskBridge {
                remote_task_id: event_task_id,
                operation: event_operation,
                version: expected_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::MessageInterventionPolicies {
            operation,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let result = async {
                if idempotency_key.is_empty() || idempotency_key.len() > 128 {
                    return Err("invalid_intervention_idempotency_key".into());
                }
                if expected_version > 1 {
                    return Err("intervention_stale_version".into());
                }
                let request: InterventionRequest = serde_json::from_slice(&payload)
                    .map_err(|_| "invalid_intervention_payload".to_string())?;
                let verdict = crate::message_intervention_policies::evaluate(
                    &request.policy,
                    &request.context,
                    request.seen,
                )
                .map_err(|e| e.to_string())?;
                serde_json::to_vec(&serde_json::json!({
                    "status": "evaluated",
                    "operation": operation,
                    "version": 1,
                    "verdict": verdict,
                    "redacted": true,
                }))
                .map_err(|_| "serialization_failed".to_string())
            }
            .await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::MessageInterventionPolicies {
                operation: event_operation,
                version: expected_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::BatchInvocationRuntime {
            operation,
            batch_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_id = batch_id.clone();
            let event_operation = operation.clone();
            let result = async {
                if idempotency_key.is_empty() {
                    return Err("invalid_batch_idempotency_key".into());
                }
                let journal = state
                    .lock()
                    .await
                    .journal
                    .clone()
                    .ok_or_else(|| "storage journal is not configured".to_string())?;
                let database = journal.database().lock().await;
                use crate::batch_invocation_runtime as batch;
                use evohime_local_storage::batch_invocation_runtime_store as store;
                let policy = batch::default_policy();
                match operation.as_str() {
                    "create" => {
                        let request: batch::CreateBatchRequest =
                            serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_batch_payload".to_string())?;
                        let value = batch::new_batch(batch::NewBatchInput {
                            id: batch_id.clone(),
                            definition_ref: request.definition_ref,
                            definition_version: request.definition_version,
                            inputs: request.inputs,
                            max_concurrency: request.max_concurrency,
                            failure_policy: request.failure_policy,
                            now_ms: crate::task_memory::now_millis() as i64,
                            policy: policy.clone(),
                        })
                        .map_err(|e| e.to_string())?;
                        let json = serde_json::to_vec(&value)
                            .map_err(|_| "serialization_failed".to_string())?;
                        if !store::put(
                            database.connection(),
                            &value.id,
                            value.version,
                            &format!("{:?}", value.status),
                            &value.content_hash,
                            &json,
                            value.updated_at_ms,
                        )
                        .map_err(|_| "storage_failed".to_string())?
                        {
                            return Err("batch_duplicate".into());
                        }
                        serde_json::to_vec(&batch::projection(&value))
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    "get" | "resume" | "start" => {
                        let (version, json) = store::get(database.connection(), &batch_id)
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "batch_not_found".to_string())?;
                        let mut value: batch::BatchInvocation = serde_json::from_slice(&json)
                            .map_err(|_| "corrupt_batch".to_string())?;
                        if operation == "resume" {
                            batch::resume_pending(
                                &mut value,
                                expected_version.max(version),
                                crate::task_memory::now_millis() as i64,
                                &policy,
                            )
                            .map_err(|e| e.to_string())?;
                        } else if operation == "start" {
                            batch::start_batch(
                                &mut value,
                                expected_version.max(version),
                                crate::task_memory::now_millis() as i64,
                                &policy,
                            )
                            .map_err(|e| e.to_string())?;
                        }
                        if operation != "get" {
                            let json = serde_json::to_vec(&value)
                                .map_err(|_| "serialization_failed".to_string())?;
                            if !store::put(
                                database.connection(),
                                &value.id,
                                value.version,
                                &format!("{:?}", value.status),
                                &value.content_hash,
                                &json,
                                value.updated_at_ms,
                            )
                            .map_err(|_| "storage_failed".to_string())?
                            {
                                return Err("batch_stale_version".into());
                            }
                        }
                        serde_json::to_vec(&batch::projection(&value))
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    "result" => {
                        let (version, json) = store::get(database.connection(), &batch_id)
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "batch_not_found".to_string())?;
                        let mut value: batch::BatchInvocation = serde_json::from_slice(&json)
                            .map_err(|_| "corrupt_batch".to_string())?;
                        let request: batch::RecordResultRequest = serde_json::from_slice(&payload)
                            .map_err(|_| "invalid_batch_result".to_string())?;
                        batch::record_result(batch::RecordResultInput {
                            batch: &mut value,
                            item_id: &request.item_id,
                            expected_version: expected_version.max(version),
                            status: request.status,
                            result_ref: request.result_ref,
                            error_class: request.error_class,
                            now_ms: crate::task_memory::now_millis() as i64,
                            policy: &policy,
                        })
                        .map_err(|e| e.to_string())?;
                        let json = serde_json::to_vec(&value)
                            .map_err(|_| "serialization_failed".to_string())?;
                        if !store::put(
                            database.connection(),
                            &value.id,
                            value.version,
                            &format!("{:?}", value.status),
                            &value.content_hash,
                            &json,
                            value.updated_at_ms,
                        )
                        .map_err(|_| "storage_failed".to_string())?
                        {
                            return Err("batch_stale_version".into());
                        }
                        serde_json::to_vec(&batch::projection(&value))
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    _ => Err("unsupported_batch_operation".into()),
                }
            }
            .await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::BatchInvocationRuntime {
                batch_id: event_id,
                operation: event_operation,
                version: expected_version,
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

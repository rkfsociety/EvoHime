use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::GuidedCalibrationSessions {
            operation,
            session_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_session_id = session_id.clone();
            let result = async {
                    use crate::guided_calibration_sessions as v;
                    use evohime_local_storage::guided_calibration_sessions_store as store;
                    if session_id.is_empty() || idempotency_key.is_empty() || idempotency_key.len() > 128 { return Err("invalid_calibration_request".into()); }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?; let db = journal.database().lock().await;
                    let request: CalibrationRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_calibration_payload".to_string())?;
                    match operation.as_str() {
                        "create" => { let owner=request.owner_scope.as_deref().ok_or_else(||"owner_scope_required".to_string())?; let subject=request.subject_ref.as_deref().ok_or_else(||"subject_ref_required".to_string())?; let actor=request.actor_ref.as_deref().ok_or_else(||"actor_ref_required".to_string())?; let policy=request.policy_snapshot_hash.as_deref().ok_or_else(||"policy_snapshot_hash_required".to_string())?; let s=v::new_session(session_id.clone(),owner.into(),subject.into(),actor.into(),policy.into()); v::validate_session(&s).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&s).map_err(|_|"serialization_failed".to_string())?; if !store::save(db.connection(),store::SaveInput{id:&session_id,expected:expected_version,revision:s.revision,json:&json,dataset_hash:&s.dataset_hash,idempotency_key:&idempotency_key,now:crate::task_memory::now_millis() as i64}).map_err(|_|"storage_failed".to_string())? {return Err("stale_version_or_idempotency_conflict".into())}; serde_json::to_vec(&serde_json::json!({"status":"created","session_id":session_id,"revision":s.revision,"dataset_hash":s.dataset_hash,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "inspect" | "replay" => { let (revision,json,dataset)=store::load(db.connection(),&session_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"session_not_found".to_string())?; let s:v::CalibrationSession=serde_json::from_slice(&json).map_err(|_|"corrupt_calibration_session".to_string())?; v::validate_session(&s).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"available","session_id":session_id,"revision":revision,"iteration_count":s.iterations.len(),"candidate_count":s.candidates.len(),"dataset_hash":dataset,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "iteration" => { let (revision, json, _)=store::load(db.connection(),&session_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"session_not_found".to_string())?; let mut s:v::CalibrationSession=serde_json::from_slice(&json).map_err(|_|"corrupt_calibration_session".to_string())?; let i=request.iteration.ok_or_else(||"iteration_required".to_string())?; v::add_iteration(&mut s,i).map_err(|e|e.to_string())?; let out=serde_json::to_vec(&s).map_err(|_|"serialization_failed".to_string())?; if !store::save(db.connection(),store::SaveInput{id:&session_id,expected:expected_version.max(revision),revision:s.revision,json:&out,dataset_hash:&s.dataset_hash,idempotency_key:&idempotency_key,now:crate::task_memory::now_millis() as i64}).map_err(|_|"storage_failed".to_string())? {return Err("stale_version_or_idempotency_conflict".into())}; serde_json::to_vec(&serde_json::json!({"status":"iteration_recorded","session_id":session_id,"revision":s.revision,"iteration_count":s.iterations.len(),"dataset_hash":s.dataset_hash,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "consolidate" => { let (revision,json,_)=store::load(db.connection(),&session_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"session_not_found".to_string())?; let mut s:v::CalibrationSession=serde_json::from_slice(&json).map_err(|_|"corrupt_calibration_session".to_string())?; let pattern=request.pattern_key.as_deref().ok_or_else(||"pattern_key_required".to_string())?; let guidance=request.guidance_text.as_deref().ok_or_else(||"guidance_required".to_string())?; let candidate_id=request.candidate_id.as_deref().ok_or_else(||"candidate_id_required".to_string())?; let c=v::consolidate(&s,candidate_id,pattern,guidance).map_err(|e|e.to_string())?; let source=s.iterations.iter().filter(|i|c.source_iteration_ids.contains(&i.iteration_id)).collect::<Vec<_>>(); let evidence=source.iter().filter_map(|i|i.feedback.as_ref().map(|f|crate::refinement::EvidenceRefV1{source_id:f.provenance_ref.clone(),source_kind:"calibration_feedback".into(),owner_scope:crate::refinement::OwnerScope::Session,content_hash:f.correction_hash.clone(),observed_at_ms:crate::task_memory::now_millis() as i64,redacted:true})).collect::<Vec<_>>(); let task_ids=source.iter().map(|i|i.task_ref.clone()).collect::<Vec<_>>(); let rc=crate::refinement::RefinementCandidateV1::new(crate::refinement::RefinementCandidateInput{id:c.refinement_candidate_id.clone(),kind:crate::refinement::CandidateKind::Memory,target:"session_guidance".into(),scope:crate::refinement::OwnerScope::Session,pattern_key:pattern.into(),title:"guided calibration guidance".into(),rationale:"human-confirmed repeated feedback".into(),proposed_content:guidance.into(),source_task_ids:task_ids,evidence,policy_snapshot_hash:s.policy_snapshot_hash.clone(),idempotency_key:idempotency_key.clone()}).map_err(|e|e.to_string())?; crate::refinement::RefinementService::new(db.connection(),crate::refinement::AdmissionPolicy::default()).propose_memory(rc,crate::task_memory::now_millis() as i64)?; s.candidates.push(c.clone()); s.revision=revision.saturating_add(1); s.dataset_hash=v::dataset_hash(&s).map_err(|e|e.to_string())?; let out=serde_json::to_vec(&s).map_err(|_|"serialization_failed".to_string())?; if !store::save(db.connection(),store::SaveInput{id:&session_id,expected:revision,revision:s.revision,json:&out,dataset_hash:&s.dataset_hash,idempotency_key:&idempotency_key,now:crate::task_memory::now_millis() as i64}).map_err(|_|"storage_failed".to_string())? {return Err("stale_version_or_idempotency_conflict".into())}; serde_json::to_vec(&serde_json::json!({"status":"candidate_proposed_for_refinement","session_id":session_id,"candidate_id":c.candidate_id,"guidance_hash":c.guidance_hash,"refinement_candidate_id":c.refinement_candidate_id,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "close" => { let (revision,json,_)=store::load(db.connection(),&session_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"session_not_found".to_string())?; let mut s:v::CalibrationSession=serde_json::from_slice(&json).map_err(|_|"corrupt_calibration_session".to_string())?; s.status=if request.cancelled{v::SessionStatus::Cancelled}else{v::SessionStatus::Completed}; s.revision=revision.saturating_add(1); let out=serde_json::to_vec(&s).map_err(|_|"serialization_failed".to_string())?; if !store::save(db.connection(),store::SaveInput{id:&session_id,expected:revision,revision:s.revision,json:&out,dataset_hash:&s.dataset_hash,idempotency_key:&idempotency_key,now:crate::task_memory::now_millis() as i64}).map_err(|_|"storage_failed".to_string())? {return Err("stale_version_or_idempotency_conflict".into())}; serde_json::to_vec(&serde_json::json!({"status":"closed","session_id":session_id,"revision":s.revision,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        _ => Err("unsupported_calibration_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::GuidedCalibrationSessions {
                operation: event_operation,
                session_id: event_session_id,
                version: expected_version.saturating_add(1),
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            let _ = state.lock().await.events.send(event);
            let _ = reply.send(result);
        }
        CoreCommand::ExtensionConformanceKit {
            operation,
            subject_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_subject_id = subject_id.clone();
            let result=async { use crate::extension_conformance_kit as v; if subject_id.is_empty()||idempotency_key.is_empty()||expected_version>u64::MAX-1{return Err("invalid_conformance_request".into())}; let value:serde_json::Value=serde_json::from_slice(&payload).map_err(|_|"invalid_conformance_payload".to_string())?; match operation.as_str(){"run"=>{let d:v::ExtensionDescriptor=serde_json::from_value(value.get("descriptor").cloned().ok_or_else(||"descriptor_required".to_string())?).map_err(|_|"invalid_descriptor".to_string())?;let p:v::ConformanceProbe=serde_json::from_value(value.get("probe").cloned().ok_or_else(||"probe_required".to_string())?).map_err(|_|"invalid_probe".to_string())?;let fault:v::FaultMode=serde_json::from_value(value.get("fault").cloned().unwrap_or(serde_json::json!("none"))).map_err(|_|"invalid_fault".to_string())?;if d.subject_id!=subject_id{return Err("subject_id_mismatch".into())};let report=v::run(&d,&p,fault).map_err(|e|e.to_string())?;serde_json::to_vec(&report).map_err(|_|"serialization_failed".into())},"register"=>{let descriptors:Vec<v::ExtensionDescriptor>=serde_json::from_value(value.get("descriptors").cloned().ok_or_else(||"descriptors_required".to_string())?).map_err(|_|"invalid_descriptors".to_string())?;let fault:v::FaultMode=serde_json::from_value(value.get("fault").cloned().unwrap_or(serde_json::json!("none"))).map_err(|_|"invalid_fault".to_string())?;let mut t=v::RegistrationTransaction::default();for d in descriptors{t.stage(d).map_err(|e|e.to_string())?};let committed=t.commit(fault).map_err(|e|e.to_string())?;serde_json::to_vec(&serde_json::json!({"status":"registered_ephemeral","count":committed.len(),"redacted":true})).map_err(|_|"serialization_failed".into())},"inspect"=>serde_json::to_vec(&serde_json::json!({"status":"available","schema_version":v::SCHEMA_VERSION,"kinds":["integration_provider","external_agent_adapter","workbench","ui_extension","declarative_component_provider"],"production_execution":false,"redacted":true})).map_err(|_|"serialization_failed".into()),_=>Err("unsupported_conformance_operation".into())}}.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::ExtensionConformanceKit {
                operation: event_operation,
                subject_id: event_subject_id,
                version: expected_version.saturating_add(1),
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            let _ = state.lock().await.events.send(event);
            let _ = reply.send(result);
        }
        CoreCommand::PersistentAgentOrganizationRegistry {
            operation,
            agent_id,
            owner_scope,
            actor,
            payload,
            expected_revision,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_agent_id = agent_id.clone();
            let result = async {
                let journal = state
                    .lock()
                    .await
                    .journal
                    .clone()
                    .ok_or_else(|| "storage journal is not configured".to_string())?;
                journal
                    .persistent_agent_registry_command(
                        crate::persistent_agent_registry::RegistryCommand {
                            operation,
                            agent_id,
                            owner_scope,
                            actor,
                            payload,
                            expected_revision,
                            idempotency_key,
                        },
                    )
                    .await
                    .map_err(|error| error.to_string())
            }
            .await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::PersistentAgentOrganizationRegistry {
                agent_id: event_agent_id,
                operation: event_operation,
                revision: expected_revision,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            let _ = state.lock().await.events.send(event);
            let _ = reply.send(result);
        }
        CoreCommand::GetMemory { id, reply } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let record = journal
                    .get_memory(&id)
                    .await?
                    .ok_or_else(|| "memory record was not found".to_string())?;
                let chain = journal.memory_supersession_chain(&id, 32).await?;
                let body = memory_record_body_json(&record)?;
                serde_json::to_vec(&serde_json::json!({
                    "record": body,
                    "supersession_chain": chain,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ListMemoryPending {
            scope_kind,
            project_id,
            secondary_id,
            limit,
            workspace_path,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let store_scope = memory_store_scope(&scope_kind)?;
                let scope_id = memory_scope_id(&workspace_path, &project_id, &secondary_id);
                // Expiry is applied before reading so an expired record is
                // never reported as still awaiting a decision.
                journal
                    .expire_due_memory(&memory_now_ms().to_string())
                    .await?;
                let pending = journal
                    .list_memory_by_state(
                        store_scope,
                        &scope_id,
                        crate::memory_extraction::ConfirmationState::PendingConfirmation.as_str(),
                        limit,
                    )
                    .await?;
                let mut counts = journal
                    .count_memory_by_state(store_scope, &scope_id)
                    .await?
                    .into_iter()
                    .collect::<std::collections::BTreeMap<String, i64>>();
                let mut pending = pending;
                // Услышанное живёт в своём scope: речь у стола не
                // принадлежит рабочему каталогу. Но очередь подтверждения
                // у пользователя одна, и прятать ambient-кандидатов от
                // неё значило бы, что подтвердить их негде.
                let ambient_scope = evohime_local_storage::memory_store::MemoryScope::Workspace;
                if !(store_scope == ambient_scope && scope_id == AMBIENT_MEMORY_SCOPE_ID) {
                    pending.extend(
                        journal
                            .list_memory_by_state(
                                ambient_scope,
                                AMBIENT_MEMORY_SCOPE_ID,
                                crate::memory_extraction::ConfirmationState::PendingConfirmation
                                    .as_str(),
                                limit,
                            )
                            .await?,
                    );
                    for (state, count) in journal
                        .count_memory_by_state(ambient_scope, AMBIENT_MEMORY_SCOPE_ID)
                        .await?
                    {
                        *counts.entry(state).or_insert(0) += count;
                    }
                }
                let counts = counts
                    .into_iter()
                    .map(|(state, count)| (state, serde_json::json!(count)))
                    .collect::<serde_json::Map<_, _>>();
                let records = pending
                    .iter()
                    .map(memory_record_to_json)
                    .collect::<Result<Vec<_>, _>>()?;
                serde_json::to_vec(&serde_json::json!({
                    "records": records,
                    "counts": counts,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::GetMemoryConflicts {
            scope_kind,
            project_id,
            secondary_id,
            limit,
            workspace_path,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let store_scope = memory_store_scope(&scope_kind)?;
                let scope_id = memory_scope_id(&workspace_path, &project_id, &secondary_id);
                let pending = journal
                    .list_memory_by_state(
                        store_scope,
                        &scope_id,
                        crate::memory_extraction::ConfirmationState::PendingConfirmation.as_str(),
                        limit,
                    )
                    .await?;
                let mut conflicts = Vec::new();
                for candidate in &pending {
                    let active = journal
                        .memory_conflict_candidates(
                            store_scope,
                            &scope_id,
                            &candidate.extraction.kind,
                            100,
                        )
                        .await?;
                    let Some(existing) = memory_conflicting_record(candidate, &active) else {
                        continue;
                    };
                    let chain = journal.memory_supersession_chain(&existing.id, 32).await?;
                    conflicts.push(serde_json::json!({
                        "pending": memory_record_to_json(candidate)?,
                        "active": memory_record_to_json(existing)?,
                        "conflict_key": format!(
                            "{}|{}|{}",
                            candidate.extraction.kind,
                            memory_conflict_subject(candidate),
                            candidate.scope.as_str()
                        ),
                        "supersession_chain": chain,
                    }));
                }
                serde_json::to_vec(&serde_json::json!({ "conflicts": conflicts }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ConfirmMemory {
            ids,
            approval_id,
            idempotency_key,
            reply,
        } => {
            let result = TaskCoordinator::apply_memory_decision(
                &state,
                ids,
                approval_id,
                idempotency_key,
                crate::memory_api::MemoryOperation::Confirm,
                crate::memory_extraction::ConfirmationState::Confirmed,
                "memory.confirmed",
            )
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::RejectMemory {
            ids,
            approval_id,
            idempotency_key,
            reply,
        } => {
            let result = TaskCoordinator::apply_memory_decision(
                &state,
                ids,
                approval_id,
                idempotency_key,
                crate::memory_api::MemoryOperation::Reject,
                crate::memory_extraction::ConfirmationState::Rejected,
                "memory.rejected",
            )
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ReviseMemoryCandidate {
            id,
            statement,
            session_only,
            session_id,
            approval_id,
            idempotency_key,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                crate::memory_api::Approval::new(
                    approval_id.clone(),
                    crate::memory_api::MemoryOperation::Update,
                )
                .map_err(|error| error.to_string())?;
                validate_memory_idempotency_key(&idempotency_key)?;
                let record = journal
                    .get_memory(&id)
                    .await?
                    .ok_or_else(|| "memory record was not found".to_string())?;
                let statement = if statement.trim().is_empty() {
                    record.content.clone()
                } else {
                    statement
                };

                if session_only {
                    // "Только на эту сессию": no persistent row survives.
                    // The candidate is rejected outright and the statement
                    // lives on solely as a session note that expires by
                    // itself, so it can never reach long-term retrieval.
                    if session_id.trim().is_empty() {
                        return Err("session_id is required for a session-only note".to_string());
                    }
                    let now_ms = memory_now_ms();
                    let expires_at =
                        now_ms.saturating_add(crate::memory_extraction::SESSION_SUMMARY_GRACE_MS);
                    journal
                        .save_memory_session_note(SessionMemoryNote {
                            id: &uuid::Uuid::new_v4().to_string(),
                            session_id: &session_id,
                            scope: record.scope,
                            scope_id: &record.scope_id,
                            kind: &record.extraction.kind,
                            statement: &statement,
                            created_at: &now_ms.to_string(),
                            expires_at: &expires_at.to_string(),
                        })
                        .await?;
                    let actual = journal
                        .transition_memory_state(
                            &id,
                            crate::memory_extraction::ConfirmationState::Rejected.as_str(),
                        )
                        .await?;
                    TaskCoordinator::record_audit(
                        &state,
                        crate::audit::AuditKind::Approval,
                        id.clone(),
                        "memory.session_only",
                        [
                            ("memory_id".to_owned(), id.clone()),
                            ("session_id".to_owned(), session_id.clone()),
                            ("approval_id".to_owned(), approval_id),
                            ("idempotency_key".to_owned(), idempotency_key),
                        ],
                    )
                    .await;
                    return serde_json::to_vec(&serde_json::json!({
                        "id": id,
                        "state": actual,
                        "session_only": true,
                        "expires_at_ms": expires_at,
                    }))
                    .map_err(|error| error.to_string());
                }

                journal.revise_pending_memory(&id, &statement).await?;
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Approval,
                    id.clone(),
                    "memory.revised",
                    [
                        ("memory_id".to_owned(), id.clone()),
                        ("approval_id".to_owned(), approval_id),
                        ("idempotency_key".to_owned(), idempotency_key),
                    ],
                )
                .await;
                let revised = journal
                    .get_memory(&id)
                    .await?
                    .ok_or_else(|| "memory record was not found".to_string())?;
                serde_json::to_vec(&serde_json::json!({
                    "record": memory_record_to_json(&revised)?,
                    "session_only": false,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::SupersedeMemory {
            old_id,
            new_id,
            reason,
            approval_id,
            idempotency_key,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                crate::memory_api::Approval::new(
                    approval_id.clone(),
                    crate::memory_api::MemoryOperation::Supersede,
                )
                .map_err(|error| error.to_string())?;
                validate_memory_idempotency_key(&idempotency_key)?;
                // The reason is a bounded enum, not free text: the chain
                // has to explain itself without carrying user content.
                let reason = crate::memory_extraction::SupersessionReason::parse(&reason)
                    .ok_or_else(|| format!("unsupported supersession reason: {reason}"))?;
                journal
                    .supersede_memory(&old_id, &new_id, reason.as_str())
                    .await?;
                let chain = journal.memory_supersession_chain(&new_id, 32).await?;
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Approval,
                    new_id.clone(),
                    "memory.superseded",
                    [
                        ("old_memory_id".to_owned(), old_id.clone()),
                        ("new_memory_id".to_owned(), new_id.clone()),
                        ("reason".to_owned(), reason.as_str().to_owned()),
                        ("approval_id".to_owned(), approval_id),
                        ("idempotency_key".to_owned(), idempotency_key),
                    ],
                )
                .await;
                serde_json::to_vec(&serde_json::json!({
                    "old_id": old_id,
                    "new_id": new_id,
                    "reason": reason.as_str(),
                    "supersession_chain": chain,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}

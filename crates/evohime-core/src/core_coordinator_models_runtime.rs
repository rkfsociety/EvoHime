use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::ModelPurposeRouting {
            operation,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let result = async {
                    if idempotency_key.is_empty() { return Err("invalid_model_purpose_idempotency_key".into()); }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use evohime_local_storage::model_purpose_routing_store as store;
                    match operation.as_str() {
                        "get" => {
                            let stored = store::get(database.connection(), crate::model_purpose_routing::CONTRACT_ID).map_err(|_| "storage_failed".to_string())?;
                            let (version, hash, policy) = stored
                                .and_then(|(version, hash, json)| serde_json::from_slice(&json).ok().map(|policy| (version, hash, policy)))
                                .unwrap_or_else(|| { let policy = crate::model_purpose_routing::builtin_policy(); let hash = policy.canonical_hash().unwrap_or_default(); (policy.version, hash, policy) });
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"policy_id":crate::model_purpose_routing::CONTRACT_ID,"version":version,"content_hash":hash,"routes":policy.routes,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "put" => {
                            let policy: crate::model_purpose_routing::ModelPurposeRoutingPolicy = serde_json::from_slice(&payload).map_err(|_| "invalid_model_purpose_policy".to_string())?;
                            policy.validate().map_err(|e| e.to_string())?;
                            if policy.version != expected_version.saturating_add(1) && expected_version != 0 { return Err("stale_model_purpose_policy".into()); }
                            let json = serde_json::to_vec(&policy).map_err(|_| "serialization_failed".to_string())?;
                            let hash = policy.canonical_hash().map_err(|e| e.to_string())?;
                            if !store::put(database.connection(), &policy.policy_id, policy.version, &hash, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? { return Err("stale_model_purpose_policy".into()); }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"policy_id":policy.policy_id,"version":policy.version,"content_hash":hash,"route_count":policy.routes.len(),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_model_purpose_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event_version = serde_json::from_str::<serde_json::Value>(&projection_json)
                .ok()
                .and_then(|v| v.get("version").and_then(serde_json::Value::as_u64))
                .unwrap_or(expected_version);
            let event = CoreEvent::ModelPurposeRouting {
                operation: operation.clone(),
                version: event_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::CodeAnchoredIntentMarkers {
            operation,
            file_path,
            revision,
            payload,
            idempotency_key,
            reply,
        } => {
            let result = async {
                    if idempotency_key.is_empty() { return Err("invalid_marker_idempotency_key".into()); }
                    let ranges: Vec<crate::code_anchored_intent_markers::CommentRange> = serde_json::from_slice(&payload).map_err(|_| "invalid_comment_ranges".to_string())?;
                    if operation != "scan" && operation != "propose" { return Err("unsupported_marker_operation".into()); }
                    let provenance = if operation == "scan" { crate::code_anchored_intent_markers::Provenance::ExistingRepository } else { crate::code_anchored_intent_markers::Provenance::UserTrusted };
                    let mut markers = crate::code_anchored_intent_markers::parse_comment_ranges(&file_path, &revision, &ranges, provenance).map_err(|e| e.to_string())?;
                    crate::code_anchored_intent_markers::deduplicate(&mut markers);
                    if operation == "scan" {
                        state.lock().await.marker_gate.admit_scan(&mut markers, crate::task_memory::now_millis());
                    }
                    if operation == "propose" {
                        let marker = markers.first_mut().ok_or_else(|| "marker_not_found".to_string())?;
                        crate::code_anchored_intent_markers::can_auto_propose(marker).map_err(|e| e.to_string())?;
                        let task_id = format!("code-intent-{}", marker.marker_id);
                        let prompt = format!("Code intent at {}:{}-{}: {}", marker.file_path, marker.range_start, marker.range_end, marker.text);
                        marker.status = crate::code_anchored_intent_markers::MarkerStatus::Proposed;
                        let command_tx = state.lock().await.command_tx.clone();
                        command_tx.send(CoreCommand::StartTask { task_id: task_id.clone(), prompt, workspace_root: None, preferred_route_hint: None }).await.map_err(|_| "task_queue_closed".to_string())?;
                        return serde_json::to_vec(&serde_json::json!({"status":"task_started","task_id":task_id,"marker_id":marker.marker_id,"redacted":true})).map_err(|_| "serialization_failed".to_string());
                    }
                    serde_json::to_vec(&serde_json::json!({"status":"candidates","count":markers.len(),"markers":markers.iter().map(|m|serde_json::json!({"marker_id":m.marker_id,"kind":m.kind,"range_start":m.range_start,"range_end":m.range_end,"revision":m.revision,"provenance":m.provenance})).collect::<Vec<_>>(),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::CodeAnchoredIntentMarkers {
                operation: operation.clone(),
                version: 1,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::AgentGitChangeSets {
            operation,
            change_set_id,
            workspace_root,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_change_set_id = change_set_id.clone();
            let result = async {
                if idempotency_key.is_empty() {
                    return Err("invalid_agent_git_idempotency_key".to_string());
                }
                if idempotency_key.len() > git_sets::MAX_REFERENCE_BYTES
                    || change_set_id.len() > git_sets::MAX_REFERENCE_BYTES
                    || idempotency_key
                        .chars()
                        .any(|character| character.is_control())
                    || change_set_id
                        .chars()
                        .any(|character| character.is_control())
                {
                    return Err("agent_git_reference_limit_exceeded".into());
                }
                let journal = state
                    .lock()
                    .await
                    .journal
                    .clone()
                    .ok_or_else(|| "storage journal is not configured".to_string())?;
                use crate::agent_git_change_sets as git_sets;
                use evohime_local_storage::agent_git_change_sets_store as store;
                {
                    let database = journal.database().lock().await;
                    if let Some(response) =
                        store::get_idempotent(database.connection(), &idempotency_key)
                            .map_err(|_| "storage_failed".to_string())?
                    {
                        return Ok(response);
                    }
                }
                let now = crate::task_memory::now_millis() as i64;
                let response = match operation.as_str() {
                    "observe" => {
                        let set = git_sets::observe(&payload, &change_set_id, &workspace_root, now)
                            .await
                            .map_err(|error| error.to_string())?;
                        validate_agent_git_integrations(&journal, &set).await?;
                        let json = serde_json::to_vec(&set)
                            .map_err(|_| "serialization_failed".to_string())?;
                        let database = journal.database().lock().await;
                        if !store::put_change_set(
                            database.connection(),
                            &set.id,
                            set.version,
                            &set.content_hash,
                            &json,
                            set.created_at_ms,
                        )
                        .map_err(|_| "storage_failed".to_string())?
                        {
                            return Err("agent_git_change_set_already_exists".into());
                        }
                        serde_json::to_vec(&serde_json::json!({
                            "schema_version": 1,
                            "change_set_id": set.id,
                            "revision": set.revision,
                            "status": "observed",
                            "path_count": set.paths.len(),
                            "baseline_fingerprint": set.base_dirty_fingerprint,
                            "redacted": true
                        }))
                        .map_err(|_| "serialization_failed".to_string())?
                    }
                    "candidate" => {
                        let set = load_change_set(&journal, &change_set_id).await?;
                        validate_agent_git_integrations(&journal, &set).await?;
                        if expected_version != 0 && expected_version != set.revision {
                            return Err("agent_git_change_set_stale_version".into());
                        }
                        let (mut next, candidate) = git_sets::make_candidate(&set, &payload, now)
                            .await
                            .map_err(|error| error.to_string())?;
                        next.revision = set.revision.saturating_add(1);
                        let mut candidate = candidate;
                        candidate.revision = next.revision;
                        next.content_hash = recompute_content_hash(&next)?;
                        let set_json = serde_json::to_vec(&next)
                            .map_err(|_| "serialization_failed".to_string())?;
                        let candidate_json = serde_json::to_vec(&candidate)
                            .map_err(|_| "serialization_failed".to_string())?;
                        let database = journal.database().lock().await;
                        if !store::update_change_set(
                            database.connection(),
                            &set.id,
                            set.revision,
                            next.version,
                            &next.content_hash,
                            &set_json,
                            now,
                        )
                        .map_err(|_| "storage_failed".to_string())?
                        {
                            return Err("agent_git_change_set_stale_version".into());
                        }
                        store::put_candidate(
                            database.connection(),
                            &candidate.id,
                            &set.id,
                            &candidate.diff_hash,
                            &candidate_json,
                            candidate.created_at_ms,
                        )
                        .map_err(|_| "storage_failed".to_string())?;
                        bounded_projection(serde_json::json!({
                            "schema_version": 1,
                            "change_set_id": next.id,
                            "candidate_id": candidate.id,
                            "revision": next.revision,
                            "status": "candidate_ready",
                            "included_paths": candidate.included_paths,
                            "excluded_paths": candidate.excluded_paths,
                            "diff_hash": candidate.diff_hash,
                            "verification_status": candidate.verification_status,
                            "redacted": true
                        }))?
                    }
                    "get_candidate" => {
                        let candidate = load_candidate(&journal, &change_set_id).await?;
                        candidate_projection(&candidate)?
                    }
                    "keep" => {
                        let candidate = load_candidate(&journal, &change_set_id).await?;
                        let mut set = load_change_set(&journal, &candidate.change_set_ref).await?;
                        validate_agent_git_integrations(&journal, &set).await?;
                        if candidate.revision != set.revision {
                            return Err("agent_git_change_set_stale_version".into());
                        }
                        if expected_version != 0 && expected_version != set.revision {
                            return Err("agent_git_change_set_stale_version".into());
                        }
                        set.status = git_sets::ChangeSetStatus::Kept;
                        let old_revision = set.revision;
                        set.revision = set.revision.saturating_add(1);
                        set.content_hash = recompute_content_hash(&set)?;
                        let mut candidate = candidate;
                        candidate.verification_status = "kept".into();
                        let set_json = serde_json::to_vec(&set)
                            .map_err(|_| "serialization_failed".to_string())?;
                        let database = journal.database().lock().await;
                        if !store::update_change_set(
                            database.connection(),
                            &set.id,
                            old_revision,
                            set.version,
                            &set.content_hash,
                            &set_json,
                            now,
                        )
                        .map_err(|_| "storage_failed".to_string())?
                        {
                            return Err("agent_git_change_set_stale_version".into());
                        }
                        candidate.revision = set.revision;
                        let candidate_json = serde_json::to_vec(&candidate)
                            .map_err(|_| "serialization_failed".to_string())?;
                        store::update_candidate(
                            database.connection(),
                            &candidate.id,
                            &candidate.diff_hash,
                            &candidate_json,
                            now,
                        )
                        .map_err(|_| "storage_failed".to_string())?;
                        candidate_projection(&candidate)?
                    }
                    "commit" => {
                        let candidate = load_candidate(&journal, &change_set_id).await?;
                        if candidate.verification_status == "commit_pending" {
                            return Err("commit_outcome_unknown".into());
                        }
                        if candidate.commit_id.is_some() {
                            candidate_projection(&candidate)?
                        } else {
                            let set = load_change_set(&journal, &candidate.change_set_ref).await?;
                            validate_agent_git_integrations(&journal, &set).await?;
                            if candidate.revision != set.revision {
                                return Err("agent_git_change_set_stale_version".into());
                            }
                            if expected_version != 0 && expected_version != set.revision {
                                return Err("agent_git_change_set_stale_version".into());
                            }
                            git_sets::preflight_candidate(&candidate)
                                .await
                                .map_err(|error| error.to_string())?;
                            let mut pending_candidate = candidate.clone();
                            pending_candidate.verification_status = "commit_pending".into();
                            let pending_json = serde_json::to_vec(&pending_candidate)
                                .map_err(|_| "serialization_failed".to_string())?;
                            {
                                let database = journal.database().lock().await;
                                if !store::update_candidate(
                                    database.connection(),
                                    &pending_candidate.id,
                                    &pending_candidate.diff_hash,
                                    &pending_json,
                                    now,
                                )
                                .map_err(|_| "storage_failed".to_string())?
                                {
                                    return Err("storage_failed".into());
                                }
                            }
                            let commit_id = git_sets::commit_candidate(&candidate)
                                .await
                                .map_err(|_| "commit_outcome_unknown".to_string())?;
                            let mut committed_candidate = candidate;
                            committed_candidate.commit_id = Some(commit_id.clone());
                            committed_candidate.verification_status = "committed".into();
                            let mut committed_set = set;
                            committed_set.status = git_sets::ChangeSetStatus::Committed;
                            let old_revision = committed_set.revision;
                            committed_set.revision = committed_set.revision.saturating_add(1);
                            committed_candidate.revision = committed_set.revision;
                            committed_set.content_hash = recompute_content_hash(&committed_set)?;
                            let set_json = serde_json::to_vec(&committed_set)
                                .map_err(|_| "serialization_failed".to_string())?;
                            let candidate_json = serde_json::to_vec(&committed_candidate)
                                .map_err(|_| "serialization_failed".to_string())?;
                            let database = journal.database().lock().await;
                            if !store::update_change_set(
                                database.connection(),
                                &committed_set.id,
                                old_revision,
                                committed_set.version,
                                &committed_set.content_hash,
                                &set_json,
                                now,
                            )
                            .map_err(|_| "storage_failed".to_string())?
                            {
                                return Err("commit_outcome_unknown".into());
                            }
                            store::update_candidate(
                                database.connection(),
                                &committed_candidate.id,
                                &committed_candidate.diff_hash,
                                &candidate_json,
                                now,
                            )
                            .map_err(|_| "commit_outcome_unknown".to_string())?;
                            candidate_projection(&committed_candidate)?
                        }
                    }
                    "undo" => {
                        let candidate = load_candidate(&journal, &change_set_id).await?;
                        if candidate.verification_status == "commit_pending" {
                            return Err("commit_outcome_unknown".into());
                        }
                        let set = load_change_set(&journal, &candidate.change_set_ref).await?;
                        validate_agent_git_integrations(&journal, &set).await?;
                        if candidate.revision != set.revision {
                            return Err("agent_git_change_set_stale_version".into());
                        }
                        if expected_version != 0 && expected_version != set.revision {
                            return Err("agent_git_change_set_stale_version".into());
                        }
                        let head = git_sets::undo_candidate(&candidate, &set.baseline)
                            .await
                            .map_err(|error| error.to_string())?;
                        let mut undone_candidate = candidate;
                        undone_candidate.verification_status = "undone".into();
                        let mut undone_set = set;
                        undone_set.status = git_sets::ChangeSetStatus::Kept;
                        let old_revision = undone_set.revision;
                        undone_set.revision = undone_set.revision.saturating_add(1);
                        undone_candidate.revision = undone_set.revision;
                        undone_set.content_hash = recompute_content_hash(&undone_set)?;
                        let set_json = serde_json::to_vec(&undone_set)
                            .map_err(|_| "serialization_failed".to_string())?;
                        let candidate_json = serde_json::to_vec(&undone_candidate)
                            .map_err(|_| "serialization_failed".to_string())?;
                        let database = journal.database().lock().await;
                        if !store::update_change_set(
                            database.connection(),
                            &undone_set.id,
                            old_revision,
                            undone_set.version,
                            &undone_set.content_hash,
                            &set_json,
                            now,
                        )
                        .map_err(|_| "commit_outcome_unknown".to_string())?
                        {
                            return Err("commit_outcome_unknown".into());
                        }
                        store::update_candidate(
                            database.connection(),
                            &undone_candidate.id,
                            &undone_candidate.diff_hash,
                            &candidate_json,
                            now,
                        )
                        .map_err(|_| "commit_outcome_unknown".to_string())?;
                        serde_json::to_vec(&serde_json::json!({
                            "schema_version": 1,
                            "candidate_id": undone_candidate.id,
                            "status": "undone",
                            "head": head,
                            "redacted": true
                        }))
                        .map_err(|_| "serialization_failed".to_string())?
                    }
                    _ => return Err("unsupported_agent_git_change_sets_operation".into()),
                };
                let database = journal.database().lock().await;
                store::put_idempotent(&database.connection(), &idempotency_key, &response, now)
                    .map_err(|_| "storage_failed".to_string())?;
                Ok(response)
            }
            .await;
            let projection_json = match &result {
                Ok(bytes) => String::from_utf8(bytes.clone()).unwrap_or_else(|_| "{}".into()),
                Err(error) => serde_json::json!({
                    "status": "error",
                    "error_code": error,
                    "redacted": true
                })
                .to_string(),
            };
            let event_version = serde_json::from_str::<serde_json::Value>(&projection_json)
                .ok()
                .and_then(|value| {
                    value
                        .get("revision")
                        .or_else(|| value.get("version"))
                        .and_then(serde_json::Value::as_u64)
                })
                .unwrap_or(expected_version);
            let event = CoreEvent::AgentGitChangeSets {
                change_set_id: event_change_set_id,
                operation: event_operation,
                version: event_version,
                projection_json,
            };
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::PolicyAwareToolResultCache {
            operation,
            cache_key,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_key = cache_key.clone();
            let event_operation = operation.clone();
            let result = async {
                    if idempotency_key.is_empty() { return Err("invalid_cache_idempotency_key".into()); }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::policy_aware_tool_result_cache as cache;
                    use evohime_local_storage::policy_aware_tool_result_cache_store as store;
                    let policy = cache::default_policy();
                    match operation.as_str() {
                        "inspect" => serde_json::to_vec(&serde_json::json!({"status":"available","cache_key":cache_key,"default_cacheability":"never","max_entries":policy.max_entries,"redacted":true})).map_err(|_| "serialization_failed".to_string()),
                        "put" => { let entry: cache::CacheEntry = serde_json::from_slice(&payload).map_err(|_| "invalid_cache_entry".to_string())?; cache::validate_entry(&entry, &policy, crate::task_memory::now_millis() as i64, cache::Freshness::UseCache).map_err(|e| e.to_string())?; let json=serde_json::to_vec(&entry).map_err(|_| "serialization_failed".to_string())?; if !store::put(database.connection(), &cache_key, expected_version.max(1), &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? { return Err("cache_stale_version".into()); } serde_json::to_vec(&serde_json::json!({"status":"stored","cache_key":cache_key,"redacted":true})).map_err(|_| "serialization_failed".to_string()) },
                        "get" => { let hit=store::get(database.connection(), &cache_key).map_err(|_| "storage_failed".to_string())?.and_then(|(_,json)|serde_json::from_slice::<cache::CacheEntry>(&json).ok()).and_then(|entry|cache::validate_entry(&entry,&policy,crate::task_memory::now_millis() as i64,cache::Freshness::UseCache).ok().map(|_|entry)); serde_json::to_vec(&serde_json::json!({"status":if hit.is_some(){"hit"}else{"miss"},"cache_key":cache_key,"provenance_ref":hit.map(|e|e.provenance_ref),"redacted":true})).map_err(|_| "serialization_failed".to_string()) },
                        "invalidate" => { if let Some((version,json))=store::get(database.connection(), &cache_key).map_err(|_| "storage_failed".to_string())? { let mut entry:cache::CacheEntry=serde_json::from_slice(&json).map_err(|_|"corrupt_cache".to_string())?; entry.status=cache::CacheStatus::Invalidated; let json=serde_json::to_vec(&entry).map_err(|_|"serialization_failed".to_string())?; if !store::put(database.connection(),&cache_key,version+1,&json,crate::task_memory::now_millis() as i64).map_err(|_|"storage_failed".to_string())? {return Err("cache_stale_version".into())}; } serde_json::to_vec(&serde_json::json!({"status":"invalidated","cache_key":cache_key,"redacted":true})).map_err(|_|"serialization_failed".to_string()) },
                        _ => Err("unsupported_cache_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::PolicyAwareToolResultCache {
                cache_key: event_key,
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
        CoreCommand::ArchitectEditorModelPipeline {
            operation,
            pipeline_id,
            payload,
            expected_version,
            idempotency_key: _,
            reply,
        } => {
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::architect_editor_model_pipeline as pipeline;
                    use evohime_local_storage::architect_editor_model_pipeline_store as store;
                    match operation.as_str() {
                        "create" => { let p: pipeline::ModelPhasePipeline = serde_json::from_slice(&payload).map_err(|_| "invalid_pipeline".to_string())?; pipeline::validate_pipeline(&p).map_err(|e| e.to_string())?; let json=serde_json::to_vec(&p).map_err(|_| "serialization_failed".to_string())?; store::put(database.connection(),&p.id,p.schema_version,&p.content_hash,&json,crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"pipeline_id":p.id,"status":p.status,"same_model":p.same_model,"redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                        "accept_intent" => { let json=store::get(database.connection(),&pipeline_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "pipeline_not_found".to_string())?; let mut p:pipeline::ModelPhasePipeline=serde_json::from_slice(&json).map_err(|_| "corrupt_pipeline".to_string())?; let req:AcceptIntentRequest=serde_json::from_slice(&payload).map_err(|_| "invalid_intent".to_string())?; pipeline::accept_intent(&mut p,req.intent,&req.workspace_revision).map_err(|e| e.to_string())?; if expected_version!=0 && expected_version!=1 {return Err("pipeline_stale_version".into())}; serde_json::to_vec(&serde_json::json!({"schema_version":1,"pipeline_id":p.id,"status":p.status,"intent_ready":true,"redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                        "get" => { let json=store::get(database.connection(),&pipeline_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "pipeline_not_found".to_string())?; let p:pipeline::ModelPhasePipeline=serde_json::from_slice(&json).map_err(|_| "corrupt_pipeline".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"pipeline_id":p.id,"status":p.status,"workspace_revision":p.workspace_revision,"same_model":p.same_model,"intent_present":p.intent.is_some(),"redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                        _ => Err("unsupported_architect_editor_operation".into())
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::ArchitectEditorModelPipeline {
                pipeline_id,
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
        CoreCommand::EventVisualizerRegistry {
            operation,
            visualizer_id,
            payload,
            expected_version,
            idempotency_key: _,
            reply,
        } => {
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::event_visualizer_registry as registry;
                    use evohime_local_storage::event_visualizer_registry_store as store;
                    match operation.as_str() {
                        "list" => { let mut descriptors = registry::builtins(); let rows = store::list(database.connection()).map_err(|_| "storage_failed".to_string())?; for row in rows { if let Ok(d) = serde_json::from_slice(&row) { descriptors.push(d); } } serde_json::to_vec(&serde_json::json!({"schema_version":1,"descriptors":descriptors,"redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                        "register" => { let d: registry::VisualizerDescriptor = serde_json::from_slice(&payload).map_err(|_| "invalid_visualizer_descriptor".to_string())?; registry::validate_descriptor(&d).map_err(|e| e.to_string())?; let json=serde_json::to_vec(&d).map_err(|_| "serialization_failed".to_string())?; store::put(database.connection(),&d.id,d.version,&d.content_hash,&json,crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"visualizer_id":d.id,"status":"registered","redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                        "resolve" => { let matcher: registry::VisualizerMatcher = serde_json::from_slice(&payload).map_err(|_| "invalid_visualizer_matcher".to_string())?; let mut descriptors=registry::builtins(); let rows=store::list(database.connection()).map_err(|_| "storage_failed".to_string())?; for row in rows { if let Ok(d)=serde_json::from_slice(&row) { descriptors.push(d); } } let resolution=registry::resolve(&descriptors,&matcher).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"resolution":resolution,"redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                        _ => Err("unsupported_event_visualizer_registry_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::EventVisualizerRegistry {
                visualizer_id,
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
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}

async fn load_change_set(
    journal: &EventJournal,
    id: &str,
) -> Result<crate::agent_git_change_sets::AgentGitChangeSet, String> {
    let database = journal.database().lock().await;
    let json = evohime_local_storage::agent_git_change_sets_store::get_change_set(
        database.connection(),
        id,
    )
    .map_err(|_| "storage_failed".to_string())?
    .ok_or_else(|| "change_set_not_found".to_string())?;
    serde_json::from_slice(&json).map_err(|_| "corrupt_agent_git_change_set".to_string())
}

async fn validate_agent_git_integrations(
    journal: &EventJournal,
    set: &crate::agent_git_change_sets::AgentGitChangeSet,
) -> Result<(), String> {
    crate::agent_git_change_sets::validate_integration_references(set)
        .map_err(|error| error.to_string())?;
    let database = journal.database().lock().await;
    if let Some(run_id) = &set.incremental_change_run_id {
        let run = evohime_local_storage::incremental_change_protocol_store::get(
            database.connection(),
            run_id,
        )
        .map_err(|_| "incremental_change_storage_failed".to_string())?
        .ok_or_else(|| "incremental_change_reference_not_found".to_string())?;
        if matches!(
            run.state.as_str(),
            "applied" | "cancelled" | "unknown_reconciliation_required"
        ) {
            return Err("incremental_change_reference_terminal".into());
        }
    }
    if let Some(worktree_id) = &set.task_worktree_id {
        let worktree =
            evohime_local_storage::domains::workflow::get(database.connection(), worktree_id)
                .map_err(|_| "task_worktree_storage_failed".to_string())?
                .ok_or_else(|| "task_worktree_reference_not_found".to_string())?;
        if !matches!(worktree.state.as_str(), "ready" | "integrating")
            || set.base_git_head.as_deref() != Some(worktree.base_commit.as_str())
        {
            return Err("task_worktree_reference_stale".into());
        }
    }
    Ok(())
}

async fn load_candidate(
    journal: &EventJournal,
    id: &str,
) -> Result<crate::agent_git_change_sets::GitCommitCandidate, String> {
    let database = journal.database().lock().await;
    let store = evohime_local_storage::agent_git_change_sets_store::get_candidate(
        database.connection(),
        id,
    )
    .map_err(|_| "storage_failed".to_string())?;
    let json = match store {
        Some(json) => json,
        None => evohime_local_storage::agent_git_change_sets_store::get_latest_candidate(
            database.connection(),
            id,
        )
        .map_err(|_| "storage_failed".to_string())?
        .ok_or_else(|| "candidate_not_found".to_string())?,
    };
    serde_json::from_slice(&json).map_err(|_| "corrupt_agent_git_candidate".to_string())
}

fn candidate_projection(
    candidate: &crate::agent_git_change_sets::GitCommitCandidate,
) -> Result<Vec<u8>, String> {
    bounded_projection(serde_json::json!({
        "schema_version": 1,
        "candidate_id": candidate.id,
        "change_set_id": candidate.change_set_ref,
        "revision": candidate.revision,
        "included_paths": candidate.included_paths,
        "excluded_paths": candidate.excluded_paths,
        "diff_hash": candidate.diff_hash,
        "proposed_message": candidate.proposed_message,
        "verification_status": candidate.verification_status,
        "commit_id": candidate.commit_id,
        "redacted": true
    }))
}

fn bounded_projection(value: serde_json::Value) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(&value).map_err(|_| "serialization_failed".to_string())?;
    if bytes.len() > crate::agent_git_change_sets::MAX_EVIDENCE_BYTES {
        return Err("projection_too_large".into());
    }
    Ok(bytes)
}

fn recompute_content_hash(
    set: &crate::agent_git_change_sets::AgentGitChangeSet,
) -> Result<String, String> {
    let mut copy = set.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| "serialization_failed".to_string())?;
    Ok(crate::agent_git_change_sets::sha256(&bytes))
}

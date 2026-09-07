use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::TeamCoordinator {
            operation,
            work_item_id,
            payload,
            expected_revision,
            idempotency_key,
            reply,
        } => {
            let result = async {
                let journal = state
                    .lock()
                    .await
                    .journal
                    .clone()
                    .ok_or_else(|| "storage journal is not configured".to_string())?;
                let database = journal.database().lock().await;
                use crate::team_coordinator as coordinator;
                use evohime_local_storage::team_coordinator_store as store;
                let now = crate::task_memory::now_millis() as i64;
                if !idempotency_key.is_empty() {
                    if let Some(cached) =
                        store::get_idempotency(database.connection(), &idempotency_key)
                            .map_err(|_| "storage_failed".to_string())?
                    {
                        return Ok(cached);
                    }
                }
                let encode = |item: &coordinator::TeamWorkItem| {
                    serde_json::to_vec(item).map_err(|_| "serialization_failed".to_string())
                };
                let status = |item: &coordinator::TeamWorkItem| {
                    serde_json::to_value(item.status)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned))
                        .unwrap_or_else(|| "unknown".to_owned())
                };
                match operation.as_str() {
                    "create" => {
                        let item: coordinator::TeamWorkItem =
                            serde_json::from_slice(&payload).map_err(|_| "invalid_work_item")?;
                        if item.id != work_item_id {
                            return Err("work_item_id_mismatch".into());
                        }
                        coordinator::validate_work_item(&item).map_err(|e| e.to_string())?;
                        let json = encode(&item)?;
                        store::put_work_item(
                            database.connection(),
                            store::PutWorkItemInput {
                                item_id: &item.id,
                                revision: item.revision as i64,
                                status: &status(&item),
                                assigned_instance_id: item.assigned_instance_id.as_deref(),
                                attempt: item.attempt as i64,
                                item_json: &json,
                                now_ms: now,
                            },
                        )
                        .map_err(|e| {
                            if matches!(e, rusqlite::Error::SqliteFailure(_, _)) {
                                "work_item_exists".to_string()
                            } else {
                                "storage_failed".to_string()
                            }
                        })?;
                        Ok(json)
                    }
                    "get" => store::get_work_item(database.connection(), &work_item_id)
                        .map_err(|_| "storage_failed".to_string())?
                        .ok_or_else(|| "work_item_not_found".to_string()),
                    "list" => {
                        let rows = store::list_work_items(
                            database.connection(),
                            coordinator::MAX_WORK_ITEMS,
                        )
                        .map_err(|_| "storage_failed".to_string())?;
                        let items: Vec<serde_json::Value> = rows
                            .into_iter()
                            .filter_map(|json| serde_json::from_slice(&json).ok())
                            .collect();
                        serde_json::to_vec(&serde_json::json!({
                            "schema_version": coordinator::SCHEMA_VERSION,
                            "queue_count": items.len(),
                            "work_items": items,
                            "candidate_count": 0,
                            "assignment_count": 0,
                            "consultation_count": 0,
                            "escalation": null,
                        }))
                        .map_err(|_| "serialization_failed".to_string())
                    }
                    "propose" => {
                        let request: AssignmentProposalRequest =
                            serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_proposal_request")?;
                        let item: coordinator::TeamWorkItem = if let Some(item) = request.item {
                            item
                        } else {
                            let json = store::get_work_item(database.connection(), &work_item_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "work_item_not_found".to_string())?;
                            serde_json::from_slice(&json).map_err(|_| "corrupt_work_item")?
                        };
                        let candidates = request.candidates;
                        let termination_policy =
                            request.termination.as_ref().map(|value| &value.policy);
                        let termination_state =
                            request.termination.as_ref().map(|value| &value.state);
                        let termination_event =
                            request.termination.as_ref().map(|value| &value.event);
                        let proposal = coordinator::propose_assignment_with_termination(
                            &item,
                            &candidates,
                            termination_policy,
                            termination_state,
                            termination_event,
                        )
                        .map_err(|e| e.to_string())?;
                        serde_json::to_vec(&proposal)
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    "assign" => {
                        let request: AssignmentRequest = serde_json::from_slice(&payload)
                            .map_err(|_| "invalid_assignment_request")?;
                        let mut item = request.item;
                        let proposal = request.proposal;
                        let candidate = request.candidate;
                        if item.id != work_item_id {
                            return Err("work_item_id_mismatch".into());
                        }
                        coordinator::validate_proposal(&item, &proposal, &candidate)
                            .map_err(|e| e.to_string())?;
                        coordinator::transition(
                            &mut item,
                            coordinator::WorkItemStatus::Assigned,
                            expected_revision,
                        )
                        .map_err(|e| e.to_string())?;
                        item.assigned_instance_id = Some(candidate.instance_id.clone());
                        let json = encode(&item)?;
                        if !store::replace_work_item(
                            database.connection(),
                            store::ReplaceWorkItemInput {
                                item_id: &item.id,
                                expected_revision: expected_revision as i64,
                                revision: item.revision as i64,
                                status: &status(&item),
                                assigned_instance_id: item.assigned_instance_id.as_deref(),
                                attempt: item.attempt as i64,
                                item_json: &json,
                                now_ms: now,
                            },
                        )
                        .map_err(|_| "storage_failed")?
                        {
                            return Err("stale_work_item_revision".into());
                        }
                        let assignment_id = uuid::Uuid::now_v7().to_string();
                        let proposal_json =
                            serde_json::to_vec(&proposal).map_err(|_| "serialization_failed")?;
                        store::put_assignment(
                            database.connection(),
                            &assignment_id,
                            &item.id,
                            &candidate.instance_id,
                            &proposal_json,
                            now,
                        )
                        .map_err(|_| "storage_failed")?;
                        Ok(json)
                    }
                    "consult" => {
                        let query: coordinator::SpecialistQuery =
                            serde_json::from_slice(&payload).map_err(|_| "invalid_consultation")?;
                        coordinator::validate_consultation(&query).map_err(|e| e.to_string())?;
                        let json =
                            serde_json::to_vec(&query).map_err(|_| "serialization_failed")?;
                        store::put_consultation(database.connection(), &query.id, &json, now)
                            .map_err(|_| "storage_failed")?;
                        Ok(json)
                    }
                    "review" => {
                        let review: coordinator::CoordinationReview =
                            serde_json::from_slice(&payload).map_err(|_| "invalid_review")?;
                        if review.work_item_id != work_item_id {
                            return Err("work_item_id_mismatch".into());
                        }
                        coordinator::validate_review(&review).map_err(|e| e.to_string())?;
                        let json =
                            serde_json::to_vec(&review).map_err(|_| "serialization_failed")?;
                        let decision_id = uuid::Uuid::now_v7().to_string();
                        store::put_decision(
                            database.connection(),
                            &decision_id,
                            &work_item_id,
                            &json,
                            now,
                        )
                        .map_err(|_| "storage_failed")?;
                        Ok(json)
                    }
                    "decompose" => {
                        let proposal: coordinator::DecompositionProposal =
                            serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_decomposition")?;
                        if proposal.parent_work_item_id != work_item_id {
                            return Err("work_item_id_mismatch".into());
                        }
                        coordinator::validate_decomposition(&proposal)
                            .map_err(|e| e.to_string())?;
                        serde_json::to_vec(&proposal)
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    "reassign" => {
                        let mut item: coordinator::TeamWorkItem =
                            serde_json::from_slice(&payload).map_err(|_| "invalid_work_item")?;
                        if item.id != work_item_id {
                            return Err("work_item_id_mismatch".into());
                        }
                        coordinator::validate_reassignment(&item).map_err(|e| e.to_string())?;
                        coordinator::transition(
                            &mut item,
                            coordinator::WorkItemStatus::Proposed,
                            expected_revision,
                        )
                        .map_err(|e| e.to_string())?;
                        item.assigned_instance_id = None;
                        item.attempt = item.attempt.saturating_add(1);
                        coordinator::validate_reassignment(&item).map_err(|e| e.to_string())?;
                        let json = encode(&item)?;
                        if !store::replace_work_item(
                            database.connection(),
                            store::ReplaceWorkItemInput {
                                item_id: &item.id,
                                expected_revision: expected_revision as i64,
                                revision: item.revision as i64,
                                status: &status(&item),
                                assigned_instance_id: None,
                                attempt: item.attempt as i64,
                                item_json: &json,
                                now_ms: now,
                            },
                        )
                        .map_err(|_| "storage_failed")?
                        {
                            return Err("stale_work_item_revision".into());
                        }
                        Ok(json)
                    }
                    "cancel" => {
                        let json = store::get_work_item(database.connection(), &work_item_id)
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "work_item_not_found".to_string())?;
                        let mut item: coordinator::TeamWorkItem =
                            serde_json::from_slice(&json).map_err(|_| "corrupt_work_item")?;
                        coordinator::transition(
                            &mut item,
                            coordinator::WorkItemStatus::Cancelled,
                            expected_revision,
                        )
                        .map_err(|e| e.to_string())?;
                        let json = encode(&item)?;
                        if !store::replace_work_item(
                            database.connection(),
                            store::ReplaceWorkItemInput {
                                item_id: &item.id,
                                expected_revision: expected_revision as i64,
                                revision: item.revision as i64,
                                status: &status(&item),
                                assigned_instance_id: item.assigned_instance_id.as_deref(),
                                attempt: item.attempt as i64,
                                item_json: &json,
                                now_ms: now,
                            },
                        )
                        .map_err(|_| "storage_failed")?
                        {
                            return Err("stale_work_item_revision".into());
                        }
                        Ok(json)
                    }
                    _ => Err("unsupported_team_coordinator_operation".into()),
                }
            }
            .await;
            if !idempotency_key.is_empty() {
                if let Ok(bytes) = &result {
                    if let Some(journal) = state.lock().await.journal.clone() {
                        let database = journal.database().lock().await;
                        let _ = evohime_local_storage::team_coordinator_store::put_idempotency(
                            database.connection(),
                            &idempotency_key,
                            bytes,
                        );
                    }
                }
            }
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".to_owned());
            let revision = serde_json::from_str::<serde_json::Value>(&projection_json)
                .ok()
                .and_then(|value| value.get("revision").and_then(serde_json::Value::as_u64))
                .unwrap_or(expected_revision);
            let event = CoreEvent::TeamCoordinator {
                work_item_id,
                operation,
                revision,
                projection_json,
            };
            let journal = state.lock().await.journal.clone();
            if let Some(journal) = journal {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::ProjectInstructionStack {
            operation,
            workspace_root,
            payload,
            relevant_paths,
            expected_revision,
            idempotency_key,
            reply,
        } => {
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::project_instruction_stack as stack;
                    use evohime_local_storage::project_instruction_stack_store as store;
                    if !idempotency_key.is_empty() {
                        if let Some(cached) = store::get_idempotency(database.connection(), &idempotency_key).map_err(|_| "storage_failed".to_string())? { return Ok(cached); }
                    }
                    let root = std::path::PathBuf::from(&workspace_root);
                    let mut rules = stack::discover_rules(&root, stack::global_rules_root_from_env().as_deref()).map_err(|e| e.to_string())?;
                    for stored in store::list_rules(database.connection(), stack::MAX_RULES).map_err(|_| "storage_failed".to_string())? {
                        if let Ok(saved) = serde_json::from_slice::<stack::ProjectRule>(&stored) {
                            if let Some(rule) = rules.iter_mut().find(|rule| rule.id == saved.id) {
                                rule.enabled = saved.enabled;
                                rule.source_revision = rule.source_revision.max(saved.source_revision);
                            }
                        }
                    }
                    for rule in &rules {
                        let json = serde_json::to_vec(rule).map_err(|_| "serialization_failed")?;
                        let source_kind = serde_json::to_string(&rule.source_kind).map_err(|_| "serialization_failed")?;
                        store::put_rule(database.connection(), store::PutRuleInput { rule_id: &rule.id, revision: rule.source_revision as i64, source_kind: &source_kind, source_ref: &rule.source_ref, content_hash: &rule.content_hash, rule_json: &json, now_ms: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed")?;
                    }
                    let now = crate::task_memory::now_millis() as i64;
                    match operation.as_str() {
                        "discover" => {
                            let projection: Vec<_> = rules.iter().map(|rule| stack::project_rule(rule, "discovered")).collect();
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"rules":projection,"rule_count":projection.len(),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "compile" => {
                            let request: InstructionStackCompileRequest = if payload.is_empty() { InstructionStackCompileRequest::default() } else { serde_json::from_slice(&payload).map_err(|_| "invalid_stack_request")? };
                            let explicit_ids = request.explicit_ids;
                            let policy = request.policy.unwrap_or_else(stack::default_policy);
                            let snapshot = stack::compile_snapshot(&root, rules.clone(), &relevant_paths, &explicit_ids, &policy, now).map_err(|e| e.to_string())?;
                            let snapshot_id = uuid::Uuid::now_v7().to_string();
                            let snapshot_json = serde_json::to_vec(&snapshot).map_err(|_| "serialization_failed")?;
                            store::put_snapshot(database.connection(), &snapshot_id, &workspace_root, &snapshot.content_hash, &snapshot_json, now).map_err(|_| "storage_failed")?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"snapshot_id":snapshot_id,"snapshot":stack::project_snapshot(&snapshot)})).map_err(|_| "serialization_failed".to_string())
                        }
                        "get" => {
                            let snapshot_id = String::from_utf8(payload).map_err(|_| "invalid_snapshot_id")?;
                            let json = store::get_snapshot(database.connection(), &snapshot_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "snapshot_not_found".to_string())?;
                            let snapshot: stack::InstructionSnapshot = serde_json::from_slice(&json).map_err(|_| "corrupt_snapshot")?;
                            serde_json::to_vec(&stack::project_snapshot(&snapshot)).map_err(|_| "serialization_failed".to_string())
                        }
                        "toggle" => {
                            let request: InstructionStackToggleRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_toggle")?;
                            let mut rule = rules.into_iter().find(|rule| rule.id == request.rule_id).ok_or("rule_not_found")?;
                            if rule.source_kind == stack::SourceKind::Global { return Err("global_rule_requires_user_scope".into()); }
                            if rule.source_revision != expected_revision && expected_revision != 0 { return Err("stale_rule_revision".into()); }
                            rule.enabled = request.enabled; rule.source_revision = rule.source_revision.saturating_add(1);
                            let json = serde_json::to_vec(&rule).map_err(|_| "serialization_failed")?;
                            let source_kind = serde_json::to_string(&rule.source_kind).map_err(|_| "serialization_failed")?;
                            store::put_rule(database.connection(), store::PutRuleInput { rule_id: &rule.id, revision: rule.source_revision as i64, source_kind: &source_kind, source_ref: &rule.source_ref, content_hash: &rule.content_hash, rule_json: &json, now_ms: now }).map_err(|_| "storage_failed")?;
                            serde_json::to_vec(&stack::project_rule(&rule, "toggled")).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_project_instruction_operation".into()),
                    }
                }.await;
            if !idempotency_key.is_empty() {
                if let Ok(bytes) = &result {
                    if let Some(journal) = state.lock().await.journal.clone() {
                        let database = journal.database().lock().await;
                        let _ =
                            evohime_local_storage::project_instruction_stack_store::put_idempotency(
                                database.connection(),
                                &idempotency_key,
                                bytes,
                            );
                    }
                }
            }
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".to_owned());
            let event = CoreEvent::ProjectInstructionStack {
                workspace_root,
                operation,
                revision: expected_revision,
                projection_json,
            };
            let journal = state.lock().await.journal.clone();
            if let Some(journal) = journal {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::WorkspaceSets {
            operation,
            set_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let result = async {
                    let journal = state
                        .lock()
                        .await
                        .journal
                        .clone()
                        .ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::workspace_sets as sets;
                    use evohime_local_storage::workspace_sets_store as store;
                    if !idempotency_key.is_empty() {
                        if let Some(cached) = store::get_idempotency(database.connection(), &idempotency_key)
                            .map_err(|_| "storage_failed".to_string())? {
                            return Ok(cached);
                        }
                    }
                    let policy = sets::default_policy();
                    match operation.as_str() {
                        "search" => {
                            let json = store::get(database.connection(), &set_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "workspace_set_not_found".to_string())?;
                            let set: sets::WorkspaceSet = serde_json::from_slice(&json)
                                .map_err(|_| "corrupt_workspace_set".to_string())?;
                            let scope: sets::SearchScope = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_workspace_search".to_string())?;
                            let matches = sets::search(&set, &scope, &policy)
                                .map_err(|error| error.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"set_id":set.id,"match_count":matches.len(),"matches":matches,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "bind" => {
                            let request: WorkspaceSetBindingRequest = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_workspace_set_binding".to_string())?;
                            let task_id = request.task_id.as_str();
                            let requested_roots = request.root_ids;
                            let json = store::get(database.connection(), &set_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "workspace_set_not_found".to_string())?;
                            let set: sets::WorkspaceSet = serde_json::from_slice(&json)
                                .map_err(|_| "corrupt_workspace_set".to_string())?;
                            if expected_version != 0 && expected_version != set.version {
                                return Err("workspace_set_stale_version".into());
                            }
                            let roots: Vec<_> = if requested_roots.is_empty() {
                                set.roots.iter().filter(|root| root.enabled).collect()
                            } else {
                                set.roots.iter().filter(|root| requested_roots.iter().any(|id| id == &root.root_id) && root.enabled).collect()
                            };
                            if roots.is_empty() || roots.len() > sets::MAX_ROOTS {
                                return Err("workspace_set_no_enabled_roots".into());
                            }
                            let binding = serde_json::json!({
                                "schema_version": 1,
                                "task_id": task_id,
                                "set_id": set.id,
                                "set_version": set.version,
                                "set_hash": set.content_hash,
                                "roots": roots.iter().map(|root| serde_json::json!({"root_id":root.root_id,"alias":root.alias,"canonical_path":root.canonical_path,"kind":root.kind,"grants":root.grants,"vcs":root.vcs,"revision":root.vcs.as_ref().map(|v| v.working_tree_revision)})).collect::<Vec<_>>(),
                                "pinned": true
                            });
                            let binding_json = serde_json::to_vec(&binding).map_err(|_| "serialization_failed".to_string())?;
                            if binding_json.len() > sets::MAX_BINDING_SNAPSHOT_BYTES { return Err("workspace_set_binding_too_large".into()); }
                            store::bind_run(database.connection(), task_id, &set.id, set.version, &binding_json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"task_id":task_id,"set_id":set.id,"set_version":set.version,"set_hash":set.content_hash,"root_count":roots.len(),"pinned":true,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "create" => {
                            let set: sets::WorkspaceSet = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_workspace_set".to_string())?;
                            let set = sets::canonicalize_and_hash(set, &policy)
                                .map_err(|error| error.to_string())?;
                            let json = serde_json::to_vec(&set).map_err(|_| "serialization_failed".to_string())?;
                            if !store::create(database.connection(), &set.id, &json, &set.content_hash, crate::task_memory::now_millis() as i64)
                                .map_err(|_| "storage_failed".to_string())? {
                                return Err("workspace_set_duplicate".into());
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"set":set,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "get" => {
                            let json = store::get(database.connection(), &set_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "workspace_set_not_found".to_string())?;
                            let set: sets::WorkspaceSet = serde_json::from_slice(&json)
                                .map_err(|_| "corrupt_workspace_set".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"id":set.id,"version":set.version,"name":set.name,"root_count":set.roots.len(),"default_root_id":set.default_root_id,"content_hash":set.content_hash,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "update" => {
                            let set: sets::WorkspaceSet = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_workspace_set".to_string())?;
                            if set.id != set_id || set.version != expected_version.saturating_add(1) {
                                return Err("workspace_set_stale_version".into());
                            }
                            let set = sets::canonicalize_and_hash(set, &policy)
                                .map_err(|error| error.to_string())?;
                            let json = serde_json::to_vec(&set).map_err(|_| "serialization_failed".to_string())?;
                            if !store::update(database.connection(), &set_id, expected_version, set.version, &json, &set.content_hash, crate::task_memory::now_millis() as i64)
                                .map_err(|_| "storage_failed".to_string())? {
                                return Err("workspace_set_stale_version".into());
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"id":set.id,"version":set.version,"root_count":set.roots.len(),"content_hash":set.content_hash,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_workspace_sets_operation".into()),
                    }
                }
                .await;
            if !idempotency_key.is_empty() {
                if let Ok(bytes) = &result {
                    if let Some(journal) = state.lock().await.journal.clone() {
                        let database = journal.database().lock().await;
                        let _ = evohime_local_storage::workspace_sets_store::put_idempotency(
                            database.connection(),
                            &idempotency_key,
                            bytes,
                        );
                    }
                }
            }
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".to_owned());
            let event = CoreEvent::WorkspaceSets {
                set_id,
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

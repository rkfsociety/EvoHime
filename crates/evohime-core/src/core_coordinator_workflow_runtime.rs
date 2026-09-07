use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::WorkspaceBootstrapManifest {
            operation,
            project_id,
            workspace_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_workspace_id = workspace_id.clone();
            let result = async {
                    if workspace_id.is_empty() || workspace_id.len() > crate::workspace_bootstrap_manifest::MAX_ID {
                        return Err("invalid workspace_id".to_string());
                    }
                    let manifest_payload = if operation == "discover" && payload.is_empty() {
                        let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                        let connection = journal.database().lock().await;
                        let project = connection.get_project(&project_id).map_err(|e| e.to_string())?
                            .ok_or_else(|| "project not found".to_string())?;
                        let root = std::path::PathBuf::from(project.workspace_path);
                        if crate::task_memory::workspace_scope_id(&root) != workspace_id {
                            return Err("workspace identity mismatch".to_string());
                        }
                        std::fs::read(root.join(".evohime").join("bootstrap.json")).map_err(|_| "bootstrap manifest not found".to_string())?
                    } else { payload };
                    let manifest: crate::workspace_bootstrap_manifest::WorkspaceBootstrapManifest =
                        serde_json::from_slice(&manifest_payload).map_err(|e| e.to_string())?;
                    if manifest.workspace_id != workspace_id {
                        return Err("workspace scope mismatch".to_string());
                    }
                    crate::workspace_bootstrap_manifest::validate_manifest(&manifest)
                        .map_err(|e| e.to_string())?;
                    match operation.as_str() {
                        "validate" | "discover" | "save" | "approve" | "run" => {
                            if operation == "save" {
                                let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                                let connection = journal.database().lock().await;
                                let json = serde_json::to_string(&manifest).map_err(|e| e.to_string())?;
                                let saved = evohime_local_storage::workspace_bootstrap_manifest_store::put_manifest(
                                    connection.connection(),
                                    (&manifest.id, &manifest.workspace_id, manifest.revision, &manifest.content_hash, &json, "policy-v1", crate::task_memory::now_millis() as i64),
                                ).map_err(|e| e.to_string())?;
                                return serde_json::to_vec(&serde_json::json!({
                                    "status": if saved { "saved" } else { "duplicate" },
                                    "manifest_id": manifest.id,
                                    "revision": manifest.revision,
                                    "content_hash": manifest.content_hash,
                                })).map_err(|e| e.to_string());
                            }
                            if operation == "run" {
                                if idempotency_key.is_empty() {
                                    return Err("idempotency key required".to_string());
                                }
                                let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                                {
                                    let connection = journal.database().lock().await;
                                    let trust = evohime_local_storage::workspace_bootstrap_manifest_store::manifest_trust(
                                        connection.connection(), &manifest.id, manifest.revision,
                                    ).map_err(|e| e.to_string())?;
                                    if !matches!(trust, Some((status, hash)) if status == "trusted" && hash == manifest.content_hash) {
                                        return Err("trust_required".to_string());
                                    }
                                }
                                let root = {
                                    let connection = journal.database().lock().await;
                                    let project = connection.get_project(&project_id).map_err(|e| e.to_string())?
                                        .ok_or_else(|| "project not found".to_string())?;
                                    let root = std::path::PathBuf::from(project.workspace_path);
                                    if crate::task_memory::workspace_scope_id(&root) != manifest.workspace_id {
                                        return Err("workspace identity mismatch".to_string());
                                    }
                                    root
                                };
                                let now_ms = crate::task_memory::now_millis() as i64;
                                let lease_id = uuid::Uuid::new_v4().to_string();
                                let reserved = {
                                    let connection = journal.database().lock().await;
                                    let _ = evohime_local_storage::workspace_bootstrap_manifest_store::fence_expired_preparations(
                                        connection.connection(), now_ms.saturating_sub(30 * 60 * 1000),
                                    ).map_err(|e| e.to_string())?;
                                    evohime_local_storage::workspace_bootstrap_manifest_store::reserve_preparation(
                                        connection.connection(), &manifest.workspace_id, &manifest.id,
                                        &manifest.content_hash, &manifest.content_hash, &lease_id,
                                        now_ms,
                                    ).map_err(|e| e.to_string())?
                                };
                                if !reserved {
                                    let connection = journal.database().lock().await;
                                    if let Some((_, status, version)) = evohime_local_storage::workspace_bootstrap_manifest_store::get_preparation(
                                        connection.connection(), &manifest.workspace_id, &manifest.id,
                                        &manifest.content_hash, &manifest.content_hash,
                                    ).map_err(|e| e.to_string())? {
                                        if expected_version != 0 && expected_version != version as u64 {
                                            return Err("version_conflict".to_string());
                                        }
                                        if status == "prepared" {
                                            return serde_json::to_vec(&serde_json::json!({"status": status, "manifest_id": manifest.id, "content_hash": manifest.content_hash, "idempotent": true})).map_err(|e| e.to_string());
                                        }
                                    }
                                    return Err("already_running_or_prepared".to_string());
                                }
                                let run = crate::workspace_bootstrap_manifest::run_bounded(&root, &manifest).await;
                                let (status, result_json, error) = match run {
                                    Ok(results) => ("prepared", Some(serde_json::to_string(&results).map_err(|e| e.to_string())?), None),
                                    Err(e) => (if matches!(e, crate::workspace_bootstrap_manifest::BootstrapManifestError::TimedOut) { "unknown_outcome" } else { "failed" }, None, Some(e.to_string())),
                                };
                                let connection = journal.database().lock().await;
                                evohime_local_storage::workspace_bootstrap_manifest_store::complete_preparation(
                                    connection.connection(), &manifest.workspace_id, &manifest.id, &lease_id,
                                    status, result_json.as_deref(), crate::task_memory::now_millis() as i64,
                                ).map_err(|e| e.to_string())?;
                                if let Some(error) = error { return Err(error); }
                                return serde_json::to_vec(&serde_json::json!({"status": status, "manifest_id": manifest.id, "content_hash": manifest.content_hash})).map_err(|e| e.to_string());
                            }
                            if operation == "approve" {
                                let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                                let connection = journal.database().lock().await;
                                let approved = evohime_local_storage::workspace_bootstrap_manifest_store::approve_manifest(
                                    connection.connection(), &manifest.id, manifest.revision, &manifest.content_hash, "restricted-process-v1",
                                ).map_err(|e| e.to_string())?;
                                return serde_json::to_vec(&serde_json::json!({"status": if approved { "trusted" } else { "trust_unchanged" }, "manifest_id": manifest.id, "content_hash": manifest.content_hash})).map_err(|e| e.to_string());
                            }
                            serde_json::to_vec(&serde_json::json!({
                            "status": if operation == "discover" { "pending_review" } else { "valid" },
                            "manifest_id": manifest.id,
                            "revision": manifest.revision,
                            "content_hash": manifest.content_hash,
                        })).map_err(|e| e.to_string())
                        }
                        _ => Err("unsupported workspace bootstrap operation".to_string()),
                    }
                }.await;
            let event_payload = result
                .as_ref()
                .ok()
                .and_then(|payload| serde_json::from_slice::<serde_json::Value>(payload).ok());
            let event = CoreEvent::WorkspaceBootstrapManifest {
                workspace_id: event_workspace_id,
                operation: event_operation,
                status: event_payload
                    .as_ref()
                    .and_then(|value| value.get("status"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("failed")
                    .to_owned(),
                manifest_id: event_payload
                    .as_ref()
                    .and_then(|value| value.get("manifest_id"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                revision: event_payload
                    .as_ref()
                    .and_then(|value| value.get("revision"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                content_hash: event_payload
                    .as_ref()
                    .and_then(|value| value.get("content_hash"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                projection_json: event_payload
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "{}".into()),
            };
            let (journal, events) = {
                let guard = state.lock().await;
                (guard.journal.clone(), guard.events.clone())
            };
            if let Some(journal) = journal {
                let _ = journal.record(&event).await;
            }
            let _ = events.send(event).await;
            let _ = reply.send(result);
        }
        CoreCommand::TeamCoordinationPolicies {
            operation,
            team_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_team_id = team_id.clone();
            let result = async {
                    if team_id.is_empty() || team_id.len() > crate::team_coordination_policies::MAX_TEXT || idempotency_key.is_empty() {
                        return Err("invalid coordination request".to_string());
                    }
                    let request: TeamCoordinationRequest = serde_json::from_slice(&payload).map_err(|_| "invalid coordination payload".to_string())?;
                    let spec = request.team;
                    if spec.id != team_id { return Err("team identity mismatch".to_string()); }
                    crate::team_coordination_policies::validate_team(&spec).map_err(|e| e.to_string())?;
                    match operation.as_str() {
                        "validate_policy" => serde_json::to_vec(&serde_json::json!({"status":"valid","team_id":team_id,"revision":spec.revision,"content_hash":crate::team_coordination_policies::canonical_hash(&spec).map_err(|e| e.to_string())?})).map_err(|e| e.to_string()),
                        "select" => {
                            let state = request.state.as_ref().ok_or_else(|| "state required".to_string())?;
                            let (next, decision) = crate::team_coordination_policies::select_next(&spec, state, request.handoff_from.as_deref(), request.selector_role.as_deref(), request.event_type.as_deref(), &request.event_ids).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"selected","team_id":team_id,"version":expected_version.saturating_add(1),"state":next,"decision":decision})).map_err(|e| e.to_string())
                        }
                        "save_state" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let state_value = request.state.ok_or_else(|| "state required".to_string())?;
                            let json = serde_json::to_vec(&state_value).map_err(|e| e.to_string())?;
                            let connection = journal.database().lock().await;
                            let saved = evohime_local_storage::team_coordination_policies_store::save_state(connection.connection(), &team_id, spec.revision, &json, expected_version, &idempotency_key, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            if !saved { return Err("version_conflict_or_duplicate".to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"saved","team_id":team_id,"version":expected_version.saturating_add(1)})).map_err(|e| e.to_string())
                        }
                        "save_policy" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let json = serde_json::to_vec(&spec).map_err(|e| e.to_string())?;
                            let hash = crate::team_coordination_policies::canonical_hash(&spec).map_err(|e| e.to_string())?;
                            let connection = journal.database().lock().await;
                            let saved = evohime_local_storage::team_coordination_policies_store::save_policy(connection.connection(), &team_id, spec.revision, &json, &hash, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":if saved {"saved"} else {"duplicate"},"team_id":team_id,"revision":spec.revision,"content_hash":hash})).map_err(|e| e.to_string())
                        }
                        "select_strategy" => {
                            let strategy = request.strategy.ok_or_else(|| "strategy required".to_string())?;
                            crate::team_coordination_policies::validate_strategy(&strategy).map_err(|e| e.to_string())?;
                            if strategy.eligible_roles.iter().any(|role| !spec.members.iter().any(|member| member.role == *role)) {
                                return Err("strategy eligible set exceeds team roster".to_string());
                            }
                            let snapshot = request.protocol_snapshot.ok_or_else(|| "protocol snapshot required".to_string())?;
                            if snapshot.protocol_id != strategy.protocol_id || snapshot.content_hash != strategy.protocol_hash {
                                return Err("protocol snapshot mismatch".to_string());
                            }
                            let protocol: crate::team_sop_protocols::TeamProtocol = serde_json::from_slice(&snapshot.protocol_json).map_err(|_| "invalid protocol snapshot".to_string())?;
                            crate::team_sop_protocols::validate_protocol(&protocol).map_err(|e| e.to_string())?;
                            let strategy_state = request.strategy_state.as_ref().ok_or_else(|| "strategy state required".to_string())?;
                            let participant = request.participant.as_ref();
                            let handoff_from = request.handoff_from.as_deref();
                            if matches!(&strategy.kind, crate::team_coordination_policies::TeamCoordinationStrategyKind::HandoffSwarm { .. } | crate::team_coordination_policies::TeamCoordinationStrategyKind::GraphDirected { .. }) {
                                let from = handoff_from.ok_or_else(|| "handoff source required".to_string())?;
                                let to = participant.as_ref().map(|item| item.role.as_str()).ok_or_else(|| "handoff target required".to_string())?;
                                crate::team_coordination_policies::validate_protocol_route(&snapshot, from, to).map_err(|e| e.to_string())?;
                            }
                            let (next, decision) = crate::team_coordination_policies::select_strategy(&strategy, strategy_state, participant, handoff_from, &request.event_ids).map_err(|e| e.to_string())?;
                            let strategy_json = serde_json::to_vec(&strategy).map_err(|e| e.to_string())?;
                            let next_json = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let connection = journal.database().lock().await;
                            let saved = evohime_local_storage::team_coordination_policies_store::save_strategy_state(connection.connection(), evohime_local_storage::team_coordination_policies_store::StrategyStateInput {
                                session_id: &strategy.session_id,
                                strategy_id: &strategy.strategy_id,
                                strategy_revision: strategy.revision,
                                protocol_hash: &strategy.protocol_hash,
                                strategy_json: &strategy_json,
                                state_json: &next_json,
                                expected_version,
                                idempotency_key: &idempotency_key,
                                now_ms: crate::task_memory::now_millis() as i64,
                            }).map_err(|e| e.to_string())?;
                            if !saved { return Err("version_conflict_or_duplicate".to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"selected","team_id":team_id,"session_id":strategy.session_id,"strategy_id":strategy.strategy_id,"protocol_hash":strategy.protocol_hash,"version":expected_version.saturating_add(1),"state":next,"decision":decision})).map_err(|e| e.to_string())
                        }
                        _ => Err("unsupported coordination operation".to_string()),
                    }
                }.await;
            let event_value = result
                .as_ref()
                .ok()
                .and_then(|payload| serde_json::from_slice::<serde_json::Value>(payload).ok());
            let event = CoreEvent::TeamCoordinationPolicies {
                team_id: event_team_id,
                operation: event_operation,
                status: event_value
                    .as_ref()
                    .and_then(|value| value.get("status"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("failed")
                    .to_owned(),
                version: event_value
                    .as_ref()
                    .and_then(|value| value.get("version"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                projection_json: event_value
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "{}".into()),
            };
            let (journal, events) = {
                let guard = state.lock().await;
                (guard.journal.clone(), guard.events.clone())
            };
            if let Some(journal) = journal {
                let _ = journal.record(&event).await;
            }
            let _ = events.send(event).await;
            let _ = reply.send(result);
        }
        CoreCommand::TypedAgentHandoffContract {
            operation,
            handoff_id,
            packet_json,
            actor,
            reason,
            expected_version,
            idempotency_key: _,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_handoff_id = handoff_id.clone();
            let result = async {
                    if handoff_id.is_empty() || handoff_id.len() > crate::typed_agent_handoff_contract::MAX_TEXT {
                        return Err("invalid handoff id".to_string());
                    }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let connection = journal.database().lock().await;
                    match operation.as_str() {
                        "propose" => {
                            let packet: crate::typed_agent_handoff_contract::HandoffPacket = serde_json::from_slice(&packet_json).map_err(|_| "invalid handoff packet".to_string())?;
                            if packet.handoff_id != handoff_id { return Err("handoff identity mismatch".to_string()); }
                            let record = crate::typed_agent_handoff_contract::propose(packet, "ipc-request").map_err(|e| e.to_string())?;
                            let packet_bytes = serde_json::to_vec(&record.packet).map_err(|e| e.to_string())?;
                            let state_bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
                            let saved = evohime_local_storage::typed_agent_handoff_contract_store::put(connection.connection(), &handoff_id, &packet_bytes, &state_bytes, "proposed", crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            if !saved { return serde_json::to_vec(&serde_json::json!({"status":"duplicate","handoff_id":handoff_id,"version":1,"idempotent":true})).map_err(|e| e.to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"proposed","handoff_id":handoff_id,"version":record.version})).map_err(|e| e.to_string())
                        }
                        "transition" => {
                            let (_, state_bytes, _, _) = evohime_local_storage::typed_agent_handoff_contract_store::load(connection.connection(), &handoff_id).map_err(|e| e.to_string())?.ok_or_else(|| "handoff_not_found".to_string())?;
                            let mut record: crate::typed_agent_handoff_contract::HandoffRecord = serde_json::from_slice(&state_bytes).map_err(|_| "handoff_state_corrupt".to_string())?;
                            let next: crate::typed_agent_handoff_contract::HandoffState = serde_json::from_slice(&packet_json).map_err(|_| "invalid handoff state".to_string())?;
                            crate::typed_agent_handoff_contract::transition(&mut record, next, &actor, &reason, expected_version, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            let bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
                            if !evohime_local_storage::typed_agent_handoff_contract_store::transition(connection.connection(), &handoff_id, &bytes, &format!("{:?}", record.state).to_lowercase(), expected_version, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())? { return Err("stale_handoff".to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"transitioned","handoff_id":handoff_id,"state":record.state,"version":record.version})).map_err(|e| e.to_string())
                        }
                        "get" => {
                            let (_, state_bytes, state, version) = evohime_local_storage::typed_agent_handoff_contract_store::load(connection.connection(), &handoff_id).map_err(|e| e.to_string())?.ok_or_else(|| "handoff_not_found".to_string())?;
                            let record: serde_json::Value = serde_json::from_slice(&state_bytes).map_err(|_| "handoff_state_corrupt".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":state,"handoff_id":handoff_id,"version":version,"record":record})).map_err(|e| e.to_string())
                        }
                        _ => Err("unsupported handoff operation".to_string()),
                    }
                }.await;
            let event_value = result
                .as_ref()
                .ok()
                .and_then(|payload| serde_json::from_slice::<serde_json::Value>(payload).ok());
            let event = CoreEvent::TypedAgentHandoffContract {
                handoff_id: event_handoff_id,
                operation: event_operation,
                state: event_value
                    .as_ref()
                    .and_then(|value| value.get("state"))
                    .or_else(|| event_value.as_ref().and_then(|value| value.get("status")))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("failed")
                    .to_owned(),
                version: event_value
                    .as_ref()
                    .and_then(|value| value.get("version"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                projection_json: event_value
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "{}".into()),
            };
            let (journal, events) = {
                let guard = state.lock().await;
                (guard.journal.clone(), guard.events.clone())
            };
            if let Some(journal) = journal {
                let _ = journal.record(&event).await;
            }
            let _ = events.send(event).await;
            let _ = reply.send(result);
        }
        CoreCommand::SchemaDrivenAgentConfiguration {
            operation,
            scope,
            payload,
            expected_revision,
            idempotency_key: _,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_scope = scope.clone();
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    let scope_kind = match scope.as_str() { "application" => crate::schema_driven_agent_configuration::ConfigurationScope::ApplicationDefaults, "workspace" => crate::schema_driven_agent_configuration::ConfigurationScope::WorkspaceDefaults, "agent" => crate::schema_driven_agent_configuration::ConfigurationScope::AgentProfile, "conversation" => crate::schema_driven_agent_configuration::ConfigurationScope::ConversationDefaults, "run" => crate::schema_driven_agent_configuration::ConfigurationScope::RunOverride, _ => return Err("invalid_configuration_scope".into()) };
                    let schema = crate::schema_driven_agent_configuration::builtin_schema(scope_kind);
                    match operation.as_str() {
                        "get_schema" => serde_json::to_vec(&schema).map_err(|e| e.to_string()),
                        "get_snapshot" => {
                            let Some((_, snapshot, _)) = evohime_local_storage::schema_driven_agent_configuration_store::load(database.connection(), &scope).map_err(|e| e.to_string())? else { return serde_json::to_vec(&serde_json::json!({"status":"not_configured","scope":scope,"schema":schema})).map_err(|e| e.to_string()); };
                            Ok(snapshot)
                        }
                        "apply" => {
                            let input: serde_json::Value = serde_json::from_slice(&payload).map_err(|_| "invalid_configuration_payload".to_string())?;
                            let mut values = input.get("values").and_then(serde_json::Value::as_object).cloned().unwrap_or_default();
                            let patches = if let Some(raw) = input.get("patches").and_then(serde_json::Value::as_array) {
                                let mut parsed = Vec::with_capacity(raw.len());
                                for item in raw { let object = item.as_object().ok_or_else(|| "invalid_configuration_patch".to_string())?; let kind = match object.get("kind").and_then(serde_json::Value::as_str).unwrap_or("") { "SetField" => crate::schema_driven_agent_configuration::PatchKind::SetField, "ClearOverride" => crate::schema_driven_agent_configuration::PatchKind::ClearOverride, "ResetSection" => crate::schema_driven_agent_configuration::PatchKind::ResetSection, "BindReference" => crate::schema_driven_agent_configuration::PatchKind::BindReference, _ => return Err("invalid_configuration_patch_kind".into()) }; let field = object.get("field").and_then(serde_json::Value::as_str).ok_or_else(|| "patch_field_required".to_string())?.to_owned(); let value = object.get("value").cloned(); if matches!(kind, crate::schema_driven_agent_configuration::PatchKind::SetField | crate::schema_driven_agent_configuration::PatchKind::BindReference) { if let Some(value) = &value { values.insert(field.clone(), value.clone()); } } else if matches!(kind, crate::schema_driven_agent_configuration::PatchKind::ClearOverride) { values.remove(&field); } else { values.clear(); } parsed.push(crate::schema_driven_agent_configuration::ConfigurationPatch { kind, field, value_json: value }); }
                                parsed
                            } else { values.iter().map(|(field, value)| crate::schema_driven_agent_configuration::ConfigurationPatch { kind: crate::schema_driven_agent_configuration::PatchKind::SetField, field: field.clone(), value_json: Some(value.clone()) }).collect::<Vec<_>>() };
                            crate::schema_driven_agent_configuration::validate_patches(&schema, &patches).map_err(|e| e.to_string())?;
                            let layers = [("requested", &values)];
                            let current = evohime_local_storage::schema_driven_agent_configuration_store::load(database.connection(), &scope).map_err(|e| e.to_string())?;
                            let revision = current.as_ref().map(|(_, _, revision)| *revision).unwrap_or(0);
                            if current.is_some() && revision != expected_revision { return Err("configuration_revision_conflict".into()); }
                            let snapshot = crate::schema_driven_agent_configuration::effective_snapshot(scope_kind, &schema, revision + 1, &layers).map_err(|e| e.to_string())?;
                            let schema_json = serde_json::to_vec(&schema).map_err(|e| e.to_string())?; let snapshot_json = serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?;
                            if !evohime_local_storage::schema_driven_agent_configuration_store::save(database.connection(), &scope, &schema_json, &snapshot_json, revision + 1, crate::task_memory::now_millis() as i64, expected_revision).map_err(|e| e.to_string())? { return Err("configuration_revision_conflict".into()); }
                            Ok(snapshot_json)
                        }
                        _ => Err("unsupported_configuration_operation".into()),
                    }
                }.await;
            let revision = result
                .as_ref()
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
                .and_then(|v| v.get("revision").and_then(serde_json::Value::as_u64))
                .unwrap_or(0);
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::SchemaDrivenAgentConfiguration {
                scope: event_scope,
                operation: event_operation,
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
        CoreCommand::ExperienceReplayLibrary {
            operation,
            scope,
            scope_id,
            payload,
            expected_revision,
            idempotency_key: _,
            reply,
        } => {
            let event_scope = scope.clone();
            let event_operation = operation.clone();
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    let scope_kind = match scope.as_str() { "Session"=>crate::experience_replay_library::ExperienceScope::Session,"Project"=>crate::experience_replay_library::ExperienceScope::Project,"User"=>crate::experience_replay_library::ExperienceScope::User,"RoleProfile"=>crate::experience_replay_library::ExperienceScope::RoleProfile,"WorkflowProfile"=>crate::experience_replay_library::ExperienceScope::WorkflowProfile,_=>return Err("invalid_experience_scope".into()) };
                    match operation.as_str() {
                        "write" => { let record: crate::experience_replay_library::ExperienceRecord = serde_json::from_slice(&payload).map_err(|_| "invalid_experience_record".to_string())?; if record.scope != scope_kind || record.scope_id != scope_id { return Err("experience_scope_denied".into()); } crate::experience_replay_library::validate_and_write_gate(&record).map_err(|e|e.to_string())?; let hash=record.content_hash.clone(); let json=serde_json::to_vec(&record).map_err(|e|e.to_string())?; let saved=evohime_local_storage::experience_replay_library_store::put(database.connection(),&record.id,&scope,&scope_id,&json,&hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":if saved{"stored"}else{"duplicate"},"id":record.id,"revision":1,"idempotent":!saved})).map_err(|e|e.to_string()) }
                        "list" => { let records=evohime_local_storage::experience_replay_library_store::list(database.connection(),&scope,&scope_id,64).map_err(|e|e.to_string())?; let records:Vec<serde_json::Value>=records.into_iter().filter_map(|b|serde_json::from_slice(&b).ok()).collect(); serde_json::to_vec(&serde_json::json!({"status":"ok","scope":scope,"records":records})).map_err(|e|e.to_string()) }
                        "context" => { let records=evohime_local_storage::experience_replay_library_store::list(database.connection(),&scope,&scope_id,64).map_err(|e|e.to_string())?; let records:Vec<crate::experience_replay_library::ExperienceRecord>=records.into_iter().filter_map(|b|serde_json::from_slice(&b).ok()).collect(); let context=crate::experience_replay_library::project_context(&records,crate::experience_replay_library::MAX_CONTEXT_BYTES).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"ok","context":context,"max_bytes":crate::experience_replay_library::MAX_CONTEXT_BYTES})).map_err(|e|e.to_string()) }
                        _ => Err("unsupported_experience_operation".into()),
                    }
                }.await;
            let revision = result
                .as_ref()
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok())
                .and_then(|v| v.get("revision").and_then(serde_json::Value::as_u64))
                .unwrap_or(expected_revision);
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::ExperienceReplayLibrary {
                scope: event_scope,
                operation: event_operation,
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
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}

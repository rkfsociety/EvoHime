use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::ArchitectureSnapshot {
            operation,
            snapshot_id,
            workspace_root,
            payload,
            expected_version: _,
            idempotency_key,
            reply,
        } => {
            let result = async {
                    if idempotency_key.is_empty() { return Err("invalid_architecture_snapshot_idempotency_key".into()); }
                    if workspace_root.len() > crate::architecture_snapshot::MAX_ID * 4 { return Err("workspace_root_too_long".into()); }
                    let request: ArchitectureSnapshotRequest = if payload.is_empty() {
                        ArchitectureSnapshotRequest::default()
                    } else {
                        serde_json::from_slice(&payload).map_err(|_| "invalid_architecture_snapshot_payload".to_string())?
                    };
                    let root = request.workspace_root.as_deref().filter(|v| !v.is_empty()).unwrap_or(&workspace_root);
                    let id = if snapshot_id.is_empty() { "architecture-current" } else { snapshot_id.as_str() };
                    match operation.as_str() {
                        "get" | "evidence" | "open_evidence" | "upstream" | "downstream" | "route" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let db = journal.database().lock().await;
                            let record = evohime_local_storage::architecture_snapshot_store::get(db.connection(), id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "architecture_snapshot_not_found".to_string())?;
                            let json = record.record_json;
                            let snapshot: crate::architecture_snapshot::ArchitectureSnapshot = serde_json::from_slice(&json).map_err(|_| "corrupt_architecture_snapshot".to_string())?;
                            if operation == "evidence" || operation == "open_evidence" { return serde_json::to_vec(&serde_json::json!({"status":"ok","snapshot_id":id,"evidence":snapshot.components.iter().flat_map(|c| c.evidence.iter()).collect::<Vec<_>>(),"open_mode":operation == "open_evidence","redacted":true})).map_err(|_| "serialization_failed".to_string()); }
                            if operation == "get" { return serde_json::to_vec(&serde_json::json!({"status":"ok","snapshot":snapshot,"redacted":true})).map_err(|_| "serialization_failed".to_string()); }
                            let subject = request.subject_id.as_deref().unwrap_or_default();
                            let ids: Vec<&str> = match operation.as_str() {
                                "upstream" => snapshot.relationships.iter().filter(|r| r.to == subject).map(|r| r.from.as_str()).take(64).collect(),
                                "downstream" => snapshot.relationships.iter().filter(|r| r.from == subject).map(|r| r.to.as_str()).take(64).collect(),
                                _ => snapshot.relationships.iter().filter(|r| r.from == subject || r.to == subject).flat_map(|r| [r.from.as_str(), r.to.as_str()]).take(64).collect(),
                            };
                            serde_json::to_vec(&serde_json::json!({"status":"ok","operation":operation,"subject_id":subject,"related_ids":ids,"route_is_not_impact":true,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "refresh" | "rebuild" | "current" => {
                            let revision = request.source_revision.as_deref().unwrap_or("working-tree");
                            let root_path = std::path::Path::new(root);
                            let allowed_roots = if request.allowed_roots.is_empty() {
                                vec![root.to_owned()]
                            } else {
                                request.allowed_roots.clone()
                            };
                            crate::architecture_snapshot_runtime::authorize_root(root_path, &allowed_roots).map_err(|e| e.to_string())?;
                            let workspace_identity = crate::architecture_snapshot_runtime::source_fingerprint(root_path, revision);
                            let snapshot = crate::architecture_snapshot_runtime::extract(root_path, &workspace_identity, revision, id).map_err(|e| e.to_string())?;
                            let hash = crate::architecture_snapshot::snapshot_hash(&snapshot).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&snapshot).map_err(|_| "serialization_failed".to_string())?;
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let db = journal.database().lock().await;
                            evohime_local_storage::architecture_snapshot_store::set_refresh_state(db.connection(), id, "accepted", None, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?;
                            evohime_local_storage::architecture_snapshot_store::put(db.connection(), evohime_local_storage::architecture_snapshot_store::PutInput { snapshot_id: id, workspace_identity: &workspace_identity, source_revision: revision, snapshot_hash: &hash, state: "accepted", record_json: &json, updated_at_ms: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"accepted","snapshot_id":id,"snapshot_hash":hash,"projection":snapshot,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "compare" => {
                            let before = request.before.ok_or_else(|| "before_required".to_string())?;
                            let after = request.after.ok_or_else(|| "after_required".to_string())?;
                            let delta = crate::architecture_snapshot::delta(&before, &after).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"compared","delta":delta,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "review" => {
                            let expected = request.expected.ok_or_else(|| "expected_required".to_string())?;
                            let actual = request.actual.ok_or_else(|| "actual_required".to_string())?;
                            let result = crate::architecture_snapshot::review(&expected, &actual).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"reviewed","review":result,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "inspect" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let db = journal.database().lock().await;
                            let identity = crate::architecture_snapshot_runtime::source_fingerprint(std::path::Path::new(root), "working-tree");
                            let records = evohime_local_storage::architecture_snapshot_store::list(db.connection(), &identity, 64).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"ok","snapshots":records,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_architecture_snapshot_operation".into()),
                    }
                }.await;
            let projection_json = match String::from_utf8(result.clone().unwrap_or_default()) {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(%error, "architecture snapshot projection is not UTF-8");
                    "{}".into()
                }
            };
            let event = CoreEvent::ArchitectureSnapshot {
                snapshot_id,
                operation,
                version: 1,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::LocalModelRuntimeManager {
            operation,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let result = async {
                    if idempotency_key.is_empty() { return Err("invalid_local_model_manager_idempotency_key".into()); }
                    let request: LocalModelRuntimeRequest = if payload.is_empty() {
                        LocalModelRuntimeRequest::default()
                    } else {
                        serde_json::from_slice(&payload).map_err(|_| "invalid_local_model_manager_payload".to_string())?
                    };
                    let value = serde_json::to_value(&request).map_err(|_| "invalid_local_model_manager_payload".to_string())?;
                    match operation.as_str() {
                        "calibration_admit" => {
                            let session: crate::local_model_runtime_manager::LocalModelRuntimeSession = serde_json::from_value(value.get("session").cloned().ok_or_else(|| "session_required".to_string())?).map_err(|_| "invalid_session".to_string())?;
                            let model: crate::local_model_runtime_manager::LocalModelDescriptor = serde_json::from_value(value.get("model").cloned().ok_or_else(|| "model_required".to_string())?).map_err(|_| "invalid_model".to_string())?;
                            let runtime: crate::local_model_runtime_manager::LocalInferenceRuntime = serde_json::from_value(value.get("runtime").cloned().ok_or_else(|| "runtime_required".to_string())?).map_err(|_| "invalid_runtime".to_string())?;
                            let adapter = value.get("stream_adapter_available").and_then(serde_json::Value::as_bool).unwrap_or(false);
                            let admission = crate::local_model_performance_calibration::admit_calibration(&session, &model, &runtime, adapter).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"admission_evaluated","admission":format!("{admission:?}").to_ascii_lowercase(),"measured_profile_created":false,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "calibration_inspect" => serde_json::to_vec(&serde_json::json!({"contract_id":crate::local_model_performance_calibration::CONTRACT_ID,"status":"unavailable_adapter","measured_profile_created":false,"routing_integration":"unavailable_integration","redacted":true})).map_err(|_| "serialization_failed".to_string()),
                        "hardware" => {
                            let profile = crate::local_model_runtime_manager::discover_hardware().map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"discovered","hardware":profile,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "start" => {
                            let model_id = request.model_id.as_deref().filter(|v| !v.is_empty()).ok_or_else(|| "model_id_required".to_string())?;
                            let request_id = request.request_id.as_deref().filter(|v| !v.is_empty()).ok_or_else(|| "request_id_required".to_string())?;
                            #[cfg(windows)]
                            { let response = crate::analysis_kernel::supervisor_command(serde_json::json!({"op":"launch","model_id":model_id,"request_id":request_id})).await.map_err(|_| "runtime_unavailable".to_string())?; if response.get("accepted") != Some(&serde_json::Value::Bool(true)) { return Err("runtime_unavailable".into()); } let health = crate::analysis_kernel::supervisor_command(serde_json::json!({"op":"probe","model_id":model_id})).await.map_err(|_| "health_gate_unavailable".to_string())?; if health.get("healthy") != Some(&serde_json::Value::Bool(true)) { let _ = crate::analysis_kernel::supervisor_command(serde_json::json!({"op":"stop","model_id":model_id,"request_id":request_id})).await; return Err("health_gate_failed".into()); } serde_json::to_vec(&serde_json::json!({"status":"ready","model_id":model_id,"supervised":true,"health_gate":"passed","redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                            #[cfg(not(windows))]
                            { let _ = (model_id, request_id); Err("runtime_unavailable".into()) }
                        }
                        "stop" => {
                            let model_id = request.model_id.as_deref().filter(|v| !v.is_empty()).ok_or_else(|| "model_id_required".to_string())?;
                            let request_id = request.request_id.as_deref().filter(|v| !v.is_empty()).ok_or_else(|| "request_id_required".to_string())?;
                            #[cfg(windows)]
                            { let response = crate::analysis_kernel::supervisor_command(serde_json::json!({"op":"stop","model_id":model_id,"request_id":request_id})).await.map_err(|_| "runtime_unavailable".to_string())?; if response.get("accepted") != Some(&serde_json::Value::Bool(true)) { return Err("runtime_unavailable".into()); } serde_json::to_vec(&serde_json::json!({"status":"stopped","model_id":model_id,"supervised":true,"redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                            #[cfg(not(windows))]
                            { let _ = (model_id, request_id); Err("runtime_unavailable".into()) }
                        }
                        "probe" => {
                            let model_id = request.model_id.as_deref().filter(|v| !v.is_empty()).ok_or_else(|| "model_id_required".to_string())?;
                            #[cfg(windows)]
                            { let response = crate::analysis_kernel::supervisor_command(serde_json::json!({"op":"probe","model_id":model_id})).await.map_err(|_| "health_gate_unavailable".to_string())?; if response.get("healthy") != Some(&serde_json::Value::Bool(true)) { return Err("health_gate_failed".into()); } serde_json::to_vec(&serde_json::json!({"status":"ready","model_id":model_id,"health_gate":"passed","redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                            #[cfg(not(windows))]
                            { let _ = model_id; Err("runtime_unavailable".into()) }
                        }
                        "verify_artifact" => {
                            let state = request.state.ok_or_else(|| "state_required".to_string())?;
                            let trust = request.trust.ok_or_else(|| "trust_required".to_string())?;
                            let observed = request.observed_hash.as_deref().ok_or_else(|| "observed_hash_required".to_string())?;
                            let expected = request.expected_hash.as_deref().ok_or_else(|| "expected_hash_required".to_string())?;
                            crate::local_model_runtime_manager::allow_artifact_promotion(state, trust, observed, expected).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"verified","artifact_state":"installed","content_hash":expected,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "promote_artifact" => {
                            let staging = request.staging_relative_path.as_deref().ok_or_else(|| "staging_path_required".to_string())?;
                            let destination = request.destination_relative_path.as_deref().ok_or_else(|| "destination_path_required".to_string())?;
                            let staging = std::path::Path::new(staging);
                            let destination = std::path::Path::new(destination);
                            crate::local_model_runtime_manager::validate_artifact_relative_path(staging).map_err(|e| e.to_string())?;
                            crate::local_model_runtime_manager::validate_artifact_relative_path(destination).map_err(|e| e.to_string())?;
                            let expected = value.get("expected_hash").and_then(serde_json::Value::as_str).ok_or_else(|| "expected_hash_required".to_string())?;
                            let expected_size = value.get("expected_size_bytes").and_then(serde_json::Value::as_u64).ok_or_else(|| "expected_size_required".to_string())?;
                            let root = crate::get_data_directory();
                            let models_root = root.join("models");
                            std::fs::create_dir_all(&models_root).map_err(|_| "artifact_root_unavailable".to_string())?;
                            let staging_path = crate::local_model_runtime_manager::managed_artifact_path(&models_root, staging).map_err(|e| e.to_string())?;
                            let destination_path = crate::local_model_runtime_manager::managed_artifact_path(&models_root, destination).map_err(|e| e.to_string())?;
                            crate::local_model_runtime_manager::atomic_promote_verified_artifact(&staging_path, &destination_path, expected, expected_size).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"installed","artifact_state":"installed","content_hash":expected,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "transition" => {
                            let from: crate::local_model_runtime_manager::ArtifactState = serde_json::from_value(value.get("from").cloned().ok_or_else(|| "from_required".to_string())?).map_err(|_| "invalid_from_state".to_string())?;
                            let to: crate::local_model_runtime_manager::ArtifactState = serde_json::from_value(value.get("to").cloned().ok_or_else(|| "to_required".to_string())?).map_err(|_| "invalid_to_state".to_string())?;
                            crate::local_model_runtime_manager::allow_transition(from, to).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"transitioned","from":from,"to":to,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "profile" => {
                            let session: crate::local_model_runtime_manager::LocalModelRuntimeSession = serde_json::from_value(value.get("session").cloned().ok_or_else(|| "session_required".to_string())?).map_err(|_| "invalid_session".to_string())?;
                            let descriptor: crate::local_model_runtime_manager::LocalModelDescriptor = serde_json::from_value(value.get("model").cloned().ok_or_else(|| "model_required".to_string())?).map_err(|_| "invalid_model".to_string())?;
                            let runtime: crate::local_model_runtime_manager::LocalInferenceRuntime = serde_json::from_value(value.get("runtime").cloned().ok_or_else(|| "runtime_required".to_string())?).map_err(|_| "invalid_runtime".to_string())?;
                            let profile = crate::local_model_runtime_manager::managed_profile(&session, &descriptor, &runtime).map_err(|e| e.to_string())?;
                            let resilience = crate::local_model_runtime_manager::resilience_profile_ref(&profile);
                            serde_json::to_vec(&serde_json::json!({"profile": profile, "resilience_profile": resilience, "redacted": true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "register_model" | "register_runtime" | "register_artifact" | "register_session" => {
                            let (kind, record_id, revision, record_json) = match operation.as_str() {
                                "register_model" => { let record: crate::local_model_runtime_manager::LocalModelDescriptor = serde_json::from_value(value.get("model").cloned().ok_or_else(|| "model_required".to_string())?).map_err(|_| "invalid_model".to_string())?; record.validate().map_err(|e| e.to_string())?; let id = format!("model:{}:{}", record.model_id, record.revision); ("model", id, record.revision, serde_json::to_vec(&record).map_err(|_| "serialization_failed".to_string())?) }
                                "register_runtime" => { let record: crate::local_model_runtime_manager::LocalInferenceRuntime = serde_json::from_value(value.get("runtime").cloned().ok_or_else(|| "runtime_required".to_string())?).map_err(|_| "invalid_runtime".to_string())?; record.validate().map_err(|e| e.to_string())?; let id = format!("runtime:{}:{}", record.runtime_id, record.revision); ("runtime", id, record.revision, serde_json::to_vec(&record).map_err(|_| "serialization_failed".to_string())?) }
                                "register_artifact" => { let record: crate::local_model_runtime_manager::LocalArtifactRecord = serde_json::from_value(value.get("artifact").cloned().ok_or_else(|| "artifact_required".to_string())?).map_err(|_| "invalid_artifact".to_string())?; record.validate().map_err(|e| e.to_string())?; let id = format!("artifact:{}:{}", record.model_id, record.model_revision); ("artifact", id, record.model_revision, serde_json::to_vec(&record).map_err(|_| "serialization_failed".to_string())?) }
                                _ => { let record: crate::local_model_runtime_manager::LocalModelRuntimeSession = serde_json::from_value(value.get("session").cloned().ok_or_else(|| "session_required".to_string())?).map_err(|_| "invalid_session".to_string())?; record.validate().map_err(|e| e.to_string())?; let id = format!("session:{}", record.session_id); ("session", id, expected_version.max(1), serde_json::to_vec(&record).map_err(|_| "serialization_failed".to_string())?) }
                            };
                            let hash = crate::local_model_runtime_manager::canonical_hash(&record_json);
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?; let db = journal.database().lock().await;
                            if !evohime_local_storage::local_model_runtime_manager_store::put_record(db.connection(), &record_id, kind, revision, &hash, &record_json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? { return Err("stale_local_model_manager_record".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"registered","record_id":record_id,"record_kind":kind,"revision":revision,"content_hash":hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "recover" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?; let db = journal.database().lock().await;
                            let artifacts = evohime_local_storage::local_model_runtime_manager_store::list_records(db.connection(), "artifact", 256).map_err(|_| "storage_failed".to_string())?;
                            let runtimes = evohime_local_storage::local_model_runtime_manager_store::list_records(db.connection(), "runtime", 256).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"reconciled","artifacts":artifacts.len(),"runtimes":runtimes.len(),"ready_requires_fresh_probe":true,"orphan_processes":"unavailable_until_identity_probe","redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "download_artifact" => {
                            let url = value.get("url").and_then(serde_json::Value::as_str).ok_or_else(|| "artifact_url_required".to_string())?;
                            let staging = value.get("staging_relative_path").and_then(serde_json::Value::as_str).ok_or_else(|| "staging_path_required".to_string())?;
                            let expected = value.get("expected_hash").and_then(serde_json::Value::as_str).ok_or_else(|| "expected_hash_required".to_string())?;
                            let expected_size = value.get("expected_size_bytes").and_then(serde_json::Value::as_u64).ok_or_else(|| "expected_size_required".to_string())?;
                            let relative = std::path::Path::new(staging);
                            crate::local_model_runtime_manager::validate_artifact_relative_path(relative).map_err(|e| e.to_string())?;
                            let root = crate::get_data_directory();
                            let models_root = root.join("models");
                            std::fs::create_dir_all(&models_root).map_err(|_| "artifact_root_unavailable".to_string())?;
                            let staging_path = crate::local_model_runtime_manager::managed_artifact_path(&models_root, relative).map_err(|e| e.to_string())?;
                            crate::local_model_runtime_manager::download_verified_artifact(url, &staging_path, expected, expected_size, &tokio_util::sync::CancellationToken::new()).await.map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"verified_staging","artifact_state":"verifying","staging_relative_path":staging,"expected_size_bytes":expected_size,"content_hash":expected,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "inspect" => serde_json::to_vec(&serde_json::json!({"schema_version":1,"contract_id":crate::local_model_runtime_manager::CONTRACT_ID,"status":"metadata_only","runtime_execution":"supervisor_boundary_required","artifact_download":"not_started","redacted":true})).map_err(|_| "serialization_failed".to_string()),
                        "fit" => {
                            let hardware: crate::local_model_runtime_manager::LocalHardwareProfile = serde_json::from_value(value.get("hardware").cloned().ok_or_else(|| "hardware_required".to_string())?).map_err(|_| "invalid_hardware".to_string())?;
                            let model: crate::local_model_runtime_manager::LocalModelDescriptor = serde_json::from_value(value.get("model").cloned().ok_or_else(|| "model_required".to_string())?).map_err(|_| "invalid_model".to_string())?;
                            let fit = crate::local_model_runtime_manager::compute_fit(&hardware, &model).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&fit).map_err(|_| "serialization_failed".to_string())
                        }
                        "get_policy" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?; let db = journal.database().lock().await;
                            let stored = evohime_local_storage::local_model_runtime_manager_store::get(db.connection(), crate::local_model_runtime_manager::CONTRACT_ID).map_err(|_| "storage_failed".to_string())?;
                            if let Some((version, hash, json)) = stored { return serde_json::to_vec(&serde_json::json!({"status":"loaded","version":version,"content_hash":hash,"policy":serde_json::from_slice::<serde_json::Value>(&json).unwrap_or(serde_json::json!({})),"redacted":true})).map_err(|_| "serialization_failed".to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"missing","version":0,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "save_policy" => {
                            let policy: crate::local_model_runtime_manager::LocalModelManagerPolicy = serde_json::from_value(value.get("policy").cloned().ok_or_else(|| "policy_required".to_string())?).map_err(|_| "invalid_policy".to_string())?;
                            policy.validate().map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&policy).map_err(|_| "serialization_failed".to_string())?; let hash = crate::local_model_runtime_manager::canonical_hash(&policy);
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?; let db = journal.database().lock().await;
                            if !evohime_local_storage::local_model_runtime_manager_store::put(db.connection(), &policy.policy_id, policy.version, &hash, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? { return Err("stale_local_model_manager_policy".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"saved","policy_id":policy.policy_id,"version":policy.version,"content_hash":hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_local_model_manager_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let version = serde_json::from_str::<serde_json::Value>(&projection_json)
                .ok()
                .and_then(|v| v.get("version").and_then(serde_json::Value::as_u64))
                .unwrap_or(expected_version);
            let event = CoreEvent::LocalModelRuntimeManager {
                operation: operation.clone(),
                version,
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

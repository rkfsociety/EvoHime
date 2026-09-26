use super::*;

async fn transition_adaptation_job(
    state: &Arc<Mutex<CoordinatorState>>,
    mut job: crate::local_model_adaptation::AdaptationJob,
    next: crate::local_model_adaptation::AdaptationState,
) -> Result<crate::local_model_adaptation::AdaptationJob, String> {
    let journal = state
        .lock()
        .await
        .journal
        .clone()
        .ok_or_else(|| "storage journal is not configured".to_string())?;
    let mut db = journal.database().lock().await;
    let current = evohime_local_storage::local_model_adaptation_store::get_job(
        db.connection(),
        &job.request.job_id,
    )
    .map_err(|_| "storage_failed".to_string())?
    .ok_or_else(|| "adaptation_job_not_found".to_string())?;
    if current.0 != job.revision || current.1 != job.state.storage_key() {
        return Err("adaptation_job_revision_conflict".into());
    }
    job.transition(next, job.evidence.clone())
    .map_err(|_| "adaptation_transition_denied".to_string())?;
    let snapshot = serde_json::to_vec(&job).map_err(|_| "serialization_failed".to_string())?;
    let transaction = db.connection_mut().transaction()
        .map_err(|_| "storage_failed".to_string())?;
    if !evohime_local_storage::local_model_adaptation_store::put_job(
        &transaction,
        &job.request.job_id,
        job.revision,
        job.state.storage_key(),
        &job.request.idempotency_key,
        &current.2,
        &job.content_sha256,
        &current.4,
        &snapshot,
        crate::task_memory::now_millis() as i64,
    )
    .map_err(|_| "storage_failed".to_string())?
    {
        return Err("adaptation_job_revision_conflict".into());
    }
    if matches!(current.1.as_str(),
        "benchmarking" | "cancelling" | "rejecting"
    ) && job.state.terminal()
    {
        if let Some((_, _, run_state, _)) = evohime_local_storage::benchmark_store::get_run(
            &transaction, &job.request.job_id,
        ).map_err(|_| "benchmark_run_storage_failed".to_string())? {
            if run_state == "running" {
                let report = serde_json::to_string(&serde_json::json!({
                    "status":job.state.storage_key(),"redacted":true
                })).map_err(|_| "benchmark_report_serialization_failed".to_string())?;
                if !evohime_local_storage::benchmark_store::save_report(
                    &transaction, &job.request.job_id, &report, job.state.storage_key(),
                    crate::task_memory::now_millis() as i64,
                ).map_err(|_| "benchmark_report_storage_failed".to_string())? {
                    return Err("benchmark_report_run_missing".into());
                }
            }
        }
    }
    transaction.commit().map_err(|_| "storage_failed".to_string())?;
    Ok(job)
}

async fn mark_adaptation_waiting_for_resources(
    state: &Arc<Mutex<CoordinatorState>>,
    mut job: crate::local_model_adaptation::AdaptationJob,
) -> Result<crate::local_model_adaptation::AdaptationJob, String> {
    if job.state == crate::local_model_adaptation::AdaptationState::Created {
        job = transition_adaptation_job(
            state,
            job,
            crate::local_model_adaptation::AdaptationState::Preflighted,
        )
        .await?;
    }
    if job.state == crate::local_model_adaptation::AdaptationState::Preflighted {
        job = transition_adaptation_job(
            state,
            job,
            crate::local_model_adaptation::AdaptationState::WaitingForResources,
        )
        .await?;
    }
    Ok(job)
}

#[cfg(windows)]
async fn cancel_supervisor_quantizer(job_id: &str) -> Result<(), String> {
    let response = crate::analysis_kernel::supervisor_command(serde_json::json!({
        "op":"adaptation_quantize_cancel", "job_id":job_id
    }))
    .await
    .map_err(|_| "quantizer_cancel_failed".to_string())?;
    let already_stopped = response.get("reason").and_then(serde_json::Value::as_str)
        == Some("job_not_running");
    if response.get("accepted") != Some(&serde_json::Value::Bool(true)) && !already_stopped {
        return Err("quantizer_cancel_rejected".into());
    }
    Ok(())
}

#[cfg(windows)]
async fn stop_adaptation_runtime(job_id: &str) -> Result<(), String> {
    let response = crate::analysis_kernel::supervisor_command(serde_json::json!({
        "op":"adaptation_runtime_stop", "job_id":job_id
    })).await.map_err(|_| "adaptation_runtime_stop_failed".to_string())?;
    let already_stopped = response.get("reason").and_then(serde_json::Value::as_str)
        == Some("job_not_running");
    if response.get("accepted") != Some(&serde_json::Value::Bool(true)) && !already_stopped {
        return Err("adaptation_runtime_stop_rejected".into());
    }
    Ok(())
}

#[cfg(not(windows))]
async fn stop_adaptation_runtime(_job_id: &str) -> Result<(), String> {
    Err("quantizer_requires_windows_supervisor".into())
}

async fn persist_adaptation_job(
    state: &Arc<Mutex<CoordinatorState>>,
    job: &crate::local_model_adaptation::AdaptationJob,
    expected_revision: u64,
    report: Option<&crate::agent_benchmark_matrix::BenchmarkReport>,
) -> Result<(), String> {
    let journal = state.lock().await.journal.clone()
        .ok_or_else(|| "storage journal is not configured".to_string())?;
    let mut db = journal.database().lock().await;
    let current = evohime_local_storage::local_model_adaptation_store::get_job(
        db.connection(), &job.request.job_id,
    ).map_err(|_| "storage_failed".to_string())?
        .ok_or_else(|| "adaptation_job_not_found".to_string())?;
    if current.0 != expected_revision
        || current.1 != crate::local_model_adaptation::AdaptationState::Benchmarking.storage_key()
        || job.revision != expected_revision.saturating_add(1)
        || !matches!(job.state,
            crate::local_model_adaptation::AdaptationState::ReadyForPromotion
                | crate::local_model_adaptation::AdaptationState::Failed)
    {
        return Err("adaptation_job_revision_conflict".into());
    }
    let snapshot = serde_json::to_vec(job).map_err(|_| "serialization_failed".to_string())?;
    let transaction = db.connection_mut().transaction()
        .map_err(|_| "storage_failed".to_string())?;
    if !evohime_local_storage::local_model_adaptation_store::put_job(
        &transaction, &job.request.job_id, job.revision, job.state.storage_key(),
        &job.request.idempotency_key, &current.2, &job.content_sha256, &current.4,
        &snapshot, crate::task_memory::now_millis() as i64,
    ).map_err(|_| "storage_failed".to_string())? {
        return Err("adaptation_job_revision_conflict".into());
    }
    let report_json = if let Some(report) = report {
        let json = serde_json::to_string(report).map_err(|_| "benchmark_report_serialization_failed".to_string())?;
        if json.len() > 2 * 1024 * 1024
            || job.evidence.benchmark_sha256.as_deref()
                != Some(crate::local_model_runtime_manager::canonical_hash(report).as_str()) {
            return Err("benchmark_report_integrity_failed".into());
        }
        json
    } else {
        "{\"status\":\"failed\",\"redacted\":true}".to_owned()
    };
    if !evohime_local_storage::benchmark_store::save_report(
        &transaction, &job.request.job_id, &report_json, job.state.storage_key(),
        crate::task_memory::now_millis() as i64,
    ).map_err(|_| "benchmark_report_storage_failed".to_string())? {
        return Err("benchmark_report_run_missing".into());
    }
    transaction.commit().map_err(|_| "storage_failed".to_string())?;
    Ok(())
}

async fn execute_adaptation_benchmark(
    job: &crate::local_model_adaptation::AdaptationJob,
    suite: &crate::agent_benchmark_matrix::BenchmarkSuite,
    policy: &crate::agent_benchmark_matrix::BenchmarkPolicy,
    baselines: &std::collections::BTreeMap<String, crate::agent_benchmark_matrix::Baseline>,
) -> Result<crate::agent_benchmark_matrix::BenchmarkReport, String> {
    let probe = serde_json::json!({"op":"adaptation_runtime_probe","job_id":job.request.job_id});
    #[cfg(windows)]
    let runtime = crate::analysis_kernel::supervisor_command(probe).await
        .map_err(|_| "adaptation_runtime_probe_failed".to_string())?;
    #[cfg(not(windows))]
    let runtime: serde_json::Value = return Err("quantizer_requires_windows_supervisor".into());
    if runtime.get("accepted") != Some(&serde_json::Value::Bool(true))
        || runtime.get("state").and_then(serde_json::Value::as_str) != Some("ready") {
        return Err("adaptation_runtime_not_ready".into());
    }
    let port = runtime.get("port").and_then(serde_json::Value::as_u64)
        .and_then(|port| u16::try_from(port).ok()).filter(|port| *port != 0)
        .ok_or_else(|| "adaptation_runtime_invalid".to_string())?;
    let alias = runtime.get("model_alias").and_then(serde_json::Value::as_str)
        .ok_or_else(|| "adaptation_runtime_invalid".to_string())?;
    let output_hash = job.evidence.output_sha256.as_deref()
        .ok_or_else(|| "adaptation_output_missing".to_string())?;
    if alias != format!("evohime-adaptation-{}", &output_hash[..16]) {
        return Err("adaptation_runtime_identity_mismatch".into());
    }
    crate::agent_benchmark_matrix::run_local_model_matrix(
        suite, policy, &job.request.job_id, "local-adaptation-v1", port, alias, baselines,
    ).await.map_err(|_| "real_benchmark_failed_closed".to_string())
}

#[cfg(windows)]
async fn ensure_adaptation_runtime_ready(
    job: &crate::local_model_adaptation::AdaptationJob,
) -> Result<bool, String> {
    let hardware = crate::local_model_runtime_manager::discover_hardware()
        .map_err(|_| "adaptation_hardware_unavailable".to_string())?;
    let available_memory = crate::local_model_runtime_manager::available_memory_bytes()
        .map_err(|_| "adaptation_hardware_unavailable".to_string())?;
    if hardware.ram_bytes < 1024 * 1024 * 1024 || available_memory < 512 * 1024 * 1024 {
        return Err("adaptation_resources_unavailable".into());
    }
    let memory_limit = hardware.ram_bytes.min(available_memory)
        .saturating_mul(3).checked_div(4).unwrap_or(0)
        .clamp(512 * 1024 * 1024, 32 * 1024 * 1024 * 1024);
    let output_hash = job.evidence.output_sha256.as_deref()
        .ok_or_else(|| "adaptation_output_missing".to_string())?;
    let started = match crate::analysis_kernel::supervisor_command(serde_json::json!({
        "op":"adaptation_runtime_start",
        "job_id":job.request.job_id,
        "model_relative_path":crate::local_model_adaptation::staging_relative_path(&job.request.job_id),
        "model_sha256":output_hash,
        "model_size_bytes":job.evidence.output_size_bytes,
        "threads":std::thread::available_parallelism().map(usize::from).unwrap_or(1).clamp(1, 16),
        "memory_limit_bytes":memory_limit,
        "cpu_limit_percent":75
    })).await {
        Ok(started) => started,
        Err(_) => {
            stop_adaptation_runtime(&job.request.job_id).await
                .map_err(|_| "adaptation_runtime_cleanup_failed".to_string())?;
            return Err("adaptation_runtime_start_failed".into());
        }
    };
    if started.get("accepted") != Some(&serde_json::Value::Bool(true)) {
        return Err(started.get("reason").and_then(serde_json::Value::as_str)
            .unwrap_or("adaptation_runtime_start_rejected").to_owned());
    }
    let runtime = match crate::analysis_kernel::supervisor_command(serde_json::json!({
        "op":"adaptation_runtime_probe", "job_id":job.request.job_id
    })).await {
        Ok(runtime) => runtime,
        Err(_) => {
            stop_adaptation_runtime(&job.request.job_id).await
                .map_err(|_| "adaptation_runtime_cleanup_failed".to_string())?;
            return Err("adaptation_runtime_probe_failed".into());
        }
    };
    if runtime.get("accepted") != Some(&serde_json::Value::Bool(true)) {
        stop_adaptation_runtime(&job.request.job_id).await
            .map_err(|_| "adaptation_runtime_cleanup_failed".to_string())?;
        return Err("adaptation_runtime_not_ready".into());
    }
    if runtime.get("state").and_then(serde_json::Value::as_str) != Some("ready") {
        return Ok(false);
    }
    let Some(alias) = runtime.get("model_alias").and_then(serde_json::Value::as_str) else {
        stop_adaptation_runtime(&job.request.job_id).await
            .map_err(|_| "adaptation_runtime_cleanup_failed".to_string())?;
        return Err("adaptation_runtime_invalid".into());
    };
    if alias != format!("evohime-adaptation-{}", &output_hash[..16]) {
        stop_adaptation_runtime(&job.request.job_id).await
            .map_err(|_| "adaptation_runtime_cleanup_failed".to_string())?;
        return Err("adaptation_runtime_identity_mismatch".into());
    }
    Ok(true)
}

#[cfg(not(windows))]
async fn ensure_adaptation_runtime_ready(
    _job: &crate::local_model_adaptation::AdaptationJob,
) -> Result<bool, String> {
    Err("quantizer_requires_windows_supervisor".into())
}

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
                        "ollama_pull" => {
                            let model = request
                                .model_id
                                .as_deref()
                                .filter(|value| !value.is_empty())
                                .ok_or_else(|| "model_id_required".to_string())?;
                            let base_url = request
                                .base_url
                                .as_deref()
                                .unwrap_or(evohime_model_gateway::providers::ollama::DEFAULT_BASE_URL);
                            let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
                            let pull = evohime_model_gateway::providers::ollama::pull_model_with_progress(
                                base_url,
                                model,
                                move |progress| {
                                    let _ = progress_tx.send(progress);
                                },
                            );
                            tokio::pin!(pull);
                            let pull_result = loop {
                                tokio::select! {
                                    result = &mut pull => break result,
                                    Some(progress) = progress_rx.recv() => {
                                        let completed = progress.completed_bytes;
                                        let total = progress.total_bytes;
                                        let percent = ollama_pull_percent(completed, total);
                                        let phase = if percent.is_some() {
                                            "downloading"
                                        } else {
                                            "preparing"
                                        };
                                        let projection = serde_json::json!({
                                            "status": phase,
                                            "model": model,
                                            "stage": progress.status,
                                            "completed_bytes": completed,
                                            "total_bytes": total,
                                            "percent": percent,
                                            "redacted": true
                                        });
                                        let event = CoreEvent::LocalModelRuntimeManager {
                                            operation: "ollama_pull".into(),
                                            version: expected_version,
                                            projection_json: projection.to_string(),
                                        };
                                        TaskCoordinator::emit_state_event(&state, event).await;
                                    }
                                }
                            };
                            pull_result.map_err(|error| error.to_string())?;
                            serde_json::to_vec(&serde_json::json!({
                                "status": "pulled",
                                "model": model,
                                "redacted": true
                            }))
                            .map_err(|_| "serialization_failed".to_string())
                        }
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
                        "adapter_status" => {
                            let tools_root = crate::get_data_directory().join("tools");
                            let adapter_dir = tools_root.join("llama.cpp").join(crate::local_model_adaptation::LLAMA_CPP_VERSION);
                            let installed = crate::local_model_adaptation::adapter_install_is_valid(&adapter_dir);
                            serde_json::to_vec(&serde_json::json!({
                                "status": if installed { "installed" } else { "unavailable" },
                                "adapter_id": "llama.cpp-cpu",
                                "version": crate::local_model_adaptation::LLAMA_CPP_VERSION,
                                "archive_sha256": crate::local_model_adaptation::LLAMA_CPP_ARCHIVE_SHA256,
                                "archive_size_bytes": crate::local_model_adaptation::LLAMA_CPP_ARCHIVE_SIZE_BYTES,
                                "explicit_install_required": !installed,
                                "redacted": true
                            })).map_err(|_| "serialization_failed".to_string())
                        }
                        "install_adapter" => {
                            let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
                            let data_root = crate::get_data_directory();
                            let install = crate::local_model_adaptation::install_pinned_adapter(
                                &data_root,
                                progress_tx,
                            );
                            tokio::pin!(install);
                            let _installed_at = loop {
                                tokio::select! {
                                    result = &mut install => break result.map_err(|error| error.to_string())?,
                                    Some(progress) = progress_rx.recv() => {
                                        let projection = serde_json::json!({
                                            "status": progress.phase,
                                            "adapter_id": "llama.cpp-cpu",
                                            "version": crate::local_model_adaptation::LLAMA_CPP_VERSION,
                                            "completed_bytes": progress.completed_bytes,
                                            "total_bytes": progress.total_bytes,
                                            "redacted": true
                                        });
                                        TaskCoordinator::emit_state_event(&state, CoreEvent::LocalModelRuntimeManager {
                                            operation: "install_adapter".into(),
                                            version: expected_version,
                                            projection_json: projection.to_string(),
                                        }).await;
                                    }
                                }
                            };
                            serde_json::to_vec(&serde_json::json!({
                                "status": "installed",
                                "adapter_id": "llama.cpp-cpu",
                                "version": crate::local_model_adaptation::LLAMA_CPP_VERSION,
                                "archive_sha256": crate::local_model_adaptation::LLAMA_CPP_ARCHIVE_SHA256,
                                "archive_size_bytes": crate::local_model_adaptation::LLAMA_CPP_ARCHIVE_SIZE_BYTES,
                                "redacted": true
                            })).map_err(|_| "serialization_failed".to_string())
                        }
                        "adaptation_create" => {
                            let adaptation = request.adaptation.ok_or_else(|| "adaptation_required".to_string())?;
                            adaptation.validate().map_err(|_| "invalid_adaptation_request".to_string())?;
                            let suite: crate::agent_benchmark_matrix::BenchmarkSuite = serde_json::from_value(
                                request.benchmark_suite.clone().ok_or_else(|| "benchmark_suite_required".to_string())?
                            ).map_err(|_| "invalid_benchmark_suite".to_string())?;
                            let policy: crate::agent_benchmark_matrix::BenchmarkPolicy = serde_json::from_value(
                                request.benchmark_policy.clone().ok_or_else(|| "benchmark_policy_required".to_string())?
                            ).map_err(|_| "invalid_benchmark_policy".to_string())?;
                            let baselines: std::collections::BTreeMap<String, crate::agent_benchmark_matrix::Baseline> = serde_json::from_value(
                                request.benchmark_baselines.clone().ok_or_else(|| "benchmark_baselines_required".to_string())?
                            ).map_err(|_| "invalid_benchmark_baselines".to_string())?;
                            suite.validate().map_err(|_| "invalid_benchmark_suite".to_string())?;
                            policy.validate().map_err(|_| "invalid_benchmark_policy".to_string())?;
                            if policy.mode != crate::agent_benchmark_matrix::BenchmarkMode::Real
                                || suite.canonical_hash().map_err(|_| "benchmark_suite_hash_failed".to_string())?
                                    != adaptation.benchmark_suite_sha256
                                || crate::local_model_runtime_manager::canonical_hash(&(&policy, &baselines))
                                    != adaptation.baseline_sha256 {
                                return Err("frozen_benchmark_identity_mismatch".into());
                            }
                            let benchmark_input = serde_json::to_vec(&(&suite, &policy, &baselines))
                                .map_err(|_| "benchmark_input_serialization_failed".to_string())?;
                            if benchmark_input.len() > 192 * 1024 {
                                return Err("benchmark_input_too_large".into());
                            }
                            let benchmark_input_hash = crate::local_model_runtime_manager::canonical_hash(&benchmark_input);
                            let request_hash = adaptation.content_sha256().map_err(|_| "invalid_adaptation_request".to_string())?;
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let db = journal.database().lock().await;
                            let mut policy_found = false;
                            for row in evohime_local_storage::approval_policy_profiles_store::list(db.connection())
                                .map_err(|_| "approval_policy_storage_failed".to_string())? {
                                let profile: crate::approval_policy_profiles::ApprovalPolicyProfile =
                                    serde_json::from_slice(&row).map_err(|_| "approval_policy_corrupt".to_string())?;
                                if profile.id != adaptation.approval_policy_id { continue; }
                                crate::approval_policy_profiles::validate(&profile)
                                    .map_err(|_| "approval_policy_invalid".to_string())?;
                                if !profile.enabled || profile.version as u64 != adaptation.policy_revision
                                    || crate::local_model_runtime_manager::canonical_hash(&profile)
                                        != adaptation.approval_policy_sha256 {
                                    return Err("approval_policy_identity_mismatch".into());
                                }
                                let resource = format!("model:{}:{}", adaptation.source.model_id, adaptation.source.revision);
                                let decision = crate::approval_policy_profiles::decide(
                                    &profile, &profile.scope_id, "local_model_promotion", &resource, 2,
                                    crate::task_memory::now_millis() as i64,
                                ).map_err(|_| "approval_policy_invalid".to_string())?;
                                if decision.require_prompt || decision.profile_id.as_deref() != Some(profile.id.as_str()) {
                                    return Err("local_model_promotion_approval_required".into());
                                }
                                policy_found = true;
                                break;
                            }
                            if !policy_found {
                                return Err("approval_policy_not_found".into());
                            }
                            for (key, baseline) in &baselines {
                                let expected_key = format!("{}:{}:{}", baseline.challenge_id,
                                    suite.model_profiles.iter().find(|profile| profile.content_hash == baseline.model_profile_hash)
                                        .map(|profile| profile.id.as_str()).unwrap_or_default(),
                                    suite.agent_profiles.iter().find(|profile| profile.content_hash == baseline.agent_profile_hash)
                                        .map(|profile| profile.id.as_str()).unwrap_or_default());
                                if key != &expected_key || baseline.suite_version != suite.version
                                    || !suite.challenges.iter().any(|challenge| challenge.id == baseline.challenge_id)
                                    || !suite.model_profiles.iter().any(|profile| profile.content_hash == baseline.model_profile_hash)
                                    || !suite.agent_profiles.iter().any(|profile| profile.content_hash == baseline.agent_profile_hash)
                                    || baseline.revision == 0 || baseline.metrics.attempts == 0 {
                                    return Err("benchmark_baseline_incompatible".into());
                                }
                                let stored_baseline = evohime_local_storage::benchmark_store::get_baseline(
                                    db.connection(), &baseline.id,
                                ).map_err(|_| "benchmark_baseline_storage_failed".to_string())?
                                    .ok_or_else(|| "benchmark_baseline_not_registered".to_string())?;
                                if stored_baseline.0 != baseline.suite_version
                                    || stored_baseline.1 != baseline.challenge_id
                                    || stored_baseline.2 != baseline.model_profile_hash
                                    || stored_baseline.3 != baseline.agent_profile_hash
                                    || serde_json::from_str::<crate::agent_benchmark_matrix::Metrics>(&stored_baseline.4)
                                        .map_err(|_| "benchmark_baseline_corrupt".to_string())? != baseline.metrics
                                    || stored_baseline.5 != baseline.source_commit
                                    || stored_baseline.6 != baseline.revision {
                                    return Err("benchmark_baseline_identity_mismatch".into());
                                }
                            }
                            if let Some(stored) = evohime_local_storage::local_model_adaptation_store::get_job_by_idempotency_key(
                                db.connection(), &adaptation.idempotency_key,
                            ).map_err(|_| "storage_failed".to_string())? {
                                if stored.2 != request_hash {
                                    return Err("adaptation_idempotency_conflict".into());
                                }
                                let existing: crate::local_model_adaptation::AdaptationJob = serde_json::from_slice(&stored.5)
                                    .map_err(|_| "corrupt_adaptation_job".to_string())?;
                                existing.validate().map_err(|_| "corrupt_adaptation_job".to_string())?;
                                if existing.request.job_id != adaptation.job_id
                                    || stored.0 != existing.revision
                                    || stored.1 != existing.state.storage_key()
                                    || stored.3 != existing.content_sha256
                                {
                                    return Err("adaptation_job_integrity_failed".into());
                                }
                                let stored_input = evohime_local_storage::local_model_adaptation_store::get_benchmark_inputs(
                                    db.connection(), &adaptation.job_id,
                                ).map_err(|_| "benchmark_input_storage_failed".to_string())?
                                    .ok_or_else(|| "benchmark_input_missing".to_string())?;
                                if stored_input.0 != benchmark_input_hash || stored_input.1 != benchmark_input {
                                    return Err("frozen_benchmark_input_conflict".into());
                                }
                                return serde_json::to_vec(&serde_json::json!({"status":"existing","job":existing,"redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            }
                            let retained_jobs = evohime_local_storage::local_model_adaptation_store::list_jobs(
                                db.connection(), crate::local_model_adaptation::MAX_JOBS as u32,
                            ).map_err(|_| "storage_failed".to_string())?;
                            if retained_jobs.len() >= crate::local_model_adaptation::MAX_JOBS {
                                return Err("adaptation_capacity_reached".into());
                            }
                            let model_key = format!("model:{}:{}", adaptation.source.model_id, adaptation.source.revision);
                            let artifact_key = format!("artifact:{}:{}", adaptation.source.model_id, adaptation.source.revision);
                            let model_record = evohime_local_storage::local_model_runtime_manager_store::get_record(db.connection(), &model_key)
                                .map_err(|_| "storage_failed".to_string())?
                                .filter(|(kind, revision, _, _)| kind == "model" && *revision == adaptation.source.revision)
                                .ok_or_else(|| "managed_source_not_found".to_string())?;
                            let model: crate::local_model_runtime_manager::LocalModelDescriptor = serde_json::from_slice(&model_record.3)
                                .map_err(|_| "corrupt_model_record".to_string())?;
                            model.validate().map_err(|_| "invalid_model_record".to_string())?;
                            if model_record.2 != crate::local_model_runtime_manager::canonical_hash(&model_record.3)
                                || model.model_id != adaptation.source.model_id
                                || model.revision != adaptation.source.revision
                                || model.format != "gguf"
                                || model.quantization != adaptation.source.source_quantization
                                || model.artifact_hash != adaptation.source.artifact_sha256
                                || model.artifact_size_bytes != adaptation.source.artifact_size_bytes
                            {
                                return Err("source_model_identity_mismatch".into());
                            }
                            let artifact_record = evohime_local_storage::local_model_runtime_manager_store::get_record(db.connection(), &artifact_key)
                                .map_err(|_| "storage_failed".to_string())?
                                .filter(|(kind, revision, _, _)| kind == "artifact" && *revision == adaptation.source.revision)
                                .ok_or_else(|| "managed_artifact_not_found".to_string())?;
                            let artifact: crate::local_model_runtime_manager::LocalArtifactRecord = serde_json::from_slice(&artifact_record.3)
                                .map_err(|_| "corrupt_artifact_record".to_string())?;
                            artifact.validate().map_err(|_| "invalid_artifact_record".to_string())?;
                            if artifact_record.2 != crate::local_model_runtime_manager::canonical_hash(&artifact_record.3)
                                || artifact.model_id != adaptation.source.model_id
                                || artifact.model_revision != adaptation.source.revision
                                || artifact.state != crate::local_model_runtime_manager::ArtifactState::Installed
                                || artifact.expected_hash != adaptation.source.artifact_sha256
                                || artifact.expected_size_bytes != adaptation.source.artifact_size_bytes
                                || artifact.content_hash.as_deref() != Some(adaptation.source.artifact_sha256.as_str())
                            {
                                return Err("source_artifact_not_verified".into());
                            }
                            let relative_path = artifact.relative_path.as_deref().ok_or_else(|| "source_artifact_path_missing".to_string())?;
                            let relative_path = relative_path.to_owned();
                            drop(db);
                            let models_root = crate::get_data_directory().join("models");
                            let expected_hash = adaptation.source.artifact_sha256.clone();
                            let expected_size = adaptation.source.artifact_size_bytes;
                            let source_quantization = adaptation.source.source_quantization.clone();
                            tokio::task::spawn_blocking(move || {
                                let path = crate::local_model_runtime_manager::verify_managed_artifact(
                                    &models_root,
                                    std::path::Path::new(&relative_path),
                                    &expected_hash,
                                    expected_size,
                                ).map_err(|_| "source_artifact_verification_failed")?;
                                crate::local_model_adaptation::verify_gguf_source(
                                    &path,
                                    &source_quantization,
                                ).map_err(|_| "source_gguf_verification_failed")
                            }).await.map_err(|_| "source_artifact_verification_failed".to_string())?
                                .map_err(str::to_owned)?;
                            let job = crate::local_model_adaptation::AdaptationJob::create(adaptation.clone())
                                .map_err(|_| "invalid_adaptation_request".to_string())?;
                            let request_json = serde_json::to_vec(&adaptation).map_err(|_| "serialization_failed".to_string())?;
                            let snapshot_json = serde_json::to_vec(&job).map_err(|_| "serialization_failed".to_string())?;
                            let mut db = journal.database().lock().await;
                            let transaction = db.connection_mut().transaction()
                                .map_err(|_| "storage_failed".to_string())?;
                            let inserted = evohime_local_storage::local_model_adaptation_store::put_job(
                                &transaction, &adaptation.job_id, job.revision,
                                job.state.storage_key(), &adaptation.idempotency_key, &request_hash, &job.content_sha256,
                                &request_json, &snapshot_json, crate::task_memory::now_millis() as i64,
                            ).map_err(|_| "storage_failed".to_string())?;
                            if !inserted {
                                drop(transaction);
                                drop(db);
                                let db = journal.database().lock().await;
                                if let Some(stored) = evohime_local_storage::local_model_adaptation_store::get_job_by_idempotency_key(
                                    db.connection(), &adaptation.idempotency_key,
                                ).map_err(|_| "storage_failed".to_string())? {
                                    if stored.2 != request_hash {
                                        return Err("adaptation_idempotency_conflict".into());
                                    }
                                    let existing: crate::local_model_adaptation::AdaptationJob = serde_json::from_slice(&stored.5)
                                        .map_err(|_| "corrupt_adaptation_job".to_string())?;
                                    existing.validate().map_err(|_| "corrupt_adaptation_job".to_string())?;
                                    if existing.request.job_id != adaptation.job_id
                                        || stored.0 != existing.revision
                                        || stored.1 != existing.state.storage_key()
                                        || stored.3 != existing.content_sha256
                                    {
                                        return Err("adaptation_job_integrity_failed".into());
                                    }
                                    let stored_input = evohime_local_storage::local_model_adaptation_store::get_benchmark_inputs(
                                        db.connection(), &adaptation.job_id,
                                    ).map_err(|_| "benchmark_input_storage_failed".to_string())?
                                        .ok_or_else(|| "benchmark_input_missing".to_string())?;
                                    if stored_input.0 != benchmark_input_hash || stored_input.1 != benchmark_input {
                                        return Err("frozen_benchmark_input_conflict".into());
                                    }
                                    return serde_json::to_vec(&serde_json::json!({"status":"existing","job":existing,"redacted":true}))
                                        .map_err(|_| "serialization_failed".to_string());
                                }
                                return Err("adaptation_job_already_exists".into());
                            }
                            if !evohime_local_storage::local_model_adaptation_store::put_benchmark_inputs(
                                &transaction, &adaptation.job_id, &benchmark_input_hash, &benchmark_input,
                                crate::task_memory::now_millis() as i64,
                            ).map_err(|_| "benchmark_input_storage_failed".to_string())? {
                                return Err("benchmark_input_write_conflict".into());
                            }
                            transaction.commit().map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"created","job":job,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "adaptation_start" | "adaptation_poll" | "adaptation_calibrate" | "adaptation_benchmark" => {
                            let job_id = request.job_id.as_deref().filter(|id| {
                                !id.is_empty() && id.len() <= 128 && !id.bytes().any(|byte| byte.is_ascii_control())
                            }).ok_or_else(|| "job_id_required".to_string())?;
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let mut db = journal.database().lock().await;
                            let stored = evohime_local_storage::local_model_adaptation_store::get_job(db.connection(), job_id)
                                .map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "adaptation_job_not_found".to_string())?;
                            let mut job: crate::local_model_adaptation::AdaptationJob = serde_json::from_slice(&stored.5)
                                .map_err(|_| "corrupt_adaptation_job".to_string())?;
                            job.validate().map_err(|_| "corrupt_adaptation_job".to_string())?;
                            if stored.0 != job.revision || stored.1 != job.state.storage_key() || stored.3 != job.content_sha256 {
                                return Err("adaptation_job_integrity_failed".into());
                            }
                            if operation == "adaptation_start" {
                                if !matches!(job.state,
                                    crate::local_model_adaptation::AdaptationState::Created
                                        | crate::local_model_adaptation::AdaptationState::Preflighted
                                        | crate::local_model_adaptation::AdaptationState::WaitingForResources
                                ) {
                                    return Err("adaptation_start_state_conflict".into());
                                }
                                let key = format!("artifact:{}:{}", job.request.source.model_id, job.request.source.revision);
                                let artifact = evohime_local_storage::local_model_runtime_manager_store::get_record(db.connection(), &key)
                                    .map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "managed_artifact_not_found".to_string())?;
                                let record: crate::local_model_runtime_manager::LocalArtifactRecord = serde_json::from_slice(&artifact.3)
                                    .map_err(|_| "corrupt_artifact_record".to_string())?;
                                record.validate().map_err(|_| "invalid_artifact_record".to_string())?;
                                if artifact.2 != crate::local_model_runtime_manager::canonical_hash(&artifact.3)
                                    || record.state != crate::local_model_runtime_manager::ArtifactState::Installed
                                    || record.content_hash.as_deref() != Some(job.request.source.artifact_sha256.as_str())
                                    || record.expected_size_bytes != job.request.source.artifact_size_bytes {
                                    return Err("source_artifact_not_verified".into());
                                }
                                let relative = record.relative_path.ok_or_else(|| "source_artifact_path_missing".to_string())?;
                                let adapter = crate::get_data_directory().join("tools/llama.cpp").join(crate::local_model_adaptation::LLAMA_CPP_VERSION);
                                if !crate::local_model_adaptation::adapter_install_is_valid(&adapter) {
                                    return Err("adapter_unavailable".into());
                                }
                                let now_ms = crate::task_memory::now_millis();
                                let pressure = state.lock().await.host_telemetry.latest().cloned();
                                let Some(pressure) = pressure else {
                                    drop(db);
                                    let job = mark_adaptation_waiting_for_resources(&state, job).await?;
                                    return serde_json::to_vec(&serde_json::json!({"status":"waiting_for_resources","reason":"adaptation_resource_pressure_unavailable","job":job,"redacted":true}))
                                        .map_err(|_| "serialization_failed".to_string());
                                };
                                if pressure.pressure != crate::host_resource_telemetry::PressureLevel::Normal
                                    || !pressure.cpu_percent.is_current(now_ms, 30_000)
                                    || !pressure.memory_available_percent.is_current(now_ms, 30_000)
                                    || !pressure.storage_free_percent.is_current(now_ms, 30_000)
                                {
                                    drop(db);
                                    let job = mark_adaptation_waiting_for_resources(&state, job).await?;
                                    return serde_json::to_vec(&serde_json::json!({"status":"waiting_for_resources","reason":"adaptation_resource_pressure_high_or_stale","job":job,"redacted":true}))
                                        .map_err(|_| "serialization_failed".to_string());
                                }
                                #[cfg(windows)]
                                let hardware = crate::local_model_runtime_manager::discover_hardware()
                                    .map_err(|_| "adaptation_hardware_unavailable".to_string())?;
                                #[cfg(not(windows))]
                                let hardware: crate::local_model_runtime_manager::LocalHardwareProfile =
                                    return Err("adaptation_requires_windows".into());
                                #[cfg(windows)]
                                let available_memory = crate::local_model_runtime_manager::available_memory_bytes()
                                    .map_err(|_| "adaptation_hardware_unavailable".to_string())?;
                                #[cfg(not(windows))]
                                let available_memory: u64 = return Err("adaptation_requires_windows".into());
                                let minimum_disk_bytes = job.request.source.artifact_size_bytes
                                    .checked_add(job.request.max_output_bytes)
                                    .and_then(|bytes| bytes.checked_add(1024 * 1024 * 1024))
                                    .ok_or_else(|| "adaptation_disk_reservation_overflow".to_string())?;
                                let already_reserved = evohime_local_storage::local_model_adaptation_store::active_disk_reservations(
                                    db.connection(), job_id,
                                ).map_err(|_| "adaptation_disk_reservation_read_failed".to_string())?;
                                let required_disk_bytes = minimum_disk_bytes
                                    .checked_add(already_reserved)
                                    .ok_or_else(|| "adaptation_disk_reservation_overflow".to_string())?;
                                if hardware.ram_bytes < 1024 * 1024 * 1024
                                    || available_memory < 512 * 1024 * 1024
                                    || hardware.disk_free_bytes < required_disk_bytes
                                {
                                    drop(db);
                                    let job = mark_adaptation_waiting_for_resources(&state, job).await?;
                                    return serde_json::to_vec(&serde_json::json!({"status":"waiting_for_resources","reason":"adaptation_resources_unavailable","job":job,"redacted":true}))
                                        .map_err(|_| "serialization_failed".to_string());
                                }
                                let process_memory_limit = hardware.ram_bytes.min(available_memory)
                                    .saturating_mul(3)
                                    .checked_div(4)
                                    .unwrap_or(0)
                                    .clamp(512 * 1024 * 1024, 32 * 1024 * 1024 * 1024);
                                if job.state == crate::local_model_adaptation::AdaptationState::Created {
                                    job.transition(crate::local_model_adaptation::AdaptationState::Preflighted, job.evidence.clone())
                                        .map_err(|_| "adaptation_transition_denied".to_string())?;
                                    let snapshot = serde_json::to_vec(&job).map_err(|_| "serialization_failed".to_string())?;
                                    if !evohime_local_storage::local_model_adaptation_store::put_job(
                                        db.connection(), job_id, job.revision, job.state.storage_key(),
                                        &job.request.idempotency_key, &stored.2, &job.content_sha256,
                                        &stored.4, &snapshot, crate::task_memory::now_millis() as i64,
                                    ).map_err(|_| "storage_failed".to_string())? {
                                        return Err("adaptation_job_revision_conflict".into());
                                    }
                                }
                                // Fence duplicate start requests durably before the external
                                // process can be accepted. Recovery treats this state as an
                                // in-flight process and deterministically interrupts it.
                                let previous_revision = job.revision;
                                job.transition(crate::local_model_adaptation::AdaptationState::Running, job.evidence.clone())
                                    .map_err(|_| "adaptation_transition_denied".to_string())?;
                                let snapshot = serde_json::to_vec(&job).map_err(|_| "serialization_failed".to_string())?;
                                let mut transaction = db.connection_mut().transaction()
                                    .map_err(|_| "adaptation_start_transaction_failed".to_string())?;
                                let current = evohime_local_storage::local_model_adaptation_store::get_job(&transaction, job_id)
                                    .map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "adaptation_job_not_found".to_string())?;
                                if current.0 != previous_revision {
                                    return Err("adaptation_job_revision_conflict".into());
                                }
                                if !evohime_local_storage::local_model_adaptation_store::reserve_disk(
                                    &transaction, job_id, minimum_disk_bytes,
                                    crate::task_memory::now_millis() as i64,
                                ).map_err(|_| "adaptation_disk_reservation_write_failed".to_string())? {
                                    return Err("adaptation_disk_reservation_conflict".into());
                                }
                                if !evohime_local_storage::local_model_adaptation_store::put_job(
                                        &transaction, job_id, job.revision, job.state.storage_key(),
                                        &job.request.idempotency_key, &current.2, &job.content_sha256,
                                        &current.4, &snapshot, crate::task_memory::now_millis() as i64,
                                    ).map_err(|_| "storage_failed".to_string())? {
                                    return Err("adaptation_job_revision_conflict".into());
                                }
                                transaction.commit()
                                    .map_err(|_| "adaptation_start_transaction_failed".to_string())?;
                                drop(db);
                                let command = serde_json::json!({
                                    "op":"adaptation_quantize_start", "job_id":job_id,
                                    "source_relative_path":relative,
                                    "source_sha256":job.request.source.artifact_sha256,
                                    "source_size_bytes":job.request.source.artifact_size_bytes,
                                    "target":job.request.target.llama_argument(),
                                    "threads":std::thread::available_parallelism().map(usize::from).unwrap_or(1).clamp(1, 16),
                                    "memory_limit_bytes":process_memory_limit,
                                    "cpu_limit_percent":75,
                                    "max_output_bytes":job.request.max_output_bytes
                                });
                                #[cfg(windows)]
                                let response = crate::analysis_kernel::supervisor_command(command).await
                                    .map_err(|_| "quantizer_dispatch_failed".to_string())?;
                                #[cfg(not(windows))]
                                let response: serde_json::Value = return Err("quantizer_requires_windows_supervisor".into());
                                if response.get("accepted") != Some(&serde_json::Value::Bool(true)) {
                                    if response.get("reason").and_then(serde_json::Value::as_str)
                                        == Some("capacity_or_duplicate") {
                                        let job = transition_adaptation_job(
                                            &state,
                                            job,
                                            crate::local_model_adaptation::AdaptationState::WaitingForResources,
                                        ).await?;
                                        return serde_json::to_vec(&serde_json::json!({
                                            "status":"waiting_for_resources",
                                            "reason":"supervisor_process_capacity",
                                            "retry_required":true,
                                            "job":job,
                                            "redacted":true
                                        })).map_err(|_| "serialization_failed".to_string());
                                    }
                                    let job = transition_adaptation_job(
                                        &state,
                                        job,
                                        crate::local_model_adaptation::AdaptationState::Failed,
                                    ).await?;
                                    return serde_json::to_vec(&serde_json::json!({"status":"failed","job":job,"reason":"quantizer_dispatch_rejected","redacted":true}))
                                        .map_err(|_| "serialization_failed".to_string());
                                }
                                let journal = state.lock().await.journal.clone()
                                    .ok_or_else(|| "storage journal is not configured".to_string())?;
                                let current = {
                                    let database = journal.database().lock().await;
                                    evohime_local_storage::local_model_adaptation_store::get_job(
                                        database.connection(), job_id,
                                    )
                                };
                                let current = match current {
                                    Ok(Some(current)) => current,
                                    _ => {
                                        #[cfg(windows)]
                                        cancel_supervisor_quantizer(job_id).await?;
                                        return Err("adaptation_job_state_unavailable_after_dispatch".into());
                                    }
                                };
                                let durable_job: crate::local_model_adaptation::AdaptationJob =
                                    match serde_json::from_slice(&current.5) {
                                        Ok(job) => job,
                                        Err(_) => {
                                            #[cfg(windows)]
                                            cancel_supervisor_quantizer(job_id).await?;
                                            return Err("corrupt_adaptation_job_after_dispatch".into());
                                        }
                                    };
                                if durable_job.validate().is_err() || current.1 != durable_job.state.storage_key()
                                    || current.3 != durable_job.content_sha256 {
                                    #[cfg(windows)]
                                    cancel_supervisor_quantizer(job_id).await?;
                                    return Err("adaptation_job_integrity_failed_after_dispatch".into());
                                }
                                if current.0 != job.revision
                                    || durable_job.state != crate::local_model_adaptation::AdaptationState::Running {
                                    #[cfg(windows)]
                                    cancel_supervisor_quantizer(job_id).await?;
                                    return serde_json::to_vec(&serde_json::json!({
                                        "status":durable_job.state.storage_key(),"job":durable_job,"redacted":true
                                    })).map_err(|_| "serialization_failed".to_string());
                                }
                                return serde_json::to_vec(&serde_json::json!({"status":"running","job":job,"redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            }
                            if operation == "adaptation_calibrate" {
                                if job.state != crate::local_model_adaptation::AdaptationState::Benchmarking {
                                    return Err("adaptation_not_benchmarking".into());
                                }
                                if job.evidence.calibration_sha256.is_some() {
                                    return serde_json::to_vec(&serde_json::json!({"status":"already_calibrated","job":job,"redacted":true}))
                                        .map_err(|_| "serialization_failed".to_string());
                                }
                                drop(db);
                                let probe = serde_json::json!({"op":"adaptation_runtime_probe","job_id":job_id});
                                #[cfg(windows)]
                                let runtime = crate::analysis_kernel::supervisor_command(probe).await
                                    .map_err(|_| "adaptation_runtime_probe_failed".to_string())?;
                                #[cfg(not(windows))]
                                let runtime: serde_json::Value = return Err("quantizer_requires_windows_supervisor".into());
                                if runtime.get("accepted") != Some(&serde_json::Value::Bool(true))
                                    || runtime.get("state").and_then(serde_json::Value::as_str) != Some("ready") {
                                    return Err("adaptation_runtime_not_ready".into());
                                }
                                let port = runtime.get("port").and_then(serde_json::Value::as_u64)
                                    .and_then(|port| u16::try_from(port).ok()).ok_or_else(|| "adaptation_runtime_invalid".to_string())?;
                                let alias = runtime.get("model_alias").and_then(serde_json::Value::as_str)
                                    .ok_or_else(|| "adaptation_runtime_invalid".to_string())?;
                                let output_hash = job.evidence.output_sha256.as_deref()
                                    .ok_or_else(|| "adaptation_output_missing".to_string())?;
                                let expected_alias = format!("evohime-adaptation-{}", &output_hash[..16]);
                                if alias != expected_alias {
                                    let job = transition_adaptation_job(&state, job, crate::local_model_adaptation::AdaptationState::Failed).await?;
                                    return serde_json::to_vec(&serde_json::json!({"status":"failed","job":job,"reason":"adaptation_runtime_identity_mismatch","redacted":true}))
                                        .map_err(|_| "serialization_failed".to_string());
                                }
                                let stream = crate::local_model_adaptation::local_inference_stream(
                                    port, alias, "Reply with exactly: adaptation-runtime-ready", 16,
                                    Some("adaptation-runtime-ready"),
                                ).await.map_err(|_| "adaptation_inference_unavailable".to_string())?;
                                if stream.expected_match != Some(true) {
                                    let job = transition_adaptation_job(&state, job, crate::local_model_adaptation::AdaptationState::Failed).await?;
                                    return serde_json::to_vec(&serde_json::json!({"status":"failed","job":job,"reason":"adaptation_runtime_probe_mismatch","redacted":true}))
                                        .map_err(|_| "serialization_failed".to_string());
                                }
                                let calibration_hash = crate::local_model_runtime_manager::canonical_hash(&stream);
                                job.evidence.calibration_sha256 = Some(calibration_hash);
                                job.transition(crate::local_model_adaptation::AdaptationState::Benchmarking, job.evidence.clone())
                                    .map_err(|_| "adaptation_transition_denied".to_string())?;
                                let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                                let db = journal.database().lock().await;
                                let current = evohime_local_storage::local_model_adaptation_store::get_job(db.connection(), job_id)
                                    .map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "adaptation_job_not_found".to_string())?;
                                if current.0 + 1 != job.revision { return Err("adaptation_job_revision_conflict".into()); }
                                let snapshot = serde_json::to_vec(&job).map_err(|_| "serialization_failed".to_string())?;
                                if !evohime_local_storage::local_model_adaptation_store::put_job(db.connection(), job_id, job.revision,
                                    job.state.storage_key(), &job.request.idempotency_key, &current.2, &job.content_sha256,
                                    &current.4, &snapshot, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? {
                                    return Err("adaptation_job_revision_conflict".into());
                                }
                                return serde_json::to_vec(&serde_json::json!({"status":"calibrated","job":job,"inference":stream,"redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            }
                            if operation == "adaptation_benchmark" {
                                if job.state != crate::local_model_adaptation::AdaptationState::Benchmarking {
                                    return Err("adaptation_not_benchmarking".into());
                                }
                                if job.evidence.calibration_sha256.is_none() {
                                    return Err("adaptation_calibration_required".into());
                                }
                                let resume_existing_run = job.evidence.benchmark_started;
                                if job.evidence.benchmark_sha256.is_some() {
                                    return Err("adaptation_benchmark_already_completed".into());
                                }
                                if resume_existing_run
                                    && state.lock().await.adaptation_benchmark_cancellations.contains_key(job_id)
                                {
                                    return serde_json::to_vec(&serde_json::json!({"status":"benchmarking","job":job,"already_running":true,"redacted":true}))
                                        .map_err(|_| "serialization_failed".to_string());
                                }
                                let suite: crate::agent_benchmark_matrix::BenchmarkSuite = serde_json::from_value(
                                    request.benchmark_suite.clone().ok_or_else(|| "benchmark_suite_required".to_string())?
                                ).map_err(|_| "invalid_benchmark_suite".to_string())?;
                                let policy: crate::agent_benchmark_matrix::BenchmarkPolicy = serde_json::from_value(
                                    request.benchmark_policy.clone().ok_or_else(|| "benchmark_policy_required".to_string())?
                                ).map_err(|_| "invalid_benchmark_policy".to_string())?;
                                let baselines: std::collections::BTreeMap<String, crate::agent_benchmark_matrix::Baseline> = serde_json::from_value(
                                    request.benchmark_baselines.clone().ok_or_else(|| "benchmark_baselines_required".to_string())?
                                ).map_err(|_| "invalid_benchmark_baselines".to_string())?;
                                let suite_hash = suite.canonical_hash().map_err(|_| "benchmark_suite_hash_failed".to_string())?;
                                let baseline_hash = crate::local_model_runtime_manager::canonical_hash(&(&policy, &baselines));
                                if suite_hash != job.request.benchmark_suite_sha256
                                    || baseline_hash != job.request.baseline_sha256 {
                                    return Err("frozen_benchmark_identity_mismatch".into());
                                }
                                let benchmark_input = serde_json::to_vec(&(&suite, &policy, &baselines))
                                    .map_err(|_| "benchmark_input_serialization_failed".to_string())?;
                                if benchmark_input.len() > 192 * 1024 {
                                    return Err("benchmark_input_too_large".into());
                                }
                                let benchmark_input_hash = crate::local_model_runtime_manager::canonical_hash(&benchmark_input);
                                if let Some((stored_hash, stored_input)) =
                                    evohime_local_storage::local_model_adaptation_store::get_benchmark_inputs(
                                        db.connection(), job_id,
                                    ).map_err(|_| "benchmark_input_storage_failed".to_string())? {
                                    if stored_hash != benchmark_input_hash || stored_input != benchmark_input {
                                        return Err("frozen_benchmark_input_conflict".into());
                                    }
                                }
                                for baseline in baselines.values() {
                                    let stored_baseline = evohime_local_storage::local_model_adaptation_store::get_benchmark_baseline(
                                        db.connection(), &baseline.id,
                                    ).map_err(|_| "benchmark_baseline_storage_failed".to_string())?
                                        .ok_or_else(|| "benchmark_baseline_not_registered".to_string())?;
                                    let stored_metrics: crate::agent_benchmark_matrix::Metrics = serde_json::from_str(&stored_baseline.4)
                                        .map_err(|_| "benchmark_baseline_corrupt".to_string())?;
                                    if stored_baseline.0 != baseline.suite_version
                                        || stored_baseline.1 != baseline.challenge_id
                                        || stored_baseline.2 != baseline.model_profile_hash
                                        || stored_baseline.3 != baseline.agent_profile_hash
                                        || stored_metrics != baseline.metrics
                                        || stored_baseline.5 != baseline.source_commit
                                        || stored_baseline.6 != baseline.revision {
                                        return Err("benchmark_baseline_identity_mismatch".into());
                                    }
                                }
                                let policy_json = serde_json::to_string(&(&policy, &baselines))
                                    .map_err(|_| "benchmark_policy_serialization_failed".to_string())?;
                                drop(db);
                                let runtime_ready = ensure_adaptation_runtime_ready(&job).await?;
                                let mut db = journal.database().lock().await;
                                let current = evohime_local_storage::local_model_adaptation_store::get_job(
                                    db.connection(), job_id,
                                ).map_err(|_| "storage_failed".to_string())?
                                    .ok_or_else(|| "adaptation_job_not_found".to_string())?;
                                if current.0 != job.revision
                                    || current.1 != crate::local_model_adaptation::AdaptationState::Benchmarking.storage_key()
                                    || current.2 != job.request.content_sha256().map_err(|_| "adaptation_job_integrity_failed".to_string())?
                                    || current.3 != job.content_sha256
                                    || current.4 != serde_json::to_vec(&job.request).map_err(|_| "adaptation_job_integrity_failed".to_string())? {
                                    let cancellation_pending = matches!(
                                        current.1.as_str(), "cancelling" | "rejecting" | "cancelled" | "rejected"
                                    );
                                    drop(db);
                                    if cancellation_pending {
                                        stop_adaptation_runtime(job_id).await?;
                                    }
                                    return Err("adaptation_job_revision_conflict".into());
                                }
                                if !runtime_ready {
                                    return serde_json::to_vec(&serde_json::json!({
                                        "status":"benchmarking","job":job,
                                        "runtime":"loading","retry_required":true,"redacted":true
                                    })).map_err(|_| "serialization_failed".to_string());
                                }
                                // Reserve a bounded background slot before writing the run row.
                                // Otherwise resource pressure can leave a durable run stuck in `running`
                                // even though no executor ever accepted it.
                                let task_group = Arc::clone(&state.lock().await.background_tasks);
                                let permit = task_group.try_acquire()
                                    .ok_or_else(|| "adaptation_background_capacity_reached".to_string())?;
                                let transaction = db.connection_mut().transaction()
                                    .map_err(|_| "benchmark_run_storage_failed".to_string())?;
                                if resume_existing_run {
                                    let existing_run = evohime_local_storage::benchmark_store::get_run(
                                        &transaction, job_id,
                                    ).map_err(|_| "benchmark_run_storage_failed".to_string())?
                                        .ok_or_else(|| "benchmark_run_not_resumable".to_string())?;
                                    let existing_policy = evohime_local_storage::benchmark_store::get_run_policy_json(
                                        &transaction, job_id,
                                    ).map_err(|_| "benchmark_run_storage_failed".to_string())?
                                        .ok_or_else(|| "benchmark_run_not_resumable".to_string())?;
                                    if existing_run.0 != suite.id || existing_run.1 != suite.version
                                        || existing_run.2 != "running" || existing_run.3.is_some()
                                        || existing_policy != policy_json {
                                        return Err("benchmark_run_not_resumable".into());
                                    }
                                } else if !evohime_local_storage::benchmark_store::save_run(
                                    &transaction, job_id, &suite.id, &suite.version, &policy_json,
                                    "running", crate::task_memory::now_millis() as i64,
                                ).map_err(|_| "benchmark_run_storage_failed".to_string())? {
                                    return Err("benchmark_run_id_conflict".into());
                                }
                                let previous_revision = job.revision;
                                let stored_input = evohime_local_storage::local_model_adaptation_store::get_benchmark_inputs(
                                    &transaction, job_id,
                                ).map_err(|_| "benchmark_input_storage_failed".to_string())?
                                    .ok_or_else(|| "benchmark_input_missing".to_string())?;
                                if stored_input.0 != benchmark_input_hash || stored_input.1 != benchmark_input {
                                    return Err("frozen_benchmark_input_conflict".into());
                                }
                                job.evidence.benchmark_started = true;
                                job.transition(
                                    crate::local_model_adaptation::AdaptationState::Benchmarking,
                                    job.evidence.clone(),
                                ).map_err(|_| "adaptation_transition_denied".to_string())?;
                                let snapshot = serde_json::to_vec(&job).map_err(|_| "serialization_failed".to_string())?;
                                if !evohime_local_storage::local_model_adaptation_store::put_job(
                                    &transaction, job_id, job.revision, job.state.storage_key(),
                                    &job.request.idempotency_key, &stored.2, &job.content_sha256,
                                    &stored.4, &snapshot, crate::task_memory::now_millis() as i64,
                                ).map_err(|_| "storage_failed".to_string())? {
                                    return Err("adaptation_job_revision_conflict".into());
                                }
                                transaction.commit().map_err(|_| "benchmark_run_storage_failed".to_string())?;
                                let cancellation = tokio_util::sync::CancellationToken::new();
                                state.lock().await.adaptation_benchmark_cancellations.insert(
                                    job_id.to_owned(), cancellation.clone(),
                                );
                                drop(db);
                                let current = {
                                    let database = journal.database().lock().await;
                                    evohime_local_storage::local_model_adaptation_store::get_job(
                                        database.connection(), job_id,
                                    )
                                };
                                let current = match current {
                                    Ok(Some(current)) => current,
                                    _ => {
                                        cancellation.cancel();
                                        state.lock().await.adaptation_benchmark_cancellations.remove(job_id);
                                        return Err("adaptation_job_state_unavailable_after_dispatch".into());
                                    }
                                };
                                let current_job: crate::local_model_adaptation::AdaptationJob =
                                    match serde_json::from_slice(&current.5) {
                                        Ok(job) => job,
                                        Err(_) => {
                                            cancellation.cancel();
                                            state.lock().await.adaptation_benchmark_cancellations.remove(job_id);
                                            return Err("corrupt_adaptation_job_after_benchmark_dispatch".into());
                                        }
                                };
                                let request_hash = current_job.request.content_sha256();
                                let request_snapshot = serde_json::to_vec(&current_job.request);
                                if current_job.validate().is_err()
                                    || request_hash.as_deref().ok() != Some(current.2.as_str())
                                    || request_snapshot.as_deref().ok() != Some(current.4.as_slice())
                                    || current.1 != current_job.state.storage_key()
                                    || current.3 != current_job.content_sha256
                                    || current.0 != job.revision
                                    || current_job.state != crate::local_model_adaptation::AdaptationState::Benchmarking {
                                    cancellation.cancel();
                                    state.lock().await.adaptation_benchmark_cancellations.remove(job_id);
                                    return serde_json::to_vec(&serde_json::json!({
                                        "status":current_job.state.storage_key(),"job":current_job,"redacted":true
                                    })).map_err(|_| "serialization_failed".to_string());
                                }
                                let state_for_task = Arc::clone(&state);
                                let job_for_task = job.clone();
                                task_group.spawn_reserved(permit, async move {
                                    let execution = tokio::select! {
                                        _ = cancellation.cancelled() => Err("adaptation_cancelled".to_owned()),
                                        result = execute_adaptation_benchmark(
                                            &job_for_task, &suite, &policy, &baselines,
                                        ) => result,
                                    };
                                    let report = match stop_adaptation_runtime(&job_for_task.request.job_id).await {
                                        Ok(()) => execution,
                                        Err(_) => Err("adaptation_runtime_stop_failed".to_owned()),
                                    };
                                    let benchmark_cancelled = matches!(
                                        &report,
                                        Err(reason) if reason == "adaptation_cancelled"
                                    );
                                    let finished_job_id = job_for_task.request.job_id.clone();
                                    let mut finished = job_for_task;
                                    let (next, report_json, stored_report) = match report {
                                        Ok(report) => {
                                            let report_within_limit = serde_json::to_vec(&report)
                                                .is_ok_and(|encoded| encoded.len() <= 2 * 1024 * 1024);
                                            if !report_within_limit {
                                                (crate::local_model_adaptation::AdaptationState::Failed,
                                                    Some(serde_json::json!({"reason":"benchmark_report_exceeds_storage_limit","redacted":true})),
                                                    None)
                                            } else {
                                                finished.evidence.benchmark_sha256 = Some(
                                                    crate::local_model_runtime_manager::canonical_hash(&report),
                                                );
                                                let pass = !report.comparisons.is_empty() && report.comparisons.values().all(|comparison| {
                                                    !comparison.security_hard_failure && matches!(comparison.verdict,
                                                        crate::agent_benchmark_matrix::ComparisonVerdict::Improved
                                                            | crate::agent_benchmark_matrix::ComparisonVerdict::Stable)
                                                });
                                                (if pass { crate::local_model_adaptation::AdaptationState::ReadyForPromotion }
                                                    else { crate::local_model_adaptation::AdaptationState::Failed },
                                                    Some(serde_json::json!({"report":report.clone(),"redacted":true})),
                                                    Some(report))
                                            }
                                        }
                                        Err(reason) => {
                                            (crate::local_model_adaptation::AdaptationState::Failed,
                                                Some(serde_json::json!({"reason":reason,"redacted":true})),
                                                None)
                                        }
                                    };
                                    let previous = finished.revision;
                                    let completion = match finished.transition(next, finished.evidence.clone()) {
                                        Ok(()) => persist_adaptation_job(
                                            &state_for_task, &finished, previous, stored_report.as_ref(),
                                        ).await,
                                        Err(_) => Err("adaptation_transition_denied".to_owned()),
                                    };
                                    let cancellation_won = if benchmark_cancelled {
                                        true
                                    } else if completion.is_err() {
                                        let journal = state_for_task.lock().await.journal.clone();
                                        if let Some(journal) = journal {
                                            let database = journal.database().lock().await;
                                            evohime_local_storage::local_model_adaptation_store::get_job(
                                                database.connection(), &finished_job_id,
                                            ).ok().flatten().is_some_and(|stored| matches!(
                                                stored.1.as_str(), "cancelling" | "rejecting" | "cancelled" | "rejected"
                                            ))
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    };
                                    match completion {
                                        Ok(()) => {
                                            if let Some(report) = report_json {
                                                let events = state_for_task.lock().await.events.clone();
                                                let _ = events.send(CoreEvent::LocalModelRuntimeManager {
                                                    operation: "adaptation_benchmark_completed".into(),
                                                    version: finished.revision,
                                                    projection_json: report.to_string(),
                                                }).await;
                                            }
                                        }
                                        Err(_) if cancellation_won => {}
                                        Err(_) => {
                                            let events = state_for_task.lock().await.events.clone();
                                            let _ = events.send(CoreEvent::LocalModelRuntimeManager {
                                                operation: "adaptation_benchmark_completion_failed".into(),
                                                version: finished.revision,
                                                projection_json: serde_json::json!({
                                                    "status":"completion_persistence_failed",
                                                    "job_id":finished_job_id,
                                                    "reason":"completion_persistence_failed",
                                                    "redacted":true
                                                }).to_string(),
                                            }).await;
                                        }
                                    }
                                    state_for_task.lock().await.adaptation_benchmark_cancellations.remove(&finished_job_id);
                                }).await;
                                return serde_json::to_vec(&serde_json::json!({"status":"benchmarking","job":job,"accepted":true,"revision":previous_revision + 1,"redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            }
                            if job.state == crate::local_model_adaptation::AdaptationState::Verifying {
                                // Do not hold the SQLite connection while Supervisor starts or
                                // probes llama-server. The durable transition below reacquires it.
                                drop(db);
                                #[cfg(windows)]
                                let hardware = crate::local_model_runtime_manager::discover_hardware()
                                    .map_err(|_| "adaptation_hardware_unavailable".to_string())?;
                                #[cfg(not(windows))]
                                let hardware: crate::local_model_runtime_manager::LocalHardwareProfile =
                                    return Err("adaptation_requires_windows".into());
                                #[cfg(windows)]
                                let available_memory = crate::local_model_runtime_manager::available_memory_bytes()
                                    .map_err(|_| "adaptation_hardware_unavailable".to_string())?;
                                #[cfg(not(windows))]
                                let available_memory: u64 = return Err("adaptation_requires_windows".into());
                                if hardware.ram_bytes < 1024 * 1024 * 1024
                                    || available_memory < 512 * 1024 * 1024
                                {
                                    return Err("adaptation_resources_unavailable".into());
                                }
                                let process_memory_limit = hardware.ram_bytes.min(available_memory)
                                    .saturating_mul(3)
                                    .checked_div(4)
                                    .unwrap_or(0)
                                    .clamp(512 * 1024 * 1024, 32 * 1024 * 1024 * 1024);
                                let model_relative = crate::local_model_adaptation::staging_relative_path(job_id);
                                let start_runtime = serde_json::json!({
                                    "op":"adaptation_runtime_start",
                                    "job_id":job_id,
                                    "model_relative_path":model_relative,
                                    "model_sha256":job.evidence.output_sha256,
                                    "model_size_bytes":job.evidence.output_size_bytes,
                                    "threads":std::thread::available_parallelism().map(usize::from).unwrap_or(1).clamp(1, 16),
                                    "memory_limit_bytes":process_memory_limit,
                                    "cpu_limit_percent":75
                                });
                                #[cfg(windows)]
                                let started = crate::analysis_kernel::supervisor_command(start_runtime).await
                                    .map_err(|_| "adaptation_runtime_start_failed".to_string())?;
                                #[cfg(not(windows))]
                                let started: serde_json::Value = return Err("quantizer_requires_windows_supervisor".into());
                                if started.get("accepted") != Some(&serde_json::Value::Bool(true)) {
                                    let reason = started.get("reason").and_then(serde_json::Value::as_str).unwrap_or("runtime_start_rejected");
                                    return Err(reason.to_owned());
                                }
                                let probe = serde_json::json!({"op":"adaptation_runtime_probe","job_id":job_id});
                                #[cfg(windows)]
                                let runtime = crate::analysis_kernel::supervisor_command(probe).await
                                    .map_err(|_| "adaptation_runtime_probe_failed".to_string())?;
                                #[cfg(not(windows))]
                                let runtime: serde_json::Value = return Err("quantizer_requires_windows_supervisor".into());
                                if runtime.get("accepted") != Some(&serde_json::Value::Bool(true)) {
                                    let interrupted = runtime.get("reason").and_then(serde_json::Value::as_str) == Some("job_not_running");
                                    let next = if interrupted {
                                        crate::local_model_adaptation::AdaptationState::Interrupted
                                    } else {
                                        crate::local_model_adaptation::AdaptationState::Failed
                                    };
                                    let job = transition_adaptation_job(&state, job, next).await?;
                                    return serde_json::to_vec(&serde_json::json!({"status":job.state.storage_key(),"job":job,"reason":"adaptation_runtime_unavailable","redacted":true}))
                                        .map_err(|_| "serialization_failed".to_string());
                                }
                                if runtime.get("state").and_then(serde_json::Value::as_str) != Some("ready") {
                                    return serde_json::to_vec(&serde_json::json!({"status":"verifying","job":job,"runtime":"loading","redacted":true}))
                                        .map_err(|_| "serialization_failed".to_string());
                                }
                                let probe_hash = crate::local_model_runtime_manager::canonical_hash(&runtime);
                                job.evidence.runtime_probe_sha256 = Some(probe_hash);
                                job.transition(crate::local_model_adaptation::AdaptationState::Benchmarking, job.evidence.clone())
                                    .map_err(|_| "adaptation_transition_denied".to_string())?;
                                let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                                let db = journal.database().lock().await;
                                let current = evohime_local_storage::local_model_adaptation_store::get_job(db.connection(), job_id)
                                    .map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "adaptation_job_not_found".to_string())?;
                                if current.0 + 1 != job.revision { return Err("adaptation_job_revision_conflict".into()); }
                                let snapshot = serde_json::to_vec(&job).map_err(|_| "serialization_failed".to_string())?;
                                if !evohime_local_storage::local_model_adaptation_store::put_job(db.connection(), job_id, job.revision,
                                    job.state.storage_key(), &job.request.idempotency_key, &current.2, &job.content_sha256,
                                    &current.4, &snapshot, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? {
                                    return Err("adaptation_job_revision_conflict".into());
                                }
                                return serde_json::to_vec(&serde_json::json!({"status":"benchmarking","job":job,"benchmark":"ready_for_explicit_run","redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            }
                            if operation == "adaptation_poll"
                                && matches!(job.state,
                                    crate::local_model_adaptation::AdaptationState::WaitingForResources
                                        | crate::local_model_adaptation::AdaptationState::Cancelling
                                        | crate::local_model_adaptation::AdaptationState::Rejecting
                                        | crate::local_model_adaptation::AdaptationState::Benchmarking
                                        | crate::local_model_adaptation::AdaptationState::ReadyForPromotion
                                        | crate::local_model_adaptation::AdaptationState::Failed
                                        | crate::local_model_adaptation::AdaptationState::Interrupted
                                )
                            {
                                return serde_json::to_vec(&serde_json::json!({"status":job.state.storage_key(),"job":job,"redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            }
                            if job.state != crate::local_model_adaptation::AdaptationState::Running {
                                return Err("adaptation_not_running".into());
                            }
                            drop(db);
                            let command = serde_json::json!({"op":"adaptation_quantize_poll","job_id":job_id});
                            #[cfg(windows)]
                            let response = crate::analysis_kernel::supervisor_command(command).await
                                .map_err(|_| "quantizer_poll_failed".to_string())?;
                            #[cfg(not(windows))]
                            let response: serde_json::Value = return Err("quantizer_requires_windows_supervisor".into());
                            if response.get("state").and_then(serde_json::Value::as_str) == Some("running") {
                                return serde_json::to_vec(&serde_json::json!({"status":"running","job":job,"redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            }
                            if response.get("accepted") != Some(&serde_json::Value::Bool(true)) {
                                let interrupted = response.get("reason").and_then(serde_json::Value::as_str) == Some("job_not_running");
                                let next = if interrupted {
                                    crate::local_model_adaptation::AdaptationState::Interrupted
                                } else {
                                    crate::local_model_adaptation::AdaptationState::Failed
                                };
                                let job = transition_adaptation_job(&state, job, next).await?;
                                return serde_json::to_vec(&serde_json::json!({"status":job.state.storage_key(),"job":job,"reason":if interrupted { "quantizer_interrupted" } else { "quantizer_failed" },"redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            }
                            let output_relative = response.get("output_relative_path").and_then(serde_json::Value::as_str)
                                .ok_or_else(|| "quantizer_output_invalid".to_string())?;
                            if output_relative != crate::local_model_adaptation::staging_relative_path(job_id) {
                                let job = transition_adaptation_job(&state, job, crate::local_model_adaptation::AdaptationState::Failed).await?;
                                return serde_json::to_vec(&serde_json::json!({"status":"failed","job":job,"reason":"quantizer_output_path_mismatch","redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            }
                            let Some(hash) = response.get("output_sha256").and_then(serde_json::Value::as_str) else {
                                let job = transition_adaptation_job(&state, job, crate::local_model_adaptation::AdaptationState::Failed).await?;
                                return serde_json::to_vec(&serde_json::json!({"status":"failed","job":job,"reason":"quantizer_output_invalid","redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            };
                            let Some(size) = response.get("output_size_bytes").and_then(serde_json::Value::as_u64) else {
                                let job = transition_adaptation_job(&state, job, crate::local_model_adaptation::AdaptationState::Failed).await?;
                                return serde_json::to_vec(&serde_json::json!({"status":"failed","job":job,"reason":"quantizer_output_invalid","redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            };
                            let models_root = crate::get_data_directory().join("models");
                            let relative_path = output_relative.to_owned();
                            let expected_hash = hash.to_owned();
                            let output_check = tokio::task::spawn_blocking(move || crate::local_model_runtime_manager::verify_managed_artifact(
                                &models_root,
                                std::path::Path::new(&relative_path),
                                &expected_hash,
                                size,
                            )).await;
                            if !matches!(output_check, Ok(Ok(_))) {
                                let job = transition_adaptation_job(&state, job, crate::local_model_adaptation::AdaptationState::Failed).await?;
                                return serde_json::to_vec(&serde_json::json!({"status":"failed","job":job,"reason":"quantizer_output_invalid","redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            }
                            job.evidence.output_sha256 = Some(hash.to_owned());
                            job.evidence.output_size_bytes = Some(size);
                            job.transition(crate::local_model_adaptation::AdaptationState::Verifying, job.evidence.clone())
                                .map_err(|_| "adaptation_transition_denied".to_string())?;
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let db = journal.database().lock().await;
                            let current = evohime_local_storage::local_model_adaptation_store::get_job(db.connection(), job_id)
                                .map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "adaptation_job_not_found".to_string())?;
                            if current.0 + 1 != job.revision { return Err("adaptation_job_revision_conflict".into()); }
                            let snapshot = serde_json::to_vec(&job).map_err(|_| "serialization_failed".to_string())?;
                            if !evohime_local_storage::local_model_adaptation_store::put_job(db.connection(), job_id, job.revision,
                                job.state.storage_key(), &job.request.idempotency_key, &current.2, &job.content_sha256,
                                &current.4, &snapshot, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? {
                                return Err("adaptation_job_revision_conflict".into());
                            }
                            serde_json::to_vec(&serde_json::json!({"status":"verifying","job":job,"runtime_verification":"pending","redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "adaptation_promote" => {
                            let job_id = request.job_id.as_deref().filter(|id| {
                                !id.is_empty() && id.len() <= 128 && !id.bytes().any(|byte| byte.is_ascii_control())
                            }).ok_or_else(|| "job_id_required".to_string())?;
                            let expected_revision = request.job_revision.filter(|revision| *revision > 0)
                                .ok_or_else(|| "job_revision_required".to_string())?;
                            let expected_output = request.expected_output_sha256.as_deref()
                                .filter(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
                                .ok_or_else(|| "expected_output_sha256_required".to_string())?;
                            let expected_benchmark = request.expected_benchmark_sha256.as_deref()
                                .filter(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
                                .ok_or_else(|| "expected_benchmark_sha256_required".to_string())?;
                            let journal = state.lock().await.journal.clone()
                                .ok_or_else(|| "storage journal is not configured".to_string())?;
                            let database = journal.database().lock().await;
                            let stored = evohime_local_storage::local_model_adaptation_store::get_job(
                                database.connection(), job_id,
                            ).map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "adaptation_job_not_found".to_string())?;
                            let mut job: crate::local_model_adaptation::AdaptationJob = serde_json::from_slice(&stored.5)
                                .map_err(|_| "corrupt_adaptation_job".to_string())?;
                            job.validate().map_err(|_| "corrupt_adaptation_job".to_string())?;
                            if stored.0 != job.revision || stored.1 != job.state.storage_key()
                                || stored.2 != job.request.content_sha256().map_err(|_| "corrupt_adaptation_job".to_string())?
                                || stored.3 != job.content_sha256
                                || stored.4 != serde_json::to_vec(&job.request).map_err(|_| "corrupt_adaptation_job".to_string())? {
                                return Err("adaptation_job_integrity_failed".into());
                            }
                            if job.state == crate::local_model_adaptation::AdaptationState::Promoted {
                                if job.evidence.output_sha256.as_deref() == Some(expected_output)
                                    && job.evidence.benchmark_sha256.as_deref() == Some(expected_benchmark) {
                                    let publication = evohime_local_storage::local_model_adaptation_store::get_publication(
                                        database.connection(), job_id,
                                    ).map_err(|_| "storage_failed".to_string())?
                                        .ok_or_else(|| "adaptation_publication_missing".to_string())?;
                                    return serde_json::to_vec(&serde_json::json!({"status":"already_promoted","model_id":publication.0,"model_revision":publication.1,"artifact_relative_path":publication.2,"artifact_sha256":publication.3,"redacted":true}))
                                        .map_err(|_| "serialization_failed".to_string());
                                }
                                return Err("adaptation_promotion_identity_mismatch".into());
                            }
                            if job.revision != expected_revision
                                || job.state != crate::local_model_adaptation::AdaptationState::ReadyForPromotion
                                || job.evidence.output_sha256.as_deref() != Some(expected_output)
                                || job.evidence.benchmark_sha256.as_deref() != Some(expected_benchmark) {
                                return Err("adaptation_promotion_precondition_failed".into());
                            }
                            let mut approval_found = false;
                            for row in evohime_local_storage::approval_policy_profiles_store::list(database.connection())
                                .map_err(|_| "approval_policy_storage_failed".to_string())? {
                                let Ok(profile) = serde_json::from_slice::<crate::approval_policy_profiles::ApprovalPolicyProfile>(&row) else { continue };
                                if profile.id != job.request.approval_policy_id
                                    || profile.version as u64 != job.request.policy_revision
                                    || !profile.enabled { continue; }
                                if crate::local_model_runtime_manager::canonical_hash(&profile)
                                    != job.request.approval_policy_sha256 {
                                    return Err("approval_policy_identity_mismatch".into());
                                }
                                let resource = format!("model:{}:{}", job.request.source.model_id, job.request.source.revision);
                                let decision = crate::approval_policy_profiles::decide(
                                    &profile, &profile.scope_id, "local_model_promotion", &resource, 2,
                                    crate::task_memory::now_millis() as i64,
                                ).map_err(|_| "approval_policy_invalid".to_string())?;
                                if !decision.require_prompt && decision.profile_id.is_some() {
                                    approval_found = true;
                                    break;
                                }
                            }
                            if !approval_found {
                                return Err("local_model_promotion_approval_required".into());
                            }
                            let source_id = format!("model:{}:{}", job.request.source.model_id, job.request.source.revision);
                            let source_row = evohime_local_storage::local_model_runtime_manager_store::get_record(
                                database.connection(), &source_id,
                            ).map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "source_model_not_found".to_string())?;
                            if source_row.0 != "model" || source_row.1 != job.request.source.revision {
                                return Err("source_model_identity_mismatch".into());
                            }
                            let source_model: crate::local_model_runtime_manager::LocalModelDescriptor = serde_json::from_slice(&source_row.3)
                                .map_err(|_| "corrupt_source_model".to_string())?;
                            if source_row.2 != crate::local_model_runtime_manager::canonical_hash(&source_row.3)
                                || source_model.model_id != job.request.source.model_id
                                || source_model.revision != job.request.source.revision
                                || source_model.artifact_hash != job.request.source.artifact_sha256 {
                                return Err("source_model_identity_mismatch".into());
                            }
                            let output_size = job.evidence.output_size_bytes
                                .ok_or_else(|| "adaptation_output_size_missing".to_string())?;
                            let model_job_hash = crate::local_model_runtime_manager::canonical_hash(&job_id);
                            let model_id = format!("adapt-{}-{}", &model_job_hash[..16],
                                job.request.target.llama_argument().to_ascii_lowercase());
                            let relative_path = format!("adapted/{model_id}/1.gguf");
                            let model = crate::local_model_runtime_manager::LocalModelDescriptor {
                                model_id: model_id.clone(), revision: 1, format: "gguf".into(),
                                quantization: job.request.target.llama_argument().into(),
                                artifact_size_bytes: output_size, artifact_hash: expected_output.into(),
                                required_ram_bytes: source_model.required_ram_bytes,
                                required_accelerator_bytes: None, context_limit: source_model.context_limit,
                                capabilities: source_model.capabilities.clone(),
                                trust: crate::local_model_runtime_manager::TrustLevel::ManagedVerified,
                            };
                            model.validate().map_err(|_| "adapted_model_descriptor_invalid".to_string())?;
                            let artifact = crate::local_model_runtime_manager::LocalArtifactRecord {
                                model_id: model_id.clone(), model_revision: 1,
                                relative_path: Some(relative_path.clone()), expected_hash: expected_output.into(),
                                expected_size_bytes: output_size,
                                state: crate::local_model_runtime_manager::ArtifactState::Installed,
                                content_hash: Some(expected_output.into()),
                            };
                            artifact.validate().map_err(|_| "adapted_artifact_record_invalid".to_string())?;
                            let model_json = serde_json::to_vec(&model).map_err(|_| "serialization_failed".to_string())?;
                            let artifact_json = serde_json::to_vec(&artifact).map_err(|_| "serialization_failed".to_string())?;
                            let publication_hash = crate::local_model_runtime_manager::canonical_hash(&(
                                &job.request, &model, &artifact, expected_output, expected_benchmark,
                            ));
                            let existing_publication = evohime_local_storage::local_model_adaptation_store::get_publication(
                                database.connection(), job_id,
                            ).map_err(|_| "storage_failed".to_string())?;
                            if existing_publication.as_ref().is_some_and(|entry| {
                                entry.0 != model_id || entry.1 != 1 || entry.2 != relative_path
                                    || entry.3 != expected_output || entry.4 != output_size
                                    || !matches!(entry.5.as_str(), "prepared" | "registered")
                                    || entry.6 != publication_hash
                            }) {
                                return Err("adaptation_publication_identity_mismatch".into());
                            }
                            let mut db = database;
                            if !evohime_local_storage::local_model_adaptation_store::put_publication(
                                db.connection(), job_id, &model_id, 1, &relative_path, expected_output,
                                output_size, "prepared", &publication_hash, crate::task_memory::now_millis() as i64,
                            ).map_err(|_| "publication_journal_failed".to_string())? {
                                return Err("publication_journal_conflict".into());
                            }
                            drop(db);
                            let models_root = crate::get_data_directory().join("models");
                            let staging_path = crate::local_model_runtime_manager::managed_artifact_path(
                                &models_root, std::path::Path::new(&crate::local_model_adaptation::staging_relative_path(job_id)),
                            ).map_err(|_| "adaptation_staging_path_invalid".to_string())?;
                            let destination_path = crate::local_model_runtime_manager::managed_artifact_path(
                                &models_root, std::path::Path::new(&relative_path),
                            ).map_err(|_| "adaptation_destination_path_invalid".to_string())?;
                            let expected_hash = expected_output.to_owned();
                            let stage = staging_path.clone();
                            let destination = destination_path.clone();
                            let fs_relative_path = relative_path.clone();
                            let publish = tokio::task::spawn_blocking(move || {
                                if destination.exists() {
                                    crate::local_model_runtime_manager::verify_managed_artifact(
                                        &models_root, std::path::Path::new(&fs_relative_path), &expected_hash, output_size,
                                    ).map(|_| ())
                                } else {
                                    crate::local_model_runtime_manager::atomic_promote_verified_artifact(
                                        &stage, &destination, &expected_hash, output_size,
                                    )
                                }
                            }).await.map_err(|_| "artifact_publication_worker_failed".to_string())?;
                            publish.map_err(|_| "artifact_publication_failed".to_string())?;
                            let mut database = journal.database().lock().await;
                            let transaction = database.connection_mut().transaction()
                                .map_err(|_| "publication_transaction_failed".to_string())?;
                            let current = evohime_local_storage::local_model_adaptation_store::get_job(
                                &transaction, job_id,
                            ).map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "adaptation_job_not_found".to_string())?;
                            if current.0 != expected_revision || current.1 != crate::local_model_adaptation::AdaptationState::ReadyForPromotion.storage_key()
                                || current.3 != job.content_sha256 {
                                return Err("adaptation_job_revision_conflict".into());
                            }
                            for (record_id, kind, hash, json) in [
                                (format!("model:{model_id}:1"), "model", crate::local_model_runtime_manager::canonical_hash(&model_json), model_json),
                                (format!("artifact:{model_id}:1"), "artifact", crate::local_model_runtime_manager::canonical_hash(&artifact_json), artifact_json),
                            ] {
                                if let Some(existing) = evohime_local_storage::local_model_runtime_manager_store::get_record(&transaction, &record_id)
                                    .map_err(|_| "storage_failed".to_string())? {
                                    if existing.0 != kind || existing.1 != 1 || existing.2 != hash || existing.3 != json {
                                        return Err("model_registry_identity_conflict".into());
                                    }
                                } else if !evohime_local_storage::local_model_runtime_manager_store::put_record(
                                    &transaction, &record_id, kind, 1, &hash, &json,
                                    crate::task_memory::now_millis() as i64,
                                ).map_err(|_| "storage_failed".to_string())? {
                                    return Err("model_registry_write_conflict".into());
                                }
                            }
                            if !evohime_local_storage::local_model_adaptation_store::put_publication(
                                &transaction, job_id, &model_id, 1, &relative_path, expected_output,
                                output_size, "registered", &publication_hash, crate::task_memory::now_millis() as i64,
                            ).map_err(|_| "publication_journal_failed".to_string())? {
                                return Err("publication_journal_conflict".into());
                            }
                            job.transition(crate::local_model_adaptation::AdaptationState::Promoted, job.evidence.clone())
                                .map_err(|_| "adaptation_transition_denied".to_string())?;
                            let snapshot = serde_json::to_vec(&job).map_err(|_| "serialization_failed".to_string())?;
                            if !evohime_local_storage::local_model_adaptation_store::put_job(
                                &transaction, job_id, job.revision, job.state.storage_key(),
                                &job.request.idempotency_key, &current.2, &job.content_sha256,
                                &current.4, &snapshot, crate::task_memory::now_millis() as i64,
                            ).map_err(|_| "storage_failed".to_string())? {
                                return Err("adaptation_job_revision_conflict".into());
                            }
                            transaction.commit().map_err(|_| "publication_transaction_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"promoted","job":job,"model_id":model_id,"model_revision":1,"artifact_relative_path":relative_path,"artifact_sha256":expected_output,"active":false,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "adaptation_get" => {
                            let job_id = request.job_id.as_deref().filter(|id| {
                                !id.is_empty() && id.len() <= 128 && !id.bytes().any(|byte| byte.is_ascii_control())
                            }).ok_or_else(|| "job_id_required".to_string())?;
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let db = journal.database().lock().await;
                            let stored = evohime_local_storage::local_model_adaptation_store::get_job(db.connection(), job_id)
                                .map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "adaptation_job_not_found".to_string())?;
                            let job: crate::local_model_adaptation::AdaptationJob = serde_json::from_slice(&stored.5)
                                .map_err(|_| "corrupt_adaptation_job".to_string())?;
                            job.validate().map_err(|_| "corrupt_adaptation_job".to_string())?;
                            if stored.0 != job.revision || stored.1 != job.state.storage_key()
                                || stored.2 != job.request.content_sha256().map_err(|_| "corrupt_adaptation_job".to_string())?
                                || stored.3 != job.content_sha256
                                || stored.4 != serde_json::to_vec(&job.request).map_err(|_| "corrupt_adaptation_job".to_string())?
                            {
                                return Err("adaptation_job_integrity_failed".into());
                            }
                            serde_json::to_vec(&serde_json::json!({"status":"ok","job":job,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "adaptation_cancel" | "adaptation_reject" => {
                            let job_id = request.job_id.as_deref().filter(|id| {
                                !id.is_empty() && id.len() <= 128 && !id.bytes().any(|byte| byte.is_ascii_control())
                            }).ok_or_else(|| "job_id_required".to_string())?;
                            let expected_revision = request.job_revision.filter(|revision| *revision > 0)
                                .ok_or_else(|| "job_revision_required".to_string())?;
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let db = journal.database().lock().await;
                            let stored = evohime_local_storage::local_model_adaptation_store::get_job(db.connection(), job_id)
                                .map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "adaptation_job_not_found".to_string())?;
                            let mut job: crate::local_model_adaptation::AdaptationJob = serde_json::from_slice(&stored.5)
                                .map_err(|_| "corrupt_adaptation_job".to_string())?;
                            job.validate().map_err(|_| "corrupt_adaptation_job".to_string())?;
                            if stored.0 != job.revision || stored.1 != job.state.storage_key()
                                || stored.2 != job.request.content_sha256().map_err(|_| "corrupt_adaptation_job".to_string())?
                                || stored.3 != job.content_sha256
                                || stored.4 != serde_json::to_vec(&job.request).map_err(|_| "corrupt_adaptation_job".to_string())?
                            {
                                return Err("adaptation_job_integrity_failed".into());
                            }
                            if job.revision != expected_revision {
                                return Err("adaptation_job_revision_conflict".into());
                            }
                            let pending = if operation == "adaptation_cancel" {
                                crate::local_model_adaptation::AdaptationState::Cancelling
                            } else {
                                crate::local_model_adaptation::AdaptationState::Rejecting
                            };
                            let terminal = if operation == "adaptation_cancel" {
                                crate::local_model_adaptation::AdaptationState::Cancelled
                            } else {
                                crate::local_model_adaptation::AdaptationState::Rejected
                            };
                            if job.state == terminal {
                                return serde_json::to_vec(&serde_json::json!({"status":job.state.storage_key(),"job":job,"redacted":true}))
                                    .map_err(|_| "serialization_failed".to_string());
                            }
                            if job.state.terminal() {
                                return Err("adaptation_job_terminal".into());
                            }
                            if matches!(job.state,
                                crate::local_model_adaptation::AdaptationState::Cancelling
                                    | crate::local_model_adaptation::AdaptationState::Rejecting
                            ) && job.state != pending {
                                return Err("adaptation_terminal_intent_conflict".into());
                            }
                            drop(db);
                            if job.state != pending {
                                job = transition_adaptation_job(&state, job, pending).await?;
                            }
                            if let Some(cancellation) = state.lock().await
                                .adaptation_benchmark_cancellations.remove(job_id) {
                                cancellation.cancel();
                            }
                            let quantizer = serde_json::json!({"op":"adaptation_quantize_cancel","job_id":job_id});
                            #[cfg(windows)]
                            let response = crate::analysis_kernel::supervisor_command(quantizer).await
                                .map_err(|_| "quantizer_cancel_failed".to_string())?;
                            #[cfg(not(windows))]
                            let response: serde_json::Value = return Err("quantizer_requires_windows_supervisor".into());
                            let already_stopped = response.get("reason").and_then(serde_json::Value::as_str)
                                == Some("job_not_running");
                            if response.get("accepted") != Some(&serde_json::Value::Bool(true)) && !already_stopped {
                                return Err("quantizer_cancel_rejected".into());
                            }
                            let runtime = serde_json::json!({"op":"adaptation_runtime_stop","job_id":job_id});
                            #[cfg(windows)]
                            let response = crate::analysis_kernel::supervisor_command(runtime).await
                                .map_err(|_| "adaptation_runtime_stop_failed".to_string())?;
                            #[cfg(not(windows))]
                            let response: serde_json::Value = return Err("quantizer_requires_windows_supervisor".into());
                            let already_stopped = response.get("reason").and_then(serde_json::Value::as_str)
                                == Some("job_not_running");
                            if response.get("accepted") != Some(&serde_json::Value::Bool(true)) && !already_stopped {
                                return Err("adaptation_runtime_stop_rejected".into());
                            }
                            job = transition_adaptation_job(&state, job, terminal).await?;
                            serde_json::to_vec(&serde_json::json!({"status":job.state.storage_key(),"job":job,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "adaptation_list" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let db = journal.database().lock().await;
                            let rows = evohime_local_storage::local_model_adaptation_store::list_jobs(db.connection(), 256)
                                .map_err(|_| "storage_failed".to_string())?;
                            let mut jobs = Vec::with_capacity(rows.len());
                            for (job_id, revision, stored_state, snapshot) in rows {
                                let job: crate::local_model_adaptation::AdaptationJob = serde_json::from_slice(&snapshot)
                                    .map_err(|_| "corrupt_adaptation_job".to_string())?;
                                job.validate().map_err(|_| "corrupt_adaptation_job".to_string())?;
                                if job.request.job_id != job_id || job.revision != revision
                                    || stored_state != job.state.storage_key()
                                {
                                    return Err("adaptation_job_integrity_failed".into());
                                }
                                jobs.push(job);
                            }
                            serde_json::to_vec(&serde_json::json!({"status":"ok","jobs":jobs,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
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
            let projection_json = match result.as_ref() {
                Ok(bytes) => String::from_utf8(bytes.clone()).unwrap_or_else(|_| "{}".into()),
                Err(error) => serde_json::json!({
                    "status": "failed",
                    "error": error,
                    "redacted": true
                })
                .to_string(),
            };
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

fn ollama_pull_percent(completed: Option<u64>, total: Option<u64>) -> Option<u64> {
    match (completed, total) {
        (Some(completed), Some(total)) if total > 0 => {
            Some(completed.saturating_mul(100).min(total.saturating_mul(100)) / total)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::ollama_pull_percent;

    #[test]
    fn ollama_pull_percent_is_bounded_and_requires_total() {
        assert_eq!(ollama_pull_percent(Some(250), Some(1000)), Some(25));
        assert_eq!(ollama_pull_percent(Some(1200), Some(1000)), Some(100));
        assert_eq!(ollama_pull_percent(Some(250), None), None);
        assert_eq!(ollama_pull_percent(Some(250), Some(0)), None);
    }
}

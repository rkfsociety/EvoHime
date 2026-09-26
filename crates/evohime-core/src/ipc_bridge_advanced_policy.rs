use super::*;

impl IpcBridge {
    pub(crate) async fn dispatch_benchmark_matrix(
        &self,
        request: generated::AgentBenchmarkMatrixCommand,
    ) -> serde_json::Value {
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
        {
            return serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "operation": request.operation,
                "status": "rejected",
                "error_code": "invalid_request"
            });
        }
        match request.operation.as_str() {
            "list" => {
                let database = self.journal.database().lock().await;
                let runs = match database.connection().prepare(
                    "SELECT run_id,suite_id,suite_version,state,updated_at_ms FROM benchmark_runs ORDER BY updated_at_ms DESC LIMIT 128",
                ) {
                    Ok(mut statement) => statement.query_map([], |row| Ok(serde_json::json!({
                        "run_id": row.get::<_, String>(0)?,
                        "suite_id": row.get::<_, String>(1)?,
                        "suite_version": row.get::<_, String>(2)?,
                        "state": row.get::<_, String>(3)?,
                        "updated_at_ms": row.get::<_, i64>(4)?
                    }))).map(|rows| rows.filter_map(Result::ok).collect::<Vec<_>>()).unwrap_or_default(),
                    Err(_) => Vec::new(),
                };
                serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "operation": "list",
                "status": "ok",
                "runs": runs,
                "error_code": ""
            })
            },
            "approveBaseline" => {
                let result = async {
                    let payload: serde_json::Value = serde_json::from_slice(&request.payload)
                        .map_err(|_| "invalid_approval_request")?;
                    let run_id = payload.get("run_id").and_then(serde_json::Value::as_str)
                        .filter(|value| !value.is_empty() && value.len() <= 128)
                        .ok_or("run_id_required")?;
                    let challenge_id = payload.get("challenge_id").and_then(serde_json::Value::as_str)
                        .filter(|value| !value.is_empty() && value.len() <= crate::agent_benchmark_matrix::MAX_ID_CHARS)
                        .ok_or("challenge_id_required")?;
                    let model_profile_id = payload.get("model_profile_id").and_then(serde_json::Value::as_str)
                        .filter(|value| !value.is_empty() && value.len() <= crate::agent_benchmark_matrix::MAX_ID_CHARS)
                        .ok_or("model_profile_id_required")?;
                    let agent_profile_id = payload.get("agent_profile_id").and_then(serde_json::Value::as_str)
                        .filter(|value| !value.is_empty() && value.len() <= crate::agent_benchmark_matrix::MAX_ID_CHARS)
                        .ok_or("agent_profile_id_required")?;
                    let expected_report_hash = payload.get("report_sha256").and_then(serde_json::Value::as_str)
                        .filter(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
                        .ok_or("report_sha256_required")?;
                    let expected_revision = request.expected_version;
                    if expected_revision == 0 {
                        return Err("expected_version_required");
                    }
                    let baseline_id = format!("baseline-{}", &crate::local_model_runtime_manager::canonical_hash(&(
                        run_id, challenge_id, model_profile_id, agent_profile_id,
                    ))[..32]);
                    let mut database = self.journal.database().lock().await;
                    // A committed approval remains replayable even if the job
                    // has since advanced to another revision or been promoted.
                    if let Some((existing_id, owner_scope, existing_run, report_hash)) =
                        evohime_local_storage::benchmark_store::get_baseline_approval_by_key(
                            database.connection(), &request.idempotency_key,
                        ).map_err(|_| "benchmark_baseline_storage_failed")? {
                        if existing_id != baseline_id || owner_scope != request.owner_scope
                            || existing_run != run_id || report_hash != expected_report_hash {
                            return Err("benchmark_baseline_approval_conflict");
                        }
                        let existing = evohime_local_storage::benchmark_store::get_baseline(
                            database.connection(), &baseline_id,
                        ).map_err(|_| "benchmark_baseline_storage_failed")?
                            .ok_or("benchmark_baseline_storage_failed")?;
                        return Ok::<_, &'static str>(serde_json::json!({
                            "schema_version":1,"request_id":request.request_id,"operation":"approveBaseline",
                            "status":"approved","baseline_id":baseline_id,"revision":existing.6,
                            "run_id":run_id,"challenge_id":challenge_id,
                            "model_profile_id":model_profile_id,"agent_profile_id":agent_profile_id,
                            "report_sha256":expected_report_hash,"redacted":true,"error_code":""
                        }));
                    }
                    if let Some((owner_scope, existing_run, report_hash)) =
                        evohime_local_storage::benchmark_store::get_baseline_approval(
                            database.connection(), &baseline_id,
                        ).map_err(|_| "benchmark_baseline_storage_failed")? {
                        if owner_scope != request.owner_scope || existing_run != run_id
                            || report_hash != expected_report_hash {
                            return Err("benchmark_baseline_approval_conflict");
                        }
                        let existing = evohime_local_storage::benchmark_store::get_baseline(
                            database.connection(), &baseline_id,
                        ).map_err(|_| "benchmark_baseline_storage_failed")?
                            .ok_or("benchmark_baseline_storage_failed")?;
                        return Ok::<_, &'static str>(serde_json::json!({
                            "schema_version":1,"request_id":request.request_id,"operation":"approveBaseline",
                            "status":"approved","baseline_id":baseline_id,"revision":existing.6,
                            "run_id":run_id,"challenge_id":challenge_id,
                            "model_profile_id":model_profile_id,"agent_profile_id":agent_profile_id,
                            "report_sha256":expected_report_hash,"redacted":true,"error_code":""
                        }));
                    }
                    let stored = evohime_local_storage::local_model_adaptation_store::get_job(
                        database.connection(), run_id,
                    ).map_err(|_| "adaptation_job_storage_failed")?.ok_or("adaptation_job_not_found")?;
                    let job: crate::local_model_adaptation::AdaptationJob = serde_json::from_slice(&stored.5)
                        .map_err(|_| "adaptation_job_corrupt")?;
                    job.validate().map_err(|_| "adaptation_job_corrupt")?;
                    if job.revision != expected_revision || stored.0 != job.revision
                        || stored.1 != job.state.storage_key() || stored.3 != job.content_sha256
                        || stored.2 != job.request.content_sha256().map_err(|_| "adaptation_job_corrupt")?
                        || stored.4 != serde_json::to_vec(&job.request).map_err(|_| "adaptation_job_corrupt")?
                        || !matches!(job.state,
                            crate::local_model_adaptation::AdaptationState::ReadyForPromotion
                                | crate::local_model_adaptation::AdaptationState::Failed)
                    {
                        return Err("adaptation_job_revision_conflict");
                    }
                    let run = evohime_local_storage::benchmark_store::get_run(database.connection(), run_id)
                        .map_err(|_| "benchmark_run_storage_failed")?.ok_or("benchmark_run_not_found")?;
                    if run.3.as_deref().is_none() || run.0.is_empty()
                        || !matches!(run.2.as_str(), "ready_for_promotion" | "failed") {
                        return Err("benchmark_report_unavailable");
                    }
                    let report: crate::agent_benchmark_matrix::BenchmarkReport = serde_json::from_str(
                        run.3.as_deref().unwrap_or_default(),
                    ).map_err(|_| "benchmark_report_corrupt")?;
                    if report.run_id != run_id || report.suite_id != run.0
                        || report.suite_version != run.1
                        || report.contract_id != crate::agent_benchmark_matrix::CONTRACT_ID
                        || report.redaction_status != "redacted"
                        || job.evidence.benchmark_sha256.as_deref()
                            != Some(crate::local_model_runtime_manager::canonical_hash(&report).as_str())
                        || expected_report_hash != crate::local_model_runtime_manager::canonical_hash(&report) {
                        return Err("benchmark_report_identity_mismatch");
                    }
                    let (input_hash, input_json) = evohime_local_storage::local_model_adaptation_store::get_benchmark_inputs(
                        database.connection(), run_id,
                    ).map_err(|_| "benchmark_input_storage_failed")?.ok_or("benchmark_input_not_found")?;
                    if input_hash != crate::local_model_runtime_manager::canonical_hash(&input_json)
                        || input_json.len() > 192 * 1024 {
                        return Err("benchmark_input_integrity_failed");
                    }
                    let (suite, _policy, _baselines): (
                        crate::agent_benchmark_matrix::BenchmarkSuite,
                        crate::agent_benchmark_matrix::BenchmarkPolicy,
                        std::collections::BTreeMap<String, crate::agent_benchmark_matrix::Baseline>,
                    ) = serde_json::from_slice(&input_json).map_err(|_| "benchmark_input_corrupt")?;
                    if suite.canonical_hash().map_err(|_| "benchmark_suite_invalid")?
                        != job.request.benchmark_suite_sha256 {
                        return Err("benchmark_suite_identity_mismatch");
                    }
                    if report.model_profile_ids != suite.model_profiles.iter().map(|item| item.id.clone()).collect::<Vec<_>>()
                        || report.agent_profile_ids != suite.agent_profiles.iter().map(|item| item.id.clone()).collect::<Vec<_>>() {
                        return Err("benchmark_report_profile_mismatch");
                    }
                    let challenge = suite.challenges.iter().find(|item| item.id == challenge_id)
                        .ok_or("challenge_not_in_suite")?;
                    let model = suite.model_profiles.iter().find(|item| item.id == model_profile_id)
                        .ok_or("model_profile_not_in_suite")?;
                    let agent = suite.agent_profiles.iter().find(|item| item.id == agent_profile_id)
                        .ok_or("agent_profile_not_in_suite")?;
                    let key = format!("{challenge_id}:{model_profile_id}:{agent_profile_id}");
                    let metrics = report.metrics.get(&key).ok_or("benchmark_metrics_missing")?;
                    let comparison = report.comparisons.get(&key).ok_or("benchmark_comparison_missing")?;
                    if metrics.attempts == 0 || metrics.completed == 0 || comparison.security_hard_failure {
                        return Err("benchmark_evidence_not_approvable");
                    }
                    let metrics_json = serde_json::to_string(metrics).map_err(|_| "benchmark_metrics_serialization_failed")?;
                    let transaction = database.connection_mut().transaction()
                        .map_err(|_| "benchmark_baseline_storage_failed")?;
                    if let Some((existing_id, owner_scope, existing_run, report_hash)) =
                        evohime_local_storage::benchmark_store::get_baseline_approval_by_key(
                            &transaction, &request.idempotency_key,
                        ).map_err(|_| "benchmark_baseline_storage_failed")? {
                        if existing_id != baseline_id || owner_scope != request.owner_scope
                            || existing_run != run_id || report_hash != expected_report_hash {
                            return Err("benchmark_baseline_approval_conflict");
                        }
                        let existing = evohime_local_storage::benchmark_store::get_baseline(
                            &transaction, &baseline_id,
                        ).map_err(|_| "benchmark_baseline_storage_failed")?
                            .ok_or("benchmark_baseline_storage_failed")?;
                        transaction.commit().map_err(|_| "benchmark_baseline_storage_failed")?;
                        return Ok::<_, &'static str>(serde_json::json!({
                            "schema_version":1,"request_id":request.request_id,"operation":"approveBaseline",
                            "status":"approved","baseline_id":baseline_id,"revision":existing.6,
                            "run_id":run_id,"challenge_id":challenge_id,
                            "model_profile_id":model_profile_id,"agent_profile_id":agent_profile_id,
                            "report_sha256":expected_report_hash,"redacted":true,"error_code":""
                        }));
                    }
                    if let Some((owner_scope, existing_run, report_hash)) =
                        evohime_local_storage::benchmark_store::get_baseline_approval(
                            &transaction, &baseline_id,
                        ).map_err(|_| "benchmark_baseline_storage_failed")? {
                        if owner_scope != request.owner_scope || existing_run != run_id
                            || report_hash != expected_report_hash {
                            return Err("benchmark_baseline_approval_conflict");
                        }
                        let existing = evohime_local_storage::benchmark_store::get_baseline(
                            &transaction, &baseline_id,
                        ).map_err(|_| "benchmark_baseline_storage_failed")?
                            .ok_or("benchmark_baseline_storage_failed")?;
                        transaction.commit().map_err(|_| "benchmark_baseline_storage_failed")?;
                        return Ok::<_, &'static str>(serde_json::json!({
                            "schema_version":1,"request_id":request.request_id,"operation":"approveBaseline",
                            "status":"approved","baseline_id":baseline_id,"revision":existing.6,
                            "run_id":run_id,"challenge_id":challenge_id,
                            "model_profile_id":model_profile_id,"agent_profile_id":agent_profile_id,
                            "report_sha256":expected_report_hash,"redacted":true,"error_code":""
                        }));
                    }
                    let existing_baseline = evohime_local_storage::benchmark_store::get_baseline(
                        &transaction, &baseline_id,
                    ).map_err(|_| "benchmark_baseline_storage_failed")?;
                    let revision = if let Some(existing) = existing_baseline {
                        if existing.0 != suite.version || existing.1 != challenge.id
                            || existing.2 != model.content_hash || existing.3 != agent.content_hash
                            || existing.4 != metrics_json || existing.5 != report.source_commit {
                            return Err("benchmark_baseline_identity_conflict");
                        }
                        existing.6
                    } else {
                        let previous = evohime_local_storage::benchmark_store::latest_baseline_revision(
                            &transaction, &suite.version, challenge_id,
                            &model.content_hash, &agent.content_hash,
                        ).map_err(|_| "benchmark_baseline_storage_failed")?;
                        let next = previous.checked_add(1).ok_or("benchmark_baseline_revision_overflow")?;
                        if !evohime_local_storage::benchmark_store::put_baseline(
                            &transaction, &baseline_id, &suite.version, &challenge.id,
                            &model.content_hash, &agent.content_hash, &metrics_json,
                            &report.source_commit, next, crate::task_memory::now_millis() as i64,
                        ).map_err(|_| "benchmark_baseline_storage_failed")? {
                            return Err("benchmark_baseline_write_conflict");
                        }
                        next
                    };
                    if !evohime_local_storage::benchmark_store::put_baseline_approval(
                        &transaction, &baseline_id, &request.owner_scope, run_id,
                        expected_report_hash, &request.idempotency_key,
                        crate::task_memory::now_millis() as i64,
                    ).map_err(|_| "benchmark_baseline_storage_failed")? {
                        return Err("benchmark_baseline_approval_conflict");
                    }
                    transaction.commit().map_err(|_| "benchmark_baseline_storage_failed")?;
                    Ok::<_, &'static str>(serde_json::json!({
                        "schema_version":1,"request_id":request.request_id,"operation":"approveBaseline",
                        "status":"approved","baseline_id":baseline_id,"revision":revision,
                        "run_id":run_id,"challenge_id":challenge_id,
                        "model_profile_id":model_profile_id,"agent_profile_id":agent_profile_id,
                        "report_sha256":expected_report_hash,"redacted":true,"error_code":""
                    }))
                }.await;
                match result {
                    Ok(value) => value,
                    Err(error_code) => serde_json::json!({"schema_version":1,"request_id":request.request_id,
                        "operation":"approveBaseline","status":"rejected","error_code":error_code}),
                }
            }
            "start" | "cancel" => serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "operation": request.operation,
                "status": "unavailable",
                "error_code": "benchmark_runtime_not_configured"
            }),
            _ => serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "operation": request.operation,
                "status": "rejected",
                "error_code": "unsupported_operation"
            }),
        }
    }

    pub(crate) fn dispatch_agent_middleware_pipeline(
        &self,
        request: generated::AgentMiddlewarePipelineCommand,
    ) -> serde_json::Value {
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request"});
        }
        match request.operation.as_str() {
            "list" => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"list","status":"ok","contract_version":crate::agent_middleware_pipeline::CONTRACT_VERSION,"contract_id":crate::agent_middleware_pipeline::CONTRACT_ID,"runs":[],"error_code":""})
            }
            "start" => {
                use crate::agent_middleware_pipeline::{
                    AgentMiddlewarePipelineService, BuiltinPolicy, FailurePolicy, HandlerMode,
                    HookPhase, MiddlewareRequest, MiddlewareSpec, PipelineDefinition,
                    PipelineRunSnapshot, StateClass,
                };
                let payload: serde_json::Value =
                    serde_json::from_slice(&request.payload).unwrap_or_default();
                let run_id = payload["runId"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .unwrap_or("ipc-run");
                let definition = match PipelineDefinition::new(
                    "default",
                    1,
                    vec![MiddlewareSpec {
                        id: "core-observer".into(),
                        version: 1,
                        priority: 0,
                        phases: HookPhase::ALL.to_vec(),
                        state_class: StateClass::Public,
                        policy: BuiltinPolicy::Observe,
                        mode: HandlerMode::ObserveOnly,
                        failure_policy: FailurePolicy::FailOpen,
                    }],
                ) {
                    Ok(value) => value,
                    Err(_) => {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"start","status":"rejected","error_code":"invalid_definition"})
                    }
                };
                let snapshot = PipelineRunSnapshot {
                    run_id: run_id.into(),
                    definition_id: definition.definition_id.clone(),
                    definition_revision: definition.revision,
                    contract_hash: definition.contract_hash.clone(),
                    policy_hash: "core-policy-v1".into(),
                    capability_snapshot_hash: "core-capability-snapshot".into(),
                };
                let mut service = match AgentMiddlewarePipelineService::new(
                    definition,
                    snapshot,
                    "core-capability-snapshot",
                ) {
                    Ok(value) => value,
                    Err(_) => {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"start","status":"rejected","error_code":"invalid_snapshot"})
                    }
                };
                let middleware_request = MiddlewareRequest {
                    run_id: run_id.into(),
                    correlation_id: request.request_id.clone(),
                    idempotency_key: request.idempotency_key.clone(),
                    phase: HookPhase::BeforeAgent,
                    input_hash: "ipc-metadata".into(),
                    capability_snapshot_hash: "core-capability-snapshot".into(),
                    intervention_depth: 0,
                };
                match service.evaluate(&middleware_request) {
                    Ok((outcome, events)) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"start","status":"accepted","run_id":run_id,"outcome":outcome,"events":events,"error_code":""})
                    }
                    Err(_) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"start","status":"rejected","error_code":"pipeline_validation_failed"})
                    }
                }
            }
            "cancel" => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"cancel","status":"accepted","error_code":""})
            }
            _ => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"unsupported_operation"})
            }
        }
    }

    pub(crate) fn dispatch_structured_response(
        &self,
        request: generated::StructuredResponseCommand,
    ) -> serde_json::Value {
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request"});
        }
        match request.operation.as_str() {
            "list" => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"list","status":"ok","run_id":"","revision":0,"contract_hash":"","strategy":"","attempts":0,"error_code":"","runs":[]})
            }
            "cancel" => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"cancel","status":"unknown","run_id":"","error_code":"no_ephemeral_run"})
            }
            _ => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"unsupported","error_code":"unsupported_operation"})
            }
        }
    }

    pub(crate) fn dispatch_sensitive_data_guardrails(
        &self,
        request: generated::SensitiveDataGuardrailsCommand,
    ) -> serde_json::Value {
        if request.schema_version != crate::sensitive_data_guardrails::CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > crate::sensitive_data_guardrails::MAX_INPUT_BYTES
        {
            return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request"});
        }
        let payload: serde_json::Value = if request.payload.is_empty() {
            serde_json::json!({})
        } else {
            match serde_json::from_slice(&request.payload) {
                Ok(value) => value,
                Err(_) => {
                    return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_payload"})
                }
            }
        };
        let destination = payload["destination"].as_str().unwrap_or("provider");
        if destination.is_empty() || destination.len() > 128 {
            return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_destination"});
        }
        let snapshot = crate::sensitive_data_guardrails::default_policy(destination);
        let metadata = if request.operation == "evaluate" {
            let Some(input) = payload["input"].as_str() else {
                return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"input_required"});
            };
            match crate::sensitive_data_guardrails::redact_text(&snapshot, input) {
                Ok(result) => result.metadata,
                Err(crate::sensitive_data_guardrails::GuardrailError::Blocked(metadata)) => {
                    metadata
                }
                Err(error) => {
                    return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":error.to_string()})
                }
            }
        } else if request.operation == "status" {
            crate::sensitive_data_guardrails::RedactionMetadata {
                contract_version: 1,
                policy_hash: snapshot.policy_hash,
                destination: destination.into(),
                action: None,
                rule_ids: Vec::new(),
                match_count: 0,
                blocked: false,
                output_bytes: 0,
            }
        } else {
            return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"unsupported_operation"});
        };
        serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"ok","policy_hash":metadata.policy_hash,"destination":metadata.destination,"action":metadata.action.map(|action| format!("{action:?}").to_ascii_lowercase()),"match_count":metadata.match_count,"blocked":metadata.blocked,"error_code":""})
    }

    pub(crate) async fn write_sensitive_data_guardrails_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: serde_json::Value = serde_json::from_slice(&payload)?;
        let result = generated::SensitiveDataGuardrailsEvent {
            schema_version: 1,
            request_id: value["request_id"].as_str().unwrap_or_default().into(),
            operation: value["operation"].as_str().unwrap_or_default().into(),
            status: value["status"].as_str().unwrap_or_default().into(),
            policy_hash: value["policy_hash"].as_str().unwrap_or_default().into(),
            destination: value["destination"].as_str().unwrap_or_default().into(),
            action: value["action"].as_str().unwrap_or_default().into(),
            match_count: value["match_count"].as_u64().unwrap_or_default() as u32,
            blocked: value["blocked"].as_bool().unwrap_or(false),
            error_code: value["error_code"].as_str().unwrap_or_default().into(),
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "sensitive_data_guardrails.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::SensitiveDataGuardrails(
                    result,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) fn dispatch_execution_policy_profiles(
        &self,
        request: generated::ExecutionPolicyProfilesCommand,
    ) -> serde_json::Value {
        if request.schema_version
            != evohime_tool_runtime::execution_policy_profiles::CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.operation.is_empty()
        {
            return serde_json::json!({
                "request_id": request.request_id,
                "operation": request.operation,
                "status": "rejected",
                "error_code": "invalid_request"
            });
        }
        let resolved = match evohime_tool_runtime::ExecutionPolicyProfile::resolve("shell.execute")
        {
            Ok(value) => value,
            Err(error) => {
                return serde_json::json!({
                    "request_id": request.request_id,
                    "operation": request.operation,
                    "status": "unavailable",
                    "error_code": error.to_string()
                })
            }
        };
        if request.operation != "list"
            && request.operation != "status"
            && request.operation != "resolve"
        {
            return serde_json::json!({
                "request_id": request.request_id,
                "operation": request.operation,
                "status": "unsupported",
                "error_code": "unsupported_operation"
            });
        }
        if !request.profile_id.is_empty() && request.profile_id != resolved.profile.profile_id {
            return serde_json::json!({
                "request_id": request.request_id,
                "operation": request.operation,
                "status": "not_found",
                "error_code": "profile_not_found"
            });
        }
        serde_json::json!({
            "request_id": request.request_id,
            "operation": request.operation,
            "status": "ok",
            "profile_id": resolved.profile.profile_id,
            "version": resolved.profile.version,
            "profile_hash": resolved.profile_hash,
            "backend": resolved.backend,
            "network_policy": "deny",
            "environment_policy": "scrubbed_allowlist",
            "timeout_ms": resolved.profile.timeout_ms,
            "max_output_bytes": resolved.profile.max_output_bytes,
            "error_code": ""
        })
    }

    pub(crate) fn dispatch_model_resilience_policy(
        &self,
        request: generated::ModelResiliencePolicyCommand,
    ) -> serde_json::Value {
        if request.schema_version != crate::model_resilience_policy::CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.operation != "status"
        {
            return serde_json::json!({"request_id": request.request_id, "operation": request.operation, "status": "rejected", "error_code": "invalid_request"});
        }
        let policy = crate::model_resilience_policy::builtin_policy();
        let hash = policy.canonical_hash().unwrap_or_default();
        serde_json::json!({
            "request_id": request.request_id,
            "operation": "status",
            "status": "ok",
            "policy_id": crate::model_resilience_policy::CONTRACT_ID,
            "policy_hash": hash,
            "attempts": policy.rules.max_attempts,
            "retries": policy.rules.max_attempts.saturating_sub(1),
            "fallbacks": if policy.rules.allow_fallback { policy.rules.max_fallbacks } else { 0 },
            "terminal_outcome": "unknown_outcome_is_not_retried",
            "error_code": "",
            "projection_json": {"schema_version": 1, "ephemeral": true, "raw_payload": false, "credentials": false}
        })
    }

    pub(crate) async fn dispatch_execution_backend_registry(
        &self,
        request: generated::ExecutionBackendRegistryCommand,
    ) -> serde_json::Value {
        use crate::execution_backend_registry::{
            BackendDefinition, BackendKind, HealthState, Registry,
        };
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request"});
        }
        let payload: ExecutionBackendPayload = if request.payload.is_empty() {
            ExecutionBackendPayload::default()
        } else {
            match serde_json::from_slice::<ExecutionBackendPayload>(&request.payload) {
                Ok(v) => v,
                Err(_) => {
                    return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_payload"})
                }
            }
        };
        let database = self.journal.database().lock().await;
        let rows = match evohime_local_storage::execution_backend_registry_store::list(
            database.connection(),
        ) {
            Ok(v) => v,
            Err(_) => {
                return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"unavailable","error_code":"storage_unavailable"})
            }
        };
        let mut registry = Registry::default();
        for row in rows.into_iter().filter(|row| row.id != "local.core") {
            let capabilities = serde_json::from_str(&row.capabilities_json).unwrap_or_default();
            let _ = registry.register(
                BackendDefinition {
                    id: row.id,
                    kind: if row.kind == "remote" {
                        BackendKind::Remote
                    } else {
                        BackendKind::Local
                    },
                    endpoint: row.endpoint,
                    auth_ref: row.auth_ref,
                    enabled: row.health != "disabled",
                    capabilities,
                    version: row.version as u64,
                    health: if row.health == "disabled" {
                        HealthState::Disabled
                    } else {
                        HealthState::Registered
                    },
                    health_failure: None,
                },
                registry.version(),
            );
        }
        if let Ok(Some(default_id)) =
            evohime_local_storage::execution_backend_registry_store::default_id(
                database.connection(),
            )
        {
            let _ = registry.set_default(&default_id, registry.version());
        }
        let outcome = match request.operation.as_str() {
            "list" => {
                serde_json::json!({"status":"ok","registry_version":registry.version(),"default_backend_id":registry.default_id(),"backends":registry.entries().map(|b| serde_json::json!({"id":b.id,"kind":b.kind,"enabled":b.enabled,"health":b.health,"capability_count":b.capabilities.len(),"has_auth_ref":b.auth_ref.is_some()})).collect::<Vec<_>>()})
            }
            "register" => {
                let id = payload.id.clone();
                let kind = if payload.kind == "remote" {
                    BackendKind::Remote
                } else {
                    BackendKind::Local
                };
                let backend = BackendDefinition {
                    id,
                    kind,
                    endpoint: payload.endpoint.clone(),
                    auth_ref: payload.auth_ref.clone(),
                    enabled: true,
                    capabilities: payload.capabilities.clone(),
                    version: 0,
                    health: HealthState::Registered,
                    health_failure: None,
                };
                match registry.register(backend.clone(), request.expected_version.max(1)) {
                    Ok(()) => {
                        let kind_s = if matches!(backend.kind, BackendKind::Remote) {
                            "remote"
                        } else {
                            "local"
                        };
                        let caps = match serde_json::to_string(&backend.capabilities) {
                            Ok(value) => value,
                            Err(error) => {
                                tracing::warn!(backend_id = %backend.id, %error, "failed to serialize backend capabilities");
                                "[]".into()
                            }
                        };
                        let _ = evohime_local_storage::execution_backend_registry_store::upsert(
                            database.connection(),
                            evohime_local_storage::execution_backend_registry_store::UpsertInput {
                                id: &backend.id,
                                kind: kind_s,
                                endpoint: backend.endpoint.as_deref(),
                                auth_ref: backend.auth_ref.as_deref(),
                                capabilities_json: &caps,
                                version: registry.version(),
                                health: "registered",
                                now_ms: crate::task_memory::now_millis() as i64,
                            },
                        );
                        serde_json::json!({"status":"ok","registry_version":registry.version()})
                    }
                    Err(e) => serde_json::json!({"status":"rejected","error_code":e.to_string()}),
                }
            }
            "handshake" => {
                let id = payload.backend_id.as_str();
                let hs = crate::execution_backend_registry::CapabilityHandshake {
                    protocol_major: payload.protocol_major,
                    protocol_minor: payload.protocol_minor,
                    backend_id: id.into(),
                    capabilities: payload.capabilities.clone(),
                    capability_hash: payload.capability_hash.clone(),
                };
                match registry.handshake(
                    id,
                    hs,
                    &["agent.execute".into(), "workflow.execute".into()],
                ) {
                    Ok(snapshot) => serde_json::json!({"status":"ok","snapshot":snapshot}),
                    Err(e) => {
                        serde_json::json!({"status":"unavailable","error_code":e.to_string()})
                    }
                }
            }
            "remove" => {
                let id = payload.id.as_str();
                if id.is_empty() || id == "local.core" {
                    serde_json::json!({"status":"rejected","error_code":"local_backend_required"})
                } else if registry.remove(id, registry.version()).is_err() {
                    serde_json::json!({"status":"not_found","error_code":"not_found"})
                } else {
                    let _ = evohime_local_storage::execution_backend_registry_store::remove(
                        database.connection(),
                        id,
                    );
                    serde_json::json!({"status":"ok","registry_version":registry.version()})
                }
            }
            "set_default" => {
                let id = payload.id.as_str();
                match registry.set_default(id, registry.version()) {
                    Ok(()) => {
                        let _ =
                            evohime_local_storage::execution_backend_registry_store::set_default(
                                database.connection(),
                                id,
                            );
                        serde_json::json!({"status":"ok","registry_version":registry.version(),"default_backend_id":id})
                    }
                    Err(e) => serde_json::json!({"status":"rejected","error_code":e.to_string()}),
                }
            }
            "disable" => {
                let id = payload.id.as_str();
                if id == "local.core" {
                    serde_json::json!({"status":"rejected","error_code":"local_backend_required"})
                } else if evohime_local_storage::execution_backend_registry_store::set_enabled(
                    database.connection(),
                    id,
                    false,
                )
                .unwrap_or(false)
                {
                    serde_json::json!({"status":"ok","registry_version":registry.version()})
                } else {
                    serde_json::json!({"status":"not_found","error_code":"not_found"})
                }
            }
            "snapshot" => {
                let id = if payload.backend_id.is_empty() {
                    registry.default_id()
                } else {
                    payload.backend_id.as_str()
                };
                if registry.entries().any(|b| b.id == id) {
                    serde_json::json!({"status":"ok","snapshot":{"backend_id":id,"registry_version":registry.version(),"handshake_hash":"pending","policy_hash":"core-policy-v1"}})
                } else {
                    serde_json::json!({"status":"not_found","error_code":"not_found"})
                }
            }
            _ => serde_json::json!({"status":"rejected","error_code":"unsupported_operation"}),
        };
        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"registry_version":outcome["registry_version"].as_u64().unwrap_or(registry.version()),"projection_json":outcome,"error_code":outcome["error_code"].as_str().unwrap_or("")})
    }

    pub(crate) async fn dispatch_tool_simulation_runtime(
        &self,
        request: generated::ToolSimulationRuntimeCommand,
    ) -> serde_json::Value {
        if request.schema_version != crate::tool_simulation_runtime::CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request","projection_json":{"ephemeral":true,"raw_payload":false}});
        }
        let runtime = self.tool_simulation.lock().await;
        match request.operation.as_str() {
            "status" => serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "operation": "status",
                "status": "ok",
                "mode": "dry_run",
                "state": "ready",
                "provenance": "synthetic_or_fixture",
                "projection_json": {"contract_id": crate::tool_simulation_runtime::CONTRACT_ID, "contract_version": 1, "ephemeral": true, "fixture_count": runtime.fixture_count(), "completed_count": runtime.completed_count(), "real_fallback": false, "raw_payload": false},
                "error_code": ""
            }),
            "run" => serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "operation": "run",
                "status": "unavailable",
                "mode": "dry_run",
                "state": "blocked",
                "provenance": "synthetic_or_fixture",
                "projection_json": {"ephemeral": true, "raw_payload": false, "real_fallback": false},
                "error_code": "payload_not_admitted"
            }),
            _ => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"unsupported_operation","projection_json":{"real_fallback":false}})
            }
        }
    }

    pub(crate) async fn write_tool_simulation_runtime_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: serde_json::Value = serde_json::from_slice(&payload)?;
        let projection = serde_json::to_vec(&value["projection_json"])?;
        let result = generated::ToolSimulationRuntimeEvent {
            schema_version: 1,
            request_id: value["request_id"].as_str().unwrap_or_default().into(),
            operation: value["operation"].as_str().unwrap_or_default().into(),
            status: value["status"].as_str().unwrap_or_default().into(),
            mode: value["mode"].as_str().unwrap_or_default().into(),
            state: value["state"].as_str().unwrap_or_default().into(),
            provenance: value["provenance"].as_str().unwrap_or_default().into(),
            run_id: value["run_id"].as_str().unwrap_or_default().into(),
            correlation_id: value["correlation_id"].as_str().unwrap_or_default().into(),
            contract_hash: value["contract_hash"].as_str().unwrap_or_default().into(),
            error_code: value["error_code"].as_str().unwrap_or_default().into(),
            projection_json: projection,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "tool_simulation_runtime.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::ToolSimulationRuntime(
                    result,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }
}

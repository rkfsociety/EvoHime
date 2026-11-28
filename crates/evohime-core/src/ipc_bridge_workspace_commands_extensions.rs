use super::*;

impl IpcBridge {
    pub(crate) async fn invoke_required_service(
        &self,
        capability_name: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, IpcBridgeError> {
        let request_id = format!("ipc-{capability_name}");
        match self
            .module_registry
            .invoke_if_ready(request_id, capability_name, payload, None, |_| Ok(()))
            .await
        {
            Some(Ok(value)) => Ok(value),
            Some(Err(err)) => Err(FrameError::Io(err.to_string()).into()),
            None => {
                Err(FrameError::Io(format!("service_module_unavailable:{capability_name}")).into())
            }
        }
    }

    pub(crate) async fn dispatch_runtime_intervention_pipeline(
        &self,
        operation: String,
        request: generated::RuntimeInterventionPipelineCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RuntimeInterventionPipeline {
                operation,
                run_id: request.run_id,
                payload: request.payload,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_code_diagnostics_feedback_loop(
        &self,
        operation: String,
        request: generated::CodeDiagnosticsFeedbackLoopCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let mut payload_bytes = request.payload;
        match operation.as_str() {
            "register_provider" => {
                let provider: serde_json::Value = serde_json::from_slice(&payload_bytes)
                    .map_err(|_| FrameError::Io("invalid_provider".into()))?;
                let validated = self
                    .invoke_required_service("tools.diagnostics_validate_provider", provider)
                    .await?;
                payload_bytes =
                    serde_json::to_vec(&validated).map_err(|e| FrameError::Io(e.to_string()))?;
            }
            "snapshot" | "gate" => {
                let snapshot: serde_json::Value = serde_json::from_slice(&payload_bytes)
                    .map_err(|_| FrameError::Io("invalid_snapshot".into()))?;
                let validated = self
                    .invoke_required_service("tools.diagnostics_validate_snapshot", snapshot)
                    .await?;
                payload_bytes =
                    serde_json::to_vec(&validated).map_err(|e| FrameError::Io(e.to_string()))?;
            }
            _ => {}
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        if operation == "delta" {
            let current: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_snapshot".into()))?;
            let (get_reply, get_resp) = oneshot::channel();
            coordinator
                .dispatch(CoreCommand::CodeDiagnosticsFeedbackLoop {
                    operation: "get_snapshot".to_owned(),
                    workspace_root_id: request.workspace_root_id.clone(),
                    payload: Vec::new(),
                    baseline_snapshot_id: request.baseline_snapshot_id.clone(),
                    expected_revision: request.expected_revision,
                    idempotency_key: request.idempotency_key.clone(),
                    reply: get_reply,
                })
                .await
                .map_err(|e| FrameError::Io(e.to_string()))?;
            let baseline_bytes = get_resp
                .await
                .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
                .map_err(FrameError::Io)
                .map_err(IpcBridgeError::from)?;
            let baseline: serde_json::Value = serde_json::from_slice(&baseline_bytes)
                .map_err(|_| FrameError::Io("invalid_baseline".into()))?;
            let delta = self
                .invoke_required_service(
                    "tools.diagnostics_delta",
                    serde_json::json!({ "baseline": baseline, "current": current }),
                )
                .await?;
            payload_bytes = serde_json::to_vec(&serde_json::json!({
                "baseline_id": baseline.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                "current_id": current.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                "delta": delta,
            }))
            .map_err(|e| FrameError::Io(e.to_string()))?;
        }
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CodeDiagnosticsFeedbackLoop {
                operation,
                workspace_root_id: request.workspace_root_id,
                payload: payload_bytes,
                baseline_snapshot_id: request.baseline_snapshot_id,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_code_review_lane(
        &self,
        request: generated::CodeReviewLaneCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CodeReviewLane {
                operation: request.operation,
                review_id: request.review_id,
                target_id: request.target_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_multi_reviewer_ensemble(
        &self,
        request: generated::MultiReviewerEnsembleCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let mut payload_bytes = request.payload;
        match request.operation.as_str() {
            "save_profile" => {
                let profile: serde_json::Value = serde_json::from_slice(&payload_bytes)
                    .map_err(|_| FrameError::Io("invalid_ensemble_profile".into()))?;
                let validated = self
                    .invoke_required_service("evals.ensemble_validate_profile", profile)
                    .await?;
                payload_bytes =
                    serde_json::to_vec(&validated).map_err(|e| FrameError::Io(e.to_string()))?;
            }
            "start" => {
                let run: serde_json::Value = serde_json::from_slice(&payload_bytes)
                    .map_err(|_| FrameError::Io("invalid_ensemble_run".into()))?;
                let prepared = self
                    .invoke_required_service(
                        "evals.ensemble_prepare_start",
                        serde_json::json!({
                            "run": run,
                            "expected_revision": request.expected_revision,
                        }),
                    )
                    .await?;
                payload_bytes =
                    serde_json::to_vec(&prepared).map_err(|e| FrameError::Io(e.to_string()))?;
            }
            "reconcile" => {
                let candidates: serde_json::Value = serde_json::from_slice(&payload_bytes)
                    .map_err(|_| FrameError::Io("invalid_candidates".into()))?;
                let clusters = self
                    .invoke_required_service("evals.ensemble_reconcile", candidates)
                    .await?;
                payload_bytes =
                    serde_json::to_vec(&clusters).map_err(|e| FrameError::Io(e.to_string()))?;
            }
            _ => {}
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        if request.operation == "cancel" {
            let (get_reply, get_resp) = oneshot::channel();
            coordinator
                .dispatch(CoreCommand::MultiReviewerEnsemble {
                    operation: "get_raw".to_owned(),
                    ensemble_id: request.ensemble_id.clone(),
                    payload: Vec::new(),
                    expected_revision: request.expected_revision,
                    idempotency_key: request.idempotency_key.clone(),
                    reply: get_reply,
                })
                .await
                .map_err(|e| FrameError::Io(e.to_string()))?;
            let raw_bytes = get_resp
                .await
                .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
                .map_err(FrameError::Io)
                .map_err(IpcBridgeError::from)?;
            let run: serde_json::Value = serde_json::from_slice(&raw_bytes)
                .map_err(|_| FrameError::Io("corrupt_ensemble_run".into()))?;
            let cancelled = self
                .invoke_required_service(
                    "evals.ensemble_prepare_cancel",
                    serde_json::json!({
                        "run": run,
                        "expected_revision": request.expected_revision,
                    }),
                )
                .await?;
            payload_bytes =
                serde_json::to_vec(&cancelled).map_err(|e| FrameError::Io(e.to_string()))?;
        }
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::MultiReviewerEnsemble {
                operation: request.operation.clone(),
                ensemble_id: request.ensemble_id,
                payload: payload_bytes,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        let out_bytes = response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)?;
        if request.operation == "status" {
            let wrapper: serde_json::Value = serde_json::from_slice(&out_bytes)
                .map_err(|_| FrameError::Io("corrupt_ensemble_run".into()))?;
            let status = wrapper
                .get("status")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let raw_run = wrapper
                .get("run")
                .cloned()
                .ok_or_else(|| FrameError::Io("corrupt_ensemble_run".into()))?;
            let run = self
                .invoke_required_service("evals.ensemble_validate_run", raw_run)
                .await?;
            let slot_count = run
                .get("slot_statuses")
                .and_then(|v| v.as_array())
                .map_or(0, Vec::len);
            return serde_json::to_vec(&serde_json::json!({
                "status": status,
                "ensemble_id": run.get("id").cloned().unwrap_or(serde_json::Value::Null),
                "revision": run.get("profile_revision").cloned().unwrap_or(serde_json::Value::Null),
                "slot_count": slot_count,
            }))
            .map_err(|e| FrameError::Io(e.to_string()))
            .map_err(IpcBridgeError::from);
        }
        Ok(out_bytes)
    }

    pub(crate) async fn dispatch_language_intelligence(
        &self,
        request: generated::LanguageIntelligenceCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::LanguageIntelligence {
                operation: request.operation,
                request_id: request.language_request_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_static_analysis_packs(
        &self,
        request: generated::StaticAnalysisPacksCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let mut payload_bytes = request.payload;
        if request.operation == "register" {
            let pack: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_analysis_pack".into()))?;
            let validated = self
                .invoke_required_service("tools.analysis_pack_validate", pack)
                .await?;
            payload_bytes = serde_json::to_vec(
                validated
                    .get("pack")
                    .ok_or_else(|| FrameError::Io("invalid_analysis_pack".into()))?,
            )
            .map_err(|e| FrameError::Io(e.to_string()))?;
        } else if request.operation == "evaluate" {
            let eval_req: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_analysis_request".into()))?;
            let evaluated = self
                .invoke_required_service(
                    "tools.analysis_pack_evaluate",
                    serde_json::json!({
                        "pack_id": request.pack_id,
                        "expected_revision": request.expected_revision,
                        "request": eval_req,
                    }),
                )
                .await?;
            payload_bytes =
                serde_json::to_vec(&evaluated).map_err(|e| FrameError::Io(e.to_string()))?;
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::StaticAnalysisPacks {
                operation: request.operation.clone(),
                pack_id: request.pack_id,
                payload: payload_bytes,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        let out_bytes = response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)?;
        if request.operation == "inspect" {
            let pack: serde_json::Value = serde_json::from_slice(&out_bytes)
                .map_err(|_| FrameError::Io("corrupt_analysis_pack".into()))?;
            let validated = self
                .invoke_required_service("tools.analysis_pack_validate", pack)
                .await?;
            let p = validated
                .get("pack")
                .ok_or_else(|| FrameError::Io("corrupt_analysis_pack".into()))?;
            let rule_count = validated
                .get("rule_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            return serde_json::to_vec(&serde_json::json!({
                "status": "ok",
                "pack_id": p.get("id").cloned().unwrap_or(serde_json::Value::Null),
                "revision": p.get("revision").cloned().unwrap_or(serde_json::Value::Null),
                "rule_count": rule_count,
                "trust_state": p.get("trust_state").cloned().unwrap_or(serde_json::Value::Null),
                "mode": p.get("default_mode").cloned().unwrap_or(serde_json::Value::Null),
            }))
            .map_err(|e| FrameError::Io(e.to_string()))
            .map_err(IpcBridgeError::from);
        }
        Ok(out_bytes)
    }
    pub(crate) async fn dispatch_context_loadouts(
        &self,
        request: generated::ContextLoadoutsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ContextLoadouts {
                operation: request.operation,
                profile_id: request.profile_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_skill_source_lifecycle(
        &self,
        request: generated::SkillSourceLifecycleCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SkillSourceLifecycle {
                operation: request.operation,
                installation_id: request.installation_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_kernel_capability_facade(
        &self,
        request: generated::KernelCapabilityFacadeCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::KernelCapabilityFacade {
                operation: request.operation,
                record_id: request.record_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_authorized_security_assessment(
        &self,
        request: generated::AuthorizedSecurityAssessmentCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::AuthorizedSecurityAssessment {
                operation: request.operation,
                assessment_id: request.assessment_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_runtime_service_graph(
        &self,
        request: generated::RuntimeServiceGraphCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RuntimeServiceGraph {
                operation: request.operation,
                graph_id: request.graph_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_agent_program_optimizer(
        &self,
        request: generated::AgentProgramOptimizerCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::AgentProgramOptimizer {
                operation: request.operation,
                program_id: request.program_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_project_knowledge_notebook(
        &self,
        request: generated::ProjectKnowledgeNotebookCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let mut payload_bytes = request.payload;
        if request.operation == "save" {
            let input: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_project_knowledge_notebook".into()))?;
            let validated = self
                .invoke_required_service("rag.notebook_validate", input)
                .await?;
            payload_bytes =
                serde_json::to_vec(&validated).map_err(|err| FrameError::Io(err.to_string()))?;
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        if request.operation == "pin" {
            let pin_req: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_notebook_pin".into()))?;
            let run_id = pin_req
                .get("run_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| FrameError::Io("run_id_required".into()))?
                .to_owned();
            let (get_reply, get_resp) = oneshot::channel();
            coordinator
                .dispatch(CoreCommand::ProjectKnowledgeNotebook {
                    operation: "get".to_owned(),
                    notebook_id: request.notebook_id.clone(),
                    payload: Vec::new(),
                    expected_revision: request.expected_revision,
                    idempotency_key: request.idempotency_key.clone(),
                    reply: get_reply,
                })
                .await
                .map_err(|e| FrameError::Io(e.to_string()))?;
            let stored_bytes = get_resp
                .await
                .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
                .map_err(FrameError::Io)
                .map_err(IpcBridgeError::from)?;
            let notebook: serde_json::Value = serde_json::from_slice(&stored_bytes)
                .map_err(|_| FrameError::Io("corrupt_notebook".into()))?;
            let pin = self
                .invoke_required_service(
                    "rag.notebook_pin",
                    serde_json::json!({ "notebook": notebook, "run_id": run_id }),
                )
                .await?;
            payload_bytes =
                serde_json::to_vec(&pin).map_err(|err| FrameError::Io(err.to_string()))?;
        }
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ProjectKnowledgeNotebook {
                operation: request.operation.clone(),
                notebook_id: request.notebook_id,
                payload: payload_bytes,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        let out_bytes = response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)?;
        if request.operation == "get" {
            let notebook: serde_json::Value = serde_json::from_slice(&out_bytes)
                .map_err(|_| FrameError::Io("corrupt_notebook".into()))?;
            let validated = self
                .invoke_required_service("rag.notebook_validate", notebook)
                .await?;
            let id = validated.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let revision = validated
                .get("revision")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let lifecycle = validated
                .get("lifecycle")
                .cloned()
                .unwrap_or_else(|| serde_json::json!("unknown"));
            let entry_count = validated
                .get("entries")
                .and_then(|v| v.as_array())
                .map_or(0, |a| a.len());
            let hash_prefix = validated
                .get("content_hash")
                .and_then(|v| v.as_str())
                .and_then(|s| s.get(..8))
                .unwrap_or("");
            return serde_json::to_vec(&serde_json::json!({
                "status": "ok",
                "notebook_id": id,
                "revision": revision,
                "lifecycle": lifecycle,
                "entry_count": entry_count,
                "content_hash_prefix": hash_prefix
            }))
            .map_err(|err| FrameError::Io(err.to_string()))
            .map_err(IpcBridgeError::from);
        }
        Ok(out_bytes)
    }
    pub(crate) async fn dispatch_git_remote_publication_protocol(
        &self,
        request: generated::GitRemotePublicationProtocolCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GitRemotePublicationProtocol {
                operation: request.operation,
                protocol_id: request.protocol_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_voice_input_dictation(
        &self,
        request: generated::VoiceInputDictationCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let mut payload_bytes = request.payload;
        if request.operation == "save" {
            let input: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_voice_input_dictation".into()))?;
            let validated = self
                .invoke_required_service("voice.dictation_validate", input)
                .await?;
            payload_bytes =
                serde_json::to_vec(&validated).map_err(|err| FrameError::Io(err.to_string()))?;
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::VoiceInputDictation {
                operation: request.operation.clone(),
                profile_id: request.profile_id,
                payload: payload_bytes,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        let stored_bytes = response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)?;
        if request.operation == "get" {
            let stored_value: serde_json::Value = serde_json::from_slice(&stored_bytes)
                .map_err(|_| FrameError::Io("corrupt_dictation_profile".into()))?;
            let availability = self
                .invoke_required_service("voice.dictation_availability", stored_value)
                .await?;
            return serde_json::to_vec(&availability)
                .map_err(|err| FrameError::Io(err.to_string()))
                .map_err(IpcBridgeError::from);
        }
        Ok(stored_bytes)
    }
    pub(crate) async fn dispatch_offline_experience_consolidation(
        &self,
        request: generated::OfflineExperienceConsolidationCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let mut payload_bytes = request.payload;
        if request.operation == "save" {
            let input: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_offline_consolidation".into()))?;
            let validated = self
                .invoke_required_service("memory.consolidation_validate", input)
                .await?;
            payload_bytes =
                serde_json::to_vec(&validated).map_err(|err| FrameError::Io(err.to_string()))?;
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        if request.operation == "consolidate" {
            let req: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_offline_consolidation".into()))?;
            let run_id = req
                .get("run_id")
                .and_then(|x| x.as_str())
                .ok_or_else(|| FrameError::Io("run_id_required".into()))?
                .to_owned();
            let (get_reply, get_resp) = oneshot::channel();
            coordinator
                .dispatch(CoreCommand::OfflineExperienceConsolidation {
                    operation: "get".to_owned(),
                    cycle_id: request.cycle_id.clone(),
                    payload: Vec::new(),
                    expected_revision: request.expected_revision,
                    idempotency_key: request.idempotency_key.clone(),
                    reply: get_reply,
                })
                .await
                .map_err(|e| FrameError::Io(e.to_string()))?;
            let stored_bytes = get_resp
                .await
                .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
                .map_err(FrameError::Io)
                .map_err(IpcBridgeError::from)?;
            let cycle: serde_json::Value = serde_json::from_slice(&stored_bytes)
                .map_err(|_| FrameError::Io("corrupt_consolidation_cycle".into()))?;
            let projection = self
                .invoke_required_service("memory.consolidation_evaluate", cycle.clone())
                .await?;
            payload_bytes = serde_json::to_vec(&serde_json::json!({
                "run_id": run_id,
                "revision": cycle.get("revision").and_then(|v| v.as_u64()).unwrap_or(0),
                "content_hash": cycle.get("content_hash").and_then(|v| v.as_str()).unwrap_or(""),
                "projection": projection
            }))
            .map_err(|err| FrameError::Io(err.to_string()))?;
        }
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::OfflineExperienceConsolidation {
                operation: request.operation.clone(),
                cycle_id: request.cycle_id,
                payload: payload_bytes,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        let out_bytes = response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)?;
        if request.operation == "get" {
            let cycle: serde_json::Value = serde_json::from_slice(&out_bytes)
                .map_err(|_| FrameError::Io("corrupt_consolidation_cycle".into()))?;
            let evaluated = self
                .invoke_required_service("memory.consolidation_evaluate", cycle)
                .await?;
            return serde_json::to_vec(&evaluated)
                .map_err(|err| FrameError::Io(err.to_string()))
                .map_err(IpcBridgeError::from);
        }
        Ok(out_bytes)
    }
    pub(crate) async fn dispatch_deterministic_review_execution_plan(
        &self,
        request: generated::DeterministicReviewExecutionPlanCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let mut payload_bytes = request.payload;
        if request.operation == "save" {
            let plan: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_review_plan".into()))?;
            let validated = self
                .invoke_required_service(
                    "evals.review_plan_validate",
                    serde_json::json!({ "plan": plan, "plan_id": request.plan_id }),
                )
                .await?;
            payload_bytes =
                serde_json::to_vec(&validated).map_err(|e| FrameError::Io(e.to_string()))?;
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::DeterministicReviewExecutionPlan {
                operation: request.operation.clone(),
                plan_id: request.plan_id,
                payload: payload_bytes,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        let out_bytes = response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)?;
        if request.operation == "get" {
            let plan: serde_json::Value = serde_json::from_slice(&out_bytes)
                .map_err(|_| FrameError::Io("corrupt_review_plan".into()))?;
            let verdict = self
                .invoke_required_service("evals.review_plan_verdict", plan)
                .await?;
            return serde_json::to_vec(&verdict)
                .map_err(|e| FrameError::Io(e.to_string()))
                .map_err(IpcBridgeError::from);
        }
        Ok(out_bytes)
    }

    pub(crate) async fn dispatch_workflow_optimization_lab(
        &self,
        operation: String,
        request: generated::WorkflowOptimizationLabCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::WorkflowOptimizationLab {
                operation,
                run_id: request.run_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_core_topic_subscription_event_bus(
        &self,
        operation: String,
        request: generated::CoreTopicSubscriptionEventBusCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CoreTopicSubscriptionEventBus {
                operation,
                payload: request.payload,
                capability: request.capability,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_dependency_aware_task_graph(
        &self,
        operation: String,
        request: generated::DependencyAwareTaskGraphCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let mut payload_bytes = request.payload;
        if operation == "validate" {
            let graph: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_graph".into()))?;
            let validated = self
                .invoke_required_service(
                    "workflow.task_graph_validate",
                    serde_json::json!({ "graph": graph, "grants": request.grants }),
                )
                .await?;
            return serde_json::to_vec(&validated)
                .map_err(|e| FrameError::Io(e.to_string()))
                .map_err(IpcBridgeError::from);
        }
        if operation == "create" {
            let graph: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_graph".into()))?;
            let _ = self
                .invoke_required_service(
                    "workflow.task_graph_validate",
                    serde_json::json!({ "graph": graph, "grants": request.grants }),
                )
                .await?;
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        if operation == "apply_patch" {
            let ops: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_patch".into()))?;
            let (get_reply, get_resp) = oneshot::channel();
            coordinator
                .dispatch(CoreCommand::DependencyAwareTaskGraph {
                    operation: "get".to_owned(),
                    graph_id: request.graph_id.clone(),
                    payload: Vec::new(),
                    expected_revision: request.expected_revision,
                    grants: request.grants.clone(),
                    reply: get_reply,
                })
                .await
                .map_err(|e| FrameError::Io(e.to_string()))?;
            let current_bytes = get_resp
                .await
                .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
                .map_err(FrameError::Io)
                .map_err(IpcBridgeError::from)?;
            let current: serde_json::Value = serde_json::from_slice(&current_bytes)
                .map_err(|_| FrameError::Io("corrupt_graph".into()))?;
            let patched = self
                .invoke_required_service(
                    "workflow.task_graph_patch",
                    serde_json::json!({
                        "current": current,
                        "ops": ops,
                        "expected_revision": request.expected_revision,
                        "grants": request.grants,
                    }),
                )
                .await?;
            payload_bytes =
                serde_json::to_vec(&patched).map_err(|e| FrameError::Io(e.to_string()))?;
        }
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::DependencyAwareTaskGraph {
                operation,
                graph_id: request.graph_id,
                payload: payload_bytes,
                expected_revision: request.expected_revision,
                grants: request.grants,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_safe_ui_extension_framework(
        &self,
        operation: String,
        request: generated::SafeUiExtensionFrameworkCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SafeUiExtensionFramework {
                operation,
                extension_id: request.extension_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_typed_context_references(
        &self,
        operation: String,
        request: generated::TypedContextReferencesCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::TypedContextReferences {
                operation,
                ref_id: request.ref_id,
                payload: request.payload,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_capability_workbench(
        &self,
        operation: String,
        request: generated::CapabilityWorkbenchCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CapabilityWorkbench {
                operation,
                instance_id: request.instance_id,
                owner_id: request.owner_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                grants: request.grants,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_team_coordinator(
        &self,
        operation: String,
        request: generated::TeamCoordinatorCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::TeamCoordinator {
                operation,
                work_item_id: request.work_item_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_project_instruction_stack(
        &self,
        operation: String,
        request: generated::ProjectInstructionStackCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ProjectInstructionStack {
                operation,
                workspace_root: request.workspace_root,
                payload: request.payload,
                relevant_paths: request.relevant_paths,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_workspace_sets(
        &self,
        operation: String,
        request: generated::WorkspaceSetsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::WorkspaceSets {
                operation,
                set_id: request.set_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_knowledge_source_registry(
        &self,
        operation: String,
        request: generated::KnowledgeSourceRegistryProjectRoleCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        let generated::KnowledgeSourceRegistryProjectRoleCommand {
            source_id,
            payload,
            expected_version,
            idempotency_key,
            ..
        } = request;
        if operation == "research_session_run" {
            let session_req: serde_json::Value = serde_json::from_slice(&payload)
                .map_err(|_| FrameError::Io("invalid_research_session_run".into()))?;
            let outcome = self
                .invoke_required_service("research.grounded_session", session_req)
                .await?;
            return serde_json::to_vec(&outcome)
                .map_err(|error| FrameError::Io(error.to_string()))
                .map_err(IpcBridgeError::from);
        }
        if matches!(
            operation.as_str(),
            "research_session" | "research_artifact" | "research_session_transition"
        ) {
            let parsed: serde_json::Value = serde_json::from_slice(&payload)
                .map_err(|_| FrameError::Io(format!("invalid_{operation}")))?;
            let validate_op = match operation.as_str() {
                "research_session" => "session",
                "research_artifact" => "artifact",
                _ => "transition_session",
            };
            self.invoke_required_service(
                "research.validate_grounded",
                serde_json::json!({
                    "operation": validate_op,
                    "payload": parsed,
                }),
            )
            .await?;
        }
        if operation == "research_artifact_promote" {
            let parsed: serde_json::Value = serde_json::from_slice(&payload)
                .map_err(|_| FrameError::Io("invalid_research_promotion".into()))?;
            let artifact = parsed
                .get("artifact")
                .cloned()
                .ok_or_else(|| FrameError::Io("invalid_research_promotion".into()))?;
            self.invoke_required_service(
                "research.validate_grounded",
                serde_json::json!({
                    "operation": "artifact",
                    "payload": artifact,
                }),
            )
            .await?;
        }
        coordinator
            .dispatch(CoreCommand::KnowledgeSourceRegistryProjectRole {
                operation,
                source_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_agent_git_change_sets(
        &self,
        operation: String,
        request: generated::AgentGitChangeSetsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::AgentGitChangeSets {
                operation,
                change_set_id: request.change_set_id,
                workspace_root: request.workspace_root,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_architect_editor_pipeline(
        &self,
        operation: String,
        request: generated::ArchitectEditorModelPipelineCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ArchitectEditorModelPipeline {
                operation,
                pipeline_id: request.pipeline_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_event_visualizer_registry(
        &self,
        operation: String,
        request: generated::EventVisualizerRegistryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::EventVisualizerRegistry {
                operation,
                visualizer_id: request.visualizer_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_reasoning_operator_library(
        &self,
        operation: String,
        request: generated::ReasoningOperatorLibraryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let mut payload_bytes = request.payload;
        match operation.as_str() {
            "register" => {
                let d: serde_json::Value = serde_json::from_slice(&payload_bytes)
                    .map_err(|_| FrameError::Io("invalid_operator_definition".into()))?;
                let validated = self
                    .invoke_required_service("model_router.operator_validate", d)
                    .await?;
                payload_bytes =
                    serde_json::to_vec(&validated).map_err(|e| FrameError::Io(e.to_string()))?;
            }
            "execute" => {
                let req_json: serde_json::Value = serde_json::from_slice(&payload_bytes)
                    .map_err(|_| FrameError::Io("invalid_operator_request".into()))?;
                let _ = self
                    .invoke_required_service(
                        "model_router.operator_validate_request",
                        serde_json::json!({
                            "operator_id": request.operator_id,
                            "request": req_json,
                        }),
                    )
                    .await?;
            }
            _ => {}
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ReasoningOperatorLibrary {
                operation: operation.clone(),
                operator_id: request.operator_id,
                payload: payload_bytes,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        let out_bytes = response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)?;
        if operation == "list" {
            let stored: serde_json::Value = serde_json::from_slice(&out_bytes)
                .map_err(|_| FrameError::Io("corrupt_operators".into()))?;
            let mut defs = self
                .invoke_required_service("model_router.operator_builtins", serde_json::json!({}))
                .await?
                .as_array()
                .cloned()
                .unwrap_or_default();
            if let Some(extra) = stored.get("stored").and_then(|v| v.as_array()) {
                defs.extend(extra.iter().cloned());
            }
            return serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "operators": defs,
                "redacted": true,
            }))
            .map_err(|e| FrameError::Io(e.to_string()))
            .map_err(IpcBridgeError::from);
        }
        Ok(out_bytes)
    }

    pub(crate) async fn dispatch_output_guardrail_pipeline(
        &self,
        operation: String,
        request: generated::OutputGuardrailPipelineCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let mut payload_bytes = request.payload;
        if operation == "evaluate" {
            let pipeline: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_guardrail_pipeline".into()))?;
            let evaluated = self
                .invoke_required_service(
                    "model_router.guardrail_evaluate",
                    serde_json::json!({
                        "pipeline_id": request.pipeline_id,
                        "version": request.expected_version,
                        "pipeline": pipeline,
                    }),
                )
                .await?;
            payload_bytes =
                serde_json::to_vec(&evaluated).map_err(|e| FrameError::Io(e.to_string()))?;
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::OutputGuardrailPipeline {
                operation,
                pipeline_id: request.pipeline_id,
                payload: payload_bytes,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_customization_inventory(
        &self,
        operation: String,
        request: generated::CustomizationInventoryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CustomizationInventory {
                operation,
                item_id: request.item_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_standing_approval_profiles(
        &self,
        operation: String,
        request: generated::StandingApprovalProfilesCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::StandingApprovalProfiles {
                operation,
                profile_id: request.profile_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_approval_policy_profiles(
        &self,
        operation: String,
        request: generated::ApprovalPolicyProfilesCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ApprovalPolicyProfiles {
                operation,
                profile_id: request.profile_id,
                payload: request.payload,
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_checkpoint_forking(
        &self,
        operation: String,
        request: generated::CheckpointForkingCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::CheckpointForking {
            operation,
            fork_run_id: request.fork_run_id,
            payload: request.payload,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_privacy_telemetry_governance(
        &self,
        operation: String,
        request: generated::PrivacyTelemetryGovernanceCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::PrivacyTelemetryGovernance {
            operation,
            category: request.category,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_conversation_bridge_adapters(
        &self,
        operation: String,
        request: generated::ConversationBridgeAdaptersCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::ConversationBridgeAdapters {
            operation,
            bridge_id: request.bridge_id,
            payload: request.payload,
            expected_revision: request.expected_revision,
            idempotency_key: request.idempotency_key,
            correlation_id: request.correlation_id,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_remote_conversation_channels(
        &self,
        operation: String,
        request: generated::RemoteConversationChannelsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::RemoteConversationChannels {
            operation,
            connection_id: request.connection_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_prompt_cache_planner(
        &self,
        operation: String,
        request: generated::PromptCachePlannerCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::PromptCachePlanner {
            operation,
            plan_id: request.plan_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_declarative_runtime_components(
        &self,
        operation: String,
        request: generated::DeclarativeRuntimeComponentsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::DeclarativeRuntimeComponents {
            operation,
            component_id: request.component_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_guided_calibration_sessions(
        &self,
        operation: String,
        request: generated::GuidedCalibrationSessionsCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::GuidedCalibrationSessions {
            operation,
            session_id: request.session_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_extension_conformance_kit(
        &self,
        operation: String,
        request: generated::ExtensionConformanceKitCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        if request.subject_id.is_empty()
            || request.idempotency_key.is_empty()
            || request.expected_version > u64::MAX - 1
        {
            return Err(FrameError::Io("invalid_conformance_request".into()).into());
        }
        let value: serde_json::Value = serde_json::from_slice(&request.payload)
            .map_err(|_| FrameError::Io("invalid_conformance_payload".into()))?;
        let computed = match operation.as_str() {
            "run" => {
                self.invoke_required_service(
                    "evals.conformance_run",
                    serde_json::json!({
                        "subject_id": request.subject_id,
                        "value": value,
                    }),
                )
                .await?
            }
            "register" => {
                self.invoke_required_service("evals.conformance_register", value)
                    .await?
            }
            "inspect" => {
                self.invoke_required_service("evals.conformance_inspect", serde_json::json!({}))
                    .await?
            }
            _ => return Err(FrameError::Io("unsupported_conformance_operation".into()).into()),
        };
        let payload_bytes =
            serde_json::to_vec(&computed).map_err(|e| FrameError::Io(e.to_string()))?;
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::ExtensionConformanceKit {
            operation,
            subject_id: request.subject_id,
            payload: payload_bytes,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_durable_remote_task_bridge(
        &self,
        operation: String,
        request: generated::DurableRemoteTaskBridgeCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::DurableRemoteTaskBridge {
            operation,
            remote_task_id: request.remote_task_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_message_intervention_policies(
        &self,
        operation: String,
        request: generated::MessageInterventionPoliciesCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::MessageInterventionPolicies {
            operation,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_batch_invocation_runtime(
        &self,
        operation: String,
        request: generated::BatchInvocationRuntimeCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let now_ms = crate::task_memory::now_millis() as i64;
        let mut payload_bytes = request.payload;
        if operation == "create" {
            let req_json: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_batch_payload".into()))?;
            let created = self
                .invoke_required_service(
                    "workflow.batch_create",
                    serde_json::json!({
                        "batch_id": request.batch_id,
                        "request": req_json,
                        "now_ms": now_ms,
                    }),
                )
                .await?;
            payload_bytes =
                serde_json::to_vec(&created).map_err(|e| FrameError::Io(e.to_string()))?;
        }
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        if matches!(operation.as_str(), "get" | "resume" | "start" | "result") {
            let (get_reply, get_resp) = oneshot::channel();
            c.dispatch(CoreCommand::BatchInvocationRuntime {
                operation: "get_raw".to_owned(),
                batch_id: request.batch_id.clone(),
                payload: Vec::new(),
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key.clone(),
                reply: get_reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
            let raw_bytes = get_resp
                .await
                .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
                .map_err(FrameError::Io)
                .map_err(IpcBridgeError::from)?;
            let stored: serde_json::Value = serde_json::from_slice(&raw_bytes)
                .map_err(|_| FrameError::Io("corrupt_batch".into()))?;
            let version = stored.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
            let value = stored
                .get("value")
                .cloned()
                .ok_or_else(|| FrameError::Io("corrupt_batch".into()))?;
            let req_arg = if operation == "result" {
                Some(
                    serde_json::from_slice::<serde_json::Value>(&payload_bytes)
                        .map_err(|_| FrameError::Io("invalid_batch_result".into()))?,
                )
            } else {
                None
            };
            let transitioned = self
                .invoke_required_service(
                    "workflow.batch_transition",
                    serde_json::json!({
                        "operation": operation,
                        "value": value,
                        "expected_version": request.expected_version.max(version),
                        "now_ms": now_ms,
                        "request": req_arg,
                    }),
                )
                .await?;
            payload_bytes =
                serde_json::to_vec(&transitioned).map_err(|e| FrameError::Io(e.to_string()))?;
        }
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::BatchInvocationRuntime {
            operation,
            batch_id: request.batch_id,
            payload: payload_bytes,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_policy_aware_tool_result_cache(
        &self,
        operation: String,
        request: generated::PolicyAwareToolResultCacheCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let now_ms = crate::task_memory::now_millis() as i64;
        let mut payload_bytes = request.payload;
        if operation == "inspect" {
            let policy = self
                .invoke_required_service("tools_exec.cache_policy", serde_json::json!({}))
                .await?;
            payload_bytes =
                serde_json::to_vec(&policy).map_err(|e| FrameError::Io(e.to_string()))?;
        } else if operation == "put" {
            let entry: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_cache_entry".into()))?;
            let validated = self
                .invoke_required_service(
                    "tools_exec.cache_validate_entry",
                    serde_json::json!({ "entry": entry, "now_ms": now_ms }),
                )
                .await?;
            payload_bytes =
                serde_json::to_vec(&validated).map_err(|e| FrameError::Io(e.to_string()))?;
        }
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        if operation == "invalidate" {
            let (get_reply, get_resp) = oneshot::channel();
            c.dispatch(CoreCommand::PolicyAwareToolResultCache {
                operation: "get_raw".to_owned(),
                cache_key: request.cache_key.clone(),
                payload: Vec::new(),
                expected_version: request.expected_version,
                idempotency_key: request.idempotency_key.clone(),
                reply: get_reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
            let raw_bytes = get_resp
                .await
                .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
                .map_err(FrameError::Io)
                .map_err(IpcBridgeError::from)?;
            let raw: serde_json::Value = serde_json::from_slice(&raw_bytes)
                .map_err(|_| FrameError::Io("corrupt_cache".into()))?;
            if let Some(entry) = raw.get("entry").filter(|v| !v.is_null()).cloned() {
                let invalidated = self
                    .invoke_required_service("tools_exec.cache_invalidate", entry)
                    .await?;
                payload_bytes = serde_json::to_vec(&serde_json::json!({
                    "version": raw.get("version").cloned().unwrap_or(serde_json::json!(0)),
                    "entry": invalidated,
                }))
                .map_err(|e| FrameError::Io(e.to_string()))?;
            }
        }
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::PolicyAwareToolResultCache {
            operation: operation.clone(),
            cache_key: request.cache_key.clone(),
            payload: payload_bytes,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        let out_bytes = response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)?;
        if operation == "get" {
            let raw: serde_json::Value = serde_json::from_slice(&out_bytes)
                .map_err(|_| FrameError::Io("corrupt_cache".into()))?;
            let hit = if let Some(entry) = raw.get("entry").filter(|v| !v.is_null()).cloned() {
                self.invoke_required_service(
                    "tools_exec.cache_validate_entry",
                    serde_json::json!({ "entry": entry, "now_ms": now_ms }),
                )
                .await
                .ok()
            } else {
                None
            };
            let provenance_ref = hit.as_ref().and_then(|e| e.get("provenance_ref").cloned());
            return serde_json::to_vec(&serde_json::json!({
                "status": if hit.is_some() { "hit" } else { "miss" },
                "cache_key": request.cache_key,
                "provenance_ref": provenance_ref,
                "redacted": true,
            }))
            .map_err(|e| FrameError::Io(e.to_string()))
            .map_err(IpcBridgeError::from);
        }
        Ok(out_bytes)
    }
    pub(crate) async fn dispatch_code_anchored_intent_markers(
        &self,
        operation: String,
        request: generated::CodeAnchoredIntentMarkersCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::CodeAnchoredIntentMarkers {
            operation,
            file_path: request.file_path,
            revision: request.revision,
            payload: request.payload,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_model_purpose_routing(
        &self,
        operation: String,
        request: generated::ModelPurposeRoutingCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::ModelPurposeRouting {
            operation,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }
    pub(crate) async fn dispatch_local_model_runtime_manager(
        &self,
        operation: String,
        request: generated::LocalModelRuntimeManagerCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::LocalModelRuntimeManager {
            operation,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_architecture_snapshot(
        &self,
        operation: String,
        request: generated::ArchitectureSnapshotCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let mut payload_bytes = request.payload;
        let parsed: serde_json::Value = if payload_bytes.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_architecture_snapshot_payload".into()))?
        };
        let root = parsed
            .get("workspace_root")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .unwrap_or(&request.workspace_root);
        let id = if request.snapshot_id.is_empty() {
            "architecture-current"
        } else {
            request.snapshot_id.as_str()
        };
        match operation.as_str() {
            "compare" => {
                let before = parsed
                    .get("before")
                    .cloned()
                    .ok_or_else(|| FrameError::Io("before_required".into()))?;
                let after = parsed
                    .get("after")
                    .cloned()
                    .ok_or_else(|| FrameError::Io("after_required".into()))?;
                let out = self
                    .invoke_required_service(
                        "tools.architecture_compare",
                        serde_json::json!({ "before": before, "after": after }),
                    )
                    .await?;
                return serde_json::to_vec(&serde_json::json!({
                    "status": "compared",
                    "delta": out.get("delta").cloned().unwrap_or(serde_json::Value::Null),
                    "redacted": true,
                }))
                .map_err(|e| FrameError::Io(e.to_string()).into());
            }
            "review" => {
                let expected = parsed
                    .get("expected")
                    .cloned()
                    .ok_or_else(|| FrameError::Io("expected_required".into()))?;
                let actual = parsed
                    .get("actual")
                    .cloned()
                    .ok_or_else(|| FrameError::Io("actual_required".into()))?;
                let out = self
                    .invoke_required_service(
                        "tools.architecture_review",
                        serde_json::json!({ "expected": expected, "actual": actual }),
                    )
                    .await?;
                return serde_json::to_vec(&serde_json::json!({
                    "status": "reviewed",
                    "review": out.get("review").cloned().unwrap_or(serde_json::Value::Null),
                    "redacted": true,
                }))
                .map_err(|e| FrameError::Io(e.to_string()).into());
            }
            "refresh" | "rebuild" | "current" => {
                let revision = parsed
                    .get("source_revision")
                    .and_then(|v| v.as_str())
                    .unwrap_or("working-tree");
                let allowed_roots = parsed
                    .get("allowed_roots")
                    .and_then(|v| v.as_array())
                    .filter(|arr| !arr.is_empty())
                    .cloned()
                    .unwrap_or_else(|| vec![serde_json::Value::String(root.to_owned())]);
                let extracted = self
                    .invoke_required_service(
                        "tools.architecture_extract",
                        serde_json::json!({
                            "root": root,
                            "revision": revision,
                            "snapshot_id": id,
                            "allowed_roots": allowed_roots,
                        }),
                    )
                    .await?;
                let mut merged = parsed.as_object().cloned().unwrap_or_default();
                merged.insert(
                    "workspace_identity".to_owned(),
                    extracted
                        .get("workspace_identity")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                );
                merged.insert(
                    "snapshot_hash".to_owned(),
                    extracted
                        .get("snapshot_hash")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                );
                merged.insert(
                    "snapshot".to_owned(),
                    extracted
                        .get("snapshot")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                );
                payload_bytes =
                    serde_json::to_vec(&merged).map_err(|e| FrameError::Io(e.to_string()))?;
            }
            "inspect" => {
                let fp = self
                    .invoke_required_service(
                        "tools.architecture_fingerprint",
                        serde_json::json!({
                            "root": root,
                            "revision": "working-tree",
                        }),
                    )
                    .await?;
                let mut merged = parsed.as_object().cloned().unwrap_or_default();
                merged.insert(
                    "workspace_identity".to_owned(),
                    fp.get("workspace_identity")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                );
                payload_bytes =
                    serde_json::to_vec(&merged).map_err(|e| FrameError::Io(e.to_string()))?;
            }
            _ => {}
        }
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::ArchitectureSnapshot {
            operation,
            snapshot_id: request.snapshot_id,
            workspace_root: request.workspace_root,
            payload: payload_bytes,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_persistent_agent_organization_registry(
        &self,
        operation: String,
        request: generated::PersistentAgentOrganizationRegistryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::PersistentAgentOrganizationRegistry {
                operation,
                agent_id: request.agent_id,
                owner_scope: request.owner_scope,
                // Renderer input cannot select a privileged actor. Core-only
                // recovery uses the internal EventJournal API directly.
                actor: "user".into(),
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_execution_environment_profile(
        &self,
        operation: String,
        request: generated::ExecutionEnvironmentProfileCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ExecutionEnvironmentProfile {
                operation,
                profile_id: request.profile_id,
                owner_scope: request.owner_scope,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_context_namespace(
        &self,
        operation: String,
        request: generated::ContextNamespaceCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        if request.schema_version != crate::context_namespace::SCHEMA_VERSION {
            return Err(FrameError::Io("unsupported context namespace schema".into()).into());
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ContextNamespace {
                operation,
                namespace_id: request.namespace_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_durable_background_execution(
        &self,
        operation: String,
        request: generated::BackgroundExecutionCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        if request.schema_version != crate::durable_background_execution::SCHEMA_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.payload.len() > crate::durable_background_execution::MAX_SNAPSHOT_BYTES
        {
            return Err(FrameError::Io("invalid background execution request".into()).into());
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::DurableBackgroundExecution {
                operation,
                run_id: request.run_id,
                owner_scope: request.owner_scope,
                payload: request.payload,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_memory_views_and_adaptive_recall(
        &self,
        operation: String,
        request: generated::MemoryViewsAndAdaptiveRecallCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::MemoryViewsAndAdaptiveRecall {
            operation,
            view_id: request.view_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_model_edit_protocol_registry(
        &self,
        operation: String,
        request: generated::ModelEditProtocolRegistryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let c = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        c.dispatch(CoreCommand::ModelEditProtocolRegistry {
            operation,
            protocol_id: request.protocol_id,
            payload: request.payload,
            expected_version: request.expected_version,
            idempotency_key: request.idempotency_key,
            reply,
        })
        .await
        .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_declarative_agent_component_registry(
        &self,
        operation: String,
        request: generated::DeclarativeAgentComponentRegistryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::DeclarativeAgentComponentRegistry {
                operation,
                registry_id: request.registry_id,
                payload: request.payload,
                expected_revision: request.expected_revision,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_experience_replay_library(
        &self,
        operation: String,
        request: generated::ExperienceReplayLibraryCommand,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let mut payload_bytes = request.payload;
        if operation == "write" {
            let record: serde_json::Value = serde_json::from_slice(&payload_bytes)
                .map_err(|_| FrameError::Io("invalid_experience_record".into()))?;
            let validated = self
                .invoke_required_service(
                    "memory.replay_validate",
                    serde_json::json!({
                        "scope": request.scope,
                        "scope_id": request.scope_id,
                        "record": record
                    }),
                )
                .await?;
            payload_bytes =
                serde_json::to_vec(&validated).map_err(|err| FrameError::Io(err.to_string()))?;
        }
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ExperienceReplayLibrary {
                operation: operation.clone(),
                scope: request.scope,
                scope_id: request.scope_id,
                payload: payload_bytes,
                expected_revision: request.expected_revision,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|e| FrameError::Io(e.to_string()))?;
        let out_bytes = response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)?;
        if operation == "context" {
            let listed: serde_json::Value = serde_json::from_slice(&out_bytes)
                .map_err(|_| FrameError::Io("corrupt_experience_records".into()))?;
            let records = listed
                .get("records")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([]));
            let projected = self
                .invoke_required_service(
                    "memory.replay_context",
                    serde_json::json!({ "records": records }),
                )
                .await?;
            return serde_json::to_vec(&projected)
                .map_err(|err| FrameError::Io(err.to_string()))
                .map_err(IpcBridgeError::from);
        }
        Ok(out_bytes)
    }
}

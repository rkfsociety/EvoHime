use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::RuntimeInterventionPipeline {
            operation,
            run_id,
            payload: _,
            idempotency_key,
            reply,
        } => {
            let result = async {
                    use crate::agent_middleware_pipeline::{AgentMiddlewarePipelineService, BuiltinPolicy, FailurePolicy, HandlerMode, HookPhase, MiddlewareRequest, MiddlewareSpec, PipelineDefinition, PipelineRunSnapshot, StateClass};
                    let definition = PipelineDefinition::new("runtime-intervention", 1, vec![MiddlewareSpec { id: "core-policy".into(), version: 1, priority: 0, phases: HookPhase::ALL.to_vec(), state_class: StateClass::Public, policy: BuiltinPolicy::Observe, mode: HandlerMode::ObserveOnly, failure_policy: FailurePolicy::FailClosed }]).map_err(|e| e.to_string())?;
                    let snapshot = PipelineRunSnapshot { run_id: run_id.clone(), definition_id: definition.definition_id.clone(), definition_revision: definition.revision, contract_hash: definition.contract_hash.clone(), policy_hash: "core-policy-v1".into(), capability_snapshot_hash: "core-capability-snapshot".into() };
                    let mut service = AgentMiddlewarePipelineService::new(definition, snapshot, "core-capability-snapshot").map_err(|e| e.to_string())?;
                    let request = MiddlewareRequest { run_id: run_id.clone(), correlation_id: format!("runtime:{run_id}"), idempotency_key, phase: HookPhase::BeforeAgent, input_hash: "metadata-only".into(), capability_snapshot_hash: "core-capability-snapshot".into(), intervention_depth: 0 };
                    let (outcome, events) = service.evaluate(&request).map_err(|e| e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"ok","operation":operation,"run_id":run_id,"outcome":outcome,"events":events})).map_err(|e| e.to_string())
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::RuntimeInterventionPipeline {
                run_id,
                operation,
                projection_json,
            };
            let journal = state.lock().await.journal.clone();
            if let Some(journal) = journal {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::CodeDiagnosticsFeedbackLoop {
            operation,
            workspace_root_id,
            payload,
            baseline_snapshot_id,
            expected_revision,
            idempotency_key: _,
            reply,
        } => {
            let event_operation = operation.clone();
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use evohime_local_storage::code_diagnostics_feedback_loop_store as store;
                    match operation.as_str() {
                        "register_provider" => { let p: crate::code_diagnostics_feedback_loop::Provider = serde_json::from_slice(&payload).map_err(|_| "invalid_provider".to_string())?; crate::code_diagnostics_feedback_loop::validate_provider(&p).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&p).map_err(|e|e.to_string())?; let saved=store::put_provider(database.connection(),&p.id,&json,&p.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":if saved {"registered"} else {"duplicate"},"provider_id":p.id,"revision":1})).map_err(|e|e.to_string()) }
                        "snapshot" => { let s: crate::code_diagnostics_feedback_loop::Snapshot=serde_json::from_slice(&payload).map_err(|_| "invalid_snapshot".to_string())?; crate::code_diagnostics_feedback_loop::validate_snapshot(&s).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&s).map_err(|e|e.to_string())?; let saved=store::put_snapshot(database.connection(),&s.id,&s.workspace_fingerprint,&json,&s.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":if saved {"stored"} else {"duplicate"},"snapshot_id":s.id,"revision":1})).map_err(|e|e.to_string()) }
                        "delta" => { let current: crate::code_diagnostics_feedback_loop::Snapshot=serde_json::from_slice(&payload).map_err(|_| "invalid_snapshot".to_string())?; let baseline_json=store::get_snapshot(database.connection(),&baseline_snapshot_id).map_err(|e|e.to_string())?.ok_or_else(|| "baseline_not_found".to_string())?; let baseline: crate::code_diagnostics_feedback_loop::Snapshot=serde_json::from_slice(&baseline_json).map_err(|_| "invalid_baseline".to_string())?; let d=crate::code_diagnostics_feedback_loop::delta(&baseline,&current).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&d).map_err(|e|e.to_string())?; let id=format!("{}:{}",baseline.id,current.id); let _=store::put_delta(database.connection(),&id,&baseline.id,&current.id,&json,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"ok","revision":expected_revision.saturating_add(1),"delta":d})).map_err(|e|e.to_string()) }
                        "gate" => { let s: crate::code_diagnostics_feedback_loop::Snapshot=serde_json::from_slice(&payload).map_err(|_| "invalid_snapshot".to_string())?; crate::code_diagnostics_feedback_loop::validate_snapshot(&s).map_err(|e|e.to_string())?; let errors=s.diagnostics.iter().filter(|d|!d.stale && d.severity=="error").count(); serde_json::to_vec(&serde_json::json!({"status":if errors==0 {"passed"} else {"blocked"},"error_count":errors,"revision":expected_revision})).map_err(|e|e.to_string()) }
                        _ => Err("unsupported_diagnostics_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::CodeDiagnosticsFeedbackLoop {
                workspace_root_id,
                operation: event_operation,
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
        CoreCommand::WorkflowOptimizationLab {
            operation,
            run_id,
            payload,
            expected_revision,
            idempotency_key: _,
            reply,
        } => {
            let result = async {
                    let journal=state.lock().await.journal.clone().ok_or_else(||"storage journal is not configured".to_string())?;
                    let database=journal.database().lock().await;
                    use evohime_local_storage::workflow_optimization_lab_store as store;
                    match operation.as_str() {
                        "evaluate" => {
                            let input: BenchmarkEvaluationInput = serde_json::from_slice(&payload).map_err(|_| "invalid_benchmark_request".to_string())?;
                            let report = crate::workflow_optimization_lab::evaluate_candidate(&run_id, &input.candidate, &input.request).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"evaluated","run_id":run_id,"report":report,"revision":expected_revision.saturating_add(1)})).map_err(|e| e.to_string())
                        }
                        "save_run" => { let run:crate::workflow_optimization_lab::OptimizationRun=serde_json::from_slice(&payload).map_err(|_|"invalid_optimization_run".to_string())?; crate::workflow_optimization_lab::validate_run(&run).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&run).map_err(|e|e.to_string())?; let saved=store::put_run(database.connection(),&run.id,&json,&run.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":if saved{"stored"}else{"duplicate"},"run_id":run.id,"revision":1})).map_err(|e|e.to_string()) }
                        "get_run" => { let json=store::get_run(database.connection(),&run_id).map_err(|e|e.to_string())?.ok_or_else(||"run_not_found".to_string())?; let run:crate::workflow_optimization_lab::OptimizationRun=serde_json::from_slice(&json).map_err(|_|"corrupt_run".to_string())?; serde_json::to_vec(&serde_json::json!({"status":"ok","run":run,"revision":expected_revision})).map_err(|e|e.to_string()) }
                        "validate_candidate" => { let c:crate::workflow_optimization_lab::Candidate=serde_json::from_slice(&payload).map_err(|_|"invalid_candidate".to_string())?; crate::workflow_optimization_lab::validate_candidate(&c,crate::workflow_optimization_lab::Split::Validation).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"validated","candidate_id":c.id,"revision":expected_revision})).map_err(|e|e.to_string()) }
                        "promote" => { let c:crate::workflow_optimization_lab::Candidate=serde_json::from_slice(&payload).map_err(|_|"invalid_candidate".to_string())?; let run_json=store::get_run(database.connection(),&run_id).map_err(|e|e.to_string())?.ok_or_else(||"run_not_found".to_string())?; let run:crate::workflow_optimization_lab::OptimizationRun=serde_json::from_slice(&run_json).map_err(|_|"corrupt_run".to_string())?; crate::workflow_optimization_lab::promotion_allowed(&run,&c,true,true).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"promoted","run_id":run_id,"candidate_id":c.id,"revision":expected_revision.saturating_add(1)})).map_err(|e|e.to_string()) }
                        _ => Err("unsupported_optimization_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::WorkflowOptimizationLab {
                run_id,
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
        CoreCommand::CoreTopicSubscriptionEventBus {
            operation,
            payload,
            capability,
            idempotency_key: _,
            reply,
        } => {
            let result = async {
                    let journal=state.lock().await.journal.clone().ok_or_else(||"storage journal is not configured".to_string())?;
                    let database=journal.database().lock().await;
                    use evohime_local_storage::core_topic_subscription_event_bus_store as store;
                    let required=if operation=="publish"{"runtime.events.publish"}else{"runtime.events.read"};
                    if capability!=required{return Err("capability_denied".into())}
                    match operation.as_str() {
                        "publish"=>{let e:crate::core_topic_subscription_event_bus::Event=serde_json::from_slice(&payload).map_err(|_|"invalid_event".to_string())?;crate::core_topic_subscription_event_bus::validate_event(&e).map_err(|e|e.to_string())?;let json=serde_json::to_vec(&e).map_err(|e|e.to_string())?;let saved=store::put_event(database.connection(),&e.event_id,&json,&e.content_hash,"published",crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?;serde_json::to_vec(&serde_json::json!({"status":if saved{"published"}else{"duplicate"},"event_id":e.event_id})).map_err(|e|e.to_string())}
                        "subscribe"=>{let s:crate::core_topic_subscription_event_bus::Subscription=serde_json::from_slice(&payload).map_err(|_|"invalid_subscription".to_string())?;crate::core_topic_subscription_event_bus::validate_subscription(&s).map_err(|e|e.to_string())?;serde_json::to_vec(&serde_json::json!({"status":"subscribed","subscription_id":s.id})).map_err(|e|e.to_string())}
                        "ack"|"nack"=>{let request:DeliveryRequest=serde_json::from_slice(&payload).map_err(|_|"invalid_delivery".to_string())?;let attempt=request.attempt.min(u32::MAX as u64) as u32;let next=crate::core_topic_subscription_event_bus::transition(crate::core_topic_subscription_event_bus::DeliveryState::InFlight,operation.as_str(),attempt).map_err(|e|e.to_string())?;store::put_delivery(database.connection(),&request.subscription_id,&request.event_id,&format!("{next:?}"),attempt,request.error.as_deref(),crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?;if matches!(next,crate::core_topic_subscription_event_bus::DeliveryState::DeadLetter){store::put_dead_letter(database.connection(),&request.subscription_id,&request.event_id,attempt,"consumer_failure","redacted",crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?;}serde_json::to_vec(&serde_json::json!({"status":format!("{next:?}").to_lowercase(),"attempt":attempt})).map_err(|e|e.to_string())}
                        _=>Err("unsupported_bus_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::CoreTopicSubscriptionEventBus {
                operation,
                projection_json,
            };
            let journal = state.lock().await.journal.clone();
            if let Some(journal) = journal {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::DependencyAwareTaskGraph {
            operation,
            graph_id,
            payload,
            expected_revision,
            grants,
            reply,
        } => {
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use evohime_local_storage::dependency_aware_task_graph_store as store;
                    match operation.as_str() {
                        "validate" => { let graph: crate::dependency_aware_task_graph::TaskGraph = serde_json::from_slice(&payload).map_err(|_| "invalid_graph".to_string())?; crate::dependency_aware_task_graph::validate(&graph, &grants).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"valid","ready":crate::dependency_aware_task_graph::ready_set(&graph),"revision":graph.revision})).map_err(|e|e.to_string()) }
                        "get" => store::get(database.connection(), &graph_id).map_err(|e|e.to_string())?.ok_or_else(|| "graph_not_found".to_string()),
                        "create" => { let graph: crate::dependency_aware_task_graph::TaskGraph=serde_json::from_slice(&payload).map_err(|_|"invalid_graph".to_string())?; crate::dependency_aware_task_graph::validate(&graph,&grants).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&graph).map_err(|e|e.to_string())?; if !store::put(database.connection(),&graph_id,graph.revision,&json,&graph.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())? { return Err("graph_exists".into()); } Ok(json) }
                        "apply_patch" => { let bytes=store::get(database.connection(),&graph_id).map_err(|e|e.to_string())?.ok_or_else(||"graph_not_found".to_string())?; let current: crate::dependency_aware_task_graph::TaskGraph=serde_json::from_slice(&bytes).map_err(|_|"corrupt_graph".to_string())?; let ops: Vec<crate::dependency_aware_task_graph::PatchOp>=serde_json::from_slice(&payload).map_err(|_|"invalid_patch".to_string())?; let next=crate::dependency_aware_task_graph::apply_patch(current,&ops,expected_revision,&grants).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&next).map_err(|e|e.to_string())?; if !store::replace(database.connection(),&graph_id,expected_revision,next.revision,&json,&next.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())? { return Err("stale_graph_revision".into()); } Ok(json) }
                        _ => Err("unsupported_task_graph_operation".into())
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::DependencyAwareTaskGraph {
                graph_id,
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
        CoreCommand::DeclarativeAgentComponentRegistry {
            operation,
            registry_id,
            payload,
            expected_revision,
            reply,
        } => {
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use evohime_local_storage::declarative_agent_component_registry_store as store;
                    match operation.as_str() {
                        "get" => store::get(database.connection(), &registry_id).map_err(|e|e.to_string())?.ok_or_else(|| "registry_not_found".to_string()),
                        "validate" => { let registry: crate::declarative_agent_component_registry::Registry=serde_json::from_slice(&payload).map_err(|_|"invalid_registry".to_string())?; crate::declarative_agent_component_registry::validate_registry(&registry).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"valid","revision":registry.revision,"providers":registry.providers.len(),"components":registry.components.len()})).map_err(|e|e.to_string()) }
                        "create" => { let registry: crate::declarative_agent_component_registry::Registry=serde_json::from_slice(&payload).map_err(|_|"invalid_registry".to_string())?; crate::declarative_agent_component_registry::validate_registry(&registry).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&registry).map_err(|e|e.to_string())?; if !store::put(database.connection(),&registry_id,registry.revision,&json,&registry.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())? {return Err("registry_exists".into())} Ok(json) }
                        "replace" => { let registry: crate::declarative_agent_component_registry::Registry=serde_json::from_slice(&payload).map_err(|_|"invalid_registry".to_string())?; crate::declarative_agent_component_registry::validate_registry(&registry).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&registry).map_err(|e|e.to_string())?; if !store::replace(database.connection(),&registry_id,expected_revision,registry.revision,&json,&registry.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())? {return Err("stale_registry_revision".into())} Ok(json) }
                        "diff" => { let pair: Vec<crate::declarative_agent_component_registry::ComponentDescriptor>=serde_json::from_slice(&payload).map_err(|_|"invalid_diff".to_string())?; if pair.len()!=2{return Err("diff_requires_two_descriptors".into())}; serde_json::to_vec(&crate::declarative_agent_component_registry::diff(&pair[0],&pair[1]).map_err(|e|e.to_string())?).map_err(|e|e.to_string()) }
                        _ => Err("unsupported_component_registry_operation".into())
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::DeclarativeAgentComponentRegistry {
                registry_id,
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
        CoreCommand::TypedContextReferences {
            operation,
            ref_id,
            payload,
            reply,
        } => {
            let result = async {
                if operation == "parse" {
                    return serde_json::to_vec(&crate::typed_context_references::parse_mentions(
                        std::str::from_utf8(&payload).unwrap_or_default(),
                        true,
                    ))
                    .map_err(|e| e.to_string());
                }
                let reference: crate::typed_context_references::ContextRef =
                    serde_json::from_slice(&payload)
                        .map_err(|_| "invalid_context_ref".to_string())?;
                crate::typed_context_references::validate_ref(&reference)
                    .map_err(|e| e.to_string())?;
                let resolved = match operation.as_str() {
                    "parse" => {
                        serde_json::to_vec(&crate::typed_context_references::parse_mentions(
                            std::str::from_utf8(&payload).unwrap_or_default(),
                            true,
                        ))
                        .map_err(|e| e.to_string())?
                    }
                    "resolve" => serde_json::to_vec(
                        &crate::typed_context_references::resolve(
                            &reference,
                            reference.revision_hint.clone(),
                            None,
                        )
                        .map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?,
                    "budget" => {
                        let refs: Vec<crate::typed_context_references::ResolvedContextRef> =
                            serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_budget_refs".to_string())?;
                        serde_json::to_vec(&crate::typed_context_references::plan_budget(
                            &refs, 4096,
                        ))
                        .map_err(|e| e.to_string())?
                    }
                    "kinds" => {
                        serde_json::to_vec(&crate::typed_context_references::supported_kinds())
                            .map_err(|e| e.to_string())?
                    }
                    _ => return Err("unsupported_context_reference_operation".into()),
                };
                Ok(resolved)
            }
            .await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::TypedContextReferences {
                ref_id,
                operation,
                projection_json,
            };
            let journal = state.lock().await.journal.clone();
            if let Some(journal) = journal {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::SafeUiExtensionFramework {
            operation,
            extension_id,
            payload,
            expected_revision,
            reply,
        } => {
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use evohime_local_storage::safe_ui_extension_framework_store as store;
                    match operation.as_str() {
                        "install" => {
                            let manifest: crate::safe_ui_extension_framework::UiExtensionManifest = serde_json::from_slice(&payload).map_err(|_| "invalid_manifest".to_string())?;
                            if manifest.id != extension_id { return Err("extension_id_mismatch".into()); }
                            let installed = crate::safe_ui_extension_framework::install(manifest, "workspace", "revision-1").map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&installed).map_err(|e| e.to_string())?;
                            if !store::put(database.connection(), &extension_id, installed.revision, &format!("{:?}", installed.lifecycle), &json, &installed.manifest_hash, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())? { return Err("extension_exists".into()); }
                            Ok(json)
                        }
                        "get" => store::get(database.connection(), &extension_id).map_err(|e| e.to_string())?.ok_or_else(|| "extension_not_found".into()),
                        "validate" => {
                            let manifest: crate::safe_ui_extension_framework::UiExtensionManifest = serde_json::from_slice(&payload).map_err(|_| "invalid_manifest".to_string())?;
                            crate::safe_ui_extension_framework::validate(&manifest).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"valid","id":manifest.id,"contributions":manifest.contributions.len()})).map_err(|e| e.to_string())
                        }
                        "enable" | "disable" => {
                            let json = store::get(database.connection(), &extension_id).map_err(|e| e.to_string())?.ok_or_else(|| "extension_not_found".to_string())?;
                            let mut installed: crate::safe_ui_extension_framework::InstalledUiExtension = serde_json::from_slice(&json).map_err(|_| "corrupt_extension".to_string())?;
                            let target = if operation == "enable" { crate::safe_ui_extension_framework::Lifecycle::Enabled } else { crate::safe_ui_extension_framework::Lifecycle::Disabled };
                            crate::safe_ui_extension_framework::transition(&mut installed, target, expected_revision).map_err(|e| e.to_string())?;
                            let next = serde_json::to_vec(&installed).map_err(|e| e.to_string())?;
                            if !store::replace(database.connection(), &extension_id, installed.revision, &format!("{:?}", installed.lifecycle), &next, &installed.manifest_hash, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())? { return Err("extension_not_found".into()); }
                            Ok(next)
                        }
                        "update" => {
                            let manifest: crate::safe_ui_extension_framework::UiExtensionManifest = serde_json::from_slice(&payload).map_err(|_| "invalid_manifest".to_string())?;
                            if manifest.id != extension_id { return Err("extension_id_mismatch".into()); }
                            let current_json = store::get(database.connection(), &extension_id).map_err(|e| e.to_string())?.ok_or_else(|| "extension_not_found".to_string())?;
                            let current: crate::safe_ui_extension_framework::InstalledUiExtension = serde_json::from_slice(&current_json).map_err(|_| "corrupt_extension".to_string())?;
                            if current.revision != expected_revision { return Err("stale revision".into()); }
                            if current.manifest.required_projection_capabilities != manifest.required_projection_capabilities { return Err("capability delta requires review".into()); }
                            let mut updated = crate::safe_ui_extension_framework::install(manifest, &current.scope, &current.resolved_revision).map_err(|e| e.to_string())?;
                            updated.revision = current.revision + 1;
                            let next = serde_json::to_vec(&updated).map_err(|e| e.to_string())?;
                            if !store::replace(database.connection(), &extension_id, updated.revision, &format!("{:?}", updated.lifecycle), &next, &updated.manifest_hash, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())? { return Err("extension_not_found".into()); }
                            Ok(next)
                        }
                        _ => Err("unsupported_ui_extension_operation".into())
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|b| String::from_utf8(b.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::SafeUiExtensionFramework {
                extension_id,
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
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}

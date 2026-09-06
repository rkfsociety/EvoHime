use super::*;

impl IpcBridge {
    // ------------------------------------------------------------------
    // Workflow orchestration (план 06.3).
    //
    // Мост здесь только курьер. Он не планирует граф, не решает порядок и не
    // выполняет узлы: всё это делает `workflow_runtime`, а наружу уходит
    // bounded projection — идентификаторы, состояния и коды. Ни prompt, ни
    // сырой вывод child, ни содержимое контекста через эти команды не
    // проходят.
    // ------------------------------------------------------------------

    /// Собирает runtime под конкретный рабочий каталог.
    ///
    /// Runtime создаётся на команду, а не хранится: состояние запуска durable,
    /// поэтому «живого» объекта между командами не требуется, а рабочий
    /// каталог у каждого запуска свой.
    pub(crate) fn dispatch_list_workflow_templates(&self) -> serde_json::Value {
        let templates: Vec<serde_json::Value> = crate::workflow_templates::catalog()
            .into_iter()
            .map(|template| {
                serde_json::json!({
                    "template_id": template.template_id,
                    "version": template.version,
                    "display_name": template.display_name,
                    "description": template.description,
                    "inputs": template
                        .inputs
                        .iter()
                        .map(|input| serde_json::json!({
                            "name": input.name,
                            "title": input.title,
                            "required": input.required,
                            "max_chars": input.max_chars,
                        }))
                        .collect::<Vec<_>>(),
                    "required_capabilities": template.required_capabilities,
                    "schedule_eligibility": template.schedule_eligibility.as_str(),
                    "preview": template.preview,
                    "node_count": template.graph().nodes.len(),
                })
            })
            .collect();
        serde_json::json!({ "templates": templates, "error_code": "" })
    }

    pub(crate) fn dispatch_workflow_definition(
        &self,
        request: generated::GetWorkflowDefinition,
    ) -> serde_json::Value {
        let Some(template) = crate::workflow_templates::template(&request.template_id) else {
            return serde_json::json!({
                "template_id": request.template_id,
                "nodes": Vec::<serde_json::Value>::new(),
                "edges": Vec::<serde_json::Value>::new(),
                "error_code": "unknown_template",
            });
        };
        let graph = template.graph();
        serde_json::json!({
            "template_id": template.template_id,
            "version": template.version,
            "display_name": template.display_name,
            "graph_id": graph.graph_id,
            "graph_version": graph.version,
            "graph_hash": graph.canonical_hash(),
            "schedule_eligibility": template.schedule_eligibility.as_str(),
            "preview": template.preview,
            "nodes": graph
                .nodes
                .iter()
                .map(|node| serde_json::json!({
                    "node_id": node.id,
                    "action_kind": node.node_type.action_kind(),
                    "approval_required": node.execution.approval.required,
                    "block_id": node
                        .block
                        .as_ref()
                        .map(|block| block.block_id.clone())
                        .unwrap_or_default(),
                    "block_version": node
                        .block
                        .as_ref()
                        .map(|block| block.block_version)
                        .unwrap_or_default(),
                }))
                .collect::<Vec<_>>(),
            "edges": graph
                .edges
                .iter()
                .map(|edge| serde_json::json!({
                    "from_node": edge.from_node,
                    "to_node": edge.to_node,
                    "channel": match edge.channel {
                        crate::workflow::EdgeChannel::Failure => "failure",
                        crate::workflow::EdgeChannel::Data => "data",
                    },
                }))
                .collect::<Vec<_>>(),
            "error_code": "",
        })
    }

    pub(crate) async fn dispatch_start_workflow(
        &self,
        request: generated::StartWorkflow,
    ) -> serde_json::Value {
        let Some(template) = crate::workflow_templates::template(&request.template_id) else {
            return workflow_start_failure("unknown_template");
        };
        let inputs: std::collections::BTreeMap<String, String> = request
            .inputs
            .iter()
            .map(|input| (input.name.clone(), input.value.clone()))
            .collect();
        let graph = match template.instantiate(&inputs) {
            Ok(graph) => graph,
            Err(error) => return workflow_start_failure(error.code()),
        };

        // Идемпотентность: тот же ключ даёт тот же `run_id`, поэтому двойной
        // клик возвращает первый запуск, а не создаёт второй.
        let run_id = if request.idempotency_key.trim().is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            let digest = <sha2::Sha256 as sha2::Digest>::digest(
                format!("{}|{}", request.template_id, request.idempotency_key).as_bytes(),
            );
            format!("wf-{}", hex_encode(&digest[..16]))
        };
        if let Ok(Some(existing)) = self.journal.workflow_run(&run_id).await {
            return serde_json::json!({
                "run_id": existing.run_id,
                "state": existing.state.as_str(),
                "graph_hash": existing.graph_hash,
                "deduplicated": true,
                "error_code": "",
            });
        }

        let workspace_path = request.workspace_path.clone();
        let runtime = self.workflow_runtime(&workspace_path);
        let start = crate::workflow_runtime::StartWorkflowRequest {
            run_id: run_id.clone(),
            task_id: if request.task_id.trim().is_empty() {
                run_id.clone()
            } else {
                request.task_id.clone()
            },
            workspace_path: workspace_path.clone(),
            template_id: template.template_id.clone(),
            template_version: template.version,
            inputs,
            graph,
            parent: workflow_parent_capabilities(),
        };
        match runtime.start(start).await {
            Ok(run_id) => {
                if !self
                    .spawn_workflow_drive(run_id.clone(), workspace_path)
                    .await
                {
                    return workflow_start_failure("background_task_capacity_exhausted");
                }
                serde_json::json!({
                    "run_id": run_id,
                    "state": "pending",
                    "graph_hash": "",
                    "deduplicated": false,
                    "error_code": "",
                })
            }
            Err(error) => workflow_start_failure(error.code()),
        }
    }

    pub(crate) async fn dispatch_workflow_run(
        &self,
        request: generated::GetWorkflowRun,
    ) -> serde_json::Value {
        let workspace = self.journal.workflow_run_workspace(&request.run_id).await;
        let runtime = self.workflow_runtime(&workspace);
        match runtime.projection(&request.run_id).await {
            Ok(Some(projection)) => {
                let mut value = serde_json::to_value(&projection).unwrap_or_default();
                if let Some(object) = value.as_object_mut() {
                    object.insert("error_code".into(), serde_json::json!(""));
                }
                value
            }
            Ok(None) => serde_json::json!({
                "run_id": request.run_id,
                "nodes": Vec::<serde_json::Value>::new(),
                "state": "unknown_state",
                "error_code": "unknown_run",
            }),
            Err(error) => serde_json::json!({
                "run_id": request.run_id,
                "nodes": Vec::<serde_json::Value>::new(),
                "state": "unknown_state",
                "error_code": error.code(),
            }),
        }
    }

    pub(crate) async fn dispatch_cancel_workflow(
        &self,
        request: generated::CancelWorkflow,
    ) -> serde_json::Value {
        let now_ms = crate::task_memory::now_millis() as i64;
        let cancelled = self
            .journal
            .request_workflow_cancel(&request.run_id, now_ms)
            .await
            .unwrap_or(false);
        if cancelled {
            let workspace = self.journal.workflow_run_workspace(&request.run_id).await;
            let _ = self
                .spawn_workflow_drive(request.run_id.clone(), workspace)
                .await;
        }
        serde_json::json!({
            "run_id": request.run_id,
            "cancelled": cancelled,
            "error_code": if cancelled { "" } else { "not_cancellable" },
        })
    }

    pub(crate) async fn dispatch_list_workflow_events(
        &self,
        request: generated::ListWorkflowEvents,
    ) -> serde_json::Value {
        let limit = if request.limit <= 0 {
            100usize
        } else {
            (request.limit as usize).min(500)
        };
        match self
            .journal
            .list_workflow_events(&request.run_id, request.after_sequence, limit)
            .await
        {
            Ok(events) => serde_json::json!({
                "run_id": request.run_id,
                "events": events
                    .into_iter()
                    .map(|event| serde_json::json!({
                        "sequence": event.run_sequence,
                        "node_id": event.node_id,
                        "event_type": event.event_type,
                        "payload": event.payload_json,
                        "created_at_ms": event.created_at_ms,
                    }))
                    .collect::<Vec<_>>(),
                "error_code": "",
            }),
            Err(error) => serde_json::json!({
                "run_id": request.run_id,
                "events": Vec::<serde_json::Value>::new(),
                "error_code": error.to_string(),
            }),
        }
    }

    pub(crate) async fn dispatch_visual_workflow_builder(
        &self,
        request: generated::VisualWorkflowBuilderCommand,
    ) -> serde_json::Value {
        if request.operation == "catalog" {
            let blocks = self.workflow_registry.blocks().map(|block| serde_json::json!({"block_id": block.block_id, "block_version": block.block_version, "display_name": block.display_name, "description": block.description, "action_kind": block.action_kind, "inputs": block.inputs, "outputs": block.outputs})).collect::<Vec<_>>();
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"catalog","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"","truncated":false,"blocks":blocks});
        }
        if request.operation == "recover" {
            let database = self.journal.database().lock().await;
            return match evohime_local_storage::visual_workflow_builder_store::read_draft(
                database.connection(),
                &request.draft_id,
                &request.owner_scope,
            ) {
                Ok(Some((revision, _definition, execution_hash, layout_hash))) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"recovered","draft_id":request.draft_id,"revision":revision,"execution_hash":execution_hash,"layout_hash":layout_hash,"handoff_handle":"","error_code":"","truncated":false})
                }
                Ok(None) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"missing","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"unknown_draft","truncated":false})
                }
                Err(_) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"corrupt","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false})
                }
            };
        }
        if request.operation == "inspect" {
            let run_id = String::from_utf8(request.payload.to_vec()).unwrap_or_default();
            let workspace = self.journal.workflow_run_workspace(&run_id).await;
            let runtime = self.workflow_runtime(&workspace);
            return match runtime.projection(&run_id).await {
                Ok(Some(projection)) => {
                    let value = serde_json::to_value(projection).unwrap_or_default();
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"inspected","draft_id":request.draft_id,"revision":request.expected_revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"","truncated":false,"projection":value})
                }
                Ok(None) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"unknown","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"unknown_run","truncated":false})
                }
                Err(_error) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"runtime_error","truncated":false})
                }
            };
        }
        if request.operation == "edit" {
            let database = self.journal.database().lock().await;
            let draft = evohime_local_storage::visual_workflow_builder_store::read_draft(
                database.connection(),
                &request.draft_id,
                &request.owner_scope,
            );
            return match draft {
                Ok(Some((revision, definition_json, _, _)))
                    if revision == request.expected_revision =>
                {
                    let parsed = serde_json::from_slice::<
                        crate::visual_workflow_builder::VisualWorkflowBuilderDefinition,
                    >(&definition_json);
                    let command = serde_json::from_slice::<
                        crate::visual_workflow_builder::DraftCommand,
                    >(&request.payload);
                    match (parsed, command) {
                        (Ok(mut definition), Ok(command)) => match command
                            .apply(&mut definition)
                            .and_then(|_| self.validate_visual_workflow_definition(&definition))
                        {
                            Ok(()) => {
                                let definition_json =
                                    serde_json::to_vec(&definition).unwrap_or_default();
                                let layout_json =
                                    serde_json::to_vec(&definition.layout).unwrap_or_default();
                                let execution_hash = definition.execution_hash();
                                let layout_hash = definition.layout_hash();
                                match evohime_local_storage::visual_workflow_builder_store::save_draft(database.connection(), evohime_local_storage::visual_workflow_builder_store::SaveDraft { draft_id: &request.draft_id, owner_scope: &request.owner_scope, expected_revision: revision, definition_json: &definition_json, layout_json: &layout_json, execution_hash: &execution_hash, layout_hash: &layout_hash, composer_provenance_json: None, updated_at_ms: crate::task_memory::now_millis() as i64 }) {
                                    Ok(Ok(next_revision)) => serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"edited","draft_id":request.draft_id,"revision":next_revision,"execution_hash":execution_hash,"layout_hash":layout_hash,"handoff_handle":"","error_code":"","truncated":false}),
                                    Ok(Err(code)) => serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"conflict","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":code,"truncated":false}),
                                    Err(_) => serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false}),
                                }
                            }
                            Err(error) => {
                                serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"invalid","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":error.to_string(),"truncated":false})
                            }
                        },
                        _ => {
                            serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"invalid","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"invalid_command","truncated":false})
                        }
                    }
                }
                Ok(Some((revision, _, _, _))) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"conflict","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"stale_revision","truncated":false})
                }
                Ok(None) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"unknown_draft","truncated":false})
                }
                Err(_) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false})
                }
            };
        }
        if request.operation == "issue_handoff" {
            let database = self.journal.database().lock().await;
            let draft = evohime_local_storage::visual_workflow_builder_store::read_draft(
                database.connection(),
                &request.draft_id,
                &request.owner_scope,
            );
            return match draft {
                Ok(Some((revision, _definition, execution_hash, _layout_hash))) => {
                    let handle = format!("builder-handoff:{}:{}", request.draft_id, revision);
                    let precondition = format!("{}:{}", revision, execution_hash);
                    let result =
                        evohime_local_storage::visual_workflow_builder_store::issue_handoff(
                            database.connection(),
                            evohime_local_storage::visual_workflow_builder_store::Handoff {
                                handle: &handle,
                                draft_id: &request.draft_id,
                                owner_scope: &request.owner_scope,
                                revision,
                                draft_hash: &execution_hash,
                                precondition: &precondition,
                                created_at_ms: crate::task_memory::now_millis() as i64,
                            },
                        );
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":if result.is_ok(){"handoff_issued"}else{"error"},"draft_id":request.draft_id,"revision":revision,"execution_hash":execution_hash,"layout_hash":"","handoff_handle":if result.is_ok(){handle}else{String::new()},"error_code":if result.is_ok(){""}else{"storage_error"},"truncated":false})
                }
                Ok(None) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"unknown_draft","truncated":false})
                }
                Err(_) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false})
                }
            };
        }
        if request.operation == "publish" {
            let handle = String::from_utf8(request.payload.to_vec()).unwrap_or_default();
            let database = self.journal.database().lock().await;
            let published =
                evohime_local_storage::visual_workflow_builder_store::publish_from_handoff(
                    database.connection(),
                    &handle,
                    &request.draft_id,
                    &request.owner_scope,
                    crate::task_memory::now_millis() as i64,
                );
            return match published {
                Ok(Ok((revision, _definition, execution_hash, layout_hash))) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"published","draft_id":request.draft_id,"revision":revision,"execution_hash":execution_hash,"layout_hash":layout_hash,"handoff_handle":handle,"error_code":"","truncated":false})
                }
                Ok(Err(code)) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"conflict","draft_id":request.draft_id,"revision":request.expected_revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":code,"truncated":false})
                }
                Err(_) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":request.expected_revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false})
                }
            };
        }
        if request.operation == "validate" || request.operation == "save" {
            match serde_json::from_slice::<
                crate::visual_workflow_builder::VisualWorkflowBuilderDefinition,
            >(&request.payload)
            {
                Ok(definition) => {
                    match self.validate_visual_workflow_definition(&definition) {
                        Ok(()) if request.operation == "validate" => {
                            return serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "valid", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": definition.execution_hash(), "layout_hash": definition.layout_hash(), "handoff_handle": "", "error_code": "", "truncated": false })
                        }
                        Ok(()) if request.operation == "save" => {
                            let database = self.journal.database().lock().await;
                            let graph_json = serde_json::to_vec(&definition).unwrap_or_default();
                            let layout_json =
                                serde_json::to_vec(&definition.layout).unwrap_or_default();
                            let result =
                            evohime_local_storage::visual_workflow_builder_store::save_draft(
                                database.connection(),
                                evohime_local_storage::visual_workflow_builder_store::SaveDraft { draft_id: &request.draft_id, owner_scope: &request.owner_scope, expected_revision: request.expected_revision, definition_json: &graph_json, layout_json: &layout_json, execution_hash: &definition.execution_hash(), layout_hash: &definition.layout_hash(), composer_provenance_json: None, updated_at_ms: crate::task_memory::now_millis() as i64 },
                            );
                            return match result {
                                Ok(Ok(revision)) => {
                                    serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "saved", "draft_id": request.draft_id, "revision": revision, "execution_hash": definition.execution_hash(), "layout_hash": definition.layout_hash(), "handoff_handle": "", "error_code": "", "truncated": false })
                                }
                                Ok(Err(code)) => {
                                    serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "conflict", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": "", "layout_hash": "", "handoff_handle": "", "error_code": code, "truncated": false })
                                }
                                Err(_) => {
                                    serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "error", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": "", "layout_hash": "", "handoff_handle": "", "error_code": "storage_error", "truncated": false })
                                }
                            };
                        }
                        Ok(()) => {
                            return serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "valid", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": definition.execution_hash(), "layout_hash": definition.layout_hash(), "handoff_handle": "", "error_code": "", "truncated": false })
                        }
                        Err(error) => {
                            return serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "invalid", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": "", "layout_hash": "", "handoff_handle": "", "error_code": error.to_string(), "truncated": false })
                        }
                    }
                }
                Err(_) => {
                    return serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "invalid", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": "", "layout_hash": "", "handoff_handle": "", "error_code": "invalid_payload", "truncated": false })
                }
            }
        }
        serde_json::json!({
            "schema_version": 1,
            "request_id": request.request_id,
            "status": "unavailable",
            "draft_id": request.draft_id,
            "revision": 0,
            "execution_hash": "",
            "layout_hash": "",
            "handoff_handle": "",
            "error_code": "builder_authoring_not_wired",
            "truncated": false,
        })
    }

    pub(crate) async fn dispatch_conversational_workflow_composer(
        &self,
        request: generated::ConversationalWorkflowComposerCommand,
    ) -> serde_json::Value {
        if request.idempotency_key.trim().is_empty() {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"invalid","draft_id":request.draft_id,"revision":request.expected_revision,"proposal_id":"","execution_hash":"","layout_hash":"","error_code":"missing_idempotency_key","projection_json":[],"truncated":false});
        }
        let command_hash =
            hex_encode(&[request.operation.as_bytes(), request.payload.as_ref()].concat());
        match self
            .journal
            .record_deduplicated(
                "workflow-composer",
                &request.idempotency_key,
                &command_hash,
                &[],
            )
            .await
        {
            Ok(Some(bytes)) => {
                if let Ok(value) = serde_json::from_slice(&bytes) {
                    return value;
                }
            }
            Err(_) => {
                return serde_json::json!({
                    "schema_version": 1,
                    "request_id": request.request_id,
                    "status": "conflict",
                    "draft_id": request.draft_id,
                    "revision": request.expected_revision,
                    "proposal_id": "",
                    "execution_hash": "",
                    "layout_hash": "",
                    "error_code": "idempotency_conflict",
                    "projection_json": [],
                    "truncated": false
                });
            }
            Ok(None) => {}
        }
        let result = self
            .dispatch_conversational_workflow_composer_inner(request.clone())
            .await;
        if let Ok(bytes) = serde_json::to_vec(&result) {
            let _ = self
                .journal
                .record_deduplicated(
                    "workflow-composer",
                    &request.idempotency_key,
                    &command_hash,
                    &bytes,
                )
                .await;
        }
        result
    }

    pub(crate) async fn dispatch_conversational_workflow_composer_inner(
        &self,
        request: generated::ConversationalWorkflowComposerCommand,
    ) -> serde_json::Value {
        use crate::conversational_workflow_composer as composer;
        let base = |status: &str, error: &str| {
            serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "status": status,
                "draft_id": request.draft_id,
                "revision": request.expected_revision,
                "proposal_id": "",
                "execution_hash": "",
                "layout_hash": "",
                "error_code": error,
                "projection_json": [],
                "truncated": false
            })
        };
        if request.schema_version != 0 && request.schema_version != 1 {
            return base("invalid", "unsupported_schema_version");
        }
        if request.owner_scope.trim().is_empty() || request.draft_id.trim().is_empty() {
            return base("invalid", "invalid_scope");
        }
        match request.operation.as_str() {
            "generate" => {
                let Ok(request_hash) = composer::request_hash(&request.payload) else {
                    return base("invalid", "request_too_large");
                };
                let Some(config) = self.gateway_config.clone() else {
                    return base("unavailable", "model_unavailable");
                };
                let Ok(gateway) = evohime_model_gateway::ModelGateway::from_config(&config) else {
                    return base("unavailable", "model_unavailable");
                };
                let prompt = String::from_utf8_lossy(&request.payload).into_owned();
                let messages = vec![
                    evohime_model_gateway::providers::ChatMessage::text(
                        evohime_model_gateway::providers::ChatRole::System,
                        "Return only JSON matching composer-proposal/v1 with schema_version, proposal_id, definition, assumptions. Never add tools, permissions, credentials or executable identities.",
                    ),
                    evohime_model_gateway::providers::ChatMessage::text(
                        evohime_model_gateway::providers::ChatRole::User,
                        prompt,
                    ),
                ];
                let routing = evohime_model_gateway::RoutingRequest {
                    required_capabilities: vec!["chat".into()],
                    max_cost_micros_per_1k_tokens: None,
                    max_latency_ms: Some(30_000),
                    required_privacy: evohime_model_gateway::PrivacyClass::Internal,
                    allow_fallback: true,
                    preferred_route: Some(config.default_route.clone()),
                    task_class: Some("workflow_composer".into()),
                    offline: false,
                    allow_cloud: true,
                    estimated_input_tokens: (request.payload.len() / 4) as u32,
                    quality_delta: 0.05,
                };
                let response = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    gateway.chat_with_tools_with_policy_and_route(
                        evohime_model_gateway::RoutingMode::Balanced,
                        &routing,
                        self.selected_model.get().as_deref(),
                        &messages,
                        &[],
                    ),
                )
                .await;
                let content = match response {
                    Ok(Ok(result)) => result.result.content,
                    Ok(Err(_)) => return base("unavailable", "model_unavailable"),
                    Err(_) => return base("unavailable", "model_timeout"),
                };
                let Ok(proposal) = composer::parse_proposal(content.as_bytes()) else {
                    return base("invalid", "malformed_proposal");
                };
                let projection = serde_json::json!({
                    "proposal_id": proposal.proposal_id,
                    "assumptions": proposal.assumptions,
                    "definition": proposal.definition,
                    "request_hash": request_hash,
                    "requires_review": true,
                    "risk": "review_required",
                });
                let mut result = base("proposal", "");
                result["proposal_id"] = serde_json::json!(proposal.proposal_id);
                result["execution_hash"] = serde_json::json!(proposal.definition.execution_hash());
                result["layout_hash"] = serde_json::json!(proposal.definition.layout_hash());
                result["projection_json"] =
                    serde_json::to_vec(&projection).unwrap_or_default().into();
                result
            }
            "validate" => {
                let Ok(proposal) = composer::parse_proposal(&request.payload) else {
                    return base("invalid", "malformed_proposal");
                };
                if self
                    .validate_visual_workflow_definition(&proposal.definition)
                    .is_err()
                {
                    return base("invalid", "binding_rejected");
                }
                let mut result = base("valid", "");
                result["proposal_id"] = serde_json::json!(proposal.proposal_id);
                result["execution_hash"] = serde_json::json!(proposal.definition.execution_hash());
                result["layout_hash"] = serde_json::json!(proposal.definition.layout_hash());
                result["projection_json"] = serde_json::to_vec(&serde_json::json!({"risk":"review_required","assumptions":proposal.assumptions})).unwrap_or_default().into();
                result
            }
            "save" => {
                let Ok(proposal) = composer::parse_proposal(&request.payload) else {
                    return base("invalid", "malformed_proposal");
                };
                if self
                    .validate_visual_workflow_definition(&proposal.definition)
                    .is_err()
                {
                    return base("invalid", "binding_rejected");
                }
                let definition_json = serde_json::to_vec(&proposal.definition).unwrap_or_default();
                let layout_json =
                    serde_json::to_vec(&proposal.definition.layout).unwrap_or_default();
                let execution_hash = proposal.definition.execution_hash();
                let layout_hash = proposal.definition.layout_hash();
                let database = self.journal.database().lock().await;
                let provenance_json = serde_json::to_vec(&composer::ComposerProvenance {
                    schema_version: composer::PROVENANCE_VERSION.into(),
                    request_hash: composer::request_hash(&request.payload).unwrap_or_default(),
                    proposal_hash: composer::canonical_proposal(&proposal)
                        .ok()
                        .map(|bytes| hex::encode(<sha2::Sha256 as sha2::Digest>::digest(bytes)))
                        .unwrap_or_default(),
                    catalog_hash: "core-workflow-registry-v1".into(),
                    model_route: "core-model-gateway".into(),
                    model_version: "bounded-v1".into(),
                })
                .ok();
                match evohime_local_storage::visual_workflow_builder_store::save_draft(
                    database.connection(),
                    evohime_local_storage::visual_workflow_builder_store::SaveDraft {
                        draft_id: &request.draft_id,
                        owner_scope: &request.owner_scope,
                        expected_revision: request.expected_revision,
                        definition_json: &definition_json,
                        layout_json: &layout_json,
                        execution_hash: &execution_hash,
                        layout_hash: &layout_hash,
                        composer_provenance_json: provenance_json.as_deref(),
                        updated_at_ms: crate::task_memory::now_millis() as i64,
                    },
                ) {
                    Ok(Ok(revision)) => {
                        let mut result = base("saved", "");
                        result["proposal_id"] = serde_json::json!(proposal.proposal_id);
                        result["revision"] = serde_json::json!(revision);
                        result["execution_hash"] = serde_json::json!(execution_hash);
                        result["layout_hash"] = serde_json::json!(layout_hash);
                        result
                    }
                    Ok(Err(code)) => base("conflict", code),
                    Err(_) => base("error", "storage_error"),
                }
            }
            "edit" => {
                let database = self.journal.database().lock().await;
                let Ok(Some((revision, definition_json, _, _))) =
                    evohime_local_storage::visual_workflow_builder_store::read_draft(
                        database.connection(),
                        &request.draft_id,
                        &request.owner_scope,
                    )
                else {
                    return base("error", "unknown_draft");
                };
                if revision != request.expected_revision {
                    return base("conflict", "stale_revision");
                }
                let Ok(mut definition) = serde_json::from_slice::<
                    crate::visual_workflow_builder::VisualWorkflowBuilderDefinition,
                >(&definition_json) else {
                    return base("error", "corrupt_draft");
                };
                let Ok(command) = serde_json::from_slice::<
                    crate::visual_workflow_builder::DraftCommand,
                >(&request.payload) else {
                    return base("invalid", "invalid_edit");
                };
                if composer::apply_edit(&mut definition, &command).is_err()
                    || self
                        .validate_visual_workflow_definition(&definition)
                        .is_err()
                {
                    return base("invalid", "binding_rejected");
                }
                let definition_json = serde_json::to_vec(&definition).unwrap_or_default();
                let layout_json = serde_json::to_vec(&definition.layout).unwrap_or_default();
                let execution_hash = definition.execution_hash();
                let layout_hash = definition.layout_hash();
                match evohime_local_storage::visual_workflow_builder_store::save_draft(
                    database.connection(),
                    evohime_local_storage::visual_workflow_builder_store::SaveDraft {
                        draft_id: &request.draft_id,
                        owner_scope: &request.owner_scope,
                        expected_revision: revision,
                        definition_json: &definition_json,
                        layout_json: &layout_json,
                        execution_hash: &execution_hash,
                        layout_hash: &layout_hash,
                        composer_provenance_json: None,
                        updated_at_ms: crate::task_memory::now_millis() as i64,
                    },
                ) {
                    Ok(Ok(next)) => {
                        let mut result = base("edited", "");
                        result["revision"] = serde_json::json!(next);
                        result["execution_hash"] = serde_json::json!(execution_hash);
                        result["layout_hash"] = serde_json::json!(layout_hash);
                        result
                    }
                    Ok(Err(code)) => base("conflict", code),
                    Err(_) => base("error", "storage_error"),
                }
            }
            "handoff" => {
                let database = self.journal.database().lock().await;
                let Ok(Some((revision, _, execution_hash, layout_hash))) =
                    evohime_local_storage::visual_workflow_builder_store::read_draft(
                        database.connection(),
                        &request.draft_id,
                        &request.owner_scope,
                    )
                else {
                    return base("error", "unknown_draft");
                };
                let handle = format!("composer-handoff:{}:{}", request.draft_id, revision);
                let precondition = format!("{}:{}", revision, execution_hash);
                let result = evohime_local_storage::visual_workflow_builder_store::issue_handoff(
                    database.connection(),
                    evohime_local_storage::visual_workflow_builder_store::Handoff {
                        handle: &handle,
                        draft_id: &request.draft_id,
                        owner_scope: &request.owner_scope,
                        revision,
                        draft_hash: &execution_hash,
                        precondition: &precondition,
                        created_at_ms: crate::task_memory::now_millis() as i64,
                    },
                );
                let mut value = base(
                    if result.is_ok() { "handoff" } else { "error" },
                    if result.is_ok() { "" } else { "storage_error" },
                );
                value["revision"] = serde_json::json!(revision);
                value["execution_hash"] = serde_json::json!(execution_hash);
                value["layout_hash"] = serde_json::json!(layout_hash);
                value["projection_json"] = serde_json::to_vec(
                    &serde_json::json!({"handoff_handle":handle,"save_precondition":precondition}),
                )
                .unwrap_or_default()
                .into();
                value
            }
            "discard" => base("discarded", ""),
            _ => base("unavailable", "composer_operation_unavailable"),
        }
    }

    pub(crate) fn validate_visual_workflow_definition(
        &self,
        definition: &crate::visual_workflow_builder::VisualWorkflowBuilderDefinition,
    ) -> Result<(), crate::visual_workflow_builder::BuilderError> {
        definition.validate()?;
        self.workflow_registry
            .validate_bindings(
                &definition.graph,
                &crate::workflow_registry::ParentCapabilities::default().unrestricted_context(),
            )
            .map_err(|_| crate::visual_workflow_builder::BuilderError::RegistryRejected)
    }
}

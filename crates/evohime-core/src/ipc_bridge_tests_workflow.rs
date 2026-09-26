use super::ambient::{ambient_bridge, ambient_call};
use super::*;

fn workflow_bridge(name: &str) -> (IpcBridge, tempfile::TempDir) {
    ambient_bridge(name)
}

#[tokio::test]
async fn prompt_strategy_output_contract_ipc_returns_metadata_only() {
    let (bridge, _directory) = workflow_bridge("prompt-strategy-output-contract");
    let contract = crate::structured_response_contract::ResponseContract::new(
        "strategy.result/v1",
        1,
        serde_json::json!({"type":"object","required":["ok"],"properties":{"ok":{"type":"boolean"}}}),
        crate::structured_response_contract::ResponseStrategy::Auto,
    ).expect("contract");
    let (event_type, response) = ambient_call(
        &bridge,
        generated::command_envelope::Command::BenchmarkMatrixAction(
            generated::AgentBenchmarkMatrixCommand {
                schema_version: 1,
                request_id: "strategy-output-contract".into(),
                owner_scope: "prompt_strategy".into(),
                operation: "strategyOutputContract".into(),
                payload: serde_json::to_vec(&serde_json::json!({"contract":contract}))
                    .expect("payload"),
                expected_version: 0,
                idempotency_key: "strategy-output-contract-key".into(),
            },
        ),
    ).await;
    assert_eq!(event_type, "benchmark_matrix.result");
    assert_eq!(response["status"], "ok");
    assert_eq!(response["result"]["contract_id"], "strategy.result/v1");
    assert_eq!(response["result"]["revision"], 1);
    assert_eq!(response["result"]["inserted"], true);
    assert!(!response.to_string().contains("properties"));
    assert!(!response.to_string().contains("boolean"));
}

#[tokio::test]
async fn prompt_strategy_compatibility_and_evidence_ipc_are_metadata_only() {
    let (bridge, _directory) = workflow_bridge("prompt-strategy-inspection");
    let profile = crate::prompt_strategy::declared_baseline("general", "agent")
        .expect("baseline profile");
    let capability_hash = format!("sha256:{}", "a".repeat(64));
    let context_hash = format!("sha256:{}", "b".repeat(64));
    let fresh_evidence_hashes = std::collections::HashSet::new();
    let resolved = crate::prompt_strategy::resolve_strategy(crate::prompt_strategy::ResolverInput {
        task_kind: "general",
        role: "agent",
        baseline: &profile,
        profiles: &[],
        pinned_profile: None,
        bindings: &[],
        route_id: "route",
        model_id: "model",
        capability_trust: crate::prompt_strategy::CapabilityTrust::Unknown,
        capability_epoch: None,
        supports_tool_calls: false,
        supports_structured_output: false,
        fresh_evidence_hashes: &fresh_evidence_hashes,
        available_tool_ids: &[],
        capability_snapshot_hash: &capability_hash,
        context_profile_hash: &context_hash,
        loadout_hash: None,
        loadout_ref: None,
        run_id: "strategy-run",
        call_id: "strategy-call",
    })
    .expect("baseline resolution");
    {
        let database = bridge.journal().database().lock().await;
        crate::prompt_strategy::register_profile(
            database.connection(),
            &profile,
            crate::task_memory::now_millis() as i64,
        )
        .expect("register baseline");
        crate::prompt_strategy::persist_selection(
            database.connection(),
            &resolved.snapshot,
            "strategy-provenance",
            crate::task_memory::now_millis() as i64,
        )
        .expect("persist selection");
    }

    let (event_type, compatibility) = ambient_call(
        &bridge,
        generated::command_envelope::Command::BenchmarkMatrixAction(
            generated::AgentBenchmarkMatrixCommand {
                schema_version: 1,
                request_id: "strategy-compatibility".into(),
                owner_scope: "prompt_strategy".into(),
                operation: "strategyCompatibility".into(),
                payload: serde_json::to_vec(&serde_json::json!({
                    "profile_id": profile.profile_id,
                    "revision": profile.revision,
                    "provenance_request_id": "strategy-provenance",
                }))
                .expect("payload"),
                expected_version: 0,
                idempotency_key: "strategy-compatibility-key".into(),
            },
        ),
    )
    .await;
    assert_eq!(event_type, "benchmark_matrix.result");
    assert_eq!(compatibility["status"], "ok");
    assert_eq!(compatibility["result"]["compatible"], true);
    assert!(!compatibility.to_string().contains("prompt text"));

    let (event_type, evidence) = ambient_call(
        &bridge,
        generated::command_envelope::Command::BenchmarkMatrixAction(
            generated::AgentBenchmarkMatrixCommand {
                schema_version: 1,
                request_id: "strategy-evidence".into(),
                owner_scope: "prompt_strategy".into(),
                operation: "strategyEvidence".into(),
                payload: serde_json::to_vec(&serde_json::json!({
                    "profile_id": profile.profile_id,
                    "revision": profile.revision,
                }))
                .expect("payload"),
                expected_version: 0,
                idempotency_key: "strategy-evidence-key".into(),
            },
        ),
    )
    .await;
    assert_eq!(event_type, "benchmark_matrix.result");
    assert_eq!(evidence["status"], "ok");
    assert_eq!(evidence["result"]["evidence"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn prompt_strategy_compare_requires_and_projects_exact_holdout_evidence() {
    let (bridge, _directory) = workflow_bridge("prompt-strategy-compare");
    let mut profile = crate::prompt_strategy::declared_baseline("general", "agent")
        .expect("baseline profile");
    profile.composition = crate::prompt_strategy::StrategyComposition::ToolUse {
        required_tool_ids: vec!["search".into()],
    };
    profile.content_hash = crate::prompt_strategy::profile_hash(&profile).expect("profile hash");
    let strategy_hash = crate::prompt_strategy::strategy_candidate_hash(&profile)
        .expect("candidate hash");
    let model_hash = format!("sha256:{}", "d".repeat(64));
    let agent_hash = format!("sha256:{}", "e".repeat(64));
    let suite_hash = format!("sha256:{}", "a".repeat(64));
    let policy_hash = format!("sha256:{}", "b".repeat(64));
    let report = crate::agent_benchmark_matrix::BenchmarkReport {
        contract_id: crate::agent_benchmark_matrix::CONTRACT_ID.into(),
        contract_hash: hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
            crate::agent_benchmark_matrix::CONTRACT_ID.as_bytes(),
        )),
        run_id: "strategy-compare-run".into(),
        source_commit: "test-commit".into(),
        suite_id: "suite".into(),
        suite_version: "1".into(),
        suite_hash: suite_hash.clone(),
        policy_hash: policy_hash.clone(),
        model_profile_ids: vec!["model-profile".into()],
        model_profile_hashes: vec![model_hash.clone()],
        agent_profile_ids: vec!["agent-profile".into()],
        agent_profile_hashes: vec![agent_hash.clone()],
        strategy_profile_hash_by_agent_id: [("agent-profile".into(), strategy_hash.clone())]
            .into_iter()
            .collect(),
        holdout_evaluation: true,
        metrics: [("challenge:model:agent-profile".into(), Default::default())]
            .into_iter()
            .collect(),
        comparisons: [(
            "challenge:model:agent-profile".into(),
            crate::agent_benchmark_matrix::BenchmarkComparison {
                verdict: crate::agent_benchmark_matrix::ComparisonVerdict::Regressed,
                security_hard_failure: false,
                reason: "metric_regression".into(),
            },
        )]
        .into_iter()
        .collect(),
        redaction_status: "redacted".into(),
    };
    let evidence = crate::prompt_strategy::StrategyEvidenceRef {
        evidence_id: report.run_id.clone(),
        agent_profile_id: "agent-profile".into(),
        agent_profile_hash: agent_hash,
        strategy_candidate_hash: strategy_hash,
        evidence_hash: report.canonical_hash().expect("report hash"),
        suite_hash,
        policy_hash,
        model_profile_hash: model_hash,
        holdout: true,
    };
    profile.evidence.push(evidence);
    profile.content_hash = crate::prompt_strategy::profile_hash(&profile).expect("profile hash");
    let report_json = serde_json::to_string(&report).expect("report json");
    {
        let database = bridge.journal().database().lock().await;
        let connection = database.connection();
        crate::prompt_strategy::register_profile(
            connection,
            &profile,
            crate::task_memory::now_millis() as i64,
        )
        .expect("register profile");
        assert!(evohime_local_storage::domains::evaluation::save_run(
            connection,
            &report.run_id,
            &report.suite_id,
            &report.suite_version,
            "{}",
            "ready_for_promotion",
            crate::task_memory::now_millis() as i64,
        )
        .expect("save run"));
        assert!(evohime_local_storage::domains::evaluation::save_report(
            connection,
            &report.run_id,
            &report_json,
            "ready_for_promotion",
            crate::task_memory::now_millis() as i64,
        )
        .expect("save report"));
    }
    let (event_type, result) = ambient_call(
        &bridge,
        generated::command_envelope::Command::BenchmarkMatrixAction(
            generated::AgentBenchmarkMatrixCommand {
                schema_version: 1,
                request_id: "strategy-compare".into(),
                owner_scope: "prompt_strategy".into(),
                operation: "strategyCompare".into(),
                payload: serde_json::to_vec(&serde_json::json!({
                    "profile_id": profile.profile_id,
                    "revision": profile.revision,
                    "run_id": report.run_id,
                }))
                .expect("payload"),
                expected_version: 0,
                idempotency_key: "strategy-compare-key".into(),
            },
        ),
    )
    .await;
    assert_eq!(event_type, "benchmark_matrix.result");
    assert_eq!(result["status"], "ok");
    assert_eq!(result["result"]["rows"].as_array().map(|rows| rows.len()), Some(1));
    assert_eq!(result["result"]["rows"][0]["comparison"]["verdict"], "regressed");
    assert_eq!(result["result"]["rows"][0]["metrics"]["attempts"], 0);
}

/// Каталог отдаёт версии, входы и пригодность к расписанию, но не граф
/// целиком: renderer не должен получать материал для собственного
/// планирования.
#[tokio::test]
async fn the_template_catalog_is_bounded_and_versioned() {
    let (bridge, _directory) = workflow_bridge("workflow-templates");
    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ListWorkflowTemplates(
            generated::ListWorkflowTemplates {},
        ),
    )
    .await;
    assert_eq!(event_type, "workflow.templates");
    let templates = payload["templates"].as_array().expect("список шаблонов");
    assert_eq!(templates.len(), 3);
    let ids: Vec<&str> = templates
        .iter()
        .map(|item| item["template_id"].as_str().unwrap_or_default())
        .collect();
    assert!(ids.contains(&"repository-research"));
    assert!(ids.contains(&"plan-implement-review"));
    assert!(ids.contains(&"parallel-security-review"));
    for template in templates {
        assert!(template["version"].as_u64().unwrap_or_default() >= 1);
        assert!(!template["schedule_eligibility"]
            .as_str()
            .unwrap_or_default()
            .is_empty());
        assert!(template.get("graph").is_none(), "граф целиком не уходит");
    }
    let approval_bearing = templates
        .iter()
        .find(|item| item["template_id"] == "plan-implement-review")
        .expect("шаблон с подтверждением");
    assert_eq!(approval_bearing["schedule_eligibility"], "unavailable");
}

#[tokio::test]
async fn capability_recipe_catalog_is_fixed_and_marks_missing_adapters_unsupported() {
    let (bridge, _directory) = workflow_bridge("capability-recipe-catalog");
    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ListCapabilityRecipes(
            generated::ListCapabilityRecipes {},
        ),
    )
    .await;
    assert_eq!(event_type, "capability_recipe.catalog");
    assert_eq!(payload["error_code"], "");
    assert_eq!(
        payload["catalog_version"],
        crate::capability_recipes::CATALOG_VERSION
    );
    let recipes = payload["recipes"].as_array().expect("recipes");
    assert_eq!(
        recipes.len(),
        crate::capability_recipes::BUILTIN_RECIPE_COUNT
    );
    let model_comparison = recipes
        .iter()
        .find(|recipe| recipe["id"] == "model-comparison")
        .expect("model comparison descriptor");
    assert_eq!(model_comparison["availability"]["status"], "unsupported");
    assert_eq!(
        model_comparison["availability"]["reason_code"],
        "model_run_adapter_unavailable"
    );
    assert!(model_comparison.get("graph").is_none());
}

#[tokio::test]
async fn recipe_preflight_binds_input_and_workspace_hashes_without_echoing_values() {
    let (bridge, directory) = workflow_bridge("capability-recipe-preflight");
    let recipe = crate::capability_recipes::descriptor("knowledge-grounding")
        .expect("catalog")
        .expect("recipe");
    let workspace_path = directory.path().to_string_lossy().to_string();
    let secret_question = "private recipe input that must not return";
    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::PreflightCapabilityRecipe(
            generated::PreflightCapabilityRecipe {
                recipe_id: recipe.id,
                recipe_version: recipe.version,
                recipe_hash: recipe.content_hash,
                inputs: vec![generated::WorkflowInput {
                    name: "question".into(),
                    value: secret_question.into(),
                }],
                workspace_path: workspace_path.clone(),
            },
        ),
    )
    .await;
    assert_eq!(event_type, "capability_recipe.preflight");
    assert_eq!(payload["state"], "ready_with_warnings");
    assert_eq!(payload["input_hash"].as_str().unwrap_or_default().len(), 64);
    assert_eq!(
        payload["workspace_hash"].as_str().unwrap_or_default().len(),
        64
    );
    assert!(!payload.to_string().contains(secret_question));
    assert!(!payload.to_string().contains(&workspace_path));
}

#[tokio::test]
async fn guided_recipe_start_is_idempotent_and_stores_only_safe_attribution() {
    let (bridge, directory) = workflow_bridge("capability-recipe-start");
    let recipe = crate::capability_recipes::descriptor("knowledge-grounding")
        .expect("catalog")
        .expect("recipe");
    let workspace_path = directory.path().to_string_lossy().to_string();
    let inputs = vec![generated::WorkflowInput {
        name: "question".into(),
        value: "question not copied into recipe sidecar".into(),
    }];
    let (_, preflight) = ambient_call(
        &bridge,
        generated::command_envelope::Command::PreflightCapabilityRecipe(
            generated::PreflightCapabilityRecipe {
                recipe_id: recipe.id.clone(),
                recipe_version: recipe.version,
                recipe_hash: recipe.content_hash.clone(),
                inputs: inputs.clone(),
                workspace_path: workspace_path.clone(),
            },
        ),
    )
    .await;
    let request = || {
        generated::command_envelope::Command::StartCapabilityRecipe(
            generated::StartCapabilityRecipe {
                recipe_id: recipe.id.clone(),
                recipe_version: recipe.version,
                recipe_hash: recipe.content_hash.clone(),
                workspace_path: workspace_path.clone(),
                inputs: inputs.clone(),
                idempotency_key: "guided-recipe-start-key".into(),
                preflight_hash: preflight["preflight_hash"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            },
        )
    };
    let (event_type, first) = ambient_call(&bridge, request()).await;
    assert_eq!(event_type, "capability_recipe.started");
    assert_eq!(first["error_code"], "");
    assert_eq!(first["deduplicated"], false);
    let run_id = first["run_id"].as_str().expect("run id").to_string();

    let (_, second) = ambient_call(&bridge, request()).await;
    assert_eq!(second["run_id"], run_id);
    assert_eq!(second["deduplicated"], true);
    let (run_event, run_projection) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetCapabilityRecipeRun(
            generated::GetCapabilityRecipeRun {
                run_id: run_id.clone(),
            },
        ),
    )
    .await;
    assert_eq!(run_event, "capability_recipe.run");
    assert_eq!(
        run_projection["replay_options"]["reproduce_exact"]["availability"],
        "unavailable"
    );
    assert_eq!(
        run_projection["replay_options"]["reproduce_exact"]["reason_code"],
        "external_revision_pins_not_persisted"
    );
    assert_eq!(
        run_projection["replay_options"]["rerun_current_compatible"]["availability"],
        "unavailable"
    );
    let link = bridge
        .journal()
        .capability_recipe_run_by_workflow(&run_id)
        .await
        .expect("link query")
        .expect("recipe link");
    assert_eq!(link.recipe_id, "knowledge-grounding");
    assert_eq!(
        link.input_hash,
        preflight["input_hash"].as_str().unwrap_or_default()
    );
    assert!(!serde_json::to_string(&link)
        .expect("safe attribution serializes")
        .contains("question not copied into recipe sidecar"));
    assert_eq!(
        bridge
            .journal()
            .list_workflow_runs(10)
            .await
            .expect("workflow rows")
            .len(),
        1
    );
}

#[tokio::test]
async fn core_startup_recovers_interrupted_workflow_runs_before_ipc() {
    let (bridge, directory) = workflow_bridge("workflow-startup-recovery");
    let workspace_path = directory.path().to_string_lossy().to_string();
    let template = crate::workflow_templates::template("repository-research")
        .expect("research workflow template");
    let inputs = std::collections::BTreeMap::from([(
        "question".to_string(),
        "inspect startup recovery".to_string(),
    )]);
    let graph = template.instantiate(&inputs).expect("template inputs");
    let run_id = "startup-recovery-run";
    let runtime = bridge.workflow_runtime(&workspace_path);
    runtime
        .start(crate::workflow_runtime::StartWorkflowRequest {
            run_id: run_id.into(),
            task_id: run_id.into(),
            workspace_path,
            template_id: template.template_id,
            template_version: template.version,
            inputs,
            graph: graph.clone(),
            parent: crate::ipc_bridge::workflow_parent_capabilities(),
        })
        .await
        .expect("durable run");
    let stored_run = bridge
        .journal()
        .workflow_run(run_id)
        .await
        .expect("load stored run")
        .expect("stored workflow");
    let stored_graph: crate::workflow::WorkflowGraph =
        serde_json::from_str(&stored_run.graph_json).expect("stored graph");
    let first_node = stored_graph.nodes.first().expect("workflow node");
    {
        let journal = bridge.journal();
        let database = journal.database().lock().await;
        evohime_local_storage::workflow_store::begin_attempt(
            database.connection(),
            &evohime_local_storage::workflow_store::WorkflowAttemptRecord {
                attempt_id: format!("{run_id}:{}:1", first_node.id),
                run_id: run_id.into(),
                node_id: first_node.id.clone(),
                attempt: 1,
                graph_hash: stored_run.graph_hash,
                input_hash: String::new(),
                dispatched_at_ms: 1,
                completed_at_ms: None,
                outcome: String::new(),
                error_code: String::new(),
            },
        )
        .expect("persist open attempt");
    }

    let recovery = bridge
        .recover_workflow_runs_on_startup()
        .await
        .expect("startup recovery");
    assert_eq!(recovery.interrupted_runs, vec![run_id.to_string()]);
    assert_eq!(recovery.unknown_attempts.len(), 1);
    assert_eq!(
        bridge
            .journal()
            .workflow_run(run_id)
            .await
            .expect("load recovered run")
            .expect("recovered workflow")
            .state,
        evohime_local_storage::workflow_store::RunState::Interrupted
    );
}

#[tokio::test]
async fn recipe_fork_uses_the_pinned_template_placeholders_and_clears_child_grants() {
    let directory = tempfile::tempdir().expect("temp dir");
    let workspace_path = directory.path().to_string_lossy().to_string();
    let journal = EventJournal::open(directory.path().join("recipe-fork.db")).expect("journal");
    let bridge = IpcBridge::new(journal);
    let recipe = crate::capability_recipes::descriptor("knowledge-grounding")
        .expect("catalog")
        .expect("recipe");
    let binding = recipe.workflow_binding.as_ref().expect("binding");
    let template = crate::workflow_templates::template(&binding.template_id).expect("template");
    let inputs = std::collections::BTreeMap::from([(
        "question".to_string(),
        "private run input must not be forked".to_string(),
    )]);
    let inputs_json = serde_json::to_string(&inputs).expect("inputs");
    let graph = template
        .instantiate(&inputs)
        .expect("instantiated run graph");
    let expanded = crate::workflow_registry::WorkflowRegistry::bootstrap()
        .expand_subgraphs(&graph)
        .expect("expanded graph");
    let graph_json = serde_json::to_string(&expanded).expect("graph");
    let run_id = "completed-recipe-run";
    let created_at_ms = 1;
    let run = evohime_local_storage::workflow_store::WorkflowRunRecord {
        run_id: run_id.into(),
        task_id: run_id.into(),
        template_id: template.template_id.clone(),
        template_version: template.version,
        graph_id: expanded.graph_id.clone(),
        graph_version: expanded.version,
        graph_hash: expanded.canonical_hash(),
        graph_json,
        inputs_json: inputs_json.clone(),
        policy_json: serde_json::json!({"workspace_path": &workspace_path}).to_string(),
        state: evohime_local_storage::workflow_store::RunState::Completed,
        created_at_ms,
        updated_at_ms: created_at_ms,
        terminal_reason: String::new(),
        cancel_requested: false,
        lease_owner: String::new(),
        lease_expires_at_ms: 0,
    };
    let nodes = expanded
        .nodes
        .iter()
        .map(
            |node| evohime_local_storage::workflow_store::WorkflowNodeRecord {
                run_id: run_id.into(),
                node_id: node.id.clone(),
                action_kind: node.node_type.action_kind().into(),
                state: evohime_local_storage::workflow_store::NodeState::Pending,
                attempts: 0,
                output_json: String::new(),
                error_code: String::new(),
                error_message: String::new(),
                approval_id: String::new(),
                updated_at_ms: created_at_ms,
            },
        )
        .collect::<Vec<_>>();
    let link = evohime_local_storage::capability_recipe_store::RecipeRunLink {
        run_id: run_id.into(),
        recipe_id: recipe.id.clone(),
        recipe_version: recipe.version,
        recipe_hash: recipe.content_hash.clone(),
        template_id: binding.template_id.clone(),
        template_version: binding.template_version,
        template_graph_hash: binding.template_graph_hash.clone(),
        run_graph_hash: expanded.canonical_hash(),
        input_hash: hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
            inputs_json.as_bytes(),
        )),
        workspace_hash: hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
            workspace_path.as_bytes(),
        )),
        idempotency_key: "recipe-fork-source-run".into(),
        created_at_ms,
    };
    bridge
        .journal()
        .insert_guided_workflow_run(&run, &nodes, &link)
        .await
        .expect("completed recipe run");
    let strategy_profile = crate::prompt_strategy::declared_baseline("general", "agent")
        .expect("baseline strategy");
    let capability_hash = format!("sha256:{}", "a".repeat(64));
    let context_hash = format!("sha256:{}", "b".repeat(64));
    let available_tool_ids = Vec::new();
    let resolved_strategy = crate::prompt_strategy::resolve_strategy(
        crate::prompt_strategy::ResolverInput {
            task_kind: "general",
            role: "agent",
            baseline: &strategy_profile,
            profiles: &[],
            pinned_profile: None,
            bindings: &[],
            route_id: "recipe-route",
            model_id: "recipe-model",
            capability_trust: crate::prompt_strategy::CapabilityTrust::Unknown,
            capability_epoch: None,
            supports_tool_calls: false,
            supports_structured_output: false,
            fresh_evidence_hashes: &std::collections::HashSet::new(),
            available_tool_ids: &available_tool_ids,
            capability_snapshot_hash: &capability_hash,
            context_profile_hash: &context_hash,
            loadout_hash: None,
            loadout_ref: None,
            run_id,
            call_id: "recipe-call",
        },
    )
    .expect("resolved baseline strategy");
    {
        let database = bridge.journal().database().lock().await;
        crate::prompt_strategy::register_profile(
            database.connection(),
            &strategy_profile,
            created_at_ms,
        )
        .expect("register baseline strategy");
        crate::prompt_strategy::persist_selection(
            database.connection(),
            &resolved_strategy.snapshot,
            "recipe-provenance-request",
            created_at_ms,
        )
        .expect("persist run strategy pin");
    }

    let (event_type, forked) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ForkCapabilityRecipeRun(
            generated::ForkCapabilityRecipeRun {
                run_id: run_id.into(),
                idempotency_key: "fork-attempt-1".into(),
            },
        ),
    )
    .await;
    assert_eq!(event_type, "capability_recipe.forked");
    assert_eq!(forked["status"], "draft_created");
    assert_eq!(forked["source_run_id"], run_id);
    assert_eq!(forked["error_code"], "");
    {
        let database = bridge.journal().database().lock().await;
        let provenance = evohime_local_storage::visual_workflow_builder_store::read_draft_provenance(
            database.connection(),
            forked["draft_id"].as_str().expect("draft id"),
            &workspace_path,
        )
        .expect("read fork provenance")
        .expect("fork provenance");
        let provenance: serde_json::Value = serde_json::from_slice(&provenance)
            .expect("provenance metadata");
        assert_eq!(
            provenance["prompt_strategy_pin"]["profile"]["profile_id"],
            strategy_profile.profile_id
        );
        assert_eq!(
            provenance["prompt_strategy_pin"]["profile"]["revision"],
            strategy_profile.revision
        );
        assert_eq!(
            provenance["prompt_strategy_pin"]["selection_snapshot_id"],
            resolved_strategy.snapshot.snapshot_id
        );
    }

    let (event_type, recovered) = ambient_call(
        &bridge,
        generated::command_envelope::Command::VisualWorkflowBuilder(
            generated::VisualWorkflowBuilderCommand {
                schema_version: 1,
                request_id: "recover-forked-draft".into(),
                owner_scope: workspace_path,
                draft_id: forked["draft_id"].as_str().unwrap_or_default().into(),
                operation: "recover".into(),
                payload: Vec::new(),
                expected_revision: 0,
                idempotency_key: "recover-forked-draft".into(),
            },
        ),
    )
    .await;
    assert_eq!(event_type, "workflow_builder.result");
    assert_eq!(recovered["status"], "recovered");
    let draft_json = recovered["draft_json"].as_str().expect("draft definition");
    assert!(!draft_json.contains("private run input must not be forked"));
    let definition: crate::visual_workflow_builder::VisualWorkflowBuilderDefinition =
        serde_json::from_str(draft_json).expect("builder definition");
    for node in &definition.graph.nodes {
        if let crate::workflow::NodeType::Child { child } = &node.node_type {
            assert!(child.grants.is_empty());
            assert!(child.context_allowlist.is_empty());
            assert!(child.artifact_allowlist.is_empty());
        }
    }
    let template_base = template.graph();
    for (base, forked_node) in template_base.nodes.iter().zip(&definition.graph.nodes) {
        assert_eq!(base.execution, forked_node.execution);
    }
}

/// Неизвестный шаблон получает typed-код, а не пустой успешный ответ.
#[tokio::test]
async fn an_unknown_template_definition_is_named_not_faked() {
    let (bridge, _directory) = workflow_bridge("workflow-definition");
    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetWorkflowDefinition(
            generated::GetWorkflowDefinition {
                template_id: "does-not-exist".into(),
            },
        ),
    )
    .await;
    assert_eq!(payload["error_code"], "unknown_template");
    assert!(payload["nodes"].as_array().expect("узлы").is_empty());

    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetWorkflowDefinition(
            generated::GetWorkflowDefinition {
                template_id: "parallel-security-review".into(),
            },
        ),
    )
    .await;
    assert_eq!(payload["error_code"], "");
    assert_eq!(payload["nodes"].as_array().expect("узлы").len(), 4);
    assert_eq!(payload["graph_hash"].as_str().unwrap_or_default().len(), 64);
}

/// Пропущенный обязательный вход не запускает граф.
#[tokio::test]
async fn a_template_input_contract_violation_never_starts_a_run() {
    let (bridge, directory) = workflow_bridge("workflow-start-invalid");
    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::StartWorkflow(generated::StartWorkflow {
            template_id: "repository-research".into(),
            task_id: "task-1".into(),
            workspace_path: directory.path().to_string_lossy().to_string(),
            inputs: vec![],
            idempotency_key: "key-1".into(),
        }),
    )
    .await;
    assert_eq!(event_type, "workflow.started");
    assert_eq!(payload["error_code"], "missing_input");
    assert_eq!(payload["run_id"], "");
    assert!(bridge
        .journal()
        .list_workflow_runs(10)
        .await
        .expect("список запусков")
        .is_empty());
}

/// Один и тот же ключ идемпотентности возвращает первый запуск.
#[tokio::test]
async fn the_same_idempotency_key_returns_the_first_run() {
    let (bridge, directory) = workflow_bridge("workflow-idempotency");
    let command = || {
        generated::command_envelope::Command::StartWorkflow(generated::StartWorkflow {
            template_id: "parallel-security-review".into(),
            task_id: "task-1".into(),
            workspace_path: directory.path().to_string_lossy().to_string(),
            inputs: vec![generated::WorkflowInput {
                name: "scope".into(),
                value: "crates/evohime-core".into(),
            }],
            idempotency_key: "key-1".into(),
        })
    };
    let (_, first) = ambient_call(&bridge, command()).await;
    assert_eq!(first["error_code"], "");
    let run_id = first["run_id"].as_str().expect("идентификатор").to_string();
    assert!(!run_id.is_empty());
    assert_eq!(first["deduplicated"], false);

    let (_, second) = ambient_call(&bridge, command()).await;
    assert_eq!(second["run_id"], run_id);
    assert_eq!(second["deduplicated"], true);
    assert_eq!(
        bridge
            .journal()
            .list_workflow_runs(10)
            .await
            .expect("список запусков")
            .len(),
        1
    );
}

/// Проекция запуска несёт состояния и роли, но не цель child, не prompt и
/// не сырой вывод.
#[tokio::test]
async fn a_run_projection_carries_no_prompt_goal_or_raw_output() {
    let (bridge, directory) = workflow_bridge("workflow-projection");
    let (_, started) = ambient_call(
        &bridge,
        generated::command_envelope::Command::StartWorkflow(generated::StartWorkflow {
            template_id: "repository-research".into(),
            task_id: "task-1".into(),
            workspace_path: directory.path().to_string_lossy().to_string(),
            inputs: vec![generated::WorkflowInput {
                name: "question".into(),
                value: "секретная формулировка вопроса".into(),
            }],
            idempotency_key: "key-1".into(),
        }),
    )
    .await;
    let run_id = started["run_id"]
        .as_str()
        .expect("идентификатор")
        .to_string();

    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetWorkflowRun(generated::GetWorkflowRun {
            run_id: run_id.clone(),
        }),
    )
    .await;
    assert_eq!(event_type, "workflow.run");
    assert_eq!(payload["error_code"], "");
    assert_eq!(payload["run_id"], run_id);
    let rendered = payload.to_string();
    assert!(
        !rendered.contains("секретная формулировка вопроса"),
        "цель узла не должна доходить до renderer: {rendered}"
    );
    let nodes = payload["nodes"].as_array().expect("узлы");
    assert_eq!(nodes.len(), 4);
    for node in nodes {
        assert!(node.get("node_id").is_some());
        assert!(node.get("state").is_some());
        assert!(node.get("output").is_none(), "сырой вывод наружу не уходит");
    }
}

/// Неизвестный запуск даёт `unknown_state`, а не выдуманный успех.
#[tokio::test]
async fn an_unknown_run_is_reported_as_unknown_state() {
    let (bridge, _directory) = workflow_bridge("workflow-unknown-run");
    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetWorkflowRun(generated::GetWorkflowRun {
            run_id: "missing".into(),
        }),
    )
    .await;
    assert_eq!(payload["error_code"], "unknown_run");
    assert_eq!(payload["state"], "unknown_state");

    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::CancelWorkflow(generated::CancelWorkflow {
            run_id: "missing".into(),
        }),
    )
    .await;
    assert_eq!(payload["cancelled"], false);
    assert_eq!(payload["error_code"], "not_cancellable");
}

/// События запуска durable, монотонны и доступны для replay с любой точки.
#[tokio::test]
async fn run_events_replay_from_any_sequence() {
    let (bridge, directory) = workflow_bridge("workflow-events");
    let (_, started) = ambient_call(
        &bridge,
        generated::command_envelope::Command::StartWorkflow(generated::StartWorkflow {
            template_id: "parallel-security-review".into(),
            task_id: "task-1".into(),
            workspace_path: directory.path().to_string_lossy().to_string(),
            inputs: vec![generated::WorkflowInput {
                name: "scope".into(),
                value: "crates".into(),
            }],
            idempotency_key: "key-1".into(),
        }),
    )
    .await;
    let run_id = started["run_id"]
        .as_str()
        .expect("идентификатор")
        .to_string();

    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ListWorkflowEvents(generated::ListWorkflowEvents {
            run_id: run_id.clone(),
            after_sequence: -1,
            limit: 100,
        }),
    )
    .await;
    assert_eq!(event_type, "workflow.events");
    let events = payload["events"].as_array().expect("события");
    assert!(!events.is_empty());
    assert_eq!(events[0]["event_type"], "workflow.run_started");
    let sequences: Vec<i64> = events
        .iter()
        .map(|event| event["sequence"].as_i64().unwrap_or_default())
        .collect();
    let mut sorted = sequences.clone();
    sorted.sort();
    assert_eq!(sequences, sorted);

    let (_, tail) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ListWorkflowEvents(generated::ListWorkflowEvents {
            run_id,
            after_sequence: 0,
            limit: 100,
        }),
    )
    .await;
    let tail_events = tail["events"].as_array().expect("хвост");
    assert!(tail_events
        .iter()
        .all(|event| event["sequence"].as_i64().unwrap_or_default() > 0));
}

#[tokio::test]
async fn analysis_kernel_ipc_is_bounded_idempotent_and_version_checked() {
    let directory = tempfile::tempdir().expect("temp dir");
    let journal =
        EventJournal::open(directory.path().join("kernel-ipc.db")).expect("journal opens");
    let bridge = IpcBridge::new(journal);
    let created = bridge
        .dispatch_create_analysis_kernel(generated::CreateAnalysisKernel {
            task_id: "task-kernel-ipc".into(),
            workspace_id: "workspace-kernel-ipc".into(),
            runtime_version: "trusted-local-1".into(),
            package_manifest_hash: "a".repeat(64),
            policy_hash: "b".repeat(64),
            ..Default::default()
        })
        .await;
    assert_eq!(created.status, "running");
    assert_eq!(created.revision, 1);

    let put = generated::ExecuteAnalysisKernel {
            kernel_id: created.kernel_id.clone(),
            request_id: "object-put-request".into(),
            operation: "object_put".into(),
            args: br#"{"logical_name":"rows","type_hint":"json","value":[1,2,3],"sensitivity":"internal"}"#.to_vec(),
            correlation_id: "object-put-correlation".into(),
            idempotency_key: "object-put-idem".into(),
            ..Default::default()
        };
    let result = bridge.dispatch_execute_analysis_kernel(put.clone()).await;
    assert_eq!(result.status, "ok", "error={}", result.error_class);
    assert!(result.inline_result.is_empty());
    let object = result.object_ref.expect("metadata object ref");
    assert_eq!(object.logical_name, "rows");
    assert!(object.artifact_locator.is_empty());
    let duplicate = bridge.dispatch_execute_analysis_kernel(put).await;
    assert_eq!(duplicate.error_class, "duplicate_request");

    let denied = bridge
        .dispatch_execute_analysis_kernel(generated::ExecuteAnalysisKernel {
            kernel_id: created.kernel_id.clone(),
            request_id: "artifact-read-request".into(),
            operation: "artifact_read".into(),
            args: br#"{"locator":"artifact://missing"}"#.to_vec(),
            correlation_id: "artifact-read-correlation".into(),
            idempotency_key: "artifact-read-idem".into(),
            ..Default::default()
        })
        .await;
    assert_eq!(denied.error_class, "forbidden_capability");

    let stale = bridge
        .dispatch_reset_analysis_kernel(generated::ResetAnalysisKernel {
            kernel_id: created.kernel_id.clone(),
            expected_revision: 0,
            idempotency_key: "reset-idem".into(),
        })
        .await;
    assert_eq!(stale.error_class, "stale_revision");
    let still_running = bridge
        .dispatch_get_analysis_kernel(generated::GetAnalysisKernel {
            kernel_id: created.kernel_id.clone(),
            ..Default::default()
        })
        .await;
    assert_eq!(still_running.status, "running");
    assert_eq!(still_running.object_count, 1);

    let reset = bridge
        .dispatch_reset_analysis_kernel(generated::ResetAnalysisKernel {
            kernel_id: created.kernel_id.clone(),
            expected_revision: 1,
            idempotency_key: "reset-idem".into(),
        })
        .await;
    assert_eq!(reset.status, "reset");
    let duplicate_reset = bridge
        .dispatch_reset_analysis_kernel(generated::ResetAnalysisKernel {
            kernel_id: created.kernel_id,
            expected_revision: 1,
            idempotency_key: "reset-idem".into(),
        })
        .await;
    assert_eq!(duplicate_reset.error_class, "duplicate_request");
}

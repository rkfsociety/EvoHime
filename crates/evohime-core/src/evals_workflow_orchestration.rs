    use super::*;

    use crate::workflow::{
        BatchPolicy, ChildActionProfile, ConcurrencyClass, ContextProviderProfile, FailurePolicy,
        JoinMode, McpActionProfile, NodeAcceptance, NodeBudget, NodeType, WorkflowBudget,
        WORKFLOW_CONTRACT_VERSION,
    };
    use crate::workflow_registry::{
        BindingError, ContextProviderEntry, ContextSourceKind, McpServerEntry, McpTransport,
        ParentCapabilities, WorkflowRegistry,
    };
    use crate::workflow_runtime::{
        check_acceptance, collect_inputs, is_retryable, ready_nodes, select_batch, terminal_state,
        NodeError, NodeSuccess,
    };
    use evohime_local_storage::workflow_store::{NodeState, RunState, WorkflowNodeRecord};
    use std::collections::{BTreeMap, BTreeSet};

    fn policy() -> ExecutionPolicy {
        ExecutionPolicy {
            retry: RetryPolicy {
                max_attempts: 1,
                backoff_ms: 0,
                retryable_errors: vec![],
            },
            timeout_ms: 60_000,
            cancellation: CancellationPolicy::Cooperative,
            approval: ApprovalPolicy {
                required: false,
                reason: None,
            },
        }
    }

    fn transform(id: &str) -> WorkflowNode {
        WorkflowNode::new(id, NodeType::Transform, policy())
    }

    fn graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>, entry: &str) -> WorkflowGraph {
        WorkflowGraph {
            contract: WORKFLOW_CONTRACT_VERSION.into(),
            graph_id: "eval-workflow".into(),
            version: 1,
            entry_node: entry.into(),
            nodes,
            edges,
            budget: WorkflowBudget::default(),
        }
    }

    fn record(node_id: &str, state: NodeState, output: &str) -> WorkflowNodeRecord {
        WorkflowNodeRecord {
            run_id: "run-1".into(),
            node_id: node_id.into(),
            action_kind: "transform".into(),
            state,
            attempts: 1,
            output_json: output.into(),
            error_code: String::new(),
            error_message: String::new(),
            approval_id: String::new(),
            updated_at_ms: 0,
        }
    }

    /// Снимок состояний запуска. Узлы, которые фикстура не назвала, остаются
    /// `pending`: durable-запуск заводит строку каждому узлу графа, и eval
    /// обязан воспроизводить именно это, а не полупустую таблицу.
    fn states(
        workflow: &WorkflowGraph,
        entries: Vec<WorkflowNodeRecord>,
    ) -> BTreeMap<String, WorkflowNodeRecord> {
        let mut snapshot: BTreeMap<String, WorkflowNodeRecord> = workflow
            .nodes
            .iter()
            .map(|node| (node.id.clone(), record(&node.id, NodeState::Pending, "")))
            .collect();
        for entry in entries {
            snapshot.insert(entry.node_id.clone(), entry);
        }
        snapshot
    }

    fn parent() -> ParentCapabilities {
        ParentCapabilities {
            grants: BTreeSet::from(["fs.read".to_string()]),
            budget: NodeBudget {
                max_tokens: 10_000,
                max_seconds: 300,
                max_tool_calls: 32,
            },
            context_allowlist: BTreeSet::new(),
        }
    }

    /// 1. Валидный последовательный workflow планируется в ожидаемом порядке.
    pub fn sequential_workflow_runs_in_declared_order() -> Result<(), String> {
        let workflow = graph(
            vec![
                transform("collect").with_output("out", PortType::Text),
                transform("summarize")
                    .with_input("in", PortType::Text, true)
                    .with_output("out", PortType::Text),
                transform("publish").with_input("in", PortType::Text, true),
            ],
            vec![
                WorkflowEdge::data("collect", "out", "summarize", "in"),
                WorkflowEdge::data("summarize", "out", "publish", "in"),
            ],
            "collect",
        );
        workflow
            .validate()
            .map_err(|errors| format!("expected a valid graph, got {errors:?}"))?;
        let plan = plan_workflow(&workflow, &BTreeMap::new())
            .map_err(|error| format!("expected a plan, got {error:?}"))?;
        let order: Vec<&str> = plan
            .steps
            .iter()
            .map(|step| step.node_id.as_str())
            .collect();
        require(
            order == vec!["collect", "summarize", "publish"],
            format!("unexpected execution order: {order:?}"),
        )
    }

    /// 2. Diamond даёт bounded fan-out и детерминированный fan-in: параллельно
    ///    уходит только объявленная безопасной группа, а объединяющий узел ждёт
    ///    обе ветви.
    pub fn diamond_fan_out_is_bounded_and_fan_in_is_deterministic() -> Result<(), String> {
        let mut left = transform("left")
            .with_input("in", PortType::Text, true)
            .with_output("out", PortType::Text);
        left.concurrency = ConcurrencyClass::Parallel;
        let mut right = transform("right")
            .with_input("in", PortType::Text, true)
            .with_output("out", PortType::Text);
        right.concurrency = ConcurrencyClass::Parallel;
        let workflow = graph(
            vec![
                transform("start").with_output("out", PortType::Text),
                left,
                right,
                transform("join")
                    .with_input("left", PortType::Text, true)
                    .with_input("right", PortType::Text, true),
            ],
            vec![
                WorkflowEdge::data("start", "out", "left", "in"),
                WorkflowEdge::data("start", "out", "right", "in"),
                WorkflowEdge::data("left", "out", "join", "left"),
                WorkflowEdge::data("right", "out", "join", "right"),
            ],
            "start",
        );
        let after_start = states(
            &workflow,
            vec![record("start", NodeState::Succeeded, "{\"out\":\"value\"}")],
        );
        let ready = ready_nodes(&workflow, &after_start);
        require(
            ready == vec!["left".to_string(), "right".to_string()],
            format!("expected both branches ready, got {ready:?}"),
        )?;
        let batch = select_batch(&workflow, &ready, 2);
        require(
            batch.len() == 2,
            format!("expected a bounded parallel batch of two, got {batch:?}"),
        )?;
        let batch_of_one = select_batch(&workflow, &ready, 1);
        require(
            batch_of_one.len() == 1,
            "run budget must cap the parallel batch",
        )?;

        // Fan-in ждёт обе ветви независимо от порядка их появления.
        let one_branch = states(
            &workflow,
            vec![
                record("start", NodeState::Succeeded, "{\"out\":\"value\"}"),
                record("left", NodeState::Succeeded, "{\"out\":\"left\"}"),
            ],
        );
        require(
            !ready_nodes(&workflow, &one_branch).contains(&"join".to_string()),
            "fan-in must not start on a single branch",
        )?;
        let both = states(
            &workflow,
            vec![
                record("start", NodeState::Succeeded, "{\"out\":\"value\"}"),
                record("left", NodeState::Succeeded, "{\"out\":\"left\"}"),
                record("right", NodeState::Succeeded, "{\"out\":\"right\"}"),
            ],
        );
        require(
            ready_nodes(&workflow, &both).contains(&"join".to_string()),
            "fan-in must start once both branches are validated",
        )
    }

    /// 3. `AND` ждёт все ветви, `OR` принимает объявленный маршрут, а
    ///    неизвестный маршрут отклоняется контрактом.
    pub fn and_waits_for_all_while_or_accepts_a_declared_route() -> Result<(), String> {
        let mut any = transform("any")
            .with_input("left", PortType::Text, true)
            .with_input("right", PortType::Text, true);
        any.join = JoinMode::Any;
        let workflow = graph(
            vec![
                transform("left").with_output("out", PortType::Text),
                transform("right").with_output("out", PortType::Text),
                any,
            ],
            vec![
                WorkflowEdge::data("left", "out", "any", "left"),
                WorkflowEdge::data("right", "out", "any", "right"),
            ],
            "left",
        );
        let one = states(
            &workflow,
            vec![
                record("left", NodeState::Succeeded, "{\"out\":\"left\"}"),
                record("right", NodeState::Pending, ""),
            ],
        );
        require(
            ready_nodes(&workflow, &one).contains(&"any".to_string()),
            "OR-узел обязан стартовать по первой готовой ветви",
        )?;

        let mut source = transform("left").with_output("out", PortType::Text);
        source.routes = vec!["approved".into()];
        let injected = graph(
            vec![
                source,
                transform("sink").with_input("in", PortType::Text, true),
            ],
            vec![WorkflowEdge::data("left", "out", "sink", "in").with_route("attacker-chosen")],
            "left",
        );
        let errors = injected
            .validate()
            .err()
            .ok_or("route injection must be rejected")?;
        require(
            errors
                .iter()
                .any(|error| matches!(error, ValidationError::UnknownRoute { .. })),
            format!("expected UnknownRoute, got {errors:?}"),
        )
    }

    /// 4. Схема результата и минимум evidence проверяются до fan-in.
    pub fn child_output_is_validated_before_it_becomes_an_input() -> Result<(), String> {
        let mut node = transform("child");
        node.acceptance = NodeAcceptance {
            output_schema: Some(r#"{"type":"object","required":["status"]}"#.into()),
            required_evidence: 2,
            allowed_statuses: vec!["accepted".into()],
            retryable_error_classes: vec![],
        };
        let violating = NodeSuccess::new(serde_json::json!({"unexpected": true})).with_evidence(2);
        require(
            check_acceptance(&node, &violating)
                .err()
                .map(|error| error.code)
                == Some("output_schema_violation".into()),
            "результат вне схемы обязан быть отклонён",
        )?;
        let thin = NodeSuccess::new(serde_json::json!({"status": "accepted"})).with_evidence(1);
        require(
            check_acceptance(&node, &thin).err().map(|error| error.code)
                == Some("insufficient_evidence".into()),
            "недостаток evidence обязан быть отклонён",
        )?;
        let wrong_status =
            NodeSuccess::new(serde_json::json!({"status": "accepted"})).with_evidence(2);
        let wrong_status = NodeSuccess {
            status: "invented".into(),
            ..wrong_status
        };
        require(
            check_acceptance(&node, &wrong_status)
                .err()
                .map(|error| error.code)
                == Some("status_not_allowed".into()),
            "необъявленный статус обязан быть отклонён",
        )?;
        let good = NodeSuccess::new(serde_json::json!({"status": "accepted"})).with_evidence(2);
        require(
            check_acceptance(&node, &good).is_ok(),
            "валидный результат обязан пройти приёмку",
        )
    }

    /// 5. Повтор не применяется к неповторяемым ошибкам и ограничен классами
    ///    ошибок, объявленными узлом.
    pub fn retry_never_repeats_a_non_retryable_error() -> Result<(), String> {
        let mut node = transform("node");
        node.acceptance = NodeAcceptance {
            retryable_error_classes: vec!["timeout".into()],
            ..Default::default()
        };
        require(
            !is_retryable(&node, &NodeError::permanent("invalid_input", "нельзя")),
            "неповторяемая ошибка не должна повторяться",
        )?;
        require(
            !is_retryable(&node, &NodeError::transient("execution_failed", "сбой")),
            "класс ошибки вне списка узла не должен повторяться",
        )?;
        require(
            is_retryable(&node, &NodeError::transient("timeout", "не успел")),
            "объявленный класс ошибки обязан повторяться",
        )
    }

    /// 6 и 7. Отмена, неизвестный исход после падения и исчерпанные повторы
    /// дают разные терминальные состояния, а не общий «успех».
    pub fn terminal_states_separate_cancel_failure_and_unknown_outcome() -> Result<(), String> {
        let workflow = graph(vec![transform("only")], vec![], "only");
        let cancelled = states(&workflow, vec![record("only", NodeState::Cancelled, "")]);
        require(
            terminal_state(&workflow, &cancelled).map(|(state, _)| state)
                == Some(RunState::Cancelled),
            "отменённый запуск обязан быть отменённым",
        )?;
        let unknown = states(
            &workflow,
            vec![record("only", NodeState::UnknownOutcome, "")],
        );
        require(
            terminal_state(&workflow, &unknown).map(|(state, _)| state) == Some(RunState::Failed),
            "неизвестный исход не может считаться успехом",
        )?;
        let dead = states(&workflow, vec![record("only", NodeState::DeadLetter, "")]);
        let (state, reason) =
            terminal_state(&workflow, &dead).ok_or("dead letter must be terminal")?;
        require(
            state == RunState::Failed && reason.starts_with("dead_letter:"),
            format!("dead letter must be named, got {state:?}/{reason}"),
        )?;
        let degraded = states(&workflow, vec![record("only", NodeState::Degraded, "{}")]);
        require(
            terminal_state(&workflow, &degraded).map(|(state, _)| state)
                == Some(RunState::Degraded),
            "вырожденный результат не равен завершённому",
        )?;
        let waiting = states(
            &workflow,
            vec![record("only", NodeState::WaitingApproval, "")],
        );
        require(
            terminal_state(&workflow, &waiting).is_none(),
            "ожидание подтверждения не терминально",
        )
    }

    /// 9. Идентичность MCP-сервера, host и имя инструмента не могут прийти из
    ///    вывода модели или из renderer.
    pub fn mcp_identity_cannot_be_substituted() -> Result<(), String> {
        let mut registry = WorkflowRegistry::empty();
        registry.set_mcp_host_allowlist(Some(BTreeSet::from(["allowed.test".to_string()])));
        registry.register_mcp_server(McpServerEntry {
            server_id: "mcp.docs".into(),
            display_name: "Документы".into(),
            transport: McpTransport::RemoteJsonRpc,
            endpoint: "https://blocked.test/rpc".into(),
            allowed_tools: vec!["search".into()],
        });
        registry.register_mcp_server(McpServerEntry {
            server_id: "mcp.stdio".into(),
            display_name: "Локальный".into(),
            transport: McpTransport::Stdio,
            endpoint: "https://allowed.test/rpc".into(),
            allowed_tools: vec!["search".into()],
        });

        let node = |server: &str, tool: &str| {
            WorkflowNode::new(
                "call",
                NodeType::McpTool {
                    mcp: McpActionProfile {
                        server_id: server.into(),
                        tool_name: tool.into(),
                        arguments: Default::default(),
                    },
                },
                policy(),
            )
            .with_output("result", PortType::Json)
        };

        let host_blocked = registry
            .validate_bindings(
                &graph(vec![node("mcp.docs", "search")], vec![], "call"),
                &parent(),
            )
            .err()
            .ok_or("host outside the allowlist must be rejected")?;
        require(
            host_blocked
                .iter()
                .any(|error| error.code() == "mcp_host_not_allowed"),
            format!("expected mcp_host_not_allowed, got {host_blocked:?}"),
        )?;

        let transport_blocked = registry
            .validate_bindings(
                &graph(vec![node("mcp.stdio", "search")], vec![], "call"),
                &parent(),
            )
            .err()
            .ok_or("unsupported transport must be rejected")?;
        require(
            transport_blocked
                .iter()
                .any(|error| error.code() == "transport_unavailable"),
            format!("expected transport_unavailable, got {transport_blocked:?}"),
        )?;

        let unknown = registry
            .validate_bindings(
                &graph(vec![node("mcp.attacker", "search")], vec![], "call"),
                &parent(),
            )
            .err()
            .ok_or("unknown server must be rejected")?;
        require(
            unknown
                .iter()
                .any(|error| error.code() == "unknown_mcp_server"),
            format!("expected unknown_mcp_server, got {unknown:?}"),
        )
    }

    /// 10. Свежесть, идентичность источника и bounded-объём контекстного
    ///     провайдера проверяются до включения данных в контекст модели.
    pub fn context_provider_freshness_and_scope_are_enforced() -> Result<(), String> {
        let mut registry = WorkflowRegistry::empty();
        registry.register_provider(ContextProviderEntry {
            provider_id: "workspace.knowledge".into(),
            display_name: "Знания".into(),
            source: ContextSourceKind::WorkspaceKnowledge,
            max_items: 4,
            max_age_ms: 60_000,
            evidence_schema: crate::workflow_registry::EVIDENCE_SCHEMA.into(),
        });
        let node = |items: u32, age: u64| {
            WorkflowNode::new(
                "context",
                NodeType::ContextProvider {
                    provider: ContextProviderProfile {
                        provider_id: "workspace.knowledge".into(),
                        query: "архитектура".into(),
                        max_items: items,
                        max_age_ms: age,
                        evidence_schema: None,
                    },
                },
                policy(),
            )
            .with_output("evidence", PortType::Json)
        };
        let exceeded = registry
            .validate_bindings(
                &graph(vec![node(32, 3_600_000)], vec![], "context"),
                &parent(),
            )
            .err()
            .ok_or("provider budget must be enforced")?;
        require(
            exceeded.len() == 2
                && exceeded
                    .iter()
                    .all(|error| error.code() == "provider_budget_exceeded"),
            format!("expected two provider budget errors, got {exceeded:?}"),
        )?;
        require(
            registry
                .validate_bindings(&graph(vec![node(4, 60_000)], vec![], "context"), &parent())
                .is_ok(),
            "провайдер в пределах реестра обязан быть принят",
        )?;
        let unknown_source = registry
            .validate_bindings(
                &graph(
                    vec![WorkflowNode::new(
                        "context",
                        NodeType::ContextProvider {
                            provider: ContextProviderProfile {
                                provider_id: "attacker.source".into(),
                                query: String::new(),
                                max_items: 1,
                                max_age_ms: 1_000,
                                evidence_schema: None,
                            },
                        },
                        policy(),
                    )],
                    vec![],
                    "context",
                ),
                &parent(),
            )
            .err()
            .ok_or("unknown provider must be rejected")?;
        require(
            unknown_source
                .iter()
                .any(|error| error.code() == "unknown_context_provider"),
            format!("expected unknown_context_provider, got {unknown_source:?}"),
        )
    }

    /// 11. Цикл, эскалация прав, неограниченный цикл и слишком большой batch
    ///     отклоняются до любого эффекта.
    pub fn escalation_and_unbounded_shapes_are_rejected_before_any_effect() -> Result<(), String> {
        let mut looping = transform("loop");
        looping.node_type = NodeType::Loop {
            max_iterations: crate::workflow::MAX_LOOP_ITERATIONS + 1,
        };
        looping.batch = Some(BatchPolicy {
            max_items: crate::workflow::MAX_BATCH_ITEMS + 1,
        });
        let errors = graph(vec![looping], vec![], "loop")
            .validate()
            .err()
            .ok_or("unbounded loop and batch must be rejected")?;
        require(
            errors
                .iter()
                .any(|error| matches!(error, ValidationError::InvalidLoopBound { .. })),
            format!("expected InvalidLoopBound, got {errors:?}"),
        )?;
        require(
            errors.iter().any(|error| {
                matches!(
                    error,
                    ValidationError::InvalidBound { field, .. } if *field == "batch.max_items"
                )
            }),
            format!("expected a batch bound error, got {errors:?}"),
        )?;

        let escalating = WorkflowNode::new(
            "child",
            NodeType::Child {
                child: ChildActionProfile {
                    role: "reviewer".into(),
                    goal: "проверь".into(),
                    output_schema: None,
                    context_allowlist: vec![],
                    artifact_allowlist: vec![],
                    grants: vec!["fs.write".into()],
                    budget: NodeBudget {
                        max_tokens: 1_000_000,
                        max_seconds: 10_000,
                        max_tool_calls: 10_000,
                    },
                    max_revisions: 1,
                },
            },
            policy(),
        );
        let binding_errors = WorkflowRegistry::empty()
            .validate_bindings(&graph(vec![escalating], vec![], "child"), &parent())
            .err()
            .ok_or("grant and budget escalation must be rejected")?;
        let codes: Vec<&str> = binding_errors.iter().map(BindingError::code).collect();
        require(
            codes.contains(&"grant_escalation") && codes.contains(&"budget_escalation"),
            format!("expected grant and budget escalation, got {codes:?}"),
        )
    }

    /// 13. Неподключённая ошибка блокирует downstream, а объявленная
    ///     failure-ветвь продолжает только разрешённый fallback.
    pub fn an_unconnected_failure_blocks_downstream_but_a_declared_branch_continues(
    ) -> Result<(), String> {
        let workflow = graph(
            vec![
                transform("source").with_output("out", PortType::Text),
                transform("sink").with_input("in", PortType::Text, true),
            ],
            vec![WorkflowEdge::data("source", "out", "sink", "in")],
            "source",
        );
        let failed = states(&workflow, vec![record("source", NodeState::Failed, "")]);
        require(
            !ready_nodes(&workflow, &failed).contains(&"sink".to_string()),
            "отказ без объявленной ветви обязан блокировать downstream",
        )?;
        let node = workflow.node("sink").ok_or("sink node")?;
        require(
            collect_inputs(&workflow, node, &failed)
                .err()
                .map(|error| error.code)
                == Some("missing_required_input".into()),
            "downstream не должен получать пустой вход вместо результата",
        )?;

        let mut branching = transform("source")
            .with_output("out", PortType::Text)
            .with_output("error", PortType::Json);
        branching.on_failure = FailurePolicy::Branch;
        let with_branch = graph(
            vec![
                branching,
                transform("sink").with_input("in", PortType::Text, true),
                transform("fallback").with_input("error", PortType::Json, true),
            ],
            vec![
                WorkflowEdge::data("source", "out", "sink", "in"),
                WorkflowEdge::failure("source", "error", "fallback", "error"),
            ],
            "source",
        );
        with_branch
            .validate()
            .map_err(|errors| format!("declared branch must validate, got {errors:?}"))?;
        let branch_failed = states(&with_branch, vec![record("source", NodeState::Failed, "")]);
        let ready = ready_nodes(&with_branch, &branch_failed);
        require(
            ready == vec!["fallback".to_string()],
            format!("only the declared fallback may continue, got {ready:?}"),
        )
    }

    /// 14. Запущенный snapshot не меняется вместе с библиотекой шаблонов, а
    ///     неподдержанное расписание называется явно.
    pub fn a_started_snapshot_ignores_library_changes() -> Result<(), String> {
        let template = crate::workflow_templates::template("repository-research")
            .ok_or("template must exist")?;
        let inputs = BTreeMap::from([("question".to_string(), "вопрос".to_string())]);
        let snapshot = template
            .instantiate(&inputs)
            .map_err(|error| format!("instantiation failed: {error:?}"))?;
        let hash = snapshot.canonical_hash();

        let mut newer = template.clone();
        newer.version = 2;
        let newer_snapshot = newer
            .instantiate(&inputs)
            .map_err(|error| format!("instantiation failed: {error:?}"))?;
        require(
            snapshot.canonical_hash() == hash,
            "already-started snapshot must not change",
        )?;
        require(
            newer_snapshot.canonical_hash() == hash,
            "изменение метаданных шаблона не должно менять граф запуска",
        )?;

        let approval_bearing = crate::workflow_templates::template("plan-implement-review")
            .ok_or("template must exist")?;
        require(
            approval_bearing.schedule_eligibility
                == crate::workflow_templates::ScheduleEligibility::Unavailable,
            "шаблон с подтверждением не может уходить в расписание молча",
        )
    }

    /// 12. Клиент, не знающий команд workflow, продолжает согласовывать ту же
    ///     major-версию: набор команд additive.
    pub fn workflow_commands_stay_additive_for_older_clients() -> Result<(), String> {
        let older = ProtocolVersion::new(1, 0);
        let newer = ProtocolVersion::new(1, 7);
        let negotiated = negotiate_protocol(older, newer, &[], &[])
            .map_err(|error| format!("negotiation failed: {error:?}"))?;
        require(
            negotiated.version.major == 1 && negotiated.version.minor == 0,
            format!(
                "expected the older minor to win, got {:?}",
                negotiated.version
            ),
        )
    }


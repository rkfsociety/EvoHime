    use super::*;
    use crate::workflow::{
        ApprovalPolicy, CancellationPolicy, ExecutionPolicy, FailurePolicy, NodeAcceptance,
        PortType, RetryPolicy, WorkflowBudget, WorkflowEdge, WORKFLOW_CONTRACT_VERSION,
    };
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    fn journal() -> (EventJournal, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("core.db");
        let journal = EventJournal::open(&path).expect("journal");
        (journal, directory)
    }

    fn policy(max_attempts: u32, approval: bool) -> ExecutionPolicy {
        ExecutionPolicy {
            retry: RetryPolicy {
                max_attempts,
                backoff_ms: if max_attempts > 1 { 1 } else { 0 },
                retryable_errors: vec![],
            },
            timeout_ms: 2_000,
            cancellation: CancellationPolicy::Cooperative,
            approval: ApprovalPolicy {
                required: approval,
                reason: if approval {
                    Some("тест".into())
                } else {
                    None
                },
            },
        }
    }

    fn transform(id: &str) -> WorkflowNode {
        WorkflowNode::new(id, NodeType::Transform, policy(1, false))
    }

    fn graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>, entry: &str) -> WorkflowGraph {
        WorkflowGraph {
            contract: WORKFLOW_CONTRACT_VERSION.into(),
            graph_id: "runtime.test".into(),
            version: 1,
            entry_node: entry.into(),
            nodes,
            edges,
            budget: WorkflowBudget::default(),
        }
    }

    #[derive(Default)]
    struct ScriptedAdapter {
        calls: Mutex<Vec<String>>,
        attempts: AtomicU32,
        fail_nodes: Vec<String>,
        fail_first_n: u32,
        degraded_nodes: Vec<String>,
    }

    impl NodeAdapter for ScriptedAdapter {
        fn execute<'a>(&'a self, invocation: NodeInvocation<'a>) -> NodeFuture<'a> {
            let node_id = invocation.node.id.clone();
            self.calls.lock().unwrap().push(node_id.clone());
            let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
            let fails = self.fail_nodes.contains(&node_id);
            let degraded = self.degraded_nodes.contains(&node_id);
            let fail_first_n = self.fail_first_n;
            Box::pin(async move {
                if fails && attempt <= fail_first_n.max(1) {
                    return Err(NodeError::transient("transient", "временный сбой"));
                }
                let mut success = NodeSuccess::new(json!({"out": node_id, "text": node_id}));
                if degraded {
                    success = success.degraded("degraded");
                }
                Ok(success)
            })
        }
    }

    struct ApprovedGate;
    impl WorkflowApprovalGate for ApprovedGate {
        fn decide<'a>(&'a self, _run_id: &'a str, _node: &'a WorkflowNode) -> ApprovalFuture<'a> {
            Box::pin(async { ApprovalOutcome::Approved })
        }
    }

    struct DeniedGate;
    impl WorkflowApprovalGate for DeniedGate {
        fn decide<'a>(&'a self, _run_id: &'a str, _node: &'a WorkflowNode) -> ApprovalFuture<'a> {
            Box::pin(async { ApprovalOutcome::Denied })
        }
    }

    fn runtime(
        journal: &EventJournal,
        adapter: Arc<dyn NodeAdapter>,
        approvals: Arc<dyn WorkflowApprovalGate>,
    ) -> WorkflowRuntime {
        WorkflowRuntime::new(
            journal.clone(),
            Arc::new(WorkflowRegistry::empty()),
            adapter,
            approvals,
            "core-test",
        )
    }

    fn request(run_id: &str, graph: WorkflowGraph) -> StartWorkflowRequest {
        StartWorkflowRequest {
            run_id: run_id.into(),
            task_id: "task-1".into(),
            workspace_path: String::new(),
            template_id: "test".into(),
            template_version: 1,
            inputs: BTreeMap::new(),
            graph,
            parent: ParentCapabilities::default(),
        }
    }

    #[tokio::test]
    async fn a_sequential_run_executes_in_dependency_order_and_completes() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let graph = graph(
            vec![
                transform("a").with_output("out", PortType::Text),
                transform("b")
                    .with_input("in", PortType::Text, true)
                    .with_output("out", PortType::Text),
                transform("c").with_input("in", PortType::Text, true),
            ],
            vec![
                WorkflowEdge::data("a", "out", "b", "in"),
                WorkflowEdge::data("b", "out", "c", "in"),
            ],
            "a",
        );
        runtime.start(request("run-1", graph)).await.expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Completed);
        assert_eq!(*adapter.calls.lock().unwrap(), vec!["a", "b", "c"]);

        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.state, "completed");
        assert!(projection
            .nodes
            .iter()
            .all(|node| node.state == "succeeded"));
    }

    #[tokio::test]
    async fn a_diamond_fans_out_and_fans_in_deterministically() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let mut left = transform("left")
            .with_input("in", PortType::Text, true)
            .with_output("out", PortType::Text);
        left.concurrency = ConcurrencyClass::Parallel;
        let mut right = transform("right")
            .with_input("in", PortType::Text, true)
            .with_output("out", PortType::Text);
        right.concurrency = ConcurrencyClass::Parallel;
        let graph = graph(
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
        runtime.start(request("run-1", graph)).await.expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Completed);
        let calls = adapter.calls.lock().unwrap().clone();
        assert_eq!(calls.first().map(String::as_str), Some("start"));
        assert_eq!(calls.last().map(String::as_str), Some("join"));
        assert!(calls.contains(&"left".to_string()) && calls.contains(&"right".to_string()));
    }

    #[tokio::test]
    async fn an_unconnected_failure_blocks_downstream_instead_of_succeeding() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter {
            fail_nodes: vec!["a".into()],
            fail_first_n: u32::MAX,
            ..Default::default()
        });
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let graph = graph(
            vec![
                transform("a").with_output("out", PortType::Text),
                transform("b").with_input("in", PortType::Text, true),
            ],
            vec![WorkflowEdge::data("a", "out", "b", "in")],
            "a",
        );
        runtime.start(request("run-1", graph)).await.expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Failed);
        assert_eq!(*adapter.calls.lock().unwrap(), vec!["a"]);
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        let downstream = projection
            .nodes
            .iter()
            .find(|node| node.node_id == "b")
            .expect("b");
        assert_eq!(downstream.state, "blocked");
    }

    #[tokio::test]
    async fn a_declared_failure_branch_continues_only_the_allowed_fallback() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter {
            fail_nodes: vec!["a".into()],
            fail_first_n: u32::MAX,
            ..Default::default()
        });
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let mut source = transform("a")
            .with_output("out", PortType::Text)
            .with_output("error", PortType::Json);
        source.on_failure = FailurePolicy::Branch;
        let graph = graph(
            vec![
                source,
                transform("happy").with_input("in", PortType::Text, true),
                transform("fallback").with_input("error", PortType::Json, true),
            ],
            vec![
                WorkflowEdge::data("a", "out", "happy", "in"),
                WorkflowEdge::failure("a", "error", "fallback", "error"),
            ],
            "a",
        );
        runtime.start(request("run-1", graph)).await.expect("start");
        runtime.drive("run-1").await.expect("drive");
        let calls = adapter.calls.lock().unwrap().clone();
        assert!(calls.contains(&"fallback".to_string()));
        assert!(!calls.contains(&"happy".to_string()));
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        let happy = projection
            .nodes
            .iter()
            .find(|node| node.node_id == "happy")
            .expect("happy");
        assert_eq!(happy.state, "blocked");
    }

    #[tokio::test]
    async fn retry_is_bounded_and_exhaustion_becomes_a_dead_letter() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter {
            fail_nodes: vec!["a".into()],
            fail_first_n: u32::MAX,
            ..Default::default()
        });
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let mut node = transform("a");
        node.execution = policy(3, false);
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Failed);
        assert_eq!(adapter.calls.lock().unwrap().len(), 3);
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.nodes[0].state, "dead_letter");
        assert!(projection.terminal_reason.starts_with("dead_letter:"));
    }

    #[tokio::test]
    async fn a_non_retryable_error_is_not_retried() {
        struct PermanentAdapter {
            calls: AtomicU32,
        }
        impl NodeAdapter for PermanentAdapter {
            fn execute<'a>(&'a self, _invocation: NodeInvocation<'a>) -> NodeFuture<'a> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async {
                    Err(NodeError::permanent("invalid_input", "нельзя повторять"))
                })
            }
        }
        let (journal, _dir) = journal();
        let adapter = Arc::new(PermanentAdapter {
            calls: AtomicU32::new(0),
        });
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let mut node = transform("a");
        node.execution = policy(5, false);
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");
        runtime.drive("run-1").await.expect("drive");
        assert_eq!(adapter.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn approval_is_requested_before_the_effect_and_denial_stops_the_node() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(DeniedGate));
        let mut node = transform("a");
        node.execution = policy(1, true);
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Failed);
        assert!(adapter.calls.lock().unwrap().is_empty());
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.nodes[0].state, "denied");
    }

    #[tokio::test]
    async fn a_pending_approval_parks_the_run_without_executing_the_node() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(AlwaysPendingApproval));
        let mut node = transform("a");
        node.execution = policy(1, true);
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::WaitingApproval);
        assert_eq!(outcome.waiting_approval, vec!["a".to_string()]);
        assert!(adapter.calls.lock().unwrap().is_empty());

        // Тот же approval-идентификатор виден в projection: renderer решает
        // его существующей командой ResolveApproval.
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.nodes[0].approval_id, "run-1:a");
        assert_eq!(projection.state, "waiting_approval");
    }

    #[tokio::test]
    async fn cancellation_stops_the_run_and_marks_open_nodes() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let graph = graph(
            vec![
                transform("a").with_output("out", PortType::Text),
                transform("b").with_input("in", PortType::Text, true),
            ],
            vec![WorkflowEdge::data("a", "out", "b", "in")],
            "a",
        );
        runtime.start(request("run-1", graph)).await.expect("start");
        journal
            .request_workflow_cancel("run-1", crate::task_memory::now_millis() as i64)
            .await
            .expect("cancel");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Cancelled);
        assert!(adapter.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn output_schema_violation_is_not_accepted_as_a_result() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter, Arc::new(ApprovedGate));
        let mut node = transform("a");
        node.acceptance = NodeAcceptance {
            output_schema: Some(r#"{"type":"object","required":["required_field"]}"#.to_string()),
            ..Default::default()
        };
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Failed);
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.nodes[0].error_code, "output_schema_violation");
    }

    #[tokio::test]
    async fn insufficient_evidence_is_rejected_before_fan_in() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter, Arc::new(ApprovedGate));
        let mut node = transform("a");
        node.acceptance = NodeAcceptance {
            required_evidence: 2,
            ..Default::default()
        };
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");
        runtime.drive("run-1").await.expect("drive");
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.nodes[0].error_code, "insufficient_evidence");
    }

    #[tokio::test]
    async fn a_degraded_source_finishes_the_run_as_degraded_not_completed() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter {
            degraded_nodes: vec!["a".into()],
            ..Default::default()
        });
        let runtime = runtime(&journal, adapter, Arc::new(ApprovedGate));
        runtime
            .start(request("run-1", graph(vec![transform("a")], vec![], "a")))
            .await
            .expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Degraded);
    }

    #[tokio::test]
    async fn a_restart_between_dispatch_and_result_never_retries_blindly() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        runtime
            .start(request(
                "run-1",
                graph(
                    vec![transform("a").with_output("out", PortType::Text)],
                    vec![],
                    "a",
                ),
            ))
            .await
            .expect("start");
        // Падение сразу после dispatch marker.
        {
            let database = journal.database.lock().await;
            store::begin_attempt(
                database.connection(),
                &WorkflowAttemptRecord {
                    attempt_id: "run-1:a:1".into(),
                    run_id: "run-1".into(),
                    node_id: "a".into(),
                    attempt: 1,
                    graph_hash: "hash".into(),
                    input_hash: String::new(),
                    dispatched_at_ms: 1,
                    completed_at_ms: None,
                    outcome: String::new(),
                    error_code: String::new(),
                },
            )
            .expect("dispatch");
        }
        let recovery = journal
            .recover_workflow_runs(crate::task_memory::now_millis() as i64)
            .await
            .expect("recovery");
        assert_eq!(recovery.interrupted_runs, vec!["run-1".to_string()]);

        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Failed);
        assert!(adapter.calls.lock().unwrap().is_empty());
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.nodes[0].state, "unknown_outcome");
    }

    #[tokio::test]
    async fn a_tampered_graph_snapshot_is_refused_before_any_effect() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        runtime
            .start(request("run-1", graph(vec![transform("a")], vec![], "a")))
            .await
            .expect("start");
        {
            let database = journal.database.lock().await;
            database
                .connection()
                .execute(
                    "UPDATE workflow_runs SET graph_hash = 'tampered' WHERE run_id = 'run-1'",
                    [],
                )
                .expect("tamper");
        }
        let error = runtime.drive("run-1").await.expect_err("hash mismatch");
        assert_eq!(error.code(), "graph_hash_mismatch");
        assert!(adapter.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn events_are_durable_monotonic_and_bounded() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter, Arc::new(ApprovedGate));
        runtime
            .start(request("run-1", graph(vec![transform("a")], vec![], "a")))
            .await
            .expect("start");
        runtime.drive("run-1").await.expect("drive");
        let events = journal
            .list_workflow_events("run-1", -1, 100)
            .await
            .expect("events");
        assert!(events.len() >= 3);
        let sequences: Vec<i64> = events.iter().map(|event| event.run_sequence).collect();
        let mut sorted = sequences.clone();
        sorted.sort();
        assert_eq!(sequences, sorted);
        assert_eq!(events[0].event_type, "workflow.run_started");
        assert!(events
            .iter()
            .any(|event| event.event_type == "workflow.run_completed"));
        assert!(events
            .iter()
            .all(|event| event.payload_json.len() <= store::MAX_EVENT_PAYLOAD_BYTES));
    }

    #[tokio::test]
    async fn the_same_approval_id_resolves_the_parked_node_and_the_run_continues() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let approvals = Arc::new(WorkflowApprovalRegistry::new());
        let runtime = WorkflowRuntime::new(
            journal.clone(),
            Arc::new(WorkflowRegistry::empty()),
            adapter.clone(),
            approvals.clone(),
            "core-test",
        );
        let mut node = transform("a");
        node.execution = policy(1, true);
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");

        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::WaitingApproval);
        assert!(adapter.calls.lock().unwrap().is_empty());

        let approval_id = approvals.approval_id("run-1", "a");
        assert_eq!(approvals.run_for(&approval_id).as_deref(), Some("run-1"));
        assert!(approvals.resolve(&approval_id, true));
        assert!(!approvals.resolve("unknown-approval", true));

        let outcome = runtime.drive("run-1").await.expect("drive after approval");
        assert_eq!(outcome.state, RunState::Completed);
        assert_eq!(*adapter.calls.lock().unwrap(), vec!["a"]);
    }

    #[test]
    fn a_node_without_its_required_input_is_never_dispatched() {
        // Соединение есть, но зависимость ещё не дала значения: узел обязан
        // получить typed-ошибку до эффекта, а не пустой вход.
        let graph = graph(
            vec![
                transform("a").with_output("out", PortType::Text),
                transform("b").with_input("in", PortType::Text, true),
            ],
            vec![WorkflowEdge::data("a", "out", "b", "in")],
            "a",
        );
        let states = BTreeMap::from([(
            "a".to_string(),
            WorkflowNodeRecord {
                run_id: "run-1".into(),
                node_id: "a".into(),
                action_kind: "transform".into(),
                state: NodeState::Failed,
                attempts: 1,
                output_json: String::new(),
                error_code: "boom".into(),
                error_message: String::new(),
                approval_id: String::new(),
                updated_at_ms: 0,
            },
        )]);
        let node = graph.node("b").expect("b");
        let error = collect_inputs(&graph, node, &states).expect_err("missing input");
        assert_eq!(error.code, "missing_required_input");
    }

    #[test]
    fn a_batch_input_above_its_bound_is_rejected_instead_of_multiplying_executions() {
        let mut node = transform("b").with_input("items", PortType::Json, true);
        node.batch = Some(crate::workflow::BatchPolicy { max_items: 2 });
        let graph = graph(
            vec![
                transform("a").with_output("out", PortType::Json),
                node.clone(),
            ],
            vec![WorkflowEdge::data("a", "out", "b", "items")],
            "a",
        );
        let states = BTreeMap::from([(
            "a".to_string(),
            WorkflowNodeRecord {
                run_id: "run-1".into(),
                node_id: "a".into(),
                action_kind: "transform".into(),
                state: NodeState::Succeeded,
                attempts: 1,
                output_json: json!({"out": [1, 2, 3]}).to_string(),
                error_code: String::new(),
                error_message: String::new(),
                approval_id: String::new(),
                updated_at_ms: 0,
            },
        )]);
        let error = collect_inputs(&graph, &node, &states).expect_err("batch bound");
        assert_eq!(error.code, "batch_overflow");
    }

    #[tokio::test]
    async fn runtime_exposes_bounded_dispatch_metrics() {
        let (journal, _dir) = journal();
        let runtime = runtime(
            &journal,
            Arc::new(ScriptedAdapter::default()),
            Arc::new(ApprovedGate),
        );
        runtime
            .start(request(
                "metrics-run",
                graph(vec![transform("a")], vec![], "a"),
            ))
            .await
            .expect("start");
        runtime.drive("metrics-run").await.expect("drive");

        let metrics = runtime.metrics();
        assert_eq!(metrics.admissions_total, 1);
        assert_eq!(metrics.graph_hash_checks_total, 1);
        assert_eq!(metrics.dispatch_batches_total, 1);
        assert_eq!(metrics.dispatched_nodes_total, 1);
        assert_eq!(metrics.max_ready_nodes, 1);
    }

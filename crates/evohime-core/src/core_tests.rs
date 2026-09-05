mod tests {
    use super::{
        observability, recovery, visible_agent_text, AgentRunError, CoreCommand, CoreEvent,
        CoreVersion, EventJournal, ModelAgent, TaskCoordinator, TaskExecutor, ToolAgent,
        DEFAULT_TASK_TIMEOUT_SECONDS,
    };
    use evohime_model_gateway::{
        providers::mock::MockProvider, ChatResult, ModelGateway, NativeToolCall,
    };
    use evohime_tool_runtime::ToolRegistry;
    use futures_util::future::BoxFuture;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn codex_jsonl_agent_message_becomes_chat_event() {
        let line = r#"{"type":"item.completed","item":{"type":"agent_message","text":"Готово"}}"#;
        let (events, mut received) = tokio::sync::broadcast::channel(2);
        super::emit_codex_event(line, &events, "task-1");
        assert!(matches!(
            received.try_recv().unwrap(),
            CoreEvent::AssistantDelta { task_id, content }
                if task_id == "task-1" && content == "Готово"
        ));
    }

    #[test]
    fn selected_model_overrides_empty_gateway_model_for_provenance() {
        assert_eq!(
            super::effective_model_name("", Some("  openai/gpt-4.1-mini  ")),
            "openai/gpt-4.1-mini"
        );
        assert_eq!(
            super::effective_model_name("gateway-default", None),
            "gateway-default"
        );
        assert_eq!(
            super::effective_model_name("gateway-default", Some("  ")),
            "gateway-default"
        );
    }

    /// Связь «кандидат ↔ эпизод» существует в данных, а не на бумаге:
    /// `provenance_source_id` берёт эпизод первым, и именно по этому
    /// значению `ambient_store` отклоняет кандидатов удалённого эпизода
    /// причиной `source_deleted`.
    #[test]
    fn episode_wins_the_provenance_source_id() {
        use crate::memory_extraction::RawEvidenceLocator;

        let ambient = RawEvidenceLocator {
            episode_id: "episode-7".to_owned(),
            ..RawEvidenceLocator::default()
        };
        assert_eq!(
            super::memory_provenance_source_id(&ambient).as_deref(),
            Some("episode-7")
        );
        // Даже если извлекатель заодно назвал сообщение, эпизод старше: без
        // этого удаление эпизода не нашло бы своих кандидатов.
        let mixed = RawEvidenceLocator {
            episode_id: "episode-7".to_owned(),
            message_id: "msg-1".to_owned(),
            ..RawEvidenceLocator::default()
        };
        assert_eq!(
            super::memory_provenance_source_id(&mixed).as_deref(),
            Some("episode-7")
        );
        // Диалоговый путь не изменился.
        let dialog = RawEvidenceLocator {
            message_id: "msg-1".to_owned(),
            ..RawEvidenceLocator::default()
        };
        assert_eq!(
            super::memory_provenance_source_id(&dialog).as_deref(),
            Some("msg-1")
        );
    }

    /// Неизвестное значение переменной не включает извлечение из речи.
    #[test]
    fn ambient_memory_mode_reads_the_environment_fail_safe() {
        use crate::memory_extraction::AmbientMemoryMode;

        assert_eq!(
            AmbientMemoryMode::parse(std::env::var("EVOHIME_AMBIENT_MEMORY").ok().as_deref()),
            super::ambient_memory_mode()
        );
        assert_eq!(
            AmbientMemoryMode::parse(Some("pending")),
            AmbientMemoryMode::Pending
        );
        assert_eq!(AmbientMemoryMode::parse(Some("on")), AmbientMemoryMode::Off);
    }

    /// The chat shows what the model said before it called a tool, so the
    /// printed call itself must not travel with it.
    #[test]
    fn strips_printed_tool_calls_from_the_visible_reply() {
        let content = concat!(
            "Прочитаю документ.\n",
            "<function_calls>\n",
            "<invoke name=\"filesystem.read\">\n",
            "<parameter name=\"path\">README.md</parameter>\n",
            "</invoke>\n",
            "</function_calls>\n",
            "Жду результата..."
        );

        assert_eq!(visible_agent_text(content), "Прочитаю документ.");
    }

    #[test]
    fn keeps_a_reply_that_carries_no_tool_call() {
        assert_eq!(visible_agent_text("  Готово.  "), "Готово.");
        assert_eq!(visible_agent_text("<function_calls>\n<invoke/>"), "");
    }

    /// A task runs several model calls in a loop, so its budget must outlast a
    /// single request; the old default cut working agents off at 60 seconds.
    #[test]
    fn task_budget_outlasts_one_model_request() {
        let per_request = crate::provider_resilience::ProviderResilienceConfig::default();
        assert!(DEFAULT_TASK_TIMEOUT_SECONDS > per_request.model_timeout_secs);
    }

    struct NeverExecutor;

    #[tokio::test]
    async fn approval_coordinator_resolves_pending_request_once() {
        let coordinator = super::ApprovalCoordinator::default();
        let approval_id = uuid::Uuid::new_v4();
        let receiver = coordinator.register(approval_id).await;

        assert!(coordinator.resolve(approval_id, true).await);
        assert!(!coordinator.resolve(approval_id, false).await);
        assert!(receiver.await.expect("approval response"));
    }

    #[tokio::test]
    async fn routing_approval_waits_for_explicit_decision_and_times_out() {
        let registry = super::RoutingApprovalRegistry::default();
        let (events, mut receiver) = tokio::sync::broadcast::channel(4);
        let cancellation = CancellationToken::new();
        let waiting = {
            let registry = registry.clone();
            let cancellation = cancellation.clone();
            let events = events.clone();
            tokio::spawn(async move {
                registry
                    .wait_for_decision(super::RoutingApprovalWait {
                        task_id: "task",
                        run_id: "run",
                        trace_id: "trace",
                        route_id: "cloud",
                        timeout_ms: 1_000,
                        events: &events,
                        cancellation: &cancellation,
                    })
                    .await
            })
        };
        assert!(
            matches!(receiver.recv().await, Ok(CoreEvent::PendingRoutingApproval { route_id, .. }) if route_id == "cloud")
        );
        assert!(registry.resolve("trace", true).await.is_ok());
        assert!(waiting.await.unwrap().unwrap());

        let timeout_result = registry
            .wait_for_decision(super::RoutingApprovalWait {
                task_id: "task",
                run_id: "run",
                trace_id: "trace-timeout",
                route_id: "cloud",
                timeout_ms: 1,
                events: &events,
                cancellation: &cancellation,
            })
            .await
            .unwrap();
        assert!(!timeout_result);
        assert!(registry.resolve("trace-timeout", true).await.is_err());
    }

    #[test]
    fn agent_identity_includes_short_name() {
        assert!(super::AGENT_IDENTITY_PROMPT.contains("Ева"));
        assert!(super::AGENT_IDENTITY_PROMPT.contains("EvoHime"));
    }

    #[test]
    fn capability_archive_hash_mismatch_is_rejected_before_install() {
        let error = super::verify_capability_archive_hash(b"trusted archive", &"0".repeat(64))
            .expect_err("tampered archive must be rejected");
        assert!(error.contains("SHA-256 mismatch"));
    }

    #[test]
    fn agent_system_prompt_explains_workspace_research_flow() {
        let prompt =
            super::build_agent_system_prompt(&["filesystem.list".into(), "filesystem.read".into()]);
        assert!(!prompt.contains("C:\\Projects\\demo"));
        assert!(!prompt.contains("C:\\Users\\"));
        assert!(prompt.contains("filesystem.list"));
        assert!(prompt.contains("не сформулировал конкретное поручение"));
        assert!(prompt.contains("Не проси пользователя прислать структуру"));
        assert!(prompt.contains("до успешного результата"));
    }

    #[test]
    fn catalog_preflight_uses_safe_workspace_arguments_for_filesystem_tools() {
        assert_eq!(
            super::catalog_preflight_input("filesystem.list"),
            serde_json::json!({ "path": "." })
        );
        assert_eq!(
            super::catalog_preflight_input("filesystem.read"),
            serde_json::json!({ "path": "." })
        );
        assert_eq!(
            super::catalog_preflight_input("filesystem.search"),
            serde_json::json!({ "query": "EvoHime", "path": "." })
        );
        assert!(super::requires_workspace_research_catalog(
            "Изучи проект и расскажи, как он устроен"
        ));
        assert!(super::model_is_waiting_instead_of_reporting(
            "Жду результата от filesystem.list"
        ));
        assert!(!super::model_is_waiting_instead_of_reporting(
            "Проект состоит из Android-приложения и Gradle-модуля."
        ));
    }

    #[test]
    fn git_tool_contract_exposes_safe_repository_workflow() {
        let prompt = super::build_agent_system_prompt(&[
            "git.status".into(),
            "git.diff".into(),
            "git.commit".into(),
            "git.pull".into(),
            "git.push".into(),
        ]);
        assert!(prompt.contains("git.pull"));
        assert!(prompt.contains("git.push"));
        assert!(prompt.contains("только если пользователь явно попросил"));

        let pull = evohime_tool_runtime::builtin_input_schema("git.pull");
        assert_eq!(pull["properties"]["remote"]["type"], "string");
        assert_eq!(pull["additionalProperties"], false);
        let push = evohime_tool_runtime::builtin_input_schema("git.push");
        assert_eq!(push["properties"]["force"]["type"], "boolean");
        assert_eq!(push["additionalProperties"], false);
    }

    #[test]
    fn parses_legacy_git_mutation_calls() {
        let content = r#"
<function_calls>
[{"tool_name":"git.pull","arguments":{"remote":"origin","branch":"main"}},
 {"tool_name":"git.push","arguments":{"remote":"origin","branch":"main","force":false}}]
</function_calls>
        "#;
        let calls = super::parse_legacy_function_calls(content, 5);
        assert_eq!(
            calls
                .iter()
                .map(|call| call.name.as_str())
                .collect::<Vec<_>>(),
            ["git.pull", "git.push"]
        );
        assert!(calls[0].arguments.contains("origin"));
        assert!(calls[1].arguments.contains("force"));
    }

    #[test]
    fn parses_plain_git_calls_without_read_only_arguments() {
        let status = super::parse_plain_tool_call(
            "Выполняю последовательно.\n\ngit.status\n\nЖду результата.",
            8,
        )
        .expect("plain git status call");
        assert_eq!(status.name, "git.status");
        assert_eq!(status.arguments, "{}");

        let pull =
            super::parse_plain_tool_call("Выполняю обновление.\n\ngit.pull\n\nЖду результата.", 9)
                .expect("plain git pull call");
        assert_eq!(pull.name, "git.pull");
        assert_eq!(pull.arguments, "{}");
    }

    #[test]
    fn legacy_tool_allowlist_covers_the_runtime_registry() {
        let registry = ToolRegistry::bootstrap();
        let names = registry
            .list()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        assert!(super::LEGACY_TOOL_NAMES
            .iter()
            .all(|name| names.contains(name)));
    }

    #[test]
    fn parses_plain_no_argument_browser_tool_calls() {
        let call = super::parse_plain_tool_call(
            "Открываю текущую вкладку.\n\nbrowser.session.read\n\nЖду результата.",
            10,
        )
        .expect("plain browser read call");
        assert_eq!(call.name, "browser.session.read");
        assert_eq!(call.arguments, "{}");

        let xml =
            super::parse_xml_named_tool_call("<browser.session.close></browser.session.close>", 11)
                .expect("xml browser close call");
        assert_eq!(xml.name, "browser.session.close");
        assert_eq!(xml.arguments, "{}");
    }

    #[test]
    fn parses_legacy_text_function_calls() {
        let content = r#"
<function_calls>
<invoke name="filesystem.list">
<parameter name="path">.</parameter>
</invoke>
<invoke name="shell.execute">
<parameter name="command">dir /B</parameter>
</invoke>
        </function_calls>
"#;
        let calls = super::parse_legacy_function_calls(content, 2);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "filesystem.list");
        assert_eq!(calls[0].arguments, r#"{"path":"."}"#);
        assert_eq!(calls[1].name, "shell.execute");
        assert_eq!(calls[1].arguments, r#"{"command":"dir /B"}"#);
    }

    #[test]
    fn parses_json_function_call_blocks_for_mutating_tools() {
        let content = r#"
<function_calls>
[{"tool_name":"filesystem.patch","arguments":{"path":"tests/a.rs","patch":"--- a/tests/a.rs\n+++ b/tests/a.rs\n@@"}}]
</function_calls>
"#;
        let calls = super::parse_legacy_function_calls(content, 4);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "filesystem.patch");
        assert!(calls[0].arguments.contains("tests/a.rs"));
    }

    #[test]
    fn parses_explicit_natural_filesystem_intent() {
        let call = super::parse_natural_tool_intent(
            "Продолжу изучение. Вызываю filesystem.list для папки `crates`.",
            3,
        )
        .expect("filesystem intent");
        assert_eq!(call.name, "filesystem.list");
        assert_eq!(call.arguments, r#"{"path":"crates"}"#);
        assert!(
            super::parse_natural_tool_intent("Инструмент filesystem.list доступен.", 3).is_none()
        );
    }

    #[test]
    fn parses_nested_json_arguments_from_natural_tool_intent() {
        let call = super::parse_natural_tool_intent(
            r#"Продолжу изучение.
```json
{"tool":"filesystem.read","arguments":{"path":"Cargo.toml"}}
```"#,
            4,
        )
        .expect("filesystem intent");
        assert_eq!(call.name, "filesystem.read");
        assert_eq!(call.arguments, r#"{"path":"Cargo.toml"}"#);
    }

    impl TaskExecutor for NeverExecutor {
        fn execute(
            &self,
            _task_id: String,
            _prompt: String,
            cancellation: CancellationToken,
            _events: tokio::sync::broadcast::Sender<CoreEvent>,
        ) -> BoxFuture<'static, Result<String, AgentRunError>> {
            Box::pin(async move {
                cancellation.cancelled().await;
                Err(AgentRunError::Cancelled)
            })
        }
    }

    #[test]
    fn core_exposes_version() {
        assert!(!CoreVersion::current().is_empty());
    }

    struct ToolCallingExecutor;

    impl TaskExecutor for ToolCallingExecutor {
        fn execute(
            &self,
            task_id: String,
            _prompt: String,
            _cancellation: CancellationToken,
            events: tokio::sync::broadcast::Sender<CoreEvent>,
        ) -> BoxFuture<'static, Result<String, AgentRunError>> {
            Box::pin(async move {
                let _ = events.send(CoreEvent::ToolStarted {
                    task_id: task_id.clone(),
                    tool_name: "filesystem.list".into(),
                });
                Ok("done".into())
            })
        }
    }

    #[tokio::test]
    async fn tool_started_event_appends_a_real_audit_record() {
        let (coordinator, mut events) =
            TaskCoordinator::new_with_executor(8, Some(Arc::new(ToolCallingExecutor)));
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-audit-tool".into(),
                prompt: "list files".into(),
                workspace_root: None,
                preferred_route_hint: None,
            })
            .await
            .expect("start dispatches");
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::TaskStarted { .. })
        ));
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::ToolStarted { .. })
        ));

        let mut records = Vec::new();
        for _ in 0..50 {
            records = coordinator.audit_records().await;
            if records
                .iter()
                .any(|record| record.kind == super::audit::AuditKind::ToolCall)
            {
                break;
            }
            tokio::task::yield_now().await;
        }

        let tool_call = records
            .iter()
            .find(|record| record.kind == super::audit::AuditKind::ToolCall)
            .expect("tool call audit record is appended");
        assert_eq!(tool_call.actor, "task-audit-tool");
        assert_eq!(tool_call.event_id, "tool.started");
        assert_eq!(
            tool_call.fields.get("tool_name").map(String::as_str),
            Some("filesystem.list")
        );

        let jsonl = coordinator.audit_jsonl().await;
        assert!(jsonl.contains("\"kind\":\"tool_call\""));
        assert!(jsonl.contains("filesystem.list"));
    }

    #[tokio::test]
    async fn task_failed_event_appends_a_failure_audit_record() {
        struct FailingExecutor;
        impl TaskExecutor for FailingExecutor {
            fn execute(
                &self,
                _task_id: String,
                _prompt: String,
                _cancellation: CancellationToken,
                _events: tokio::sync::broadcast::Sender<CoreEvent>,
            ) -> BoxFuture<'static, Result<String, AgentRunError>> {
                Box::pin(async move { Err(AgentRunError::Timeout(1)) })
            }
        }

        let (coordinator, mut events) =
            TaskCoordinator::new_with_executor(8, Some(Arc::new(FailingExecutor)));
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-audit-failure".into(),
                prompt: "fail please".into(),
                workspace_root: None,
                preferred_route_hint: None,
            })
            .await
            .expect("start dispatches");
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::TaskStarted { .. })
        ));
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::RoutingTrace { .. })
        ));
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::TaskFailed { .. })
        ));

        let mut records = Vec::new();
        for _ in 0..50 {
            records = coordinator.audit_records().await;
            if records
                .iter()
                .any(|record| record.kind == super::audit::AuditKind::Failure)
            {
                break;
            }
            tokio::task::yield_now().await;
        }

        let failure = records
            .iter()
            .find(|record| record.kind == super::audit::AuditKind::Failure)
            .expect("failure audit record is appended");
        assert_eq!(failure.actor, "task-audit-failure");
        assert_eq!(failure.event_id, "task.failed");
        assert!(failure.fields.contains_key("error"));
    }

    #[tokio::test]
    async fn starts_and_stops_a_task_without_blocking_the_core() {
        let (coordinator, mut events) = TaskCoordinator::new(8);
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-1".into(),
                prompt: "hello".into(),
                workspace_root: None,
                preferred_route_hint: None,
            })
            .await
            .expect("start dispatches");
        assert_eq!(
            events.recv().await.expect("started event"),
            CoreEvent::TaskStarted {
                task_id: "task-1".into(),
                prompt: "hello".into()
            }
        );
        coordinator
            .dispatch(CoreCommand::StopTask {
                task_id: "task-1".into(),
            })
            .await
            .expect("stop dispatches");
        assert!(matches!(
            events.recv().await.expect("routing trace event"),
            CoreEvent::RoutingTrace { .. }
        ));
        assert_eq!(
            events.recv().await.expect("stopped event"),
            CoreEvent::TaskStopped {
                task_id: "task-1".into()
            }
        );
    }

    #[test]
    fn strips_legacy_function_blocks_from_user_facing_message() {
        let message = super::strip_legacy_function_blocks(
            "Готово.\n<function_calls><invoke name=\"filesystem.read\" /></function_calls>",
        );
        assert_eq!(message, "Готово.");
    }

    #[test]
    fn detects_delivery_requirements_from_change_request() {
        let requirements = super::DeliveryRequirements::from_prompt(
            "исправь код, проверь cargo test и создай commit",
        );
        assert!(requirements.mutation);
        assert!(requirements.verification);
        assert!(requirements.commit);
        assert!(!requirements.diff_check);
        assert_eq!(
            requirements.missing(false, false, true, false),
            vec!["внести изменение", "создать commit"]
        );
    }

    #[test]
    fn detects_diff_check_as_a_commit_prerequisite() {
        let requirements = super::DeliveryRequirements::from_prompt(
            "добавь тест, выполни cargo test, git diff --check и создай commit",
        );
        assert!(requirements.verification);
        assert!(requirements.diff_check);
        assert!(requirements.commit);
    }

    #[test]
    fn delivery_gate_uses_resolved_command_and_exit_code() {
        let success = super::recovery::ToolOutcome::success(evohime_tool_runtime::ToolResult {
            output: String::new(),
            structured: serde_json::json!({ "exit_code": 0, "timed_out": false }),
        });
        let failed = super::recovery::ToolOutcome::success(evohime_tool_runtime::ToolResult {
            output: String::new(),
            structured: serde_json::json!({ "exit_code": 1, "timed_out": false }),
        });
        assert_eq!(
            super::classify_shell_verification(r#"{"program":"echo","args":["check"]}"#, &success,),
            (None, None)
        );
        assert_eq!(
            super::classify_shell_verification(
                r#"{"program":"cargo","args":["test","-p","evohime-core"]}"#,
                &success,
            ),
            (Some(true), None)
        );
        assert_eq!(
            super::classify_shell_verification(
                r#"{"program":"git","args":["diff","--check"]}"#,
                &failed,
            ),
            (None, Some(false))
        );
    }

    #[test]
    fn detects_research_requirement_and_keeps_it_open_until_observed() {
        let requirements = super::DeliveryRequirements::from_prompt("изучи проект");
        assert!(requirements.research);
        assert_eq!(
            requirements.missing(false, false, false, false),
            vec!["изучить workspace и подготовить отчёт"]
        );
        assert!(!super::DeliveryRequirements::from_prompt("привет").research);
    }

    #[test]
    fn delivery_gate_finishes_research_before_mutation() {
        let requirements = super::DeliveryRequirements {
            research: true,
            mutation: true,
            verification: true,
            diff_check: true,
            commit: true,
        };
        assert!(super::delivery_next_step(
            requirements,
            super::DeliveryProgress {
                research_done: false,
                mutation_done: false,
                verification_done: false,
                commit_done: false,
                research_observations: 0,
                research_has_overview: false,
                research_has_content: false,
                research_has_search: false,
            },
        )
        .contains("read-only"));
        assert!(super::delivery_next_step(
            requirements,
            super::DeliveryProgress {
                research_done: true,
                mutation_done: false,
                verification_done: false,
                commit_done: false,
                research_observations: 5,
                research_has_overview: true,
                research_has_content: true,
                research_has_search: true,
            },
        )
        .contains("filesystem.patch"));
        assert!(super::delivery_next_step(
            super::DeliveryRequirements {
                research: true,
                ..requirements
            },
            super::DeliveryProgress {
                research_done: false,
                mutation_done: false,
                verification_done: false,
                commit_done: false,
                research_observations: 1,
                research_has_overview: true,
                research_has_content: false,
                research_has_search: false,
            },
        )
        .contains("Cargo.toml"));
    }

    #[test]
    fn parses_tagged_tool_call_format() {
        let call = super::parse_tagged_tool_call(
            r#"<tool_call>filesystem.read(path="README.md")</tool_call>"#,
            4,
        )
        .expect("tagged tool call");
        assert_eq!(call.name, "filesystem.read");
        assert_eq!(call.arguments, r#"{"path":"README.md"}"#);
        let xml_call = super::parse_tagged_tool_call(
            "<tool_name>filesystem.read</tool_name><tool_input>{\"path\": \"README.md\"}</tool_input>",
            5,
        )
        .expect("structured tool call");
        assert_eq!(xml_call.name, "filesystem.read");
        assert_eq!(xml_call.arguments, r#"{"path":"README.md"}"#);
        let code_call = super::parse_tagged_tool_call(
            r#"<tool_code>filesystem.read(path="README.md")</tool_code>"#,
            6,
        )
        .expect("tool code call");
        assert_eq!(code_call.name, "filesystem.read");
        let plain_call = super::parse_plain_tool_call("filesystem.read\npath: README.md", 7)
            .expect("plain tool call");
        assert_eq!(plain_call.name, "filesystem.read");
        assert_eq!(plain_call.arguments, r#"{"path":"README.md"}"#);
        let xml_named = super::parse_xml_named_tool_call(
            "<filesystem.read><parameter>path>README.md</parameter></filesystem.read>",
            8,
        )
        .expect("xml named tool call");
        assert_eq!(xml_named.name, "filesystem.read");
        assert_eq!(xml_named.arguments, r#"{"path":"README.md"}"#);
    }

    #[tokio::test]
    async fn stop_cancels_an_active_executor() {
        let (coordinator, mut events) =
            TaskCoordinator::new_with_executor(8, Some(Arc::new(NeverExecutor)));
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-cancel".into(),
                prompt: "wait".into(),
                workspace_root: None,
                preferred_route_hint: None,
            })
            .await
            .expect("start dispatches");
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::TaskStarted { .. })
        ));
        coordinator
            .dispatch(CoreCommand::StopTask {
                task_id: "task-cancel".into(),
            })
            .await
            .expect("stop dispatches");
        assert!(matches!(
            events.recv().await.expect("routing trace event"),
            CoreEvent::RoutingTrace { .. }
        ));
        assert_eq!(
            events.recv().await.expect("stopped event"),
            CoreEvent::TaskStopped {
                task_id: "task-cancel".into()
            }
        );
    }

    #[tokio::test]
    async fn streams_a_model_response_as_core_events() {
        let gateway = ModelGateway::from_provider(Arc::new(MockProvider::new(
            "mock",
            vec!["hello ".into(), "from core".into()],
        )));
        let agent = ModelAgent::new(Arc::new(gateway));
        let (events, mut receiver) = tokio::sync::broadcast::channel(8);
        let result = agent
            .run_once("task-2", "say hello", &events)
            .await
            .expect("mock model succeeds");
        assert_eq!(result, "hello from core");
        assert_eq!(
            receiver.recv().await.expect("first delta"),
            CoreEvent::AssistantDelta {
                task_id: "task-2".into(),
                content: "hello ".into()
            }
        );
        assert_eq!(
            receiver.recv().await.expect("second delta"),
            CoreEvent::AssistantDelta {
                task_id: "task-2".into(),
                content: "from core".into()
            }
        );
        assert_eq!(
            receiver.recv().await.expect("completed event"),
            CoreEvent::TaskCompleted {
                task_id: "task-2".into(),
                final_message: "hello from core".into()
            }
        );
    }

    #[tokio::test]
    async fn executes_a_safe_filesystem_tool_and_returns_to_the_model() {
        let workspace =
            std::env::temp_dir().join(format!("evohime-core-tool-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&workspace);
        std::fs::write(workspace.join("needle.txt"), "needle in a file").expect("fixture writes");
        let provider = MockProvider::with_tool_call_sequence(
            "mock",
            vec![
                ChatResult {
                    content: String::new(),
                    thinking: None,
                    tool_calls: vec![NativeToolCall {
                        id: "call-1".into(),
                        name: "filesystem.search".into(),
                        arguments: r#"{"query":"needle"}"#.into(),
                    }],
                    usage: None,
                },
                ChatResult {
                    content: "found it".into(),
                    ..ChatResult::default()
                },
            ],
        );
        let agent = ToolAgent::new(
            Arc::new(ModelGateway::from_provider(Arc::new(provider))),
            Arc::new(ToolRegistry::bootstrap()),
        );
        let (events, mut receiver) = tokio::sync::broadcast::channel(16);
        let result = agent
            .run_once("task-tools", "find needle", &workspace, &events)
            .await
            .expect("tool loop succeeds");
        assert_eq!(result, "found it");
        // Контекст собирается перед каждым model call (план 01), поэтому
        // `ModelContext` приходит на каждой итерации и не является разделителем
        // между остальными событиями.
        let mut observed = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            observed.push(event);
        }
        assert!(observed.iter().any(|event| matches!(
            event,
            CoreEvent::ModelContext { workspace_path, context: Some(projection), .. }
                if workspace_path == &workspace.display().to_string()
                    && projection.context_ledger_hash.len() == 64
        )));
        let tool_started = observed
            .iter()
            .position(|event| matches!(event, CoreEvent::ToolStarted { .. }))
            .expect("tool start is observed");
        let tool_output = observed
            .iter()
            .position(|event| matches!(event, CoreEvent::ToolOutput { output, .. } if output.contains("needle")))
            .expect("tool output is observed");
        let completed = observed
            .iter()
            .position(|event| matches!(event, CoreEvent::TaskCompleted { final_message, .. } if final_message == "found it"))
            .expect("task completion is observed");
        assert!(tool_started < tool_output && tool_output < completed);
        let _ = std::fs::remove_dir_all(workspace);
    }

    /// Regression: the shell is fed by pushing the journal tail whenever an
    /// event arrives. Waiting on the broadcast raced the journal writer, so the
    /// tail was read before the event landed and the last event of a task —
    /// the one saying it finished — was never sent.
    #[tokio::test]
    async fn journal_signal_arrives_after_the_event_is_readable() {
        let path =
            std::env::temp_dir().join(format!("evohime-core-signal-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(64, None, journal.clone());
        let mut journalled = coordinator.journalled();

        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-signal".into(),
                prompt: "persist me".into(),
                workspace_root: None,
                preferred_route_hint: None,
            })
            .await
            .expect("command dispatches");

        // The signal must not fire before the event can be read back.
        journalled.changed().await.expect("journal signals");
        let sequence = *journalled.borrow_and_update();
        let batch = journal
            .replay_bounded(sequence as i64 - 1, 16)
            .await
            .expect("tail reads");
        assert!(
            batch
                .events
                .iter()
                .any(|record| record.task_id == "task-signal"),
            "event must be readable when its sequence is announced"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Regression: plan review recorded its progress straight into the journal.
    /// The events were durable, but the pipe server flushes its tail only on the
    /// `journalled` signal, so the shell saw nothing and a running review looked
    /// frozen. Emitted events must both persist and raise the signal.
    #[tokio::test]
    async fn emitted_events_reach_the_journal_signal() {
        let path =
            std::env::temp_dir().join(format!("evohime-core-emit-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(64, None, journal.clone());
        let mut journalled = coordinator.journalled();

        coordinator
            .emit(CoreEvent::ReviewProgress {
                review_id: "review-emit".into(),
                stage: "reviewers".into(),
                status: "working".into(),
                model: Some("model-a".into()),
                completed: 0,
                total: 2,
            })
            .await;

        journalled.changed().await.expect("journal signals");
        let sequence = *journalled.borrow_and_update();
        let batch = journal
            .replay_bounded(sequence as i64 - 1, 16)
            .await
            .expect("tail reads");
        assert!(
            batch
                .events
                .iter()
                .any(|record| record.task_id == "review-emit"
                    && record.event_type == "review.progress"),
            "an emitted review event must be readable when its sequence is announced"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn journals_core_events_and_replays_after_a_sequence() {
        let path =
            std::env::temp_dir().join(format!("evohime-core-journal-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let first = journal
            .record(&CoreEvent::TaskStarted {
                task_id: "task-journal".into(),
                prompt: "persist me".into(),
            })
            .await
            .expect("event records");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-journal".into(),
                final_message: "done".into(),
            })
            .await
            .expect("second event records");
        let replay = journal.replay(first, 10).await.expect("events replay");
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].event_type, "task.completed");
        assert_eq!(replay[0].task_id, "task-journal");
        journal
            .record_audit(
                "run-journal",
                "build.applied",
                br#"{"snapshot_id":"snap-1"}"#,
            )
            .await
            .expect("audit records");
        let audit = journal
            .task_history("run-journal", 10)
            .await
            .expect("audit reads");
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].event_type, "build.applied");
        assert_eq!(audit[0].payload, br#"{"snapshot_id":"snap-1"}"#);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn persists_permission_audit_through_runtime_sink() {
        let path = std::env::temp_dir().join(format!(
            "evohime-core-permission-audit-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let tools = Arc::new(ToolRegistry::bootstrap());
        let sink = super::attach_permission_audit_sink(journal.clone(), &tools).await;
        let task_id = uuid::Uuid::new_v4();
        let request = tools
            .permissions()
            .create_approval(
                task_id,
                "filesystem.write",
                evohime_permissions::Permission::FilesystemWrite,
                "notes.txt",
            )
            .await;
        tools
            .permissions()
            .resolve(request.id, false)
            .await
            .expect("approval resolves");

        let mut history = Vec::new();
        for _ in 0..20 {
            history = journal
                .task_history(&task_id.to_string(), 10)
                .await
                .expect("audit reads");
            if history.len() == 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(history.len(), 2);
        assert!(history
            .iter()
            .all(|entry| entry.event_type == "approval.audit"));
        let payload: serde_json::Value =
            serde_json::from_slice(&history[1].payload).expect("audit payload is JSON");
        assert_eq!(payload["approval_id"], request.id.to_string());
        assert_eq!(payload["decision"], "denied");

        sink.abort();
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn coordinator_journal_captures_lifecycle_events() {
        let path = std::env::temp_dir().join(format!(
            "evohime-core-coordinator-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, mut events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-persisted".into(),
                prompt: "persist lifecycle".into(),
                workspace_root: None,
                preferred_route_hint: None,
            })
            .await
            .expect("start dispatches");
        let _ = events.recv().await.expect("started event");
        coordinator
            .dispatch(CoreCommand::StopTask {
                task_id: "task-persisted".into(),
            })
            .await
            .expect("stop dispatches");
        let _ = events.recv().await.expect("routing trace event");
        let _ = events.recv().await.expect("stopped event");
        let mut replay = Vec::new();
        for _ in 0..20 {
            replay = journal.replay(0, 10).await.expect("replay works");
            if replay.len() >= 5 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(replay.len(), 5);
        let event_types = replay
            .iter()
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            event_types
                .iter()
                .filter(|event| **event == "task.started")
                .count(),
            1
        );
        assert_eq!(
            event_types
                .iter()
                .filter(|event| **event == "task.checkpoint.saved")
                .count(),
            2
        );
        assert_eq!(
            event_types
                .iter()
                .filter(|event| **event == "routing.terminal")
                .count(),
            1
        );
        assert_eq!(
            event_types
                .iter()
                .filter(|event| **event == "task.stopped")
                .count(),
            1
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn approval_denied_outcome_has_ok_false() {
        let outcome =
            recovery::ToolOutcome::denied_by_user("approval denied: mutation not performed");
        // Critical: denied_by_user must set ok to false, so mutation_done remains unchanged
        assert!(
            !outcome.ok,
            "denied_by_user must set ok: false to prevent false success"
        );
        assert_eq!(outcome.output, "approval denied: mutation not performed");
        assert!(matches!(
            outcome.kind,
            Some(recovery::ToolFailureKind::Denied(
                recovery::DenialSource::User
            ))
        ));
    }

    #[test]
    fn before_commit_hook_event_is_valid() {
        let context_order = observability::ContextOrder::capture(
            ["system", "user", "assistant", "tool"]
                .into_iter()
                .map(String::from),
        )
        .unwrap();
        let payload = observability::HookPayload::new([
            ("tool_name".into(), "git.commit".to_owned()),
            ("iteration".into(), "3".to_owned()),
        ])
        .unwrap();
        let event = observability::HookEvent::new(
            observability::HookName::BeforeCommit,
            "event-1",
            "task-1",
            1,
            observability::PolicyDecision::Observe,
            context_order,
            payload,
        )
        .unwrap();
        assert_eq!(event.hook, observability::HookName::BeforeCommit);
        assert_eq!(event.task_id, "task-1");
        let json = event.to_deterministic_json();
        assert!(json.contains("\"hook\":\"before_commit\""));
    }

    #[test]
    fn after_task_hook_event_is_valid() {
        let context_order = observability::ContextOrder::capture(
            ["system", "user", "assistant", "tool"]
                .into_iter()
                .map(String::from),
        )
        .unwrap();
        let payload = observability::HookPayload::new([
            ("status".into(), "exceeded_iteration_limit".to_owned()),
            ("mutation_done".into(), "true".to_owned()),
            ("verification_done".into(), "false".to_owned()),
            ("commit_done".into(), "false".to_owned()),
        ])
        .unwrap();
        let event = observability::HookEvent::new(
            observability::HookName::AfterTask,
            "event-2",
            "task-1",
            2,
            observability::PolicyDecision::Allow,
            context_order,
            payload,
        )
        .unwrap();
        assert_eq!(event.hook, observability::HookName::AfterTask);
        assert_eq!(event.task_id, "task-1");
        let json = event.to_deterministic_json();
        assert!(json.contains("\"hook\":\"after_task\""));
        assert!(json.contains("\"status\":\"exceeded_iteration_limit\""));
    }

    #[test]
    fn observability_hooks_cover_all_gate_points() {
        // Verify that all hook types are accessible and serializable
        for hook in [
            observability::HookName::BeforeContext,
            observability::HookName::BeforeTool,
            observability::HookName::AfterTool,
            observability::HookName::BeforeCommit,
            observability::HookName::AfterTask,
        ] {
            let context_order = observability::ContextOrder::capture(
                ["system", "user", "assistant", "tool"]
                    .into_iter()
                    .map(String::from),
            )
            .unwrap();
            let payload =
                observability::HookPayload::new([("hook_name".into(), format!("{hook:?}"))])
                    .unwrap();
            let event = observability::HookEvent::new(
                hook,
                "e1",
                "t1",
                1,
                observability::PolicyDecision::Allow,
                context_order,
                payload,
            )
            .unwrap();
            let json = event.to_deterministic_json();
            assert!(!json.is_empty());
            assert!(json.len() <= observability::MAX_EVENT_BYTES);
        }
    }
}
